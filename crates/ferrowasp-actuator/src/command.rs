use crate::authority::{ActuatorAuthority, ActuatorPermission};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MotorCommandError {
    NotAuthorized,
    OutOfRange,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ValidatedMotorFrame {
    values: [u16; 4],
}

impl ValidatedMotorFrame {
    pub fn new(authority: ActuatorAuthority, values: [u16; 4]) -> Result<Self, MotorCommandError> {
        if authority.permission() != ActuatorPermission::Active {
            return Err(MotorCommandError::NotAuthorized);
        }

        if values.iter().any(|value| *value > 2000) {
            return Err(MotorCommandError::OutOfRange);
        }

        Ok(Self { values })
    }

    pub const fn values(self) -> [u16; 4] {
        self.values
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::authority::ActuatorAuthority;

    #[test]
    fn active_authority_validates_motor_frame() {
        let frame =
            ValidatedMotorFrame::new(ActuatorAuthority::active(), [0, 10, 1000, 2000]).unwrap();
        assert_eq!(frame.values(), [0, 10, 1000, 2000]);
    }

    #[test]
    fn inhibited_authority_rejects_motor_frame() {
        let result = ValidatedMotorFrame::new(ActuatorAuthority::inhibited(), [0; 4]);
        assert_eq!(result, Err(MotorCommandError::NotAuthorized));
    }
}
