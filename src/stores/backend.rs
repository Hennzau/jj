use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::time::SystemTime;

use futures::stream::BoxStream;
use jj_lib::backend::{
    Backend, BackendError, BackendInitError, ChangeId, Commit, CommitId, CopyHistory, CopyId,
    CopyRecord, FileId, MillisSinceEpoch, SecureSig, Signature, SigningFn, SymlinkId, Timestamp,
    Tree, TreeId, TreeValue, make_root_commit,
};
use jj_lib::conflict_labels::ConflictLabels;
use jj_lib::content_hash::blake2b_hash;
use jj_lib::index::Index;
use jj_lib::merge::MergeBuilder;
use jj_lib::object_id::ObjectId;

use jj_lib::repo_path::{RepoPath, RepoPathBuf, RepoPathComponentBuf};
use pollster::FutureExt;
use prost::Message;
use redb::{
    CommitError, CompactionError, Database, DatabaseError, ReadOnlyDatabase, ReadableDatabase,
    StorageError, TableDefinition, TableError, TransactionError,
};

use thiserror::Error;
use tokio::io::AsyncRead;

mod proto {
    include!("../protos/backend.rs");
}

const COMMIT_ID_LENGTH: usize = 64;
const CHANGE_ID_LENGTH: usize = 16;

pub const FILES: TableDefinition<&[u8], &[u8]> = TableDefinition::new("files");
pub const TREES: TableDefinition<&[u8], &[u8]> = TableDefinition::new("trees");
pub const COMMITS: TableDefinition<&[u8], &[u8]> = TableDefinition::new("commits");

