# M4.1-D/E — Detectic Runtime and Local GDPR Test

## M4.1-D — Detectic execution proof

**Not performed.** No legitimate shell access to the EX520V is available.

The following steps could not be carried out:

1. Copy `detectic` to `/var/run/misc/misc_rw/`
2. `chmod +x detectic`
3. `detectic --help`
4. `detectic map`
5. Verify GDPR authentication, TokenID, `gl`, collection, NetworkMap, clean exit

## M4.1-E — Local GDPR vs external GDPR

**Not tested at runtime.** However, from previous milestones:

- The GTPR/GDPR pipeline has been successfully tested against the real router
  from an external host.
- `DEV2_WIFI_APDEV_ASSOCDEV` works on the real router.
- DHCP and HOST_ENTRY work.
- The router's HTTP endpoint is accessible on the LAN.

The question of whether `127.0.0.1` works from inside the router remains
**unverified**. Some embedded web servers bind only to the LAN interface, not
to loopback. This would need to be tested with shell access.

## What is known

The Detectic binary includes the full GTPR client, collector, and publisher.
The `--no-default-features` build preserves all sensor functionality (GTPR,
collector, crypto, events, analytics, publisher). Only SQLite persistence and
SMTP notifications are excluded.

The binary's CLI supports:

```bash
detectic map          # collect and print network map
detectic watch        # continuous monitoring
detectic upload       # upload to backend
detectic --help       # help
```

## Conclusion

Detectic runtime execution and local GDPR access remain **unverified** due to
the lack of shell access. The binary is built and architecturally compatible,
but no runtime proof exists.
