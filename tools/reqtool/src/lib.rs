//! FerroWasp's requirements and verification tool.
//!
//! Requirements are YAML in `project_meta/requirements/`, rooted in the
//! governing requirements W1-W7 and allocated to items in the code. Status is
//! computed from evidence records and derived fingerprints, never stored. The
//! design is `project_meta/requirements/design.md` and ADR-0050 to ADR-0053.

pub mod details;
pub mod evidence;
pub mod fingerprint;
pub mod gates;
pub mod items;
pub mod lint;
pub mod project;
pub mod report;
pub mod roots;
pub mod runner;
pub mod schema;
pub mod status;
pub mod workspace;

use std::path::{Path, PathBuf};

/// The repository containing `start`: the nearest ancestor with the governing
/// requirements.
pub fn find_repository(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .find(|candidate| candidate.join(roots::GOVERNING_REQUIREMENTS).is_file())
        .map(Path::to_path_buf)
}
