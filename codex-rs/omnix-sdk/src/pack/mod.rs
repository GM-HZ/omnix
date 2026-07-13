//! Business Pack: composes an application's instructions, skills, and plugins
//! (design §9).
//!
//! A pack bundles the methodology/system prompt and knowledge assets a specific
//! application needs, keeping them OUT of the business-neutral Omnix runtime. In
//! SDK 0.0 the primary, fully-wired capability is instruction composition:
//! pack instructions are folded into the runtime's `base_instructions`. Skill
//! and plugin sources are represented but not yet materialized (they are
//! reserved for a later phase).

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
    /// A file path resolved at load time, relative to the pack root or absolute.
    File(std::path::PathBuf),
}

/// A skill asset reference (reserved for a later phase).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillSource {
    pub name: String,
    pub path: std::path::PathBuf,
}

/// A plugin reference (reserved for a later phase).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginSource {
    pub id: String,
    pub path: std::path::PathBuf,
}

/// A composed business pack.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusinessPack {
    pub id: String,
    pub version: String,
    #[serde(default)]
    pub instructions: Vec<InstructionSource>,
    #[serde(default)]
    pub skills: Vec<SkillSource>,
    #[serde(default)]
    pub plugins: Vec<PluginSource>,
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
            skills: Vec::new(),
            plugins: Vec::new(),
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
    /// `base_instructions` string, in order, separated by blank lines.
    ///
    /// `File` sources are read relative to `root` (or used as-is if absolute).
    /// Returns `None` when the pack contributes no instructions.
    pub fn resolve_base_instructions(
        &self,
        root: Option<&std::path::Path>,
    ) -> Result<Option<String>, PackError> {
        loader::resolve_instructions(&self.instructions, root)
    }
}
