//! FerroWasp's adapters from external sensor drivers to the sensor-class
//! traits of `ferrowasp-sensor-traits`. A driver is a self-contained
//! embedded-hal crate that knows nothing of FerroWasp; its adapter here owns
//! what FerroWasp asks of the class that the driver does not give: integer
//! units, plausibility limits, deadlines and freshness.
//!
//! Not safety-critical: nothing flies on a barometer or GNSS yet.

#![no_std]
#![forbid(unsafe_code)]

#[cfg(test)]
extern crate std;

pub mod dps310;
pub mod spl06;
