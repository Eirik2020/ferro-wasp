#!/usr/bin/env bash
set -euo pipefail

volume_name="ferrowasp-codex-home"
image_name="ferrowasp-dev:local"
codex_uid="${DEV_UID:-1000}"
codex_gid="${DEV_GID:-1000}"

for value in "${codex_uid}" "${codex_gid}"; do
    if [[ ! "${value}" =~ ^[0-9]+$ ]]; then
        printf "UID/GID must be numeric: %s\n" "${value}" >&2
        exit 2
    fi
done
if ! command -v docker >/dev/null 2>&1; then
    printf "%s\n" "docker is not installed on the host." >&2
    exit 1
fi
if ! docker image inspect "${image_name}" >/dev/null 2>&1; then
    printf "required local image is missing: %s\n" "${image_name}" >&2
    exit 1
fi

mapfile -t container_ids < <(
    docker ps \
        --filter label=com.docker.compose.project=ferrowasp \
        --filter label=com.docker.compose.service=dev \
        --format "{{.ID}}"
)
if [[ "${#container_ids[@]}" -ne 1 ]]; then
    printf "expected one running FerroWasp dev container found %s\n" \
        "${#container_ids[@]}" >&2
    exit 1
fi
container_id="${container_ids[0]}"
container_paused=false

backup_dir="$(mktemp -d -t ferrowasp-codex-migrate.XXXXXXXX)"
cleanup() {
    if [[ "${container_paused}" == true ]]; then
        docker unpause "${container_id}" >/dev/null 2>&1 || true
    fi
    if [[ -n "${backup_dir:-}" && -d "${backup_dir}" && \
          "${backup_dir}" == */ferrowasp-codex-migrate.* ]]; then
        rm -rf -- "${backup_dir}"
    fi
}
trap cleanup EXIT
mkdir -m 0700 "${backup_dir}/codex"

docker pause "${container_id}" >/dev/null
container_paused=true
docker cp \
    "${container_id}:/home/ferrowasp/.codex/." \
    "${backup_dir}/codex"
docker unpause "${container_id}" >/dev/null
container_paused=false
if [[ ! -f "${backup_dir}/codex/session_index.jsonl" ]]; then
    printf "%s\n" "copied Codex state has no session index; refusing migration." >&2
    exit 1
fi
source_files="$(find "${backup_dir}/codex" -type f | wc -l)"
if [[ "${source_files}" -eq 0 ]]; then
    printf "%s\n" "copied Codex state is empty; refusing migration." >&2
    exit 1
fi

docker volume create "${volume_name}" >/dev/null
target_entry="$(
    docker run --rm --user root \
        --volume "${volume_name}:/target" \
        "${image_name}" \
        bash -c "find /target -mindepth 1 -maxdepth 1 -print -quit"
)"
if [[ -n "${target_entry}" ]]; then
    printf "target volume is not empty; refusing to overwrite: %s\n" \
        "${volume_name}" >&2
    exit 1
fi

docker run --rm --user root \
    --volume "${volume_name}:/target" \
    --volume "${backup_dir}/codex:/source:ro" \
    "${image_name}" \
    bash -c "cp -a /source/. /target/ && chown -R ${codex_uid}:${codex_gid} /target"

target_files="$(
    docker run --rm --user root \
        --volume "${volume_name}:/target:ro" \
        "${image_name}" \
        bash -c "find /target -type f | wc -l"
)"
if [[ "${target_files}" -ne "${source_files}" ]]; then
    printf "Codex migration file-count mismatch: source=%s target=%s\n" \
        "${source_files}" "${target_files}" >&2
    exit 1
fi

printf "Migrated %s Codex files into persistent volume %s.\n" \
    "${target_files}" "${volume_name}"
printf "%s\n" \
    "Recreate the container now; further chats in the old container will not be copied."
