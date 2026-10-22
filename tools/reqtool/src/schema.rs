//! The requirement file format, as `serde` types.
//!
//! Every type refuses unknown fields, so a misspelt or invented field is an
//! error rather than silently ignored. There is deliberately no `status`
//! field anywhere: status is computed from evidence (ADR-0050). Which code
//! implements a requirement is declared in the code (design section 20).

use serde::{Deserialize, Serialize};

/// The only schema version this tool reads.
pub const SCHEMA_VERSION: u32 = 2;

/// One `project_meta/requirements/<subsystem>.yaml` file.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequirementFile {
    pub schema_version: u32,
    pub requirements: Vec<Requirement>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Requirement {
    /// `<AREA>-<NNN>`, stable and never reused.
    pub id: String,
    pub title: String,
    /// The "shall" statement.
    pub statement: String,
    pub rationale: String,
    /// Governing roots (`W1`..`W7`) or parent requirement IDs.
    pub derived_from: Vec<String>,
    pub verification: Verification,
}

/// A requirement is verified by its own entries; none means nothing
/// verifies it yet.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Verification {
    #[serde(default)]
    pub entries: Vec<Entry>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub level: Level,
    pub target: Target,
    pub setup: Setup,
    pub method: Method,
}

/// Verification levels, ordered by cost and risk (ADR-0051).
#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq, Ord, PartialOrd)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    Analysis,
    Unit,
    Sitl,
    Mcu,
    Bench,
    Flight,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Target {
    Host,
    Fcu,
    Devboard,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum Setup {
    None,
    Sim,
    PropsOff,
}

/// What runs the verification: a Rust test found by parsing, or a written
/// procedure named by `path#heading`.
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Method {
    Test(String),
    Procedure(String),
}

impl Level {
    /// Levels run on target hardware, whose evidence belongs to one board.
    pub const fn on_target(self) -> bool {
        matches!(self, Self::Mcu | Self::Bench | Self::Flight)
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Analysis => "analysis",
            Self::Unit => "unit",
            Self::Sitl => "sitl",
            Self::Mcu => "mcu",
            Self::Bench => "bench",
            Self::Flight => "flight",
        }
    }
}

impl Target {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Host => "host",
            Self::Fcu => "fcu",
            Self::Devboard => "devboard",
        }
    }
}

impl Setup {
    pub const fn name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Sim => "sim",
            Self::PropsOff => "props-off",
        }
    }
}
