# EX520 Operations Manual

## Daily monitoring

The Edge Supervisor and package server write logs locally.  Watch for:

```text
watchdog.log      (host supervisor state transitions)
package_server.log (download + callback history)
<package_dir>/done_log.txt (permanent /done callback record)
```

On the router, if accessible:

```bash
cat /var/run/misc/misc_rw/detectic/autostart.log
cat /var/run/misc/misc_rw/detectic/detectic.log
cat /var/run/misc/misc_rw/detectic/detectic.env  # 0600
```

## Sensor control

The persistent launcher script supports `start`, `stop`, `restart`, and `status`:

```bash
sh /var/run/misc/misc_rw/detectic/launcher.sh status
sh /var/run/misc/misc_rw/detectic/launcher.sh stop
sh /var/run/misc/misc_rw/detectic/launcher.sh start
```

A manual restart is bounded: after 5 consecutive restarts it refuses to keep
relaunching until the restart counter is cleared by a new bootstart run.

## Health HTTP endpoints

With the sensor running:

| Endpoint | Purpose |
|----------|---------|
| `GET /` | HTML status page |
| `GET /health` | `{status, sensor_id, version, uptime, gtpr, backend, mdns, devices, ready, port}` |
| `GET /ready` | readiness probe: `{ready, gtpr}` |
| `GET /version` | plain-text version |
| `GET /devices` | current associated station list (JSON) |
| `GET /devices/:id` | single device detail |
| `GET /events` | recent poll events (JSON) |
| `GET /metrics` | uptime, device count, last poll, statuses |

## Backup/restore

The only persistent state is in `/var/run/misc/misc_rw/detectic/`:

```text
launcher.sh      (0700)
detectic.env     (0600)
version
manifest.json
autostart.log
detectic.log
restart_count
state/
spool/
```

To back up:

```bash
tar czf detectic-backup-$(date +%Y%m%d).tar.gz -C /var/run/misc misc_rw/detectic
```

To restore:

```bash
tar xzf detectic-backup-*.tar.gz -C /var/run/misc
```

## Security reminders

* `detectic.env` is `chmod 600` and contains `DETECTIC_PASSWORD` and
  `DETECTIC_SECRET`.
* Never send the env file over insecure channels.
* Never commit `detectic.env` to git.
* The package server should be LAN-only.
* Do not expose TCP 8787 to the public internet.

## Troubleshooting

| Symptom | Check |
|---------|-------|
| Sensor not starting after reboot | Edge Supervisor logs, ping reachability, GTPR query, `/done_log.txt` |
| Sensor starts but `/health` says unhealthy | `autostart.log`, `detectic.log`, router credentials in `detectic.env` |
| Repeated Phoenix triggers | `min_boot_interval` (60s), sensor `ready` state, duplicate PID detection |
| mDNS not resolving | Multicast on the host, `detectic.local` scope, router multicast config |
| Corrupted binary | Re-run `build_package.sh`; `bootstart.sh` will reject bad hashes |
