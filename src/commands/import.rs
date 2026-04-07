use jj_cli::{
    cli_util::{CommandHelper, print_snapshot_stats},
    command_error::{CommandError, cli_error},
    ui::Ui,
};
use jj_lib::repo_path::RepoPathUiConverter;

mod local;

pub fn import(ui: &mut Ui, ch: &CommandHelper, source: String) -> Result<(), CommandError> {
    let root = ch.cwd();
    let jj_dir = root.join(".jj");
    let repo_dir = jj_dir.join("repo");

    let op_heads = repo_dir.join("op_heads").join("db");
    let op_store = repo_dir.join("op_store").join("db");
    let store = repo_dir.join("store").join("db");

    if !op_heads.exists() || !op_store.exists() || !store.exists() {
        return Err(cli_error(format!(
            "'{}' is not a valid repository",
            repo_dir.display()
        )));
    }

    let is_ssh = source.starts_with("jj@");
    let is_local = !is_ssh;

    if is_local {
        local::import_local(op_heads, op_store, store, source.into())?;
    } else if is_ssh {
        return Err(cli_error("Import with ssh is not supported"));
    } else {
        return Err(cli_error("Import protocol not supported"));
    }

    let (workspace_command, stats) = ch.recover_stale_working_copy(ui)?;
    print_snapshot_stats(
        ui,
        &stats,
        &RepoPathUiConverter::Fs {
            cwd: ch.cwd().to_owned(),
            base: workspace_command.workspace_root().to_owned(),
        },
    )?;

    Ok(())
}
