//! What a configuration change and the boot's configuration hold against
//! arming (design §11, §14), owned by the safety master.
//!
//! - A config transaction holds an arm inhibit, granted only while the craft
//!   is disarmed and not arming. Its lease is short; the coordinator
//!   releases it once the change is applied. A lease that lapses unreleased
//!   means a change may be half applied, so arming stays refused until the
//!   board reboots.
//! - A boot that fell back from the pilot's last save refuses arming until
//!   the pilot acknowledges it, disarmed.
//!
//! Requirements: CONFIG-017, CONFIG-018, CONFIG-019, CONFIG-027
//! Safety-critical.

use crate::{ArmingAbortReason, Refusal};

/// How long a config transaction may hold its arm inhibit: fifty
/// coordinator runs.
pub const CONFIG_INHIBIT_LEASE_MS: u64 = 500;

#[derive(Clone, Copy, Debug, Default)]
pub struct ConfigGate {
    /// The ticket of the transaction holding the inhibit, and when its lease
    /// ends.
    inhibit: Option<(u32, u64)>,
    /// A lease lapsed: refused until reboot.
    latched: bool,
    /// The boot fell back and the pilot has not acknowledged it.
    fallback: bool,
}

impl ConfigGate {
    pub const fn new(boot_fell_back: bool) -> Self {
        Self {
            inhibit: None,
            latched: false,
            fallback: boot_fell_back,
        }
    }

    /// Grant transaction `ticket` the arm inhibit at `now_ms`.
    pub fn request(
        &mut self,
        ticket: u32,
        armed: bool,
        arming: bool,
        now_ms: u64,
    ) -> Result<(), Refusal> {
        self.lapse(now_ms);
        if armed {
            Err(Refusal::Armed)
        } else if arming {
            Err(Refusal::ArmingInProgress)
        } else if self.inhibit.is_some() {
            Err(Refusal::Busy)
        } else {
            self.inhibit = Some((ticket, now_ms.saturating_add(CONFIG_INHIBIT_LEASE_MS)));
            Ok(())
        }
    }

    /// The transaction `ticket` finished; a release after its lease lapsed
    /// comes too late, and the refusal stays latched.
    pub fn release(&mut self, ticket: u32, now_ms: u64) {
        self.lapse(now_ms);
        if self.inhibit.is_some_and(|(held, _)| held == ticket) {
            self.inhibit = None;
        }
    }

    /// The pilot saw the boot's fallback; only while disarmed.
    pub fn acknowledge_fallback(&mut self, armed: bool) -> Result<(), Refusal> {
        if armed {
            return Err(Refusal::Armed);
        }
        self.fallback = false;
        Ok(())
    }

    /// Why arming is refused at `now_ms`, if it is.
    pub fn arming_refusal(&mut self, now_ms: u64) -> Option<ArmingAbortReason> {
        self.lapse(now_ms);
        if self.latched {
            Some(ArmingAbortReason::ConfigChangeIncomplete)
        } else if self.inhibit.is_some() {
            Some(ArmingAbortReason::ConfigChanging)
        } else if self.fallback {
            Some(ArmingAbortReason::ConfigFallback)
        } else {
            None
        }
    }

    pub const fn fallback_pending(&self) -> bool {
        self.fallback
    }

    fn lapse(&mut self, now_ms: u64) {
        if self.inhibit.is_some_and(|(_, until)| now_ms >= until) {
            self.inhibit = None;
            self.latched = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_inhibit_is_granted_only_disarmed_and_not_arming() {
        let mut gate = ConfigGate::new(false);
        assert_eq!(gate.request(1, true, false, 0), Err(Refusal::Armed));
        assert_eq!(
            gate.request(1, false, true, 0),
            Err(Refusal::ArmingInProgress)
        );
        assert_eq!(gate.request(1, false, false, 0), Ok(()));
        assert_eq!(
            gate.request(2, false, false, 1),
            Err(Refusal::Busy),
            "one at a time"
        );
        assert_eq!(
            gate.arming_refusal(1),
            Some(ArmingAbortReason::ConfigChanging)
        );
    }

    #[test]
    fn a_released_inhibit_lets_the_craft_arm() {
        let mut gate = ConfigGate::new(false);
        gate.request(7, false, false, 100).unwrap();
        gate.release(7, 120);
        assert_eq!(gate.arming_refusal(130), None);
    }

    #[test]
    fn a_lapsed_inhibit_latches_until_reboot() {
        let mut gate = ConfigGate::new(false);
        gate.request(7, false, false, 100).unwrap();
        assert_eq!(
            gate.arming_refusal(100 + CONFIG_INHIBIT_LEASE_MS),
            Some(ArmingAbortReason::ConfigChangeIncomplete)
        );
        gate.release(7, 100 + CONFIG_INHIBIT_LEASE_MS + 1);
        assert_eq!(
            gate.arming_refusal(10_000),
            Some(ArmingAbortReason::ConfigChangeIncomplete),
            "a late release does not clear it"
        );
    }

    #[test]
    fn a_fallback_refuses_arming_until_acknowledged_disarmed() {
        let mut gate = ConfigGate::new(true);
        assert_eq!(
            gate.arming_refusal(0),
            Some(ArmingAbortReason::ConfigFallback)
        );
        assert_eq!(gate.acknowledge_fallback(true), Err(Refusal::Armed));
        assert_eq!(gate.acknowledge_fallback(false), Ok(()));
        assert_eq!(gate.arming_refusal(0), None);
        assert!(!gate.fallback_pending());
    }
}
