use std::{path::PathBuf, str::FromStr, sync::Arc};

use jj_lib::{
    config::StackedConfig, default_index::DefaultIndexStore,
    default_submodule_store::DefaultSubmoduleStore, repo::RepoLoader, revset::RevsetExpression,
    settings::UserSettings,
};
use pollster::FutureExt;

fn main() {
    let repo = std::env::args().nth(1).unwrap();
    let bare = std::env::args()
        .nth(2)
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
    let repo = repo.load_at_head().block_on().unwrap();
    let expression = Arc::new(RevsetExpression::All);
    let revset = expression.evaluate(repo.as_ref()).unwrap();

    let commit_count = revset.count_estimate().unwrap().0;

    println!("Successfully loaded the repo, commits: {commit_count}")
}
