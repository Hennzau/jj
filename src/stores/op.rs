use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use jj_lib::backend::{BackendInitError, CommitId, MillisSinceEpoch, Timestamp};
use jj_lib::content_hash::blake2b_hash;
use jj_lib::merge::Merge;
use jj_lib::object_id::{HexPrefix, ObjectId, PrefixResolution};
use jj_lib::op_store::{
    OpStore, OpStoreError, Operation, OperationId, OperationMetadata, RefTarget, RemoteRef,
    RemoteRefState, RemoteView, RootOperationData, TimestampRange, View, ViewId,
};

use jj_lib::ref_name::{RefNameBuf, RemoteNameBuf, WorkspaceNameBuf};
use prost::Message;
use redb::{
    CommitError, Database, DatabaseError, ReadOnlyDatabase, ReadableDatabase, ReadableTable,
    StorageError, TableDefinition, TableError, TransactionError,
};

use thiserror::Error;

mod proto {
    include!("../protos/op_store.rs");
}

const OPERATION_ID_LENGTH: usize = 64;
const VIEW_ID_LENGTH: usize = 64;

pub const OPERATIONS: TableDefinition<&[u8], &[u8]> = TableDefinition::new("operations");
pub const VIEWS: TableDefinition<&[u8], &[u8]> = TableDefinition::new("views");

