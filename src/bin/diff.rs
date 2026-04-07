use std::{path::PathBuf, str::FromStr};

use jj::smart::PackIds;
use jj_lib::{
    config::StackedConfig, default_index::DefaultIndexStore,
    default_submodule_store::DefaultSubmoduleStore, op_store::OperationId, repo::RepoLoader,
    settings::UserSettings,
};
use pollster::FutureExt;

fn main() {
    let repo = std::env::args().nth(1).unwrap();
    let head = OperationId::from_bytes(&hex::decode(std::env::args().nth(2).unwrap()).unwrap());
    let root = OperationId::from_bytes(&hex::decode(std::env::args().nth(3).unwrap()).unwrap());

    let bare = std::env::args()
        .nth(4)
        .map(|s| s == "--bare")
        .unwrap_or(false);

    let path = if !bare {
        PathBuf::from_str(&format!("/home/jj/jj/{}/.jj/repo", repo)).unwrap()
    } else {
        PathBuf::from_str(&format!("/home/jj/jj/{}", repo)).unwrap()
    };

    let mut store_factories = jj::store_factories();

    store_factories.add_index_store(
        "default",
        Box::new(|_, b| Ok(Box::new(DefaultIndexStore::load(b)))),
    );

    store_factories.add_submodule_store(
        "default",
        Box::new(|_, b| Ok(Box::new(DefaultSubmoduleStore::load(b)))),
    );

    let config = StackedConfig::with_defaults();
    let settings = UserSettings::from_config(config)
        .expect("UserSettings from default config should never fail");

    let repo = RepoLoader::init_from_file_system(&settings, &path, &store_factories).unwrap();
    let diff = PackIds::new(repo.store(), repo.op_store(), &[head], &[root]).block_on();

    macro_rules! prettyprint {
        ($set:expr) => {
            println!(concat!(stringify!($set), ": {}"), $set.len());
            for missing in $set {
                println!("\t{missing:.32}");
            }
            println!();
        };
    }

    prettyprint!(diff.op_ids);
    prettyprint!(diff.view_ids);
    prettyprint!(diff.commit_ids);
    prettyprint!(diff.tree_ids);
    prettyprint!(diff.file_ids);
}
