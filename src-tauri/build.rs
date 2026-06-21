use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    ensure_engine_resource();
    tauri_build::build()
}

fn ensure_engine_resource() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".into()));
    let repo_root = manifest_dir.parent().unwrap_or(manifest_dir.as_path());
    let release_resource = repo_root.join("engine/target/release/hologram-engine.exe");

    if release_resource.exists() {
        return;
    }

    if let Some(source) = find_engine_binary(repo_root.join("engine/target/debug")) {
        if let Some(parent) = release_resource.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::copy(source, &release_resource);
    }
}

fn find_engine_binary(debug_dir: PathBuf) -> Option<PathBuf> {
    let direct = debug_dir.join("hologram-engine.exe");
    if direct.exists() {
        return Some(direct);
    }

    let deps_dir = debug_dir.join("deps");
    if !deps_dir.exists() {
        return None;
    }

    let entries = fs::read_dir(deps_dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        let file_name = path.file_name()?.to_string_lossy();
        if !file_name.starts_with("hologram_engine-") {
          continue;
        }
        if path.extension().is_some() {
          continue;
        }
        if is_executable(&path) {
          return Some(path);
        }
    }
    None
}

fn is_executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::metadata(path)
            .map(|meta| meta.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }

    #[cfg(not(unix))]
    {
        path.is_file()
    }
}
