# Development Container Internals

What the reference container in `compose.yaml` does for maintainers' agent
tooling. Contributors' setup is the public book's
`developer_getting_started.md`; the open container work is in
[remaining work](remaining-work.md).

## Codex Sessions

The image installs a pinned Codex CLI under `/opt/codex`. Codex keeps its
session rollouts, session index, configuration, and login state under
`/home/ferrowasp/.codex`, which is mounted from the external Docker volume
`ferrowasp-codex-home`, so recreating the container keeps chats and settings.
The executable lives outside the mount, so retained state cannot hide the one
the image supplies.

The volume holds authentication material: never copy it into the repository or
a shared archive. Docker manages it outside the Compose lifecycle, so
`docker compose down --volumes` keeps it; removing it is a separate
destructive action that deletes its chats, configuration, and login.

A container created before the volume existed is migrated once, as the last
action before recreating it:

```bash
bash tools/dev/migrate-codex-state.sh
```

The script pauses the container while it takes a bounded snapshot, refuses to
overwrite a non-empty volume, verifies the copied file count, and removes its
temporary host copy. Messages written to the old container after the snapshot
are lost. Afterwards, `codex resume`, `codex resume --last`, and
`codex resume --all` reopen retained threads.

## Seccomp Exception

The development service runs with `seccomp=unconfined` because Docker's default
profile blocks the unprivileged `unshare --user` that the Codex/Bubblewrap
sandbox needs. It keeps `no-new-privileges` and adds no Linux capabilities.
`tools/dev/check-namespaces.sh` checks the namespace prerequisites.
