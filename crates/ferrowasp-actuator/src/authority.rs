#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActuatorPermission {
    Inhibited,
    IdleOnly,
    Active,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ActuatorAuthority {
    permission: ActuatorPermission,
}

impl ActuatorAuthority {
    pub const fn inhibited() -> Self {
        Self {
            permission: ActuatorPermission::Inhibited,
        }
    }

    pub const fn active() -> Self {
        Self {
            permission: ActuatorPermission::Active,
        }
    }

    pub const fn permission(self) -> ActuatorPermission {
        self.permission
    }
}
