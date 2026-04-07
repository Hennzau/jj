use std::path::{Path, PathBuf};

use jj_lib::backend::BackendInitError;
use jj_lib::object_id::ObjectId;
use jj_lib::op_heads_store::{OpHeadsStore, OpHeadsStoreError, OpHeadsStoreLock};
use jj_lib::op_store::OperationId;
use redb::{
    CommitError, Database, DatabaseError, ReadOnlyDatabase, ReadableDatabase, ReadableTable,
    StorageError, TableDefinition, TableError, TransactionError,
};
use thiserror::Error;

const OP_HEADS: TableDefinition<&[u8], &[u8]> = TableDefinition::new("op_heads");

#[derive(Debug, Error)]
#[error("Failed to initialize operation heads store")]
pub struct RedbOpHeadsStoreInitError(#[from] pub DatabaseError);

impl From<RedbOpHeadsStoreInitError> for BackendInitError {
    fn from(err: RedbOpHeadsStoreInitError) -> Self {
        Self(err.into())
    }
}

#[derive(Debug, Error)]
pub enum RedbOpHeadsStoreError {
    #[error("OpHeads DP error")]
    Database(#[from] DatabaseError),
    #[error("OpHeads DB transaction error")]
    Transaction(#[from] TransactionError),
    #[error("OpHeads DB table error")]
    Table(#[from] TableError),
    #[error("OpHeads DB storage error")]
    Storage(#[from] StorageError),
    #[error("OpHeads DB commit error")]
    Commit(#[from] CommitError),
}

#[derive(Debug)]
pub struct RedbOpHeadsStore {
    db: PathBuf,
}

impl RedbOpHeadsStore {
    pub fn name() -> &'static str {
        "redb_op_heads_store"
    }

    pub fn init(dir: &Path) -> Result<Self, RedbOpHeadsStoreInitError> {
        Database::create(dir.join("db"))?;

        Ok(Self { db: dir.join("db") })
    }

    pub fn open(dir: &Path) -> Self {
        Self { db: dir.join("db") }
    }

    pub fn open_read_only(&self) -> Result<ReadOnlyDatabase, RedbOpHeadsStoreError> {
        Ok(ReadOnlyDatabase::open(&self.db)?)
    }

    pub fn open_write(&self) -> Result<Database, RedbOpHeadsStoreError> {
        Ok(Database::open(&self.db)?)
    }
}

struct RedbOpHeadsStoreLock;
impl OpHeadsStoreLock for RedbOpHeadsStoreLock {}

#[async_trait::async_trait]
impl OpHeadsStore for RedbOpHeadsStore {
    fn name(&self) -> &str {
        Self::name()
    }

    async fn update_op_heads(
        &self,
        old_ids: &[OperationId],
        new_id: &OperationId,
    ) -> Result<(), OpHeadsStoreError> {
        assert!(!old_ids.contains(new_id));

        let db = self.open_write().map_err(|e| OpHeadsStoreError::Write {
            new_op_id: new_id.clone(),
            source: e.into(),
        })?;

        let tx = db.begin_write().map_err(|err| OpHeadsStoreError::Write {
            new_op_id: new_id.clone(),
            source: RedbOpHeadsStoreError::Transaction(err).into(),
        })?;

        {
            let mut table = tx
                .open_table(OP_HEADS)
                .map_err(|err| OpHeadsStoreError::Write {
                    new_op_id: new_id.clone(),
                    source: RedbOpHeadsStoreError::Table(err).into(),
                })?;

            table
                .insert(new_id.as_bytes(), &[] as &[u8])
                .map_err(|err| OpHeadsStoreError::Write {
                    new_op_id: new_id.clone(),
                    source: RedbOpHeadsStoreError::Storage(err).into(),
                })?;

            for old_id in old_ids {
                table
                    .remove(old_id.as_bytes())
                    .map_err(|err| OpHeadsStoreError::Write {
                        new_op_id: new_id.clone(),
                        source: RedbOpHeadsStoreError::Storage(err).into(),
                    })?;
            }
        }

        tx.commit().map_err(|err| OpHeadsStoreError::Write {
            new_op_id: new_id.clone(),
            source: RedbOpHeadsStoreError::Commit(err).into(),
        })?;

        Ok(())
    }

    async fn get_op_heads(&self) -> Result<Vec<OperationId>, OpHeadsStoreError> {
        let db = self
            .open_read_only()
            .map_err(|e| OpHeadsStoreError::Lock(e.into()))?;

        let tx = db
            .begin_read()
            .map_err(|err| OpHeadsStoreError::Read(err.into()))?;

        let table = tx
            .open_table(OP_HEADS)
            .map_err(|err| OpHeadsStoreError::Read(err.into()))?;

        Ok(table
            .iter()
            .unwrap()
            .map(|entry| {
                let (key, _) = entry.unwrap();
                OperationId::new(key.value().to_vec())
            })
            .collect())
    }

    async fn lock(&self) -> Result<Box<dyn OpHeadsStoreLock + '_>, OpHeadsStoreError> {
        Ok(Box::new(RedbOpHeadsStoreLock))
    }
}
