//! Business Pack: composes an application's instructions, skills, and plugins
//! (design §9).
//!
//! A pack bundles the methodology/system prompt and knowledge assets a specific
//! application needs, keeping them OUT of the business-neutral Omnix runtime. In
//! SDK 0.0 the primary, fully-wired capability is instruction composition:
//! pack instructions are folded into the runtime's additive developer
//! instructions. Skill and plugin loading are intentionally outside the 0.0
//! public contract until they have an executable runtime implementation.

mod loader;

pub use loader::PackError;

use serde::Deserialize;
use serde::Serialize;

/// Where an instruction fragment comes from.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "source", content = "value")]
pub enum InstructionSource {
    /// Inline text supplied directly (or compile-time embedded via `include_str!`).
    Inline(String),
    /// A file path resolved at load time relative to the pack root.
    File(std::path::PathBuf),
}

/// A composed business pack.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusinessPack {
    pub id: String,
    pub version: String,
    #[serde(default)]
    pub instructions: Vec<InstructionSource>,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

impl BusinessPack {
    /// Create a pack with an id and version and no assets.
    pub fn new(id: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            version: version.into(),
            instructions: Vec::new(),
            metadata: serde_json::Value::Null,
        }
    }

    /// Add an inline instruction fragment.
    pub fn with_inline_instruction(mut self, text: impl Into<String>) -> Self {
        self.instructions
            .push(InstructionSource::Inline(text.into()));
        self
    }

    /// Add a file-backed instruction fragment.
    pub fn with_instruction_file(mut self, path: impl Into<std::path::PathBuf>) -> Self {
        self.instructions.push(InstructionSource::File(path.into()));
        self
    }

    /// Resolve and concatenate all instruction fragments into a single
    /// additive developer-instructions string, in order, separated by blank
    /// lines.
    ///
    /// `File` sources are read relative to the required `root`; absolute paths
    /// and paths that escape the root are rejected.
    /// Returns `None` when the pack contributes no instructions.
    pub fn resolve_developer_instructions(
        &self,
        root: Option<&std::path::Path>,
    ) -> Result<Option<String>, PackError> {
        loader::resolve_instructions(&self.instructions, root)
    }
}
