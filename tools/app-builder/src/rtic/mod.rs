//! RTIC-shaped declaration types used by reusable tasks and app composition.

/// Application-level declarations for reusable components.
pub mod component;

/// Boot-time hardware-port to service assignments.
pub mod platform_config;

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

/// Deterministic human-review maps from a fully resolved application.
pub mod report;

/// Exclusive bounded channels for authoritative safety-path messages.
pub mod safety_channel;

/// Typed, versioned application-owned task-local state.
pub mod state;