#[derive(Debug, Error)]
#[error("Failed to initialize operation store")]
pub struct RedbOpStoreInitError(#[from] pub DatabaseError);

impl From<RedbOpStoreInitError> for BackendInitError {
    fn from(err: RedbOpStoreInitError) -> Self {
        Self(err.into())
    }
}

#[derive(Debug, Error)]
pub enum RedbOpStoreError {
    #[error("OpStore DP error")]
    Database(#[from] DatabaseError),
    #[error("OpStore DB transaction error")]
    Transaction(#[from] TransactionError),
    #[error("OpStore DB table error")]
    Table(#[from] TableError),
    #[error("OpStore DB storage error")]
    Storage(#[from] StorageError),
    #[error("OpStore DB commit error")]
    Commit(#[from] CommitError),
}

#[derive(Debug)]
pub struct RedbOpStore {
    db: PathBuf,
    root_data: RootOperationData,
    root_operation_id: OperationId,
    root_view_id: ViewId,
}

impl RedbOpStore {
    pub fn name() -> &'static str {
        "redb_op_store"
    }

    pub fn init(dir: &Path, root_data: RootOperationData) -> Result<Self, RedbOpStoreInitError> {
        Database::create(dir.join("db"))?;

        Ok(Self {
            db: dir.join("db"),
            root_data,
            root_operation_id: OperationId::from_bytes(&[0u8; OPERATION_ID_LENGTH]),
            root_view_id: ViewId::from_bytes(&[0u8; VIEW_ID_LENGTH]),
        })
    }

    pub fn open(dir: &Path, root_data: RootOperationData) -> Self {
        Self {
            db: dir.join("db"),
            root_data,
            root_operation_id: OperationId::from_bytes(&[0u8; OPERATION_ID_LENGTH]),
            root_view_id: ViewId::from_bytes(&[0u8; VIEW_ID_LENGTH]),
        }
    }

    pub fn open_read_only(&self) -> Result<ReadOnlyDatabase, RedbOpStoreError> {
        Ok(ReadOnlyDatabase::open(&self.db)?)
    }

    pub fn open_write(&self) -> Result<Database, RedbOpStoreError> {
        Ok(Database::open(&self.db)?)
    }
}

#[async_trait::async_trait]
impl OpStore for RedbOpStore {
    fn name(&self) -> &str {
        Self::name()
    }

    fn root_operation_id(&self) -> &OperationId {
        &self.root_operation_id
    }

    async fn read_view(&self, id: &ViewId) -> Result<View, OpStoreError> {
        if *id == self.root_view_id {
            return Ok(View::make_root(self.root_data.root_commit_id.clone()));
        }

        let db = self
            .open_read_only()
            .map_err(|e| OpStoreError::Other(e.into()))?;

        let tx = db
            .begin_read()
            .map_err(|err| OpStoreError::Other(RedbOpStoreError::Transaction(err).into()))?;

        let table = tx
            .open_table(VIEWS)
            .map_err(|err| OpStoreError::Other(RedbOpStoreError::Table(err).into()))?;

        let value = table
            .get(id.as_bytes())
            .map_err(|err| OpStoreError::Other(RedbOpStoreError::Storage(err).into()))?
            .ok_or_else(|| OpStoreError::ObjectNotFound {
                object_type: "view".to_string(),
                hash: id.hex(),
                source: "Not found in redb".into(),
            })?;

        let proto: proto::View =
            prost::Message::decode(value.value()).map_err(|err| OpStoreError::Other(err.into()))?;

        Ok(view_from_proto(proto))
    }

    async fn write_view(&self, view: &View) -> Result<ViewId, OpStoreError> {
        let id = ViewId::new(blake2b_hash(view).to_vec());

        let proto = view_to_proto(view);
        let buf = proto.encode_to_vec();

        let db = self
            .open_write()
            .map_err(|e| OpStoreError::Other(e.into()))?;

        let tx = db
            .begin_write()
            .map_err(|err| OpStoreError::Other(RedbOpStoreError::Transaction(err).into()))?;

        {
            let mut table = tx
                .open_table(VIEWS)
                .map_err(|err| OpStoreError::Other(RedbOpStoreError::Table(err).into()))?;

            table
                .insert(id.as_bytes(), buf.as_slice())
                .map_err(|err| OpStoreError::Other(RedbOpStoreError::Storage(err).into()))?;
        }

        tx.commit()
            .map_err(|err| OpStoreError::Other(RedbOpStoreError::Commit(err).into()))?;

        Ok(id)
    }

    async fn read_operation(&self, operation_id: &OperationId) -> Result<Operation, OpStoreError> {
        if *operation_id == self.root_operation_id {
            return Ok(Operation::make_root(self.root_view_id.clone()));
        }

        let db = self
            .open_read_only()
            .map_err(|e| OpStoreError::Other(e.into()))?;

        let tx = db
            .begin_read()
            .map_err(|err| OpStoreError::Other(RedbOpStoreError::Transaction(err).into()))?;

        let table = tx
            .open_table(OPERATIONS)
            .map_err(|err| OpStoreError::Other(RedbOpStoreError::Table(err).into()))?;

        let value = table
            .get(operation_id.as_bytes())
            .map_err(|err| OpStoreError::Other(RedbOpStoreError::Storage(err).into()))?
            .ok_or_else(|| OpStoreError::ObjectNotFound {
                object_type: "operation".to_string(),
                hash: operation_id.hex(),
                source: "Not found in redb".into(),
            })?;

        let proto: proto::Operation =
            prost::Message::decode(value.value()).map_err(|err| OpStoreError::Other(err.into()))?;

        let mut operation = operation_from_proto(proto);

        if operation.parents.is_empty() {
            operation.parents.push(self.root_operation_id.clone());
        }

        Ok(operation)
    }

    async fn write_operation(&self, operation: &Operation) -> Result<OperationId, OpStoreError> {
        let id = OperationId::new(blake2b_hash(operation).to_vec());

        let proto = operation_to_proto(operation);
        let buf = proto.encode_to_vec();

        let db = self
            .open_write()
            .map_err(|e| OpStoreError::Other(e.into()))?;

        let tx = db
            .begin_write()
            .map_err(|err| OpStoreError::Other(RedbOpStoreError::Transaction(err).into()))?;

        {
            let mut table = tx
                .open_table(OPERATIONS)
                .map_err(|err| OpStoreError::Other(RedbOpStoreError::Table(err).into()))?;

            table
                .insert(id.as_bytes(), buf.as_slice())
                .map_err(|err| OpStoreError::Other(RedbOpStoreError::Storage(err).into()))?;
        }

        tx.commit()
            .map_err(|err| OpStoreError::Other(RedbOpStoreError::Commit(err).into()))?;

        Ok(id)
    }

    async fn resolve_operation_id_prefix(
        &self,
        prefix: &HexPrefix,
    ) -> Result<PrefixResolution<OperationId>, OpStoreError> {
        let db = self
            .open_read_only()
            .map_err(|e| OpStoreError::Other(e.into()))?;

        let tx = db
            .begin_read()
            .map_err(|err| OpStoreError::Other(RedbOpStoreError::Transaction(err).into()))?;

        let table = tx
            .open_table(OPERATIONS)
            .map_err(|err| OpStoreError::Other(RedbOpStoreError::Table(err).into()))?;

        let mut match_found: Option<OperationId> = None;

        for entry in table
            .iter()
            .map_err(|err| OpStoreError::Other(RedbOpStoreError::Storage(err).into()))?
        {
            let (key, _) =
                entry.map_err(|err| OpStoreError::Other(RedbOpStoreError::Storage(err).into()))?;
            let id = OperationId::new(key.value().to_vec());
            if prefix.matches(&id) {
                if match_found.is_some() {
                    return Ok(PrefixResolution::AmbiguousMatch);
                }
                match_found = Some(id);
            }
        }

        match match_found {
            Some(id) => Ok(PrefixResolution::SingleMatch(id)),
            None => Ok(PrefixResolution::NoMatch),
        }
    }

    async fn gc(&self, _: &[OperationId], _: SystemTime) -> Result<(), OpStoreError> {
        Ok(())
    }
}

fn ref_target_from_proto(proto: proto::RefTarget) -> RefTarget {
    let term_from_proto = |term: proto::ref_target::Term| term.value.map(CommitId::new);
    let removes = proto.removes.into_iter().map(term_from_proto);
    let adds = proto.adds.into_iter().map(term_from_proto);

    RefTarget::from_merge(Merge::from_removes_adds(removes, adds))
}

fn ref_target_to_proto(value: &RefTarget) -> proto::RefTarget {
    let term_to_proto = |term: &Option<CommitId>| proto::ref_target::Term {
        value: term.as_ref().map(|id| id.to_bytes()),
    };

    let merge = value.as_merge();
    let adds = merge.adds().map(term_to_proto).collect();
    let removes = merge.removes().map(term_to_proto).collect();

    proto::RefTarget { adds, removes }
}

fn remote_ref_from_proto(proto: proto::RemoteRef) -> RemoteRef {
    RemoteRef {
        target: ref_target_from_proto(proto.target.unwrap_or_default()),
        state: match proto.state {
            0 => RemoteRefState::New,
            1 => RemoteRefState::Tracked,
            _ => unreachable!(),
        },
    }
}

fn remote_ref_to_proto(value: &RemoteRef) -> proto::RemoteRef {
    proto::RemoteRef {
        state: value.state as i32,
        target: Some(ref_target_to_proto(&value.target)),
    }
}

fn remote_view_from_proto(proto: proto::RemoteView) -> RemoteView {
    RemoteView {
        bookmarks: proto
            .bookmarks
            .into_iter()
            .map(|(k, v)| (RefNameBuf::from(k), remote_ref_from_proto(v)))
            .collect(),
        tags: proto
            .tags
            .into_iter()
            .map(|(k, v)| (RefNameBuf::from(k), remote_ref_from_proto(v)))
            .collect(),
    }
}

fn remote_view_to_proto(value: &RemoteView) -> proto::RemoteView {
    proto::RemoteView {
        bookmarks: value
            .bookmarks
            .iter()
            .map(|(k, v)| (k.into(), remote_ref_to_proto(v)))
            .collect(),
        tags: value
            .tags
            .iter()
            .map(|(k, v)| (k.into(), remote_ref_to_proto(v)))
            .collect(),
    }
}

fn view_from_proto(proto: proto::View) -> View {
    View {
        head_ids: proto.head_ids.into_iter().map(CommitId::new).collect(),
        local_bookmarks: proto
            .local_bookmarks
            .into_iter()
            .map(|(k, v)| (RefNameBuf::from(k), ref_target_from_proto(v)))
            .collect(),
        local_tags: proto
            .local_tags
            .into_iter()
            .map(|(k, v)| (RefNameBuf::from(k), ref_target_from_proto(v)))
            .collect(),
        wc_commit_ids: proto
            .wc_commit_ids
            .into_iter()
            .map(|(k, v)| (WorkspaceNameBuf::from(k), CommitId::new(v)))
            .collect(),
        remote_views: proto
            .remote_views
            .into_iter()
            .map(|(k, v)| (RemoteNameBuf::from(k), remote_view_from_proto(v)))
            .collect(),
        git_head: RefTarget::absent(),
        git_refs: BTreeMap::new(),
    }
}

fn view_to_proto(value: &View) -> proto::View {
    proto::View {
        head_ids: value.head_ids.iter().map(|id| id.to_bytes()).collect(),
        local_bookmarks: value
            .local_bookmarks
            .iter()
            .map(|(k, v)| (k.into(), ref_target_to_proto(v)))
            .collect(),
        local_tags: value
            .local_tags
            .iter()
            .map(|(k, v)| (k.into(), ref_target_to_proto(v)))
            .collect(),
        wc_commit_ids: value
            .wc_commit_ids
            .iter()
            .map(|(k, v)| (k.into(), v.to_bytes()))
            .collect(),
        remote_views: value
            .remote_views
            .iter()
            .map(|(k, v)| (k.into(), remote_view_to_proto(v)))
            .collect(),
    }
}

fn timestamp_from_proto(proto: proto::Timestamp) -> Timestamp {
    Timestamp {
        timestamp: MillisSinceEpoch(proto.millis_since_epoch),
        tz_offset: proto.tz_offset,
    }
}

fn timestamp_to_proto(value: &Timestamp) -> proto::Timestamp {
    proto::Timestamp {
        millis_since_epoch: value.timestamp.0,
        tz_offset: value.tz_offset,
    }
}

fn operation_metadata_from_proto(proto: proto::OperationMetadata) -> OperationMetadata {
    let time = TimestampRange {
        start: timestamp_from_proto(proto.start_time.unwrap_or_default()),
        end: timestamp_from_proto(proto.end_time.unwrap_or_default()),
    };

    OperationMetadata {
        time,
        description: proto.description,
        hostname: proto.hostname,
        username: proto.username,
        is_snapshot: proto.is_snapshot,
        workspace_name: proto.workspace_name.clone().map(Into::into),
        tags: proto.tags,
    }
}

fn operation_metadata_to_proto(value: &OperationMetadata) -> proto::OperationMetadata {
    proto::OperationMetadata {
        start_time: Some(timestamp_to_proto(&value.time.start)),
        end_time: Some(timestamp_to_proto(&value.time.end)),
        description: value.description.clone(),
        hostname: value.hostname.clone(),
        username: value.username.clone(),
        workspace_name: value.workspace_name.clone().map(Into::into),
        is_snapshot: value.is_snapshot,
        tags: value.tags.clone(),
    }
}

fn commit_predecessors_from_proto(
    proto: Vec<proto::CommitPredecessors>,
) -> BTreeMap<CommitId, Vec<CommitId>> {
    proto
        .into_iter()
        .map(|entry| {
            let commit_id = CommitId::new(entry.commit_id);
            let predecessor_ids = entry
                .predecessor_ids
                .into_iter()
                .map(CommitId::new)
                .collect();
            (commit_id, predecessor_ids)
        })
        .collect()
}

fn commit_predecessors_to_proto(
    value: &BTreeMap<CommitId, Vec<CommitId>>,
) -> Vec<proto::CommitPredecessors> {
    value
        .iter()
        .map(|(commit_id, predecessor_ids)| proto::CommitPredecessors {
            commit_id: commit_id.to_bytes(),
            predecessor_ids: predecessor_ids
                .iter()
                .map(|id| id.to_bytes())
                .collect(),
        })
        .collect()
}

fn operation_from_proto(proto: proto::Operation) -> Operation {
    let view_id = ViewId::new(proto.view_id);
    let parents = proto.parents.into_iter().map(OperationId::new).collect();

    let metadata = operation_metadata_from_proto(proto.metadata.unwrap_or_default());
    let commit_predecessors = Some(commit_predecessors_from_proto(proto.commit_predecessors));

    Operation {
        view_id,
        parents,
        metadata,
        commit_predecessors,
    }
}

fn operation_to_proto(value: &Operation) -> proto::Operation {
    proto::Operation {
        view_id: value.view_id.to_bytes(),
        parents: value.parents.iter().map(|id| id.to_bytes()).collect(),
        metadata: Some(operation_metadata_to_proto(&value.metadata)),
        commit_predecessors: commit_predecessors_to_proto(
            value
                .commit_predecessors
                .as_ref()
                .unwrap_or(&BTreeMap::new()),
        ),
    }
}
