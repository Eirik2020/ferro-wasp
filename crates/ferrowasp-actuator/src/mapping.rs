#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MotorOutputMap(pub [usize; 4]);

impl MotorOutputMap {
    pub const fn identity() -> Self {
        Self([0, 1, 2, 3])
    }

    pub fn apply(self, logical: [u16; 4]) -> [u16; 4] {
        let mut physical = [0; 4];
        let mut index = 0;

        while index < 4 {
            physical[self.0[index]] = logical[index];
            index += 1;
        }

        physical
    }
}
