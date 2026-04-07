use jj::stores::*;
use jj_cli::command_error::{CommandError, cli_error};
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use std::path::{Path, PathBuf};

fn find_repo_path(dst: &Path) -> Option<PathBuf> {
    for base in [dst.to_path_buf(), dst.join(".jj").join("repo")] {
        if base.join("op_heads").join("db").exists()
            && base.join("op_store").join("db").exists()
            && base.join("store").join("db").exists()
        {
            return Some(base);
        }
    }
    None
}

fn merge_db(
    src: &Path,
    dst: &Path,
    tables: &[TableDefinition<&[u8], &[u8]>],
) -> Result<(), CommandError> {
    let src_db = Database::open(src)
        .map_err(|e| cli_error(format!("Failed to open {}: {e}", src.display())))?;
    let dst_db = Database::open(dst)
        .map_err(|e| cli_error(format!("Failed to open {}: {e}", dst.display())))?;

    let read_tx = src_db.begin_read().map_err(|e| cli_error(e.to_string()))?;
    let write_tx = dst_db.begin_write().map_err(|e| cli_error(e.to_string()))?;

    for table_def in tables {
        let src_table = match read_tx.open_table(*table_def) {
            Ok(t) => t,
            Err(_) => continue,
        };
        let mut dst_table = write_tx
            .open_table(*table_def)
            .map_err(|e| cli_error(e.to_string()))?;
        for entry in src_table.iter().map_err(|e| cli_error(e.to_string()))? {
            let entry = entry.map_err(|e| cli_error(e.to_string()))?;
            let key = entry.0.value();
            let val = entry.1.value();
            if dst_table
                .get(key)
                .map_err(|e| cli_error(e.to_string()))?
                .is_none()
            {
                dst_table
                    .insert(key, val)
                    .map_err(|e| cli_error(e.to_string()))?;
            }
        }
    }

    drop(read_tx);
    write_tx.commit().map_err(|e| cli_error(e.to_string()))?;
    Ok(())
}

pub fn import_local(
    src_op_heads: PathBuf,
    src_op_store: PathBuf,
    src_store: PathBuf,
    dst: PathBuf,
) -> Result<(), CommandError> {
    let repo_path = find_repo_path(&dst)
        .ok_or_else(|| cli_error(format!("'{}' is not a valid repository", dst.display())))?;

    merge_db(
        &repo_path.join("op_heads").join("db"),
        &src_op_heads,
        &[OP_HEADS],
    )?;
    merge_db(
        &repo_path.join("op_store").join("db"),
        &src_op_store,
        &[OPERATIONS, VIEWS],
    )?;
    merge_db(
        &repo_path.join("store").join("db"),
        &src_store,
        &[FILES, TREES, COMMITS],
    )?;

    Ok(())
}
