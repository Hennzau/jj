use std::path::{Path, PathBuf};

mod clone;
mod export;
mod import;
mod init;

pub use clone::*;
pub use export::*;
pub use import::*;
pub use init::*;

fn find_repo_path(dst: &Path) -> Option<PathBuf> {
    [dst.to_path_buf(), dst.join(".jj").join("repo")]
        .into_iter()
        .find(|base| {
            base.join("op_heads").join("db").exists()
                && base.join("op_store").join("db").exists()
                && base.join("store").join("db").exists()
        })
}

struct SshAddr {
    pub account: String,
    pub addr: String,
    pub path: String,
}

fn ssh_addr(s: &str) -> Option<SshAddr> {
    if let Some((account, rest)) = s.split_once('@')
        && let Some((addr, path)) = rest.split_once(':')
        && !account.is_empty()
        && !addr.is_empty()
        && !path.is_empty()
    {
        return Some(SshAddr {
            account: account.into(),
            addr: addr.into(),
            path: path.into(),
        });
    }
    None
}
