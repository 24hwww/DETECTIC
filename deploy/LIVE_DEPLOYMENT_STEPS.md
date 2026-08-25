# EX520 LIVE DEPLOYMENT — EXACT STEPS

## PREREQUISITES

1. Shell access to EX520 (Telnet or SSH)
2. The deployment package: `deploy/detectic-ex520.tar.gz`
3. This dev machine accessible from the EX520 network

---

## STEP 0: ESTABLISH ACCESS

### Option A: EX520 on this LAN (192.168.0.x)

If the EX520 is connected to the same LAN as this dev machine:
1. Find the EX520 IP: `nmap -sn 192.168.0.0/24` or check router admin
2. Ensure Telnet/SSH is enabled on EX520
3. Connect: `telnet <EX520_IP>` or `ssh root@<EX520_IP>`

### Option B: EX520 on different network

1. Connect EX520 to this dev machine's LAN
2. Or use VPN/remote access
3. Or use serial/UART connection

### Option C: Manual transfer

If no network access from dev machine to EX520:
1. Copy `deploy/detectic-ex520.tar.gz` to a USB drive
2. Or serve it from another machine on the EX520's LAN

---

## STEP 1: RECONNAISSANCE (read-only, no modifications)

Paste the recon script into the EX520 shell:

```bash
# On the dev machine, show the recon script:
cat deploy/recon.sh
# Copy the output and paste it into the EX520 Telnet/SSH session
```

Or transfer and run:
```bash
# If SCP available:
scp deploy/recon.sh root@<EX520_IP>:/tmp/
ssh root@<EX520_IP> "sh /tmp/recon.sh"
```

**Save the output.** This is the authoritative baseline.

---

## STEP 2: VERIFY MISC_RW

After reconnaissance, verify:
```bash
ls -la /var/run/misc/misc_rw/
df -h /var/run/misc/misc_rw/
# Test write:
echo "test" > /var/run/misc/misc_rw/.probe && cat /var/run/misc/misc_rw/.probe && rm /var/run/misc/misc_rw/.probe
# Test execute:
echo '#!/bin/sh' > /var/run/misc/misc_rw/.exec_test
echo 'echo OK' >> /var/run/misc/misc_rw/.exec_test
chmod +x /var/run/misc/misc_rw/.exec_test
sh /var/run/misc/misc_rw/.exec_test
rm /var/run/misc/misc_rw/.exec_test
```

Expected: write OK, execute OK.

---

## STEP 3: TRANSFER DEPLOYMENT PACKAGE

### Method A: SCP (if available)
```bash
scp deploy/detectic-ex520.tar.gz root@<EX520_IP>:/tmp/
```

### Method B: HTTP (serve from dev machine)
```bash
# On dev machine:
python3 -m http.server 8080 --directory deploy/package
# On EX520:
wget http://<DEV_IP>:8080/detectic/detectic -O /tmp/detectic
wget http://<DEV_IP>:8080/detectic/launcher.sh -O /tmp/launcher.sh
```

### Method C: Base64 over Telnet
```bash
# On dev machine, encode:
base64 deploy/package/detectic/detectic > /tmp/detectic.b64
# Split into chunks if needed:
split -b 4096 /tmp/detectic.b64 /tmp/detectic_chunk_
# On EX520, reassemble and decode:
cat > /tmp/detectic.b64 << 'ENDOFBASE64'
<paste base64 content here>
ENDOFBASE64
base64 -d /tmp/detectic.b64 > /tmp/detectic
chmod +x /tmp/detectic
```

---

## STEP 4: INSTALL

```bash
# Create directory
mkdir -p /var/run/misc/misc_rw/detectic/spool
mkdir -p /var/run/misc/misc_rw/detectic/state

# Copy binary
cp /tmp/detectic /var/run/misc/misc_rw/detectic/detectic
chmod +x /var/run/misc/misc_rw/detectic/detectic

# Copy launcher
cp /tmp/launcher.sh /var/run/misc/misc_rw/detectic/launcher.sh
chmod +x /var/run/misc/misc_rw/detectic/launcher.sh

# Verify
ls -la /var/run/misc/misc_rw/detectic/
sha256sum /var/run/misc/misc_rw/detectic/detectic
```

Expected SHA-256: `28f8dd0151dc307b9fb0e84b20142cb9098e862d02e0c133c508232e69b994e2`

---

## STEP 5: TEST EXECUTION

```bash
# Run Detectic manually (single poll)
cd /var/run/misc/misc_rw/detectic
DETECTIC_URL=http://192.168.0.1 \
DETECTIC_USER=admin \
DETECTIC_PASSWORD=<password> \
DETECTIC_SECRET=<secret> \
DETECTIC_INTERVAL=30 \
./detectic sensor --once

# Check if it produced output
echo "Exit code: $?"
```

---

## STEP 6: START VIA LAUNCHER

```bash
# Start Detectic via launcher
/var/run/misc/misc_rw/detectic/launcher.sh start

# Check status
/var/run/misc/misc_rw/detectic/launcher.sh status

# Check logs
cat /var/run/misc/misc_rw/detectic/detectic.log
```

---

## STEP 7: VERIFY NO ROUTER IMPACT

```bash
# Verify router services still running
ps | grep -E "cos|httpd|dnsmasq"
# Verify network
ip addr
# Verify Wi-Fi
iw dev
# Verify no config changes
cat /proc/version
```

---

## STEP 8: PERSISTENCE TEST (requires approval)

```bash
# Record current state
/var/run/misc/misc_rw/detectic/launcher.sh status
ls -la /var/run/misc/misc_rw/detectic/
sha256sum /var/run/misc/misc_rw/detectic/detectic

# Stop Detectic
/var/run/misc/misc_rw/detectic/launcher.sh stop

# REBOOT (requires explicit approval)
reboot

# After reboot, reconnect and verify:
ls -la /var/run/misc/misc_rw/detectic/
sha256sum /var/run/misc/misc_rw/detectic/detectic
/var/run/misc/misc_rw/detectic/launcher.sh status
```

---

## STEP 9: AUTOSTART (Phase 8)

Determine the autostart mechanism from reconnaissance results.
See Phase 8 in the main prompt for priority order.

---

## REMOVAL

```bash
/var/run/misc/misc_rw/detectic/launcher.sh stop
rm -rf /var/run/misc/misc_rw/detectic/
echo "Detectic removed"
```
