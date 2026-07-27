# Third-party notices

FerroConfigurator source code is Apache-2.0. The Windows release folder also
contains separate, unmodified executables from the official dfu-util 0.11
binary release.

## dfu-util 0.11

- Project: https://dfu-util.sourceforge.net/
- Binary source: official `dfu-util-0.11-binaries.tar.xz` release
- License: GNU General Public License, version 2
- Bundled file: `dfu-util.exe`
- SHA-256: `4C3242657D492BFF4220CE412DBF55DE1564BC3CB1508F0BB2FFD4DECB47FFB7`
- Corresponding source: `third_party/sources/dfu-util-0.11.tar.gz`
- Source SHA-256: `B4B53BA21A82EF7E3D4C47DF2952ADF5FA494F499B6B0B57C58C5D04AE8FF19E`

The program is launched as a separate process. FerroConfigurator is not linked
to dfu-util.

## libusb 1.0.24

- Project: https://github.com/libusb/libusb
- License: GNU Lesser General Public License, version 2.1 or later
- Bundled file: `libusb-1.0.dll`
- SHA-256: `3C3C1F47C8040D841A940218874BCF88A47E24F58A115A6BAFF0869D2532FB8E`
- Corresponding source: `third_party/sources/libusb-1.0.24.tar.bz2`
- Source SHA-256: `7EFD2685F7B327326DCFB85CEE426D9B871FD70E22CAA15BB68D595CE2A2B12A`

The DLL remains a separate, replaceable library beside `dfu-util.exe`.

The release package includes the applicable license texts, upstream authors,
and exact corresponding source archives. No changes were made to either
third-party binary.


