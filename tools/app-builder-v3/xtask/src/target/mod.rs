//! Board and application declarations for the selected generation target.

/// Physical hardware declared by the selected board.
pub mod board;

/// Service-to-port assignments loaded during generated RTIC initialization.
pub mod platform_config;

/// Application composition selected for this target.
pub mod app_composition;

/// Reconciliation of the selected graph with the handwritten golden app.
pub mod golden_reconciliation;
