use jj_cli::command_error::{CommandError, cli_error_with_message};

use crate::commands::SshAddr;

pub fn import(dst: SshAddr) -> Result<(), CommandError> {
    let SshAddr {
        account,
        addr,
        path,
    } = dst;

    let bin = std::env::current_exe()?;

    let mut server = std::process::Command::new("ssh")
        .args([format!("{account}@{addr}").as_str()])
        .arg(format!("bash -c 'cd {path}; jj re export --server'"))
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| {
            cli_error_with_message("Couldn't spawn the import server on destination", e)
        })?;

    let mut client = std::process::Command::new(&bin)
        .args(["re", "import"])
        .arg("--client")
        .stdin(std::process::Stdio::from(server.stdout.take().unwrap()))
        .stdout(std::process::Stdio::from(server.stdin.take().unwrap()))
        .spawn()
        .map_err(|e| cli_error_with_message("Couldn't spawn the export client on source", e))?;

    client.wait()?;
    server.wait()?;

    Ok(())
}
