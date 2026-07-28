#!/usr/bin/env bash
set -euo pipefail

usage() {
    printf "usage: %s [--tools-only|--require-userns]\n" "${0##*/}" >&2
}

mode="${1:---tools-only}"
if [[ "$#" -gt 1 ]]; then
    usage
    exit 2
fi
case "${mode}" in
    --tools-only|--require-userns) ;;
    *)
        usage
        exit 2
        ;;
esac

for executable in bwrap unshare sysctl; do
    if ! command -v "${executable}" >/dev/null 2>&1; then
        printf "missing required executable: %s\n" "${executable}" >&2
        exit 1
    fi
done

bwrap --version
unshare --version | head -n 1
sysctl user.max_user_namespaces
if command -v capsh >/dev/null 2>&1; then
    capsh --print | sed -n "1,5p"
fi
if [[ -e /proc/sys/kernel/unprivileged_userns_clone ]]; then
    sysctl kernel.unprivileged_userns_clone
fi

if [[ "${mode}" == "--tools-only" ]]; then
    printf "%s\n" "Namespace diagnostic tools are ready."
    exit 0
fi

unshare --user --map-root-user /usr/bin/true
bwrap \
    --unshare-user \
    --unshare-pid \
    --new-session \
    --die-with-parent \
    --ro-bind / / \
    --proc /proc \
    --dev /dev \
    /usr/bin/true

printf "%s\n" "Nested user namespaces and Bubblewrap are ready."
