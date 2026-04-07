use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use std::path::PathBuf;

const DBS: &[(&str, &[&str])] = &[
    ("op_heads/db", &["heads"]),
    ("op_store/db", &["operations", "views"]),
    (
        "store/db",
        &["commits", "files", "symlinks", "trees", "conflicts"],
    ),
];

fn main() {
    let repo = std::env::args().nth(1).unwrap();
    let bare = std::env::args()
        .nth(2)
        .map(|s| s == "--bare")
        .unwrap_or(false);

    let repo_dir = if bare {
        PathBuf::from(&repo)
    } else {
        PathBuf::from(&repo).join(".jj").join("repo")
    };

    for &(db_rel, tables) in DBS {
        let db_path = repo_dir.join(db_rel);
        if !db_path.exists() {
            println!("── {} (missing)", db_rel);
            continue;
        }
        let db = Database::open(&db_path).unwrap();
        let txn = db.begin_read().unwrap();

        println!("── {}", db_rel);
        for &table_name in tables {
            let def: TableDefinition<&[u8], &[u8]> = TableDefinition::new(table_name);
            let table = match txn.open_table(def) {
                Ok(t) => t,
                Err(redb::TableError::TableDoesNotExist(_)) => {
                    println!("   ├─ {} (empty)", table_name);
                    continue;
                }
                Err(e) => {
                    println!("   ├─ {} (error: {})", table_name, e);
                    continue;
                }
            };
            let count = table.iter().unwrap().count();
            println!("   ├─ {} ({} entries)", table_name, count);
            let table = txn.open_table(def).unwrap();
            for entry in table.iter().unwrap() {
                let (k, v) = entry.unwrap();
                let key_hex = hex::encode(k.value());
                let val_len = v.value().len();
                println!(
                    "   │  {} ({} bytes)",
                    &key_hex[..key_hex.len().min(32)],
                    val_len
                );
            }
        }
        println!();
    }
}
