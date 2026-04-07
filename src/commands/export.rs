use jj::smart::{Heads, PackIds};
use jj_cli::{
    cli_util::CommandHelper,
    command_error::{CommandError, cli_error},
    ui::Ui,
};

mod local;
mod ssh;

pub async fn export(
    ui: &mut Ui,
    ch: &CommandHelper,
    destination: Option<&str>,
    client: bool,
    server: bool,
) -> Result<(), CommandError> {
    match destination {
        Some(dst) => match super::ssh_addr(dst) {
            Some(dst) => ssh::export(dst),
            None => local::export(dst.into()),
        },
        None => match (client, server) {
            (true, false) => export_client(ui, ch).await,
            (false, true) => export_server(ui, ch).await,
            _ => Ok(()),
        },
    }
}

async fn export_client(_ui: &mut Ui, ch: &CommandHelper) -> Result<(), CommandError> {
    let repo_dir = ch.cwd().join(".jj").join("repo");

    let workspace = ch.load_workspace()?;
    let repo = workspace.repo_loader();

    let heads = Heads::new(&repo_dir)?;
    jj::smart::send_message(&heads)?;
    let roots: Heads = jj::smart::recv_message()?;

    if roots.heads.is_empty() {
        eprintln!("Nothing to export");
        return Ok(());
    }

    let heads: Vec<_> = heads.as_ops().collect();
    let roots: Vec<_> = roots.as_ops().collect();

    let ids = PackIds::new(repo.store(), repo.op_store(), &heads, &roots).await;
    let pack = ids.into_pack(&repo_dir)?;

    jj::smart::send_message(&pack)?;

    Ok(())
}

async fn export_server(_ui: &mut Ui, ch: &CommandHelper) -> Result<(), CommandError> {
    let repo_dir = super::find_repo_path(ch.cwd()).ok_or_else(|| {
        cli_error(format!(
            "'{}' is not a valid repository",
            ch.cwd().display()
        ))
    })?;

    let repo = jj::repo_loader(&repo_dir)?;

    let roots: Heads = jj::smart::recv_message()?;
    if !roots.are_missing(&repo_dir)? {
        jj::smart::send_message(&true)?;
        return Ok(());
    }

    jj::smart::send_message(&false)?;

    let heads = Heads::new(&repo_dir)?;

    let heads: Vec<_> = heads.as_ops().collect();
    let roots: Vec<_> = roots.as_ops().collect();

    let ids = PackIds::new(repo.store(), repo.op_store(), &heads, &roots).await;
    let pack = ids.into_pack(&repo_dir)?;

    jj::smart::send_message(&pack)?;

    Ok(())
}
