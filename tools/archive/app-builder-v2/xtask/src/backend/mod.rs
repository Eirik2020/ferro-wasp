//! MCU-backend selection and the rendered fragments consumed by the app template.

use anyhow::Result;

use crate::{board::BoardDeclaration, hw_resources::Mcu, resolve::ResolvedApp};

mod stm32f4;

/// Board declaration after validation by its selected MCU backend.
#[derive(Debug)]
pub enum ValidatedBoard<'a> {
    /// Board validated for the STM32F4 backend.
    Stm32f4(stm32f4::ValidatedBoard<'a>),
}

/// Backend-rendered Rust fragments used to assemble the RTIC application.
#[derive(Debug)]
pub struct RenderedBoardInit {
    /// Free hardware interrupt vector used by RTIC for software dispatching.
    pub dispatchers: String,

    /// RTIC interrupt binding keyed by interrupt-task identifier.
    pub interrupt_bindings: std::collections::BTreeMap<String, String>,

    /// Prelude reexports required by the resolved hardware and task operations.
    pub prelude_exports: String,

    /// System-clock constant and RTIC monotonic declarations emitted inside the app module.
    pub timing_declarations: String,

    /// Complete RTIC init attribute, including generated static local storage.
    pub init_attribute: String,

    /// Complete generated `Shared` resource-struct declaration.
    pub shared_struct: String,

    /// Expression returned by `init` to construct `Shared`.
    pub shared_value: String,

    /// Complete generated `Local` resource-struct declaration.
    pub local_struct: String,

    /// Expression returned by `init` to construct `Local`.
    pub local_value: String,

    /// Clock, monotonic, GPIO, and interrupt initialization statements.
    pub initialization: String,
}

/// Selects the MCU backend and validates target-specific board facts.
pub fn validate(board: &BoardDeclaration) -> Result<ValidatedBoard<'_>> {
    match board.target.mcu {
        Mcu::Stm32F401 | Mcu::Stm32F405 => stm32f4::validate(board).map(ValidatedBoard::Stm32f4),
    }
}

/// Renders a validated board for the resources used by a resolved application.
pub fn render(board: &ValidatedBoard<'_>, app: &ResolvedApp<'_>) -> Result<RenderedBoardInit> {
    match board {
        ValidatedBoard::Stm32f4(board) => stm32f4::render(board, app),
    }
}
