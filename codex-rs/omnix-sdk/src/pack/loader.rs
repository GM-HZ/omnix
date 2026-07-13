//! Instruction resolution for [`super::BusinessPack`].

use std::path::Path;

use super::InstructionSource;
use crate::instruction::MAX_MODEL_VISIBLE_INSTRUCTION_BYTES;

/// Rough character budget per instruction fragment (~1,000 tokens at
/// ~4 chars/token for English prose, rounded down with generous margin).
const MAX_FRAGMENT_CHARS: usize = 4_000;

/// Errors from loading pack assets.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PackError {
    /// A file-backed instruction fragment could not be read.
    #[error("failed to read instruction file `{path}`: {source}")]
    InstructionFile {
        path: String,
        source: std::io::Error,
    },
    /// File-backed fragments require an explicit pack root.
    #[error("file-backed instruction `{path}` requires pack_root")]
    MissingRoot { path: String },
    /// Pack assets must use relative paths.
    #[error("instruction path must be relative to pack_root: `{path}`")]
    AbsolutePath { path: String },
    /// A relative path or symlink escaped the pack root.
    #[error("instruction path escapes pack_root: `{path}`")]
    OutsideRoot { path: String },
    /// A single instruction fragment exceeds the per-fragment size limit.
    #[error("instruction fragment {index} exceeds {limit} characters ({approx_tokens} tokens)")]
    FragmentTooLarge {
        index: usize,
        limit: usize,
        approx_tokens: usize,
    },
    /// The total resolved instruction text exceeds the budget.
    #[error("resolved instructions exceed the {limit} byte model-visible item limit")]
    TooLarge { limit: usize },
}

