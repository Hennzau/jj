use jj_cli::{
    cli_util::CommandHelper,
    command_error::{CommandError, cli_error},
    ui::Ui,
};

mod local;

pub fn export(_ui: &mut Ui, ch: &CommandHelper, destination: String) -> Result<(), CommandError> {
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

    let is_ssh = destination.starts_with("jj@");
    let is_local = !is_ssh;

    if is_local {
        local::export_local(op_heads, op_store, store, destination.into())?;
    } else if is_ssh {
        return Err(cli_error("Export with ssh is not supported"));
    } else {
        return Err(cli_error("Export protocol not supported"));
    }

    Ok(())
}
