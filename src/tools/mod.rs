pub mod fs_read;
pub mod fs_write;
pub mod exec;

use std::path::{Path, PathBuf};

pub fn ensure_within_root(root: &Path, input: &Path) -> anyhow::Result<PathBuf> {
    // allow relative or absolute inputs; join then canonicalize
    let joined = if input.is_absolute() { input.to_path_buf() } else { root.join(input) };
    let canon_root = dunce::canonicalize(root)?;
    let canon_path = dunce::canonicalize(&joined)?;
    if canon_path.starts_with(&canon_root) { Ok(canon_path) } else { anyhow::bail!("path escapes root") }
}