/// Resolve instruction fragments into a single concatenated string with
/// per-fragment and aggregate caps to protect context-window stability.
///
/// Each individual fragment (whether inline or file-backed) must not exceed
/// `MAX_FRAGMENT_CHARS` characters. The concatenated result must not exceed
/// `MAX_MODEL_VISIBLE_INSTRUCTION_BYTES` bytes. Empty/blank fragments are trimmed and skipped before
/// the caps are checked.
pub(crate) fn resolve_instructions(
    sources: &[InstructionSource],
    root: Option<&Path>,
) -> Result<Option<String>, PackError> {
    if sources.is_empty() {
        return Ok(None);
    }

    let mut fragments: Vec<String> = Vec::with_capacity(sources.len());
    for (i, source) in sources.iter().enumerate() {
        let text = match source {
            InstructionSource::Inline(text) => text.clone(),
            InstructionSource::File(path) => {
                if path.is_absolute() {
                    return Err(PackError::AbsolutePath {
                        path: path.display().to_string(),
                    });
                }
                let root = root.ok_or_else(|| PackError::MissingRoot {
                    path: path.display().to_string(),
                })?;
                let canonical_root =
                    root.canonicalize()
                        .map_err(|source| PackError::InstructionFile {
                            path: root.display().to_string(),
                            source,
                        })?;
                let resolved = root.join(path);
                let canonical_resolved =
                    resolved
                        .canonicalize()
                        .map_err(|source| PackError::InstructionFile {
                            path: resolved.display().to_string(),
                            source,
                        })?;
                if !canonical_resolved.starts_with(&canonical_root) {
                    return Err(PackError::OutsideRoot {
                        path: path.display().to_string(),
                    });
                }
                std::fs::read_to_string(&canonical_resolved).map_err(|source| {
                    PackError::InstructionFile {
                        path: canonical_resolved.display().to_string(),
                        source,
                    }
                })?
            }
        };
        let trimmed = text.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.chars().count() > MAX_FRAGMENT_CHARS {
            return Err(PackError::FragmentTooLarge {
                index: i,
                limit: MAX_FRAGMENT_CHARS,
                approx_tokens: MAX_FRAGMENT_CHARS / 4,
            });
        }
        fragments.push(trimmed.to_string());
    }

    if fragments.is_empty() {
        return Ok(None);
    }

    let joined = fragments.join("\n\n");
    if joined.len() > MAX_MODEL_VISIBLE_INSTRUCTION_BYTES {
        return Err(PackError::TooLarge {
            limit: MAX_MODEL_VISIBLE_INSTRUCTION_BYTES,
        });
    }

    Ok(Some(joined))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_sources_resolve_to_none() {
        assert!(resolve_instructions(&[], None).unwrap().is_none());
    }

    #[test]
    fn inline_fragments_are_joined_in_order() {
        let sources = vec![
            InstructionSource::Inline("First rule.".to_string()),
            InstructionSource::Inline("Second rule.".to_string()),
        ];
        let out = resolve_instructions(&sources, None).unwrap().unwrap();
        assert_eq!(out, "First rule.\n\nSecond rule.");
    }

    #[test]
    fn blank_inline_fragments_are_dropped() {
        let sources = vec![
            InstructionSource::Inline("   ".to_string()),
            InstructionSource::Inline("Only rule.".to_string()),
        ];
        let out = resolve_instructions(&sources, None).unwrap().unwrap();
        assert_eq!(out, "Only rule.");
    }

    #[test]
    fn file_fragment_is_read_relative_to_root() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("sys.md"), "From file.").unwrap();
        let sources = vec![InstructionSource::File("sys.md".into())];
        let out = resolve_instructions(&sources, Some(dir.path()))
            .unwrap()
            .unwrap();
        assert_eq!(out, "From file.");
    }

    #[test]
    fn missing_file_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let sources = vec![InstructionSource::File("missing.md".into())];
        let err = resolve_instructions(&sources, Some(dir.path())).unwrap_err();
        assert!(matches!(err, PackError::InstructionFile { .. }));
    }

    #[test]
    fn file_without_pack_root_is_rejected() {
        let sources = vec![InstructionSource::File("sys.md".into())];
        let err = resolve_instructions(&sources, None).unwrap_err();
        assert!(matches!(err, PackError::MissingRoot { .. }));
    }

    #[test]
    fn absolute_file_path_is_rejected() {
        let sources = vec![InstructionSource::File("/tmp/sys.md".into())];
        let err = resolve_instructions(&sources, Some(Path::new("/tmp"))).unwrap_err();
        assert!(matches!(err, PackError::AbsolutePath { .. }));
    }

    #[test]
    fn parent_traversal_is_rejected() {
        let parent = tempfile::tempdir().unwrap();
        let root = parent.path().join("pack");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(parent.path().join("outside.md"), "outside").unwrap();
        let sources = vec![InstructionSource::File("../outside.md".into())];
        let err = resolve_instructions(&sources, Some(&root)).unwrap_err();
        assert!(matches!(err, PackError::OutsideRoot { .. }));
    }

    #[test]
    fn fragment_exceeding_limit_is_rejected() {
        let big = "x".repeat(MAX_FRAGMENT_CHARS + 1);
        let sources = vec![InstructionSource::Inline(big)];
        let err = resolve_instructions(&sources, None).unwrap_err();
        assert!(matches!(err, PackError::FragmentTooLarge { .. }));
    }

    #[test]
    fn total_exceeding_limit_is_rejected() {
        let sources = (0..10)
            .map(|_| InstructionSource::Inline("y".repeat(MAX_FRAGMENT_CHARS)))
            .collect::<Vec<_>>();
        let err = resolve_instructions(&sources, None).unwrap_err();
        assert!(matches!(err, PackError::TooLarge { .. }));
    }

    #[test]
    fn aggregate_byte_limit_catches_multibyte_input() {
        let over_byte_limit = "知".repeat(MAX_FRAGMENT_CHARS);
        let sources = vec![InstructionSource::Inline(over_byte_limit)];
        let err = resolve_instructions(&sources, None).unwrap_err();
        assert!(matches!(err, PackError::TooLarge { .. }));
    }

    #[test]
    fn fragment_at_limit_is_accepted() {
        let at_limit = "z".repeat(MAX_FRAGMENT_CHARS);
        let sources = vec![InstructionSource::Inline(at_limit)];
        let out = resolve_instructions(&sources, None).unwrap().unwrap();
        assert_eq!(out.chars().count(), MAX_FRAGMENT_CHARS);
    }

    #[test]
    fn aggregate_byte_boundary_is_exact() {
        let at_limit = vec![
            InstructionSource::Inline("a".repeat(MAX_FRAGMENT_CHARS)),
            InstructionSource::Inline("b".repeat(MAX_FRAGMENT_CHARS)),
            InstructionSource::Inline("c".repeat(188)),
        ];
        let out = resolve_instructions(&at_limit, None).unwrap().unwrap();
        assert_eq!(out.len(), MAX_MODEL_VISIBLE_INSTRUCTION_BYTES);

        let over_limit = vec![
            InstructionSource::Inline("a".repeat(MAX_FRAGMENT_CHARS)),
            InstructionSource::Inline("b".repeat(MAX_FRAGMENT_CHARS)),
            InstructionSource::Inline("c".repeat(189)),
        ];
        let err = resolve_instructions(&over_limit, None).unwrap_err();
        assert!(matches!(err, PackError::TooLarge { .. }));
    }
}
