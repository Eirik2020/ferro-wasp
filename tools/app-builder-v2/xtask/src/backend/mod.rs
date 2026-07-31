use anyhow::Result;

use crate::{
    board::{BoardDeclaration, Mcu},
    resolve::ResolvedApp,
};

mod stm32f4;

pub struct RenderedBoardInit {
    pub imports: String,
    pub monotonic_declaration: String,
    pub shared_struct: String,
    pub shared_value: String,
    pub local_struct: String,
    pub local_value: String,
    pub initialization: String,
}

pub fn validate(board: &BoardDeclaration) -> Result<()> {
    match board.mcu {
        Mcu::Stm32F401RE => stm32f4::validate(board),
    }
}

pub fn render(board: &BoardDeclaration, app: &ResolvedApp<'_>) -> Result<RenderedBoardInit> {
    match board.mcu {
        Mcu::Stm32F401RE => stm32f4::render(board, app),
    }
}
