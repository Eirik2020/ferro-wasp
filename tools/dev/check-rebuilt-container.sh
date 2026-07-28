#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${repo_root}"

bash tools/dev/check-git-ssh.sh --require-agent

codex_version="$(codex --version)"
if [[ "${codex_version}" != "codex-cli 0.143.0" ]]; then
    printf "unexpected Codex CLI version: %s\n" "${codex_version}" >&2
    exit 1
fi
printf "%s\n" "${codex_version}"
if ! mountpoint --quiet /home/ferrowasp/.codex; then
    printf "%s\n" "/home/ferrowasp/.codex is not a persistent mount." >&2
    exit 1
fi
printf "%s\n" "Persistent Codex state volume is mounted."

origin_url="$(git remote get-url origin)"
case "${origin_url}" in
    git@github.com:*|ssh://git@github.com/*) ;;
    *)
        printf "origin is not a GitHub SSH remote: %s\n" "${origin_url}" >&2
        exit 1
        ;;
esac

git ls-remote --exit-code origin HEAD >/dev/null

bash tools/dev/check-namespaces.sh --require-userns
printf "%s\n" "FerroWasp rebuilt-container acceptance passed."
