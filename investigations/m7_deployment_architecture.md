# M7 — Deployment Architecture

## Date
2026-08-23

## Objective
Provide a safe, verifiable, and reversible way to install, update, roll back,
and remove the Detectic sensor on a TP-Link EX520V running stock firmware.

## Constraints

- **No firmware modification.** The scripts install only into the writable
  `misc_rw` partition.
- **No auto-start on reboot.** The stock firmware does not provide a
  user-accessible, persistent startup hook. Auto-start after reboot is
  explicitly **not supported** on stock firmware.
- **All binaries must be verified** (architecture, static linking, SHA256) before
  execution.
- **No remote script execution.** Updates require a pre-verified release
  directory or a manifest URL under operator control.

## Deployment Directory

```
/var/run/misc/misc_rw/detectic/
  current -> releases/v0.1.0/          (symlink)
  previous -> releases/<older>/        (symlink)
  releases/
    v0.1.0/
      detectic
      manifest.json
  state/
    detectic.pid
    sensor_id
  spool/
    detectic_buffer.jsonl
  logs/
    detectic.log
  config/
    detectic.env
```

## Scripts

All scripts live in `deploy/` and are copied to `dist/` for release.

| Script | Purpose |
|--------|---------|
| `detectic-install.sh` | Verify binary, install to versioned release, create `current` symlink, write env template |
| `detectic-start.sh` | Load credentials, start sensor with nohup, write PID file |
| `detectic-stop.sh` | Gracefully stop sensor by PID |
| `detectic-health.sh` | Check binary, process, and `detectic status` |
| `detectic-update.sh` | Stage and atomically activate a new verified release |
| `detectic-rollback.sh` | Revert to the `previous` verified release |
| `detectic-remove.sh` | Stop and remove the `detectic` directory, show cleanup commands |

## Release Manifest

`manifest.json`:

```json
{
  "name": "detectic",
  "version": "0.1.0",
  "arch": "aarch64",
  "libc": "musl",
  "size": 1215992,
  "sha256": "...",
  "min_firmware": "EX520V stock",
  "created_at": "2026-08-23T12:00:00Z",
  "release_url": "https://github.com/<org>/<repo>/releases/download/v0.1.0/detectic-aarch64-musl"
}
```

## Update Flow

1. Operator places the new release directory on the router.
2. `detectic-update.sh /path/to/release` verifies the SHA256.
3. New release is copied into `releases/<version>/`.
4. Symlink `current` is atomically updated.
5. `detectic-health.sh` verifies the new version.
6. If it fails, `detectic-rollback.sh` reverts to `previous`.

## Reboot Persistence

| Mode | Status |
|------|--------|
| Manual start / operator-controlled | SUPPORTED |
| Auto-start after reboot on stock firmware | **NOT SUPPORTED** |

The deployment scripts do **not** patch `rcS`, do **not** modify `squashfs`,
and do **not** exploit the firmware. If a legitimate vendor/ISP startup hook
becomes available later, the install base structure is ready and a
`DETECTIC_START_MODE` wrapper can be added without changing the core binary.

## Verification

The install script checks:
- presence of `manifest.json`, binary, `.sha256`
- `file` reports `ARM aarch64`
- `file` reports `statically linked`
- SHA256 of the binary matches the `.sha256` file
