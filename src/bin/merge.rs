use jj::stores::*;
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};

const ALL_TABLES: &[TableDefinition<&[u8], &[u8]>] =
    &[OP_HEADS, OPERATIONS, VIEWS, FILES, TREES, COMMITS];

fn main() {
    let db1_path = std::env::args().nth(1).unwrap();
    let db2_path = std::env::args().nth(2).unwrap();

    let db1 = Database::open(&db1_path).unwrap();
    let db2 = Database::open(&db2_path).unwrap();

    let read_tx = db2.begin_read().unwrap();
    let write_tx = db1.begin_write().unwrap();

    for table_def in ALL_TABLES {
        let src = match read_tx.open_table(*table_def) {
            Ok(t) => t,
            Err(_) => continue,
        };
        let mut dst = write_tx.open_table(*table_def).unwrap();
        for entry in src.iter().unwrap() {
            let entry = entry.unwrap();
            let key = entry.0.value();
            let val = entry.1.value();

            if dst.get(key).unwrap().is_none() {
                dst.insert(key, val).unwrap();
            }
        }
    }

    drop(read_tx);
    write_tx.commit().unwrap();

    println!("Merged {} into {}", db2_path, db1_path);
}
