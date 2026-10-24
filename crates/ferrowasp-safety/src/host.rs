//! Whether a computer is attached, from leases its links renew through the
//! coordinator (design §11). Arming is refused while any lease stands. A
//! link renews while its host is there; a lease it stops renewing lapses by
//! itself, so no non-critical task writes what the arming guard reads.
//!
//! Requirements: ARM-003, SAFETY-014
//! Safety-critical.

/// A link a computer can be attached by.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostLink {
    /// USB, renewed about every 100 ms while a host has it configured.
    Usb,
    /// The command line on a UART, renewed by each command.
    ConfiguratorLink,
}

impl HostLink {
    /// How long one renewal holds: a few USB renewals, and the configurator
    /// link's two seconds after its last command.
    pub const fn lease_ms(self) -> u32 {
        match self {
            Self::Usb => 500,
            Self::ConfiguratorLink => 2_000,
        }
    }

    const fn index(self) -> usize {
        match self {
            Self::Usb => 0,
            Self::ConfiguratorLink => 1,
        }
    }
}

/// Each link's lease, held by the safety master.
#[derive(Clone, Copy, Debug, Default)]
pub struct HostLeases {
    until_ms: [Option<u64>; 2],
}

impl HostLeases {
    pub const fn new() -> Self {
        Self {
            until_ms: [None; 2],
        }
    }

    /// Renew `link`'s lease from `now_ms`, milliseconds since boot.
    pub fn renew(&mut self, link: HostLink, now_ms: u64) {
        self.until_ms[link.index()] = Some(now_ms.saturating_add(u64::from(link.lease_ms())));
    }

    /// Whether any lease stands at `now_ms`.
    pub fn present(&self, now_ms: u64) -> bool {
        self.until_ms.iter().flatten().any(|until| now_ms < *until)
    }

    /// When the last standing lease ends; `None` when none was ever taken.
    pub fn until_ms(&self) -> Option<u64> {
        self.until_ms.iter().flatten().copied().max()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_lease_means_no_host() {
        assert!(!HostLeases::new().present(0));
        assert_eq!(HostLeases::new().until_ms(), None);
    }

    #[test]
    fn a_lease_stands_until_its_length_and_renewal_extends_it() {
        let mut leases = HostLeases::new();
        leases.renew(HostLink::Usb, 1_000);
        assert!(leases.present(1_499));
        assert!(!leases.present(1_500));
        leases.renew(HostLink::Usb, 1_400);
        assert!(leases.present(1_899));
        assert!(!leases.present(1_900));
    }

    #[test]
    fn either_link_holds_the_host_present() {
        let mut leases = HostLeases::new();
        leases.renew(HostLink::Usb, 0);
        leases.renew(HostLink::ConfiguratorLink, 0);
        assert!(leases.present(1_000));
        assert!(!leases.present(2_000));
        assert_eq!(leases.until_ms(), Some(2_000));
    }
}
