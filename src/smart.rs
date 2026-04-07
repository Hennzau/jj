use std::{
    collections::{HashMap, HashSet},
    io::{Read, Write},
    path::Path,
    sync::Arc,
};

use futures::StreamExt;
use jj_cli::command_error::{CommandError, cli_error};
use jj_lib::{
    backend::{CommitId, FileId, TreeId, TreeValue},
    commit::Commit,
    object_id::ObjectId,
    op_store::{OpStore, OperationId, ViewId},
    op_walk, operation,
    store::Store,
};

use pollster::FutureExt;
use redb::{ReadableDatabase, ReadableTable, TableDefinition};

use crate::stores::{COMMITS, FILES, OP_HEADS, OPERATIONS, TREES, VIEWS};

include!("protos/smart.rs");

#[derive(Default)]
pub struct PackIds {
    pub op_heads: HashSet<OperationId>,
    pub op_ids: HashSet<OperationId>,
    pub view_ids: HashSet<ViewId>,
    pub commit_ids: HashSet<CommitId>,
    pub tree_ids: HashSet<TreeId>,
    pub file_ids: HashSet<FileId>,
}

fn commit_content_ids(
    commits: impl Iterator<Item = Commit>,
) -> (HashSet<CommitId>, HashSet<TreeId>, HashSet<FileId>) {
    let mut tree_ids = HashSet::new();
    let mut commit_ids = HashSet::new();

    let file_ids = commits
        .flat_map(|commit| {
            tree_ids.extend(commit.tree_ids().iter().cloned());
            commit_ids.insert(commit.id().clone());

            commit.tree().entries().flat_map(|(_, entry)| {
                entry.into_iter().flat_map(|entry| {
                    entry.into_iter().flat_map(|v| match v.clone() {
                        Some(TreeValue::File { id, .. }) => Some(id),
                        _ => None,
                    })
                })
            })
        })
        .collect();

    (commit_ids, tree_ids, file_ids)
}

impl PackIds {
    pub async fn new(
        store: &Arc<Store>,
        op_store: &Arc<dyn OpStore>,
        heads_id: &[OperationId],
        roots_id: &[OperationId],
    ) -> Self {
        let heads = heads_id
            .iter()
            .flat_map(|id| {
                op_store
                    .read_operation(id)
                    .block_on()
                    .into_iter()
                    .map(|op| operation::Operation::new(op_store.clone(), id.clone(), op))
            })
            .collect::<Vec<_>>();

        let roots = roots_id
            .iter()
            .flat_map(|id| {
                op_store
                    .read_operation(id)
                    .block_on()
                    .into_iter()
                    .map(|op| operation::Operation::new(op_store.clone(), id.clone(), op))
            })
            .collect::<Vec<_>>();

        let missing_ops = op_walk::walk_ancestors_range(&heads, &roots)
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .flat_map(|op| op.into_iter());

        let (_, _, root_file_ids) = commit_content_ids(roots.into_iter().flat_map(|op| {
            op.all_referenced_commit_ids()
                .cloned()
                .collect::<Vec<_>>()
                .into_iter()
                .flat_map(|id| store.get_commit(&id).into_iter())
        }));

        let mut pack = PackIds {
            op_heads: heads_id.iter().cloned().collect(),
            ..Default::default()
        };

        let (missing_commit_ids, missing_tree_ids, all_referenced_file_ids) =
            commit_content_ids(missing_ops.flat_map(|op| {
                pack.view_ids.insert(op.view_id().clone());
                pack.op_ids.insert(op.id().clone());

                op.all_referenced_commit_ids()
                    .cloned()
                    .collect::<Vec<_>>()
                    .into_iter()
                    .flat_map(|id| store.get_commit(&id).into_iter())
            }));

        pack.commit_ids = missing_commit_ids;
        pack.tree_ids = missing_tree_ids;
        pack.file_ids = all_referenced_file_ids
            .difference(&root_file_ids)
            .cloned()
            .collect();

        pack
    }
}

impl Pack {
    pub fn write(self, repo_dir: &Path) -> Result<(), CommandError> {
        let write_table = |db_rel: &str,
                           def: TableDefinition<&[u8], &[u8]>,
                           entries: &HashMap<String, Vec<u8>>|
         -> Result<(), CommandError> {
            if entries.is_empty() {
                return Ok(());
            }
            let db_path = repo_dir.join(db_rel);
            let db = redb::Database::create(&db_path).map_err(|e| cli_error(format!("{e}")))?;
            let txn = db.begin_write().map_err(|e| cli_error(format!("{e}")))?;
            {
                let mut table = txn.open_table(def).map_err(|e| cli_error(format!("{e}")))?;
                for (hex_key, value) in entries {
                    let key = hex::decode(hex_key).map_err(|e| cli_error(format!("{e}")))?;
                    table
                        .insert(key.as_slice(), value.as_slice())
                        .map_err(|e| cli_error(format!("{e}")))?;
                }
            }
            txn.commit().map_err(|e| cli_error(format!("{e}")))?;
            Ok(())
        };

        write_table("op_heads/db", OP_HEADS, &self.heads)?;
        write_table("op_store/db", OPERATIONS, &self.operations)?;
        write_table("op_store/db", VIEWS, &self.views)?;
        write_table("store/db", COMMITS, &self.commits)?;
        write_table("store/db", TREES, &self.trees)?;
        write_table("store/db", FILES, &self.files)?;

        Ok(())
    }
}

