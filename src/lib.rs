use std::path::Path;

use jj_cli::command_error::{CommandError, cli_error_with_message};
use jj_lib::{
    config::StackedConfig,
    repo::{RepoLoader, StoreFactories},
    settings::UserSettings,
};

use crate::stores::{RedbBackend, RedbOpHeadsStore, RedbOpStore};

pub mod smart;
pub mod stores;

pub fn store_factories() -> StoreFactories {
    let mut store_factories = StoreFactories::empty();
    store_factories.add_op_store(
        RedbOpStore::name(),
        Box::new(|_, path, root_data| Ok(Box::new(RedbOpStore::open(path, root_data)))),
    );

    store_factories.add_op_heads_store(
        RedbOpHeadsStore::name(),
        Box::new(|_, path| Ok(Box::new(RedbOpHeadsStore::open(path)))),
    );

    store_factories.add_backend(
        RedbBackend::name(),
        Box::new(|_, path| Ok(Box::new(RedbBackend::open(path)))),
    );

    store_factories
}

pub fn repo_loader(repo_dir: &Path) -> Result<RepoLoader, CommandError> {
    let mut store_factories = store_factories();
    store_factories.merge(StoreFactories::default());

    let config = StackedConfig::with_defaults();
    let settings = UserSettings::from_config(config)?;

    Ok(
        RepoLoader::init_from_file_system(&settings, repo_dir, &store_factories)
            .map_err(|e| cli_error_with_message("Couldn't create Repository Loader", e))?,
    )
}
