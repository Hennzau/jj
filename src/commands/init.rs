use jj::stores::{RedbBackend, RedbOpHeadsStore, RedbOpStore};

use jj_cli::{
    cli_util::CommandHelper,
    command_error::{CommandError, cli_error_with_message, user_error_with_message},
    ui::Ui,
};

use jj_lib::{
    ref_name::WorkspaceName,
    repo::ReadonlyRepo,
    signing::Signer,
    workspace::{Workspace, WorkspaceInitError, default_working_copy_factory},
};

use pollster::FutureExt;

pub fn init(
    ui: &mut Ui,
    ch: &CommandHelper,
    destination: String,
    bare: bool,
) -> Result<(), CommandError> {
    let root = ch.cwd().join(&destination);
    let root = jj_lib::file_util::create_or_reuse_dir(&root)
        .and_then(|_| dunce::canonicalize(root))
        .map_err(|e| user_error_with_message("Failed to create directory", e))?;

    let settings = ch.settings_for_new_workspace(ui, &root)?.0;

    if bare {
        ReadonlyRepo::init(
            &settings,
            &root,
            &|_, store_path| Ok(Box::new(RedbBackend::init(store_path)?)),
            Signer::from_settings(&settings).map_err(WorkspaceInitError::SignInit)?,
            &|_, store_path, root_data| Ok(Box::new(RedbOpStore::init(store_path, root_data)?)),
            &|_, store_path| Ok(Box::new(RedbOpHeadsStore::init(store_path)?)),
            ReadonlyRepo::default_index_store_initializer(),
            ReadonlyRepo::default_submodule_store_initializer(),
        )
        .block_on()
        .map_err(|e| cli_error_with_message("Failed to initialize bare repo", e))?;
    } else {
        Workspace::init_with_factories(
            &settings,
            &root,
            &|_, store_path| Ok(Box::new(RedbBackend::init(store_path)?)),
            Signer::from_settings(&settings).map_err(WorkspaceInitError::SignInit)?,
            &|_, store_path, root_data| Ok(Box::new(RedbOpStore::init(store_path, root_data)?)),
            &|_, store_path| Ok(Box::new(RedbOpHeadsStore::init(store_path)?)),
            ReadonlyRepo::default_index_store_initializer(),
            ReadonlyRepo::default_submodule_store_initializer(),
            &*default_working_copy_factory(),
            WorkspaceName::DEFAULT.to_owned(),
        )
        .block_on()?;
    }

    Ok(())
}