impl Heads {
    pub fn new(repo_dir: &Path) -> Result<Self, CommandError> {
        let db_path = repo_dir.join("op_heads").join("db");
        let db = redb::Database::create(&db_path).map_err(|e| cli_error(format!("{e}")))?;
        let txn = db.begin_read().map_err(|e| cli_error(format!("{e}")))?;
        let table = txn
            .open_table(OP_HEADS)
            .map_err(|e| cli_error(format!("{e}")))?;

        let heads: Vec<Vec<u8>> = table
            .iter()
            .into_iter()
            .flat_map(|entry| {
                entry
                    .into_iter()
                    .flat_map(|entry| entry.into_iter().map(|(key, _)| key.value().to_vec()))
            })
            .collect();

        Ok(Self { heads })
    }

    pub fn as_ops(&self) -> impl Iterator<Item = OperationId> {
        self.heads.iter().map(|op| OperationId::from_bytes(op))
    }

    pub fn are_missing(&self, repo_dir: &Path) -> Result<bool, CommandError> {
        let db = redb::Database::create(repo_dir.join("op_store").join("db"))
            .map_err(|e| cli_error(format!("{e}")))?;
        let txn = db.begin_read().map_err(|e| cli_error(format!("{e}")))?;

        let table = match txn.open_table(OPERATIONS) {
            Ok(table) => table,
            _ => return Ok(true),
        };

        for head in &self.heads {
            if let Ok(None) = table.get(head.as_slice()) {
                return Ok(true);
            }
        }

        Ok(false)
    }
}

impl PackIds {
    pub fn into_pack(self, repo_dir: &Path) -> Result<Pack, CommandError> {
        let mut pack = Pack::default();

        let heads_keys: Vec<&[u8]> = self.op_heads.iter().map(|id| id.as_bytes()).collect();
        let op_keys: Vec<&[u8]> = self.op_ids.iter().map(|id| id.as_bytes()).collect();
        let view_keys: Vec<&[u8]> = self.view_ids.iter().map(|id| id.as_bytes()).collect();
        let commit_keys: Vec<&[u8]> = self.commit_ids.iter().map(|id| id.as_bytes()).collect();
        let tree_keys: Vec<&[u8]> = self.tree_ids.iter().map(|id| id.as_bytes()).collect();
        let file_keys: Vec<&[u8]> = self.file_ids.iter().map(|id| id.as_bytes()).collect();

        let read_entries = |db_rel: &str,
                            def: TableDefinition<&[u8], &[u8]>,
                            keys: &[&[u8]]|
         -> Result<HashMap<String, Vec<u8>>, CommandError> {
            let db_path = repo_dir.join(db_rel);
            let db = redb::Database::open(&db_path).map_err(|e| cli_error(format!("{e}")))?;
            let txn = db.begin_read().map_err(|e| cli_error(format!("{e}")))?;
            let table = txn.open_table(def).map_err(|e| cli_error(format!("{e}")))?;
            let mut map = HashMap::new();
            for key in keys {
                if let Some(val) = table.get(key).map_err(|e| cli_error(format!("{e}")))? {
                    map.insert(hex::encode(key), val.value().to_vec());
                }
            }
            Ok(map)
        };

        pack.heads = read_entries("op_heads/db", OP_HEADS, &heads_keys)?;
        pack.operations = read_entries("op_store/db", OPERATIONS, &op_keys)?;
        pack.views = read_entries("op_store/db", VIEWS, &view_keys)?;
        pack.commits = read_entries("store/db", COMMITS, &commit_keys)?;
        pack.trees = read_entries("store/db", TREES, &tree_keys)?;
        pack.files = read_entries("store/db", FILES, &file_keys)?;

        Ok(pack)
    }
}

pub fn send_message(msg: &impl prost::Message) -> std::io::Result<()> {
    let buf = msg.encode_to_vec();
    let len = (buf.len() as u32).to_be_bytes();
    let mut out = std::io::stdout().lock();
    out.write_all(&len)?;
    out.write_all(&buf)?;
    out.flush()
}

pub fn recv_message<M: prost::Message + Default>() -> std::io::Result<M> {
    let mut r = std::io::stdin().lock();
    let mut len_buf = [0u8; 4];
    r.read_exact(&mut len_buf)?;
    let len = u32::from_be_bytes(len_buf) as usize;
    let mut buf = vec![0u8; len];
    r.read_exact(&mut buf)?;
    M::decode(&buf[..]).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}
