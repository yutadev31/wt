pub mod herdr;

use std::path::Path;

use crate::config::{Backend, Integration};

pub fn open_worktree(path: &Path, integration: &Integration) -> anyhow::Result<()> {
    match integration.backend {
        Backend::Herdr => herdr::open_worktree_with_command(path, integration.command.as_deref()),
    }
}
