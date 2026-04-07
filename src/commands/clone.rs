use jj::stores::{RedbBackend, RedbOpHeadsStore, RedbOpStore};

use jj_cli::{
    cli_util::CommandHelper,
    command_error::{CommandError, cli_error_with_message, user_error_with_message},
    ui::Ui,
};

use jj_lib::{repo::ReadonlyRepo, signing::Signer, workspace::WorkspaceInitError};

use pollster::FutureExt;

pub fn clone(
    ui: &mut Ui,
    ch: &CommandHelper,
    _source: String,
    destination: String,
) -> Result<(), CommandError> {
    let root = ch.cwd().join(&destination);
    let root = jj_lib::file_util::create_or_reuse_dir(&root)
        .and_then(|_| dunce::canonicalize(root))
        .map_err(|e| user_error_with_message("Failed to create directory", e))?;

    let jj_dir = root.join(".jj");
    let jj_dir = jj_lib::file_util::create_or_reuse_dir(&jj_dir)
        .and_then(|_| dunce::canonicalize(jj_dir))
        .map_err(|e| user_error_with_message("Failed to create directory", e))?;

    let repo_dir = jj_dir.join("repo");
    let repo_dir = jj_lib::file_util::create_or_reuse_dir(&repo_dir)
        .and_then(|_| dunce::canonicalize(repo_dir))
        .map_err(|e| user_error_with_message("Failed to create directory", e))?;

    let settings = ch.settings_for_new_workspace(ui, &root)?.0;

    let repo = ReadonlyRepo::init(
        &settings,
        &repo_dir,
        &|_, store_path| Ok(Box::new(RedbBackend::init(store_path)?)),
        Signer::from_settings(&settings).map_err(WorkspaceInitError::SignInit)?,
        &|_, store_path, root_data| Ok(Box::new(RedbOpStore::init(store_path, root_data)?)),
        &|_, store_path| Ok(Box::new(RedbOpHeadsStore::init(store_path)?)),
        ReadonlyRepo::default_index_store_initializer(),
        ReadonlyRepo::default_submodule_store_initializer(),
    )
    .block_on()
    .map_err(|e| cli_error_with_message("Failed to initialize repo", e))?;

    // operate the clone of the dbs

    let _repo = repo
        .reload_at_head()
        .block_on()
        .map_err(|e| cli_error_with_message("Failed to reload repo", e))?;

    // let workspace_store = SimpleWorkspaceStore::load(&repo_dir)?;
    // let (working_copy, repo) = init_working_copy(
    //     &repo,
    //     workspace_root,
    //     &jj_dir,
    //     working_copy_factory,
    //     workspace_name,
    // )
    // .await?;
    // let repo_loader = repo.loader().clone();
    // let repo_dir = dunce::canonicalize(&repo_dir).context(&repo_dir)?;
    // let workspace = Self::new(workspace_root, repo_dir, working_copy, repo_loader)?;
    // workspace_store.add(workspace.workspace_name(), workspace.workspace_root())?;

    Ok(())
}
