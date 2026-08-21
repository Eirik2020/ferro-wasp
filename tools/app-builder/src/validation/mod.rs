//! Strict compatibility ingestion for the two NUCLEO validation applications.
//!
//! These modules preserve the checked schema-6 manifests and architecture
//! contracts while their declarations are lifted into the richer typed RTIC
//! model. Selection is confined to the compile-time application catalog.

#![allow(missing_docs)]

pub mod architecture;
pub mod backend;
pub mod feature;
pub mod manifest;
pub mod mcu;
pub mod render;
pub mod syntax;
pub mod validate;
