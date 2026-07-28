#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${repo_root}"

expect_line() {
    local expected="$1"
    shift
    local actual
    actual="$("$@" | head -n 1)"
    if [[ "${actual}" != "${expected}" ]]; then
        printf 'expected: %s\nactual:   %s\n' "${expected}" "${actual}" >&2
        return 1
    fi
    printf '%s\n' "${actual}"
}

expect_line "Python 3.13.14" python --version
expect_line "rustc 1.99.0-nightly (77cf889bc 2026-07-12)" \
    rustc +nightly-2026-07-13 --version
expect_line "rustc 1.93.1 (01f6ddf75 2026-02-11)" \
    rustc +1.93.1 --version
expect_line "mdbook v0.5.2" mdbook --version
expect_line "mdbook-mermaid 0.17.0" mdbook-mermaid --version
expect_line "codex-cli 0.143.0" codex --version

bash tools/dev/check-git-ssh.sh --client-only
bash tools/dev/check-namespaces.sh --tools-only

rustup toolchain list | grep --quiet '^nightly-2025-12-13-'
rustup target list --toolchain nightly-2026-07-13 --installed \
    | grep --quiet '^thumbv7em-none-eabihf$'
rustup target list --toolchain nightly-2025-12-13 --installed \
    | grep --quiet '^thumbv7em-none-eabihf$'

python - <<'PY'
import matplotlib
import numpy
import serial

expected = {
    "matplotlib": (matplotlib.__version__, "3.10.0"),
    "numpy": (numpy.__version__, "2.2.1"),
    "pyserial": (serial.VERSION, "3.5"),
}
for name, (actual, wanted) in expected.items():
    if actual != wanted:
        raise SystemExit(f"{name}: expected {wanted}, got {actual}")
    print(f"{name} {actual}")
PY

for manifest in \
    Cargo.toml \
    apps/stm32f405-flight/Cargo.toml \
    apps/foxeer-f405-v2/Cargo.toml \
    apps/stm32f401-bringup/Cargo.toml \
    tools/ferro-configurator/Cargo.toml \
    tools/rtic-app-builder/Cargo.toml; do
    manifest_dir="$(dirname "${manifest}")"
    manifest_name="$(basename "${manifest}")"
    (
        cd "${manifest_dir}"
        cargo metadata \
            --manifest-path "${manifest_name}" \
            --format-version 1 \
            --locked \
            --no-deps \
            --offline >/dev/null
    )
done

printf '%s\n' "FerroWasp container environment is ready."
