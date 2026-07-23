# Temporary FerroWasp MSPv1 compatibility copy

This crate is an unchanged compatibility copy of
`ferro-wasp/crates/ferrowasp-mspv1` from the local FerroWasp flight-test tree
at commit `bc26276c5b34e20f96f603d95585893b91784d05` on 2026-07-23.

It exists so RTIC app-builder experiments cannot affect the FerroWasp flight
applications during active flight testing. Do not evolve this into a second
protocol implementation. Replace it with the canonical FerroWasp crate once a
stable cross-repository dependency boundary is available, then delete this
directory.
