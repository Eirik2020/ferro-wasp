# Temporary FerroWasp MSPv1 compatibility copy

This crate is an unchanged compatibility copy of
`ferro-wasp/crates/ferrowasp-mspv1` from an external FerroWasp source tree
at commit `bc26276c5b34e20f96f603d95585893b91784d05` on 2026-07-23.

It originally isolated RTIC app-builder experiments from that external source
and remains isolated from the FerroWasp root workspace after the monorepo
move. The commit and date are provenance, not a claim about current flight-test
or support status. Do not evolve this into a second protocol implementation.
Replace it with the canonical FerroWasp crate once a stable in-tree dependency
boundary is available, then delete this directory.
