//! FerroWasp's requirements and verification tool.
//!
//! Requirements are YAML in `project_meta/requirements/`, rooted in the
//! governing requirements W1-W7. Source files declare the requirements they
//! implement. Status is computed from evidence records and derived
//! identities, never stored. The design is
//! `project_meta/requirements/design.md`; section 20 is the tracing model.

pub mod configuration;
pub mod details;
pub mod evidence;
pub mod gates;
pub mod identity;
pub mod items;
pub mod lint;
pub mod project;
pub mod report;
pub mod roots;
pub mod runner;
pub mod schema;
pub mod status;
pub mod trace;
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
