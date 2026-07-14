//! Runtime directory derivation for the embedded Omnix SDK.
//!
//! The SDK owns a hidden `.omnix` directory under the host-provided root. The
//! host owns everything else in that root; Omnix owns the internal layout of
//! `.omnix` and callers must not depend on its file structure (they query state
//! through the SDK instead). See the design doc §6.

use std::fs;
use std::io;
use std::path::Path;
use std::path::PathBuf;

/// How the host wants its data root interpreted.
///
/// Both variants derive an independent `.omnix` runtime root; the only
/// difference is the semantics of the root the host passes in.
#[derive(Debug, Clone)]
pub enum RuntimeScope {
    /// A host application data root (e.g. Staffroom). The root itself is the
    /// agent workspace and `.omnix` lives directly beneath it.
    Application { root: PathBuf },
    /// A conventional project directory. Mirrors the standalone Omnix layout
    /// where `.omnix` sits at the project root.
    Project { root: PathBuf },
}

impl RuntimeScope {
    fn root(&self) -> &Path {
        match self {
            RuntimeScope::Application { root } | RuntimeScope::Project { root } => root,
        }
    }

    /// Non-sensitive scope label recorded in `runtime.json`.
    pub fn label(&self) -> &'static str {
        match self {
            RuntimeScope::Application { .. } => "application",
            RuntimeScope::Project { .. } => "project",
        }
    }
}

/// Resolved on-disk locations the runtime needs.
///
/// `codex_home` is the `.omnix` directory handed to the in-process app-server
/// as its home; it must be writable because startup creates and locks an
/// `installation_id` file there.
#[derive(Debug, Clone)]
pub struct RuntimePaths {
    /// The `.omnix` directory; used as the app-server `codex_home`.
    pub codex_home: PathBuf,
    /// The agent workspace (cwd), i.e. the host-provided root.
    pub workspace: PathBuf,
}

/// Sub-directories created under `.omnix` on first use.
const RUNTIME_SUBDIRS: &[&str] = &[
    "state",
    "sessions",
    "cache",
    "logs",
    "skills",
    "plugins",
    "artifacts",
];

/// Derive and create the `.omnix` runtime layout for `scope`.
///
/// Idempotent: existing directories are left untouched. Returns the resolved
/// paths once the directory tree is in place.
pub fn prepare_runtime_dirs(scope: &RuntimeScope) -> io::Result<RuntimePaths> {
    let workspace = scope.root().to_path_buf();
    let codex_home = workspace.join(".omnix");

    fs::create_dir_all(&codex_home)?;
    for subdir in RUNTIME_SUBDIRS {
        fs::create_dir_all(codex_home.join(subdir))?;
    }

    Ok(RuntimePaths {
        codex_home,
        workspace,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn application_scope_derives_dot_omnix_under_root() {
        let tmp = std::env::temp_dir().join(format!("omnix-app-{}", std::process::id()));
        let scope = RuntimeScope::Application { root: tmp.clone() };
        let paths = prepare_runtime_dirs(&scope).expect("prepare dirs");

        assert_eq!(paths.workspace, tmp);
        assert_eq!(paths.codex_home, tmp.join(".omnix"));
        assert!(paths.codex_home.is_dir());
        for subdir in RUNTIME_SUBDIRS {
            assert!(paths.codex_home.join(subdir).is_dir(), "missing {subdir}");
        }
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn project_scope_also_derives_dot_omnix_at_root() {
        let tmp = std::env::temp_dir().join(format!("omnix-proj-{}", std::process::id()));
        let scope = RuntimeScope::Project { root: tmp.clone() };
        let paths = prepare_runtime_dirs(&scope).expect("prepare dirs");
        assert_eq!(paths.codex_home, tmp.join(".omnix"));
        assert_eq!(scope.label(), "project");
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn prepare_is_idempotent() {
        let tmp = std::env::temp_dir().join(format!("omnix-idem-{}", std::process::id()));
        let scope = RuntimeScope::Application { root: tmp.clone() };
        prepare_runtime_dirs(&scope).expect("first");
        prepare_runtime_dirs(&scope).expect("second run must not fail");
        let _ = fs::remove_dir_all(&tmp);
    }
}
