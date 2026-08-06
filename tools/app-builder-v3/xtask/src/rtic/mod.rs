//! RTIC-shaped declaration types used by reusable tasks and app composition.

/// Application-level declarations for reusable components.
pub mod component;

/// Task instances, application composition, and structural validation.
pub mod composition;

/// Reusable task contracts, bindings, contexts, and authoring macros.
pub mod task;
