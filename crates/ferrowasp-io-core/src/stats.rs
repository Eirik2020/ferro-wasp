#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SaturatingCounter {
    value: u32,
}

impl SaturatingCounter {
    pub const fn new() -> Self {
        Self { value: 0 }
    }

    pub const fn get(self) -> u32 {
        self.value
    }

    pub fn increment(&mut self) {
        self.value = self.value.saturating_add(1);
    }

    pub fn add(&mut self, amount: u32) {
        self.value = self.value.saturating_add(amount);
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct IoStats {
    pub completed: SaturatingCounter,
    pub faults: SaturatingCounter,
    pub overruns: SaturatingCounter,
}
