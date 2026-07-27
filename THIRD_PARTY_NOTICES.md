# Third-Party Notices

This file covers third-party assets vendored directly into the FerroWasp
repository. Rust dependencies are resolved in the Cargo lockfiles and carry
their own package license metadata.

## Mermaid browser bundle

`mdbook/mermaid.min.js` contains a browser bundle from
[Mermaid](https://github.com/mermaid-js/mermaid), distributed under the MIT
License. Its retained header states:

> Copyright (c) 2014 - 2022 Knut Sveidqvist

Permission is hereby granted, free of charge, to any person obtaining a copy of
this software and associated documentation files (the "Software"), to deal in
the Software without restriction, including without limitation the rights to
use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software is furnished to do so,
subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.

The minified bundle also retains its generated bundled-license block for
third-party JavaScript dependencies. Preserve those embedded notices when
redistributing the file.

## mdbook-mermaid initialization script

`mdbook/mermaid-init.js` is supplied by
[mdbook-mermaid](https://github.com/badboy/mdbook-mermaid) version `0.17.0`
under the Mozilla Public License 2.0. Its source-form MPL header is retained in
the file. The license text is available at <https://mozilla.org/MPL/2.0/>.

The two vendored browser assets can be regenerated from the repository root
with the pinned documentation tool:

```powershell
cargo install mdbook-mermaid --version 0.17.0 --locked
mdbook-mermaid install mdbook
```

Review the resulting asset and license-header diff before committing it.

## FerroConfigurator Windows DFU bundle

`tools/ferro-configurator/third_party` contains reviewed Windows binaries for
dfu-util `0.11` and libusb `1.0.24`, their license texts, and the corresponding
upstream source archives required by their redistribution terms.

The release packaging script verifies the vendored executable and DLL against
pinned SHA-256 values before including them. Detailed component versions,
upstream locations, and license identifiers are recorded in
`tools/ferro-configurator/THIRD_PARTY_NOTICES.md`.
