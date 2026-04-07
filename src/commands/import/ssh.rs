use jj_cli::command_error::CommandError;

use crate::commands::SshAddr;

pub fn import(src: SshAddr) -> Result<(), CommandError> {
    let SshAddr {
        account: _account,
        addr: _addr,
        path: _path,
    } = src;

    Ok(())
}