#[derive(Debug, Error)]
#[error("Failed to initialize operation store")]
pub struct RedbBackendInitError(#[from] pub DatabaseError);

impl From<RedbBackendInitError> for BackendInitError {
    fn from(err: RedbBackendInitError) -> Self {
        Self(err.into())
    }
}

#[derive(Debug, Error)]
pub enum RedbBackendError {
    #[error("Backend DB error")]
    Database(#[from] DatabaseError),
    #[error("Backend DB transaction error")]
    Transaction(#[from] TransactionError),
    #[error("Backend DB table error")]
    Table(#[from] TableError),
    #[error("Backend DB storage error")]
    Storage(#[from] StorageError),
    #[error("Backend DB commit error")]
    Commit(#[from] CommitError),
    #[error("Backend DB compaction error")]
    Compaction(#[from] CompactionError),
}

#[derive(Debug)]
pub struct RedbBackend {
    db: PathBuf,
    root_commit_id: CommitId,
    root_change_id: ChangeId,
    empty_tree_id: TreeId,
}

impl RedbBackend {
    pub fn name() -> &'static str {
        "redb_backend"
    }

    pub fn init(dir: &Path) -> Result<Self, RedbBackendInitError> {
        Database::create(dir.join("db"))?;
        let backend = Self {
            db: dir.join("db"),
            root_commit_id: CommitId::from_bytes(&[0; COMMIT_ID_LENGTH]),
            root_change_id: ChangeId::from_bytes(&[0; CHANGE_ID_LENGTH]),
            empty_tree_id: TreeId::from_hex(
                "482ae5a29fbe856c7272f2071b8b0f0359ee2d89ff392b8a900643fbd0836eccd067b8bf41909e206c90d45d6e7d8b6686b93ecaee5fe1a9060d87b672101310",
            ),
        };

        let empty_tree_id = backend
            .write_tree(RepoPath::root(), &Tree::default())
            .block_on()
            .unwrap();

        assert_eq!(empty_tree_id, backend.empty_tree_id);

        Ok(backend)
    }

    pub fn open(dir: &Path) -> Self {
        Self {
            db: dir.join("db"),
            root_commit_id: CommitId::from_bytes(&[0; COMMIT_ID_LENGTH]),
            root_change_id: ChangeId::from_bytes(&[0; CHANGE_ID_LENGTH]),
            empty_tree_id: TreeId::from_hex(
                "482ae5a29fbe856c7272f2071b8b0f0359ee2d89ff392b8a900643fbd0836eccd067b8bf41909e206c90d45d6e7d8b6686b93ecaee5fe1a9060d87b672101310",
            ),
        }
    }

    // busy looping. i'll do a pr in redb to expose necessary api to make it async on linux (poll)
    pub fn open_read_only(&self) -> Result<ReadOnlyDatabase, RedbBackendError> {
        loop {
            match ReadOnlyDatabase::open(&self.db) {
                Ok(db) => break Ok(db),
                Err(DatabaseError::DatabaseAlreadyOpen) => continue,
                Err(e) => break Err(RedbBackendError::Database(e)),
            }
        }
    }

    pub fn open_write(&self) -> Result<Database, RedbBackendError> {
        loop {
            match Database::open(&self.db) {
                Ok(db) => break Ok(db),
                Err(DatabaseError::DatabaseAlreadyOpen) => continue,
                Err(e) => break Err(RedbBackendError::Database(e)),
            }
        }
    }
}

#[async_trait::async_trait]
impl Backend for RedbBackend {
    fn name(&self) -> &'static str {
        Self::name()
    }

    fn commit_id_length(&self) -> usize {
        COMMIT_ID_LENGTH
    }

    fn change_id_length(&self) -> usize {
        CHANGE_ID_LENGTH
    }

    fn root_commit_id(&self) -> &CommitId {
        &self.root_commit_id
    }

    fn root_change_id(&self) -> &ChangeId {
        &self.root_change_id
    }

    fn empty_tree_id(&self) -> &TreeId {
        &self.empty_tree_id
    }

    fn concurrency(&self) -> usize {
        1
    }

    async fn read_file(
        &self,
        path: &RepoPath,
        id: &FileId,
    ) -> Result<Pin<Box<dyn AsyncRead + Send>>, BackendError> {
        let db = self
            .open_read_only()
            .map_err(|e| BackendError::Other(e.into()))?;

        let tx = db.begin_read().map_err(|e| BackendError::ReadFile {
            path: path.to_owned(),
            id: id.clone(),
            source: Box::new(RedbBackendError::from(e)),
        })?;

        let table = tx.open_table(FILES).map_err(|e| BackendError::ReadFile {
            path: path.to_owned(),
            id: id.clone(),
            source: Box::new(RedbBackendError::from(e)),
        })?;

        let value = table
            .get(id.as_bytes())
            .map_err(|e| BackendError::ReadFile {
                path: path.to_owned(),
                id: id.clone(),
                source: Box::new(RedbBackendError::from(e)),
            })?
            .ok_or_else(|| BackendError::ReadFile {
                path: path.to_owned(),
                id: id.clone(),
                source: "Not found".into(),
            })?;

        let buf = value.value().to_vec();

        Ok(Box::pin(std::io::Cursor::new(buf)))
    }

    async fn write_file(
        &self,
        _path: &RepoPath,
        contents: &mut (dyn AsyncRead + Send + Unpin),
    ) -> Result<FileId, BackendError> {
        let mut buf = Vec::new();
        tokio::io::AsyncReadExt::read_to_end(contents, &mut buf)
            .await
            .map_err(|e| BackendError::WriteObject {
                object_type: "file",
                source: Box::new(e),
            })?;

        let id = FileId::new(blake2b_hash(&buf).to_vec());

        let db = self
            .open_write()
            .map_err(|e| BackendError::Other(e.into()))?;

        let tx = db.begin_write().map_err(|e| BackendError::WriteObject {
            object_type: "file",
            source: Box::new(RedbBackendError::from(e)),
        })?;

        {
            let mut table = tx
                .open_table(FILES)
                .map_err(|e| BackendError::WriteObject {
                    object_type: "file",
                    source: Box::new(RedbBackendError::from(e)),
                })?;
            table
                .insert(id.as_bytes(), buf.as_slice())
                .map_err(|e| BackendError::WriteObject {
                    object_type: "file",
                    source: Box::new(RedbBackendError::from(e)),
                })?;
        }

        tx.commit().map_err(|e| BackendError::WriteObject {
            object_type: "file",
            source: Box::new(RedbBackendError::from(e)),
        })?;

        Ok(id)
    }

    async fn read_symlink(
        &self,
        _path: &RepoPath,
        _id: &SymlinkId,
    ) -> Result<String, BackendError> {
        Err(BackendError::Unsupported(
            "The redb backend doesn't support symlinks".to_string(),
        ))
    }

    async fn write_symlink(
        &self,
        _path: &RepoPath,
        _target: &str,
    ) -> Result<SymlinkId, BackendError> {
        Err(BackendError::Unsupported(
            "The redb backend doesn't support symlinks".to_string(),
        ))
    }

    async fn read_copy(&self, _id: &CopyId) -> Result<CopyHistory, BackendError> {
        Err(BackendError::Unsupported(
            "The redb backend doesn't support copies".to_string(),
        ))
    }

    async fn write_copy(&self, _contents: &CopyHistory) -> Result<CopyId, BackendError> {
        Err(BackendError::Unsupported(
            "The redb backend doesn't support copies".to_string(),
        ))
    }

    async fn get_related_copies(
        &self,
        _copy_id: &CopyId,
    ) -> Result<Vec<CopyHistory>, BackendError> {
        Err(BackendError::Unsupported(
            "The redb backend doesn't support copies".to_string(),
        ))
    }

    async fn read_tree(&self, _path: &RepoPath, id: &TreeId) -> Result<Tree, BackendError> {
        let db = self
            .open_read_only()
            .map_err(|e| BackendError::Other(e.into()))?;

        let tx = db.begin_read().map_err(|e| BackendError::ReadObject {
            object_type: "tree".to_string(),
            hash: id.hex(),
            source: Box::new(RedbBackendError::from(e)),
        })?;

        let table = tx.open_table(TREES).map_err(|e| BackendError::ReadObject {
            object_type: "tree".to_string(),
            hash: id.hex(),
            source: Box::new(RedbBackendError::from(e)),
        })?;

        let value = table
            .get(id.as_bytes())
            .map_err(|e| BackendError::ReadObject {
                object_type: "tree".to_string(),
                hash: id.hex(),
                source: Box::new(RedbBackendError::from(e)),
            })?
            .ok_or_else(|| BackendError::ObjectNotFound {
                object_type: "tree".to_string(),
                hash: id.hex(),
                source: "Not found".into(),
            })?;

        let proto = proto::Tree::decode(value.value()).map_err(|e| BackendError::ReadObject {
            object_type: "tree".to_string(),
            hash: id.hex(),
            source: Box::new(e),
        })?;

        Ok(tree_from_proto(proto))
    }

    async fn write_tree(&self, _path: &RepoPath, tree: &Tree) -> Result<TreeId, BackendError> {
        let id = TreeId::new(blake2b_hash(tree).to_vec());

        let proto = tree_to_proto(tree);
        let buf = proto.encode_to_vec();

        let db = self
            .open_write()
            .map_err(|e| BackendError::Other(e.into()))?;

        let tx = db.begin_write().map_err(|e| BackendError::WriteObject {
            object_type: "tree",
            source: Box::new(RedbBackendError::from(e)),
        })?;

        {
            let mut table = tx
                .open_table(TREES)
                .map_err(|e| BackendError::WriteObject {
                    object_type: "tree",
                    source: Box::new(RedbBackendError::from(e)),
                })?;
            table
                .insert(id.as_bytes(), buf.as_slice())
                .map_err(|e| BackendError::WriteObject {
                    object_type: "tree",
                    source: Box::new(RedbBackendError::from(e)),
                })?;
        }

        tx.commit().map_err(|e| BackendError::WriteObject {
            object_type: "tree",
            source: Box::new(RedbBackendError::from(e)),
        })?;

        Ok(id)
    }

    async fn read_commit(&self, id: &CommitId) -> Result<Commit, BackendError> {
        if *id == self.root_commit_id {
            return Ok(make_root_commit(
                self.root_change_id().clone(),
                self.empty_tree_id.clone(),
            ));
        }

        let db = self
            .open_read_only()
            .map_err(|e| BackendError::Other(e.into()))?;

        let tx = db.begin_read().map_err(|e| BackendError::ReadObject {
            object_type: "commit".to_string(),
            hash: id.hex(),
            source: Box::new(RedbBackendError::from(e)),
        })?;

        let table = tx
            .open_table(COMMITS)
            .map_err(|e| BackendError::ReadObject {
                object_type: "commit".to_string(),
                hash: id.hex(),
                source: Box::new(RedbBackendError::from(e)),
            })?;

        let value = table
            .get(id.as_bytes())
            .map_err(|e| BackendError::ReadObject {
                object_type: "commit".to_string(),
                hash: id.hex(),
                source: Box::new(RedbBackendError::from(e)),
            })?
            .ok_or_else(|| BackendError::ObjectNotFound {
                object_type: "commit".to_string(),
                hash: id.hex(),
                source: "Not found".into(),
            })?;

        let proto = proto::Commit::decode(value.value()).map_err(|e| BackendError::ReadObject {
            object_type: "commit".to_string(),
            hash: id.hex(),
            source: Box::new(e),
        })?;

        Ok(commit_from_proto(proto))
    }

    async fn write_commit(
        &self,
        mut commit: Commit,
        sign_with: Option<&mut SigningFn>,
    ) -> Result<(CommitId, Commit), BackendError> {
        assert!(commit.secure_sig.is_none(), "commit.secure_sig was set");

        if commit.parents.is_empty() {
            return Err(BackendError::Other(
                "Cannot write a commit with no parents".into(),
            ));
        }

        let mut proto = commit_to_proto(&commit);
        let data = proto.encode_to_vec();

        if let Some(sign) = sign_with {
            let sig = sign(&data).unwrap();
            proto.secure_sig = Some(sig.clone());
            commit.secure_sig = Some(SecureSig {
                data: data.clone(),
                sig,
            });
        }

        let id = CommitId::new(blake2b_hash(&commit).to_vec());

        let db = self
            .open_write()
            .map_err(|e| BackendError::Other(e.into()))?;
        let tx = db.begin_write().map_err(|e| BackendError::WriteObject {
            object_type: "commit",
            source: Box::new(RedbBackendError::from(e)),
        })?;
        {
            let mut table = tx
                .open_table(COMMITS)
                .map_err(|e| BackendError::WriteObject {
                    object_type: "commit",
                    source: Box::new(RedbBackendError::from(e)),
                })?;
            table.insert(id.as_bytes(), data.as_slice()).map_err(|e| {
                BackendError::WriteObject {
                    object_type: "commit",
                    source: Box::new(RedbBackendError::from(e)),
                }
            })?;
        }

        tx.commit().map_err(|e| BackendError::WriteObject {
            object_type: "commit",
            source: Box::new(RedbBackendError::from(e)),
        })?;

        Ok((id, commit))
    }

    fn get_copy_records(
        &self,
        _paths: Option<&[RepoPathBuf]>,
        _root: &CommitId,
        _head: &CommitId,
    ) -> Result<BoxStream<'_, Result<CopyRecord, BackendError>>, BackendError> {
        Ok(Box::pin(futures::stream::empty()))
    }

    fn gc(&self, _index: &dyn Index, _keep_newer: SystemTime) -> Result<(), BackendError> {
        let mut db = self
            .open_write()
            .map_err(|e| BackendError::Other(e.into()))?;

        let compacted = db
            .compact()
            .map_err(|e| BackendError::Other(Box::new(RedbBackendError::Compaction(e))))?;

        println!(
            "{} compacted the DB!",
            if compacted {
                "Successfully"
            } else {
                "Couldn't"
            }
        );

        Ok(())
    }
}

fn tree_value_from_proto(proto: proto::TreeValue) -> TreeValue {
    match proto.value.unwrap() {
        proto::tree_value::Value::File(proto) => TreeValue::File {
            id: FileId::new(proto.id),
            executable: proto.executable,
            copy_id: CopyId::new(proto.copy_id),
        },
        proto::tree_value::Value::SymlinkId(proto) => TreeValue::Symlink(SymlinkId::new(proto)),
        proto::tree_value::Value::TreeId(proto) => TreeValue::Tree(TreeId::new(proto)),
    }
}

fn tree_value_to_proto(value: &TreeValue) -> proto::TreeValue {
    proto::TreeValue {
        value: Some(match value {
            TreeValue::File {
                id,
                executable,
                copy_id,
            } => proto::tree_value::Value::File(proto::tree_value::File {
                id: id.to_bytes(),
                executable: *executable,
                copy_id: copy_id.to_bytes(),
            }),
            TreeValue::Symlink(id) => proto::tree_value::Value::SymlinkId(id.to_bytes()),
            TreeValue::Tree(id) => proto::tree_value::Value::TreeId(id.to_bytes()),
            _ => panic!("unsupported tree value: {:?}", value),
        }),
    }
}

fn tree_from_proto(proto: proto::Tree) -> Tree {
    Tree::from_sorted_entries(
        proto
            .entries
            .into_iter()
            .map(|entry| {
                (
                    RepoPathComponentBuf::new(entry.name).unwrap(),
                    tree_value_from_proto(entry.value.unwrap()),
                )
            })
            .collect(),
    )
}

fn tree_to_proto(tree: &Tree) -> proto::Tree {
    proto::Tree {
        entries: tree
            .entries()
            .into_iter()
            .map(|entry| proto::tree::Entry {
                name: entry.name().as_internal_str().to_owned(),
                value: Some(tree_value_to_proto(entry.value())),
            })
            .collect(),
    }
}

fn signature_from_proto(proto: proto::commit::Signature) -> Signature {
    let timestamp = proto.timestamp.unwrap_or_default();
    Signature {
        name: proto.name,
        email: proto.email,
        timestamp: Timestamp {
            timestamp: MillisSinceEpoch(timestamp.millis_since_epoch),
            tz_offset: timestamp.tz_offset,
        },
    }
}

fn signature_to_proto(signature: Signature) -> proto::commit::Signature {
    proto::commit::Signature {
        name: signature.name.clone(),
        email: signature.email.clone(),
        timestamp: Some(proto::commit::Timestamp {
            millis_since_epoch: signature.timestamp.timestamp.0,
            tz_offset: signature.timestamp.tz_offset,
        }),
    }
}

fn commit_from_proto(mut proto: proto::Commit) -> Commit {
    let secure_sig = proto.secure_sig.take().map(|sig| SecureSig {
        data: proto.encode_to_vec(),
        sig,
    });

    let parents = proto.parents.into_iter().map(CommitId::new).collect();
    let predecessors = proto.predecessors.into_iter().map(CommitId::new).collect();
    let merge_builder: MergeBuilder<_> = proto.root_tree.into_iter().map(TreeId::new).collect();
    let root_tree = merge_builder.build();
    let conflict_labels = ConflictLabels::from_vec(proto.conflict_labels);
    let change_id = ChangeId::new(proto.change_id);
    Commit {
        parents,
        predecessors,
        root_tree,
        conflict_labels: conflict_labels.into_merge(),
        change_id,
        description: proto.description,
        author: signature_from_proto(proto.author.unwrap_or_default()),
        committer: signature_from_proto(proto.committer.unwrap_or_default()),
        secure_sig,
    }
}

fn commit_to_proto(commit: &Commit) -> proto::Commit {
    let mut proto = proto::Commit::default();
    for parent in &commit.parents {
        proto.parents.push(parent.to_bytes());
    }
    for predecessor in &commit.predecessors {
        proto.predecessors.push(predecessor.to_bytes());
    }
    proto.root_tree = commit.root_tree.iter().map(|id| id.to_bytes()).collect();
    if !commit.conflict_labels.is_resolved() {
        proto.conflict_labels = commit.conflict_labels.as_slice().to_owned();
    }
    proto.change_id = commit.change_id.to_bytes();
    proto.description = commit.description.clone();
    proto.author = Some(signature_to_proto(commit.author.clone()));
    proto.committer = Some(signature_to_proto(commit.committer.clone()));
    proto
}
