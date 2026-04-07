use jj::smart::{Heads, Pack};
use jj_cli::{
    cli_util::CommandHelper,
    command_error::{CommandError, cli_error},
    ui::Ui,
};
use pollster::FutureExt;

mod local;
mod ssh;

pub async fn import(
    ui: &mut Ui,
    ch: &CommandHelper,
    source: Option<&str>,
    client: bool,
    server: bool,
) -> Result<(), CommandError> {
    match source {
        Some(src) => match super::ssh_addr(src) {
            Some(src) => ssh::import(src),
            None => local::import(src.into()),
        },
        None => match (client, server) {
            (true, false) => import_client(ui, ch).await,
            (false, true) => import_server(ui, ch).await,
            _ => Ok(()),
        },
    }
}

pub async fn import_client(ui: &mut Ui, ch: &CommandHelper) -> Result<(), CommandError> {
    let repo_dir = ch.cwd().join(".jj").join("repo");

    let heads = Heads::new(&repo_dir)?;
    jj::smart::send_message(&heads)?;

    let up_to_date: bool = jj::smart::recv_message()?;

    if up_to_date {
        eprintln!("Up to date.");
        return Ok(());
    }

    let pack: Pack = jj::smart::recv_message()?;
    pack.write(&repo_dir)?;

    if ch.cwd().join(".jj").join("repo").exists() {
        ch.recover_stale_working_copy(ui).block_on()?;
    }

    Ok(())
}

pub async fn import_server(ui: &mut Ui, ch: &CommandHelper) -> Result<(), CommandError> {
    let repo_dir = super::find_repo_path(ch.cwd()).ok_or_else(|| {
        cli_error(format!(
            "'{}' is not a valid repository",
            ch.cwd().display()
        ))
    })?;

    let heads: Heads = jj::smart::recv_message()?;
    if !heads.are_missing(&repo_dir)? {
        jj::smart::send_message(&Heads::default())?;
        return Ok(());
    }

    let roots: Heads = Heads::new(&repo_dir)?;
    jj::smart::send_message(&roots)?;
    let pack: Pack = jj::smart::recv_message()?;
    pack.write(&repo_dir)?;

    if ch.cwd().join(".jj").join("repo").exists() {
        ch.recover_stale_working_copy(ui).block_on()?;
    }

    Ok(())
}
