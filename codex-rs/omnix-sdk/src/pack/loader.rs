//! Instruction resolution for [`super::BusinessPack`].

use std::path::Path;

use super::InstructionSource;

/// Errors from loading pack assets.
#[derive(Debug, thiserror::Error)]
pub enum PackError {
    /// A file-backed instruction fragment could not be read.
    #[error("failed to read instruction file `{path}`: {source}")]
    InstructionFile {
        path: String,
        source: std::io::Error,
    },
}

/// Resolve instruction fragments into a single concatenated string.
pub(crate) fn resolve_instructions(
    sources: &[InstructionSource],
    root: Option<&Path>,
) -> Result<Option<String>, PackError> {
    if sources.is_empty() {
        return Ok(None);
    }

    let mut fragments: Vec<String> = Vec::with_capacity(sources.len());
    for source in sources {
        match source {
            InstructionSource::Inline(text) => fragments.push(text.clone()),
            InstructionSource::File(path) => {
                let resolved = match root {
                    Some(root) if path.is_relative() => root.join(path),
                    _ => path.clone(),
                };
                let text = std::fs::read_to_string(&resolved).map_err(|source| {
                    PackError::InstructionFile {
                        path: resolved.display().to_string(),
                        source,
                    }
                })?;
                fragments.push(text);
            }
        }
    }

    // Trim each fragment and drop empties so blank files don't add noise.
    let joined = fragments
        .iter()
        .map(|f| f.trim())
        .filter(|f| !f.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");

    Ok((!joined.is_empty()).then_some(joined))
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
        let sources = vec![InstructionSource::File("/nonexistent/sys.md".into())];
        let err = resolve_instructions(&sources, None).unwrap_err();
        assert!(matches!(err, PackError::InstructionFile { .. }));
    }
}
