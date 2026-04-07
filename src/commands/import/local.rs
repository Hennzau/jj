use jj_cli::command_error::{CommandError, cli_error_with_message};

use std::path::PathBuf;

pub fn import(src: PathBuf) -> Result<(), CommandError> {
    let bin = std::env::current_exe()?;

    let mut server = std::process::Command::new(&bin)
        .current_dir(src)
        .args(["re", "export"])
        .arg("--server")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| cli_error_with_message("Couldn't spawn the export server on source", e))?;

    let mut client = std::process::Command::new(&bin)
        .args(["re", "import"])
        .arg("--client")
        .stdin(std::process::Stdio::from(server.stdout.take().unwrap()))
        .stdout(std::process::Stdio::from(server.stdin.take().unwrap()))
        .spawn()
        .map_err(|e| {
            cli_error_with_message("Couldn't spawn the import client on destination", e)
        })?;

    client.wait()?;
    server.wait()?;

    Ok(())
}
