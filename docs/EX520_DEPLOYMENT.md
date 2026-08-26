# EX520 Deployment Guide

## Overview

This guide covers the canonical, production-ready deployment of the DETECTIC
sensor on a stock TP-Link EX520 router.  It assumes the forensic investigation
has already established:

* No firmware modification is required.
* `GTPR so DEV2_LIFEMOTE_AGENT` triggers the stock `phoenix.sh`.
* Persistent storage is `/var/run/misc/misc_rw/detectic/`.
* The binary is too large for `misc_rw` and is downloaded to `/var/tmp/detectic/`
  after boot.

For the complete production architecture and acceptance matrix, see
`EX520_PRODUCTION_DEPLOYMENT.md`.

## Required hardware and software

* TP-Link EX520V (or verified compatible TP-Link EX series).
* Host machine on the same LAN with Python 3.
* `DETECTIC_PASSWORD` and `DETECTIC_SECRET` for the sensor.
* Optional: `DETECTIC_BACKEND_URL` for remote ingestion.

## Build

```bash
make router                 # cross-compiles target/aarch64-unknown-linux-musl/release/detectic
./deploy/ex520_package/build_package.sh
```

The package is created in `_fw_build/package/` and also as a tar.gz in the
project root.

## Configure

Copy the example environment file and edit it with real credentials:

```bash
cp deploy/ex520_package/detectic.env.example deploy/ex520_package/detectic.env
# edit deploy/ex520_package/detectic.env (never commit it)
```

Re-run `build_package.sh` so `detectic.env` is included in the package.

## Deploy

Copy the generated package into the package server directory:

```bash
cp _fw_build/package/* /path/to/package/server/
```

Start the package server and the host Edge Supervisor:

```bash
python3 deploy/ex520_package/package_server.py
DETECTIC_PASSWORD=<...> DETECTIC_SECRET=<...> python3 deploy/ex520_package/watchdog.py
```

The supervisor will:

1. Poll the router reachability.
2. Detect a cold boot (DOWN → UP).
3. Verify GTPR readiness.
4. Send `GTPR so DEV2_LIFEMOTE_AGENT {enable:1, URL:<package server bootstart.sh>}`.
5. Phoenix downloads `bootstart.sh` and starts the sensor.

## First manual trigger

If you want to trigger before a cold boot:

```bash
DETECTIC_PASSWORD=<...> ./target/debug/detectic --url "http://[fe80::<...>%25enp2s0]" \
    set DEV2_LIFEMOTE_AGENT "{enable:1, URL:http://<host>:8080/bootstart.sh}"
```

This sets the persistent Phoenix URL.  The supervisor will send `so` after the
next reboot.

## Verify

Once the sensor is running, the control plane is available:

```bash
curl http://192.168.0.1:8787/health        # or http://detectic.local:8787/health
curl http://192.168.0.1:8787/devices
curl http://192.168.0.1:8787/metrics
```

> **Note:** `detectic.local` requires mDNS to be reachable from the querying
> host and has only been validated locally in the dev environment, not on a live
> EX520.

## Rollback

```bash
DETECTIC_PASSWORD=<...> ./target/debug/detectic \
    --url "http://[fe80::<...>%25enp2s0]" \
    set DEV2_LIFEMOTE_AGENT "{enable:0, URL:}"
ssh root@ex520 rm -rf /var/run/misc/misc_rw/detectic
# power-cycle the router
```

This disables the Phoenix URL and removes the persistent DETECTIC directory.
