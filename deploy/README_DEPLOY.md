# Detectic — Deployment Guide

## Overview

Detectic is a Wi-Fi presence sensor for the TP-Link EX520V. It runs as a
statically-linked ARM64/musl binary on the router's writable `misc_rw`
partition, polls the GTPR/GDPR API for associated stations, optionally scans
nearby APs via `iwpriv get_site_survey`, fuses the observations, and sends
pseudonymized events to a backend.

## Build

```bash
# Router build (no SQLite, no TLS — smallest binary)
cargo build --release --no-default-features --target aarch64-unknown-linux-musl

# Host build (with SQLite persistence for development)
cargo build --features persist
```

## Release Artifacts

```
dist/
  detectic-aarch64-musl          # static ARM64 binary
  detectic-aarch64-musl.sha256   # SHA256 checksum
  manifest.json                  # release manifest
  install.sh                     # installer
  start.sh                       # start sensor
  stop.sh                        # stop sensor
  health.sh                      # health check
  update.sh                      # safe atomic update
  rollback.sh                    # revert to previous
  remove.sh                      # uninstall
```

## Install

1. Copy the `dist/` directory to the router (via the legitimate admin shell).
2. Run the installer:

```bash
cd /path/to/dist
./install.sh . /var/run/misc/misc_rw/detectic
```

The installer:
- Verifies architecture (ARM aarch64)
- Verifies ELF + static linking
- Verifies SHA256
- Generates a unique `sensor_id`
- Creates the install tree
- Installs the release into `releases/<version>/`
- Creates the `current` symlink
- Writes an install report

## Configure

Edit the generated config file:

```bash
vi /var/run/misc/misc_rw/detectic/config/detectic.env
```

Set at minimum:
- `DETECTIC_PASSWORD` — router admin password
- `DETECTIC_SECRET` — unique HMAC secret for this sensor

## Start

```bash
. /var/run/misc/misc_rw/detectic/config/detectic.env
/var/run/misc/misc_rw/detectic/current/start.sh
```

## Stop

```bash
/var/run/misc/misc_rw/detectic/current/stop.sh
```

## Health Check

```bash
/var/run/misc/misc_rw/detectic/current/health.sh
```

Or directly:

```bash
/var/run/misc/misc_rw/detectic/current/detectic health
```

## Update

```bash
# Copy new release directory to the router, then:
/var/run/misc/misc_rw/detectic/current/update.sh /var/run/misc/misc_rw/detectic /path/to/new-release
```

The update script:
1. Verifies SHA256, architecture, ELF
2. Stages the new release in `releases/<version>/`
3. Runs a health test (`detectic status`)
4. Atomically switches the `current` symlink
5. If the health test fails, cleans up and does NOT activate

## Rollback

```bash
/var/run/misc/misc_rw/detectic/current/rollback.sh
```

Reverts `current` to the `previous` release.

## Remove

```bash
/var/run/misc/misc_rw/detectic/current/remove.sh
```

Stops the sensor and removes the entire installation directory. Then disable
Telnet/Lifemote if they were enabled for deployment:

```bash
detectic set DEV2_TELNET_CFG '{"telnetLocalEnabled":"0","stack":"0,0,0,0,0,0","pstack":"0,0,0,0,0,0"}'
detectic set DEV2_LIFEMOTE_AGENT '{"enable":"0","URL":"","stack":"0,0,0,0,0,0","pstack":"0,0,0,0,0,0"}'
```

## CLI Commands

```
detectic status           # Print sensor configuration and resource usage
detectic map              # Collect and print the network map (JSON)
detectic presence         # Collect and print presence observations (JSON)
detectic sensor           # Run the continuous sensor loop
detectic sensor --once    # Poll once and exit
detectic health           # Print health snapshot (version, RSS, uptime, etc.)
detectic config           # Print effective configuration (secrets redacted)
detectic version          # Print version and architecture
detectic spool            # Show offline spool size and last entries
detectic update --check   # Check for available updates
detectic update           # Print update instructions
detectic rollback         # Print rollback instructions
detectic uninstall        # Print uninstall instructions
detectic query <oid>      # Query a single GTPR OID
detectic set <oid> <json> # Set fields on a GTPR OID
detectic op <oid>         # Trigger a GTPR action
```

## Reboot Persistence

**Auto-start after reboot is NOT supported on stock EX520V firmware.**

The stock firmware does not provide a user-accessible startup hook. After a
reboot, the sensor must be started manually with `start.sh`. No firmware
modification is performed.

If a vendor/ISP later provides a legitimate startup mechanism, the
`PersistentLauncher` abstraction (`src/persistence.rs`) can be extended
without changing the core sensor code.

## Resource Profile

| Metric | Value |
|--------|-------|
| Binary size | ~1.2 MB |
| RSS | ~1 MB |
| Threads | 1 |
| CPU idle | ~0% |
| Spool max | 256 KB (configurable) |

## Security

- No secrets in logs or `detectic config` output
- No raw MACs in backend events (HMAC-SHA256 pseudonymization)
- SHA256 verification required for all updates
- No remote script execution
- No firmware modification
- No privilege escalation
