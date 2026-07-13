//! `.omnix/runtime.json` — the non-sensitive runtime manifest (design §14).
//!
//! Records the runtime/SDK version, state-schema version, active model, and
//! scope so a later launch can detect an incompatible on-disk state and either
//! migrate or refuse to start. The API key and any prompt/tool content are
//! never written here.

use std::path::Path;

use serde::Deserialize;
use serde::Serialize;

use crate::error::RuntimeError;

/// Current runtime version (Runtime 0.0).
pub const RUNTIME_VERSION: &str = "0.0";
/// Current SDK version.
pub const SDK_VERSION: &str = "0.0";
/// Current on-disk state schema version. Bump when `.omnix` layout changes in a
/// way that requires migration.
pub const STATE_SCHEMA_VERSION: u32 = 1;

const MANIFEST_FILE: &str = "runtime.json";

/// The persisted manifest shape.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeManifest {
    pub runtime_version: String,
    pub sdk_version: String,
    pub state_schema_version: u32,
    pub model: String,
    pub scope: String,
}

impl RuntimeManifest {
    /// Build the current-version manifest for `model` and `scope_label`.
    pub fn current(model: &str, scope_label: &str) -> Self {
        Self {
            runtime_version: RUNTIME_VERSION.to_string(),
            sdk_version: SDK_VERSION.to_string(),
            state_schema_version: STATE_SCHEMA_VERSION,
            model: model.to_string(),
            scope: scope_label.to_string(),
        }
    }
}

/// Load, compatibility-check, and (re)write the manifest under `codex_home`.
///
/// - If no manifest exists, writes the current one.
/// - If one exists with a newer `state_schema_version`, refuses to start
///   (the on-disk state was written by a newer runtime we cannot migrate).
/// - Otherwise rewrites the manifest to the current values (model/scope may
///   legitimately change between launches).
pub fn reconcile_manifest(
    codex_home: &Path,
    model: &str,
    scope_label: &str,
) -> Result<RuntimeManifest, RuntimeError> {
    let path = codex_home.join(MANIFEST_FILE);
    let current = RuntimeManifest::current(model, scope_label);

    if let Ok(bytes) = std::fs::read(&path) {
        // Tolerate an unreadable/corrupt manifest by overwriting it, but reject a
        // strictly newer state schema we do not understand.
        if let Ok(existing) = serde_json::from_slice::<RuntimeManifest>(&bytes)
            && existing.state_schema_version > STATE_SCHEMA_VERSION
        {
            return Err(RuntimeError::IncompatibleState {
                found: existing.state_schema_version,
                supported: STATE_SCHEMA_VERSION,
            });
        }
    }

    let serialized = serde_json::to_vec_pretty(&current).map_err(|e| RuntimeError::Manifest {
        message: e.to_string(),
    })?;
    std::fs::write(&path, serialized).map_err(|source| RuntimeError::Dirs { source })?;

    Ok(current)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_manifest_when_absent() {
        let dir = tempfile::tempdir().unwrap();
        let manifest = reconcile_manifest(dir.path(), "deepseek-v4-flash", "application").unwrap();
        assert_eq!(manifest.state_schema_version, STATE_SCHEMA_VERSION);
        assert_eq!(manifest.model, "deepseek-v4-flash");
        assert_eq!(manifest.scope, "application");

        // The file exists and round-trips.
        let bytes = std::fs::read(dir.path().join(MANIFEST_FILE)).unwrap();
        let parsed: RuntimeManifest = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(parsed, manifest);
    }

    #[test]
    fn rejects_newer_state_schema() {
        let dir = tempfile::tempdir().unwrap();
        let future = RuntimeManifest {
            runtime_version: "9.9".to_string(),
            sdk_version: "9.9".to_string(),
            state_schema_version: STATE_SCHEMA_VERSION + 1,
            model: "future-model".to_string(),
            scope: "application".to_string(),
        };
        std::fs::write(
            dir.path().join(MANIFEST_FILE),
            serde_json::to_vec(&future).unwrap(),
        )
        .unwrap();

        let err = reconcile_manifest(dir.path(), "deepseek-v4-flash", "application").unwrap_err();
        assert!(matches!(err, RuntimeError::IncompatibleState { .. }));
    }

    #[test]
    fn rewrites_on_compatible_existing() {
        let dir = tempfile::tempdir().unwrap();
        reconcile_manifest(dir.path(), "model-a", "application").unwrap();
        // A later launch with a different model rewrites cleanly.
        let m = reconcile_manifest(dir.path(), "model-b", "project").unwrap();
        assert_eq!(m.model, "model-b");
        assert_eq!(m.scope, "project");
    }
}
