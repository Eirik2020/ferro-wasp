use std::ops::Range;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoardProfile {
    pub id: &'static str,
    pub display_name: &'static str,
    pub flash_range: Range<u32>,
    pub ram_range: Range<u32>,
    pub dfu_alt: u8,
}

impl BoardProfile {
    /// Matches `apps/foxeer-f405-v2/memory.x` in FerroWasp.
    pub const FOXEER_F405_V2: Self = Self {
        id: "foxeer-f405-v2",
        display_name: "Foxeer F405 V2 (STM32F405RGT6)",
        flash_range: 0x0800_0000..0x0810_0000,
        ram_range: 0x2000_0000..0x2002_0000,
        dfu_alt: 0,
    };
}
