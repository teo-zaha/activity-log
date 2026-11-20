use std::{
    fs, io,
    path::{Path, PathBuf},
};

use activity_log_core::ts::activity_typescript;
use anyhow::{Result, anyhow, bail};

/// Writes the contract to `out` and, beside it, an `index.ts` that re-exports it.
pub fn generate_ts(out: &Path) -> Result<()> {
    let files = generated_files(out)?;
    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent)?;
    }
    // Keep the frontend contract generation explicit so backend builds do not mutate repo files.
    for (path, content) in &files {
        fs::write(path, content)?;
    }
    println!("Generated TypeScript DTOs at {}", out.display());
    Ok(())
}

/// The files `generate_ts` would write that are missing or differ from what is on disk. Writes
/// nothing, so `verify` can tell a contract that drifted from its Rust source.
pub fn stale_files(out: &Path) -> Result<Vec<PathBuf>> {
    let mut stale = Vec::new();
    for (path, content) in generated_files(out)? {
        let current = match fs::read_to_string(&path) {
            Ok(current) => Some(current),
            Err(err) if err.kind() == io::ErrorKind::NotFound => None,
            Err(err) => return Err(anyhow!("Could not read {}: {err}", path.display())),
        };
        if current.as_deref() != Some(content.as_str()) {
            stale.push(path);
        }
    }
    Ok(stale)
}

/// The contract at `out` and the barrel that re-exports it by `out`'s file name. The name must
/// be a plain module name, and the contract must not be the barrel on a file system that
/// ignores case.
fn generated_files(out: &Path) -> Result<[(PathBuf, String); 2]> {
    let Some(module) = out
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|_| out.extension().is_some_and(|ext| ext == "ts"))
    else {
        bail!("--out must name a .ts file, such as activity.ts");
    };
    let plain = module.starts_with(|c: char| c.is_ascii_alphabetic())
        && module
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'));
    if !plain {
        bail!("--out must be named with letters, digits, `_`, `-` and `.`, starting with a letter");
    }
    if module.eq_ignore_ascii_case("index") {
        bail!("--out cannot be index.ts: that is the barrel written beside it");
    }
    let index = out
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("index.ts");
    Ok([
        (out.to_path_buf(), activity_typescript()),
        (index, format!("export * from './{module}'\n")),
    ])
}
