# Publication and Licence Status

FerroWasp is open source under the Apache License, Version 2.0.

The current milestone is publication of experimental source code. It is not a
stable firmware release: there are no tagged release artifacts, supported
airframes, compatibility guarantees, or operational-use claims.

## Current Licence Files

The current top-level licence and notice files are:

- [`LICENSE.md`](https://github.com/Eirik2020/ferro-wasp/blob/HEAD/LICENSE.md)
- [`DISCLAIMER.md`](https://github.com/Eirik2020/ferro-wasp/blob/HEAD/DISCLAIMER.md)
- [`NOTICE.md`](https://github.com/Eirik2020/ferro-wasp/blob/HEAD/NOTICE.md)
- [`THIRD_PARTY_NOTICES.md`](https://github.com/Eirik2020/ferro-wasp/blob/HEAD/THIRD_PARTY_NOTICES.md)
- [`CONTRIBUTING.md`](https://github.com/Eirik2020/ferro-wasp/blob/HEAD/CONTRIBUTING.md)
- [`SECURITY.md`](https://github.com/Eirik2020/ferro-wasp/blob/HEAD/SECURITY.md)

`LICENSE.md` contains the Apache-2.0 licence text for FerroWasp project-owned
source. Vendored third-party assets retain their own licences and notices as
listed in `THIRD_PARTY_NOTICES.md` and in any headers or bundled notice blocks
carried with those assets.

## Contribution Boundary

Issues, bug reports, bench-test notes, hardware observations, and design
feedback are welcome.

Focused code and documentation contributions are welcome under Apache-2.0.

Before the initial public snapshot, the exact published history should pass
credential and large-object scans, all supported source checks, and the mdBook
build from a clean checkout. Private bench logs and raw captures are not part
of the source distribution unless they are deliberately sanitized and added as
a small evidence sample.

## Safety Disclaimer

FerroWasp is experimental flight-control software. It is not certified,
airworthy, qualified, assured, validated, production-ready, or suitable for
operational or safety-critical use.

Use is entirely at your own risk. Propellers should be removed during bench
testing.

An operator-reported controlled FCU3 flight is useful prototype evidence but
does not change this release status. Electrical waveform/timing, measured stop
latency, estimator/control validation, and broader fault policy remain open.
