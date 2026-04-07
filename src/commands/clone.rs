use jj::stores::{RedbBackend, RedbOpHeadsStore, RedbOpStore};
use jj_cli::{
    cli_util::CommandHelper,
    command_error::{CommandError, cli_error, user_error_with_message},
    ui::Ui,
};

use jj_lib::{
    ref_name::WorkspaceName,
    repo::ReadonlyRepo,
    signing::Signer,
    workspace::{Workspace, WorkspaceInitError, default_working_copy_factory},
};

pub async fn clone(
    ui: &mut Ui,
    ch: &CommandHelper,
    source: &str,
    destination: &str,
) -> Result<(), CommandError> {
    let root = ch.cwd().join(destination);
    let root = jj_lib::file_util::create_or_reuse_dir(&root)
        .and_then(|_| dunce::canonicalize(root))
        .map_err(|e| user_error_with_message("Failed to create directory", e))?;

    let settings = ch.settings_for_new_workspace(ui, &root)?.0;

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
    .await?;

    let jj = std::env::current_exe()?;

    let status = std::process::Command::new(jj)
        .current_dir(destination)
        .arg("re")
        .arg("import")
        .arg(ch.cwd().join(source))
        .status()?;

    if !status.success() {
        return Err(cli_error("Couldn't import repository"));
    }

    Ok(())
}
