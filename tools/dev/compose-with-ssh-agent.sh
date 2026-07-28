#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

if [[ "$#" -eq 0 ]]; then
    printf "usage: %s <docker compose arguments...>\n" "${0##*/}" >&2
    exit 2
fi
if ! command -v docker >/dev/null 2>&1; then
    printf "%s\n" "docker is not installed on the host." >&2
    exit 1
fi
if ! docker compose version >/dev/null 2>&1; then
    printf "%s\n" "the Docker Compose plugin is not available." >&2
    exit 1
fi
docker volume create ferrowasp-codex-home >/dev/null
if ! command -v ssh-add >/dev/null 2>&1; then
    printf "%s\n" "ssh-add is not installed on the host." >&2
    exit 1
fi
if [[ -z "${SSH_AUTH_SOCK:-}" ]]; then
    printf "%s\n" "SSH_AUTH_SOCK is not set; start the host SSH agent first." >&2
    exit 1
fi
if [[ ! -S "${SSH_AUTH_SOCK}" ]]; then
    printf "SSH_AUTH_SOCK is not a usable Unix socket: %s\n" "${SSH_AUTH_SOCK}" >&2
    exit 1
fi
if ! ssh-add -l >/dev/null; then
    printf "%s\n" "The host SSH agent has no usable identity; run ssh-add first." >&2
    exit 1
fi

cd "${repo_root}"
exec docker compose \
    --file compose.yaml \
    --file .devcontainer/compose.ssh-agent.yaml \
    "$@"
