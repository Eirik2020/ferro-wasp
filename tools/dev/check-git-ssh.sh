#!/usr/bin/env bash
set -euo pipefail

usage() {
    printf "usage: %s [--client-only|--require-agent]\n" "${0##*/}" >&2
}

mode="${1:---client-only}"
if [[ "$#" -gt 1 ]]; then
    usage
    exit 2
fi

case "${mode}" in
    --client-only|--require-agent) ;;
    *)
        usage
        exit 2
        ;;
esac

for executable in git ssh ssh-add; do
    if ! command -v "${executable}" >/dev/null 2>&1; then
        printf "missing required executable: %s\n" "${executable}" >&2
        exit 1
    fi
done

git --version
ssh -V 2>&1 | head -n 1
ssh -G -o BatchMode=yes github.com >/dev/null

if [[ "${mode}" == "--client-only" ]]; then
    printf "%s\n" "Git and OpenSSH clients are ready."
    exit 0
fi

if [[ -z "${SSH_AUTH_SOCK:-}" ]]; then
    printf "%s\n" "SSH_AUTH_SOCK is not set; start the host SSH agent before creating the container." >&2
    exit 1
fi
if [[ ! -S "${SSH_AUTH_SOCK}" ]]; then
    printf "SSH_AUTH_SOCK is not a usable Unix socket: %s\n" "${SSH_AUTH_SOCK}" >&2
    exit 1
fi
if ! ssh-add -l; then
    printf "%s\n" "The forwarded SSH agent has no usable identity." >&2
    exit 1
fi

printf "%s\n" "Git, OpenSSH, and the forwarded SSH agent are ready."
