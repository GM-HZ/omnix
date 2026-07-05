use codex_utils_absolute_path::AbsolutePathBuf;
use dirs::home_dir;
use std::path::PathBuf;

/// Primary app config directory name (relative to `$HOME`).
const APP_DIR: &str = ".omnix";

/// Legacy Codex config directory — used as fallback during migration.
const LEGACY_APP_DIR: &str = ".codex";

/// Returns the path to the app configuration directory.
///
/// Priority:
/// 1. `OMNIX_HOME` env var (must exist and be a directory)
/// 2. `CODEX_HOME` env var (must exist and be a directory)
/// 3. `~/.omnix` if it already exists on disk
/// 4. `~/.codex` if it already exists on disk
/// 5. `~/.omnix` (default, not required to exist)
pub fn find_codex_home() -> std::io::Result<AbsolutePathBuf> {
    // 1. Env var override — `OMNIX_HOME` then `CODEX_HOME`
    if let Some(val) = std::env::var("OMNIX_HOME")
        .ok()
        .filter(|v| !v.is_empty())
        .or_else(|| std::env::var("CODEX_HOME").ok().filter(|v| !v.is_empty()))
    {
        return resolve_env_home(&val);
    }

    // 2. No env var — probe `.omnix` then `.codex`, default to `.omnix`
    let home = home_dir().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotFound, "Could not find home directory")
    })?;

    for dir in [APP_DIR, LEGACY_APP_DIR] {
        let p = home.join(dir);
        if p.is_dir() {
            return AbsolutePathBuf::from_absolute_path(p);
        }
    }

    AbsolutePathBuf::from_absolute_path(home.join(APP_DIR))
}

fn resolve_env_home(val: &str) -> std::io::Result<AbsolutePathBuf> {
    let path = PathBuf::from(val);
    let metadata = std::fs::metadata(&path).map_err(|err| match err.kind() {
        std::io::ErrorKind::NotFound => std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("OMNIX_HOME points to {val:?}, but that path does not exist"),
        ),
        _ => std::io::Error::new(
            err.kind(),
            format!("failed to read OMNIX_HOME {val:?}: {err}"),
        ),
    })?;

    if !metadata.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("OMNIX_HOME points to {val:?}, but that path is not a directory"),
        ));
    }

    let canonical = path.canonicalize().map_err(|err| {
        std::io::Error::new(
            err.kind(),
            format!("failed to canonicalize OMNIX_HOME {val:?}: {err}"),
        )
    })?;
    AbsolutePathBuf::from_absolute_path(canonical)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use std::fs;
    use std::io::ErrorKind;
    use tempfile::TempDir;

    #[test]
    fn env_var_missing_path_is_fatal() {
        let temp_home = TempDir::new().expect("temp home");
        let missing = temp_home.path().join("missing-omnix-home");
        let missing_str = missing.to_str().expect("valid utf-8");

        let err = resolve_env_home(missing_str).expect_err("missing dir");
        assert_eq!(err.kind(), ErrorKind::NotFound);
        assert!(err.to_string().contains("OMNIX_HOME"), "{err}");
    }

    #[test]
    fn env_var_file_path_is_fatal() {
        let temp_home = TempDir::new().expect("temp home");
        let file_path = temp_home.path().join("omnix-home.txt");
        fs::write(&file_path, "not a directory").expect("write temp file");
        let file_str = file_path.to_str().expect("valid utf-8");

        let err = resolve_env_home(file_str).expect_err("file path");
        assert_eq!(err.kind(), ErrorKind::InvalidInput);
        assert!(err.to_string().contains("not a directory"), "{err}");
    }

    #[test]
    fn env_var_valid_directory_canonicalizes() {
        let temp_home = TempDir::new().expect("temp home");
        let temp_str = temp_home.path().to_str().expect("valid utf-8");

        let resolved = resolve_env_home(temp_str).expect("valid dir");
        let expected = temp_home.path().canonicalize().expect("canonicalize");
        assert_eq!(resolved, AbsolutePathBuf::from_absolute_path(expected).expect("absolute"));
    }

    #[test]
    fn defaults_to_dot_omnix_or_falls_back_to_dot_codex() {
        let resolved = find_codex_home().expect("default home");
        let path = resolved.as_path();
        // On a system where ~/.omnix exists, it should be .omnix.
        // If ~/.codex exists but ~/.omnix doesn't, .codex is the fallback.
        // If neither exists, default is .omnix.
        let file_name = path.file_name().and_then(|n| n.to_str()).expect("file name");
        assert!(
            file_name == APP_DIR || file_name == LEGACY_APP_DIR,
            "expected .omnix or .codex, got {file_name:?}"
        );
    }
}
