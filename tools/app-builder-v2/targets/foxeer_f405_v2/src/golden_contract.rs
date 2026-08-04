//! Synchronization checks against the authoritative Foxeer application board support.

use std::{fs, path::Path};

use anyhow::{Context, Result, bail, ensure};

use crate::{
    board::BoardDeclaration,
    hw_resources::{ClockSource, Mcu, PinId, SerialProtocol},
};

/// Validates the builder's initial Foxeer subset against the golden board source.
pub(super) fn validate(repository_root: &Path, board: &BoardDeclaration) -> Result<()> {
    ensure!(
        board.id == "foxeer_f405_v2",
        "Foxeer builder board ID drifted"
    );
    ensure!(
        board.target.mcu == Mcu::Stm32F405,
        "Foxeer builder MCU drifted from STM32F405RGT6"
    );
    ensure!(
        board.target.clock.source
            == ClockSource::ExternalCrystal {
                frequency_hz: 8_000_000,
            },
        "Foxeer builder clock source drifted from the 8 MHz HSE"
    );
    ensure!(
        board.target.clock.sysclk_hz == 168_000_000,
        "Foxeer builder system clock drifted from 168 MHz"
    );
    ensure!(
        board.target.clock.requires_pll48,
        "Foxeer builder target must retain the USB-valid PLL48 clock"
    );

    let uart2 = board
        .hardware
        .iter()
        .find(|resource| resource.id() == "uart2")
        .and_then(|resource| resource.uart_rx_dma())
        .context("Foxeer builder target is missing its USART2 SBUS receive endpoint")?;
    ensure!(uart2.serial_port.number == 2, "Foxeer SBUS port drifted");
    ensure!(
        uart2.rx_pin == PinId::new(0, 3),
        "Foxeer SBUS RX pin drifted"
    );
    ensure!(
        (uart2.dma.controller, uart2.dma.stream, uart2.dma.channel) == (0, 5, 4),
        "Foxeer SBUS DMA route drifted"
    );
    ensure!(
        uart2.supports_profile(SerialProtocol::Sbus),
        "Foxeer USART2 endpoint no longer declares SBUS capability"
    );

    let ferro_root = repository_root
        .parent()
        .and_then(Path::parent)
        .context("app-builder-v2 must remain under the repository tools directory")?;
    let board_root = ferro_root.join("apps/foxeer-f405-v2/src/board");
    let manifest = read(&board_root.join("manifest.rs"))?;
    require_source_fact(
        &manifest,
        "target_id: \"foxeer_f405_v2\"",
        "golden target ID",
    )?;
    require_source_fact(&manifest, "mcu: \"STM32F405RGT6\"", "golden MCU")?;
    require_source_fact(
        &manifest,
        "pub const HSE_FREQUENCY_HZ: u32 = 8_000_000;",
        "golden HSE frequency",
    )?;
    require_source_fact(
        &manifest,
        "pub const SYSTEM_CLOCK_HZ: u32 = 168_000_000;",
        "golden system clock",
    )?;

    let serial = read(&board_root.join("serial.rs"))?;
    let uart2_source = const_block(&serial, "USART2_SBUS")?;
    for (fact, label) in [
        ("logical: LogicalSerialPort::Uart2", "logical serial port"),
        ("peripheral: \"USART2\"", "PAC peripheral"),
        ("tx_pin: \"PA2 AF7\"", "TX pin"),
        ("rx_pin: \"PA3 AF7\"", "RX pin"),
        ("profile: SerialProfile::sbus()", "SBUS profile"),
        ("rx_dma: \"DMA1 Stream 5 Channel 4\"", "RX DMA route"),
        ("tx_dma: None", "receive-only DMA contract"),
    ] {
        require_source_fact(uart2_source, fact, label)?;
    }

    Ok(())
}

fn read(path: &Path) -> Result<String> {
    fs::read_to_string(path)
        .with_context(|| format!("read authoritative Foxeer board source {}", path.display()))
}

fn require_source_fact(source: &str, fact: &str, label: &str) -> Result<()> {
    if !source.contains(fact) {
        bail!("authoritative Foxeer {label} no longer matches `{fact}`");
    }
    Ok(())
}

fn const_block<'a>(source: &'a str, id: &str) -> Result<&'a str> {
    let marker = format!("pub const {id}:");
    let start = source
        .find(&marker)
        .with_context(|| format!("authoritative Foxeer source is missing `{id}`"))?;
    let remaining = &source[start + marker.len()..];
    let end = remaining.find("\npub const ").unwrap_or(remaining.len());
    Ok(&remaining[..end])
}
