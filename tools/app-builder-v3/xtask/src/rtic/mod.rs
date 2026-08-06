//! RTIC-shaped declaration types used by reusable tasks and app composition.

/// Application-level declarations for reusable components.
pub mod component;

/// Task instances, application composition, and structural validation.
pub mod composition;

/// Reusable task contracts, bindings, contexts, and authoring macros.
pub mod task;

/// Application monotonic-timer declarations.
pub mod timing;

/// Whole-application component expansion and resource resolution.
pub mod resolve;

/// RTIC source rendering from a fully resolved application.
pub mod render;
