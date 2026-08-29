#!/bin/sh
# Phase 3 probe: Check autostart logs and Lifemote config persistence
trap '' 1
export PATH=$PATH:/bin:/usr/bin:/sbin:/usr/sbin
BB=/bin/busybox
BASE="http://192.168.0.27:8080"
OUT=/var/tmp/phase3_check.txt

{
echo "AUTOSTART_LOG:"
$BB cat /var/run/misc/misc_rw/detectic/autostart.log 2>/dev/null || echo "(none)"
echo ""
echo "DETECTIC_LOG_TAIL:"
$BB tail -30 /var/run/misc/misc_rw/detectic/detectic.log 2>/dev/null || echo "(none)"
echo ""
echo "SENSOR_LOG:"
$BB cat /var/run/misc/misc_rw/detectic/sensor_log.txt 2>/dev/null | tail -20 || echo "(none)"
echo ""
echo "LAUNCHER_SH_HEAD:"
$BB head -30 /var/run/misc/misc_rw/detectic/launcher.sh 2>/dev/null || echo "(none)"
echo ""
echo "VERSION:"
$BB cat /var/run/misc/misc_rw/detectic/version 2>/dev/null || echo "(none)"
echo ""
echo "RESTART_COUNT:"
$BB cat /var/run/misc/misc_rw/detectic/restart_count 2>/dev/null || echo "(none)"
echo ""
echo "DETECTIC_ENV_KEYS_ONLY (sensitive masked, values NEVER logged):"
$BB grep -E '^[A-Za-z0-9_]+=' /var/run/misc/misc_rw/detectic/detectic.env 2>/dev/null | \
  $BB cut -d'=' -f1 | \
  $BB sed -e 's/^DETECTIC_PASSWORD$/secret-key/' \
          -e 's/^DETECTIC_SECRET$/secret-key/' \
          -e 's/^DETECTIC_BACKEND_TOKEN$/secret-key/' \
          -e 's/^DETECTIC_SMTP_PASSWORD$/secret-key/' \
          -e 's/^DETECTIC_SMTP_USER$/secret-key/' \
          -e 's/^DETECTIC_D1_SYNC_URL$/secret-key/' \
          -e 's/^PASSWORD$/secret-key/' \
          -e 's/^SECRET$/secret-key/' || echo "(none)"
echo ""
echo "DETECTIC_PID:"
$BB cat /var/run/misc/misc_rw/detectic/detectic.pid 2>/dev/null || echo "(none)"
echo ""
echo "MISC_RW_BAK_LS:"
$BB ls -la /var/run/misc/misc_rw_bak/ 2>/dev/null | head -20 || echo "(none)"
echo ""
echo "MISC_RW_BAK_DETECTIC:"
$BB ls -la /var/run/misc/misc_rw_bak/detectic/ 2>/dev/null || echo "(none)"
echo ""
echo "PS_ALL:"
$BB ps 2>/dev/null
echo ""
echo "NETSTAT_TCP:"
$BB netstat -tlnp 2>/dev/null || echo "(failed)"
echo ""
echo "NETSTAT_UDP:"
$BB netstat -ulnp 2>/dev/null || echo "(failed)"
echo ""
echo "USERCONFIG_STRINGS_LIFEMOTE:"
$BB strings /var/run/misc/misc_rw/0x00300000 2>/dev/null | $BB grep -i 'lifemote\|phoenix\|agent.*url\|agent.*enable' | head -10 || echo "(none)"
echo ""
echo "UPTIME:"
$BB cat /proc/uptime 2>/dev/null
echo ""
echo "DMESG_TAIL:"
$BB dmesg 2>/dev/null | $BB tail -20 || echo "(none)"
} > "$OUT" 2>&1

# Upload each line
LINE=0
while IFS= read -r line; do
    ENCODED=$($BB echo -n "$line" | $BB sed 's/ /%20/g;s/&/%26/g;s/=/%3D/g;s/\//%2F/g;s/:/%3A/g;s/</%3C/g;s/>/%3E/g;s/|/%7C/g;s/(/%28/g;s/)/%29/g;s/;/%3B/g;s/"/%22/g;s/{/%7B/g;s/}/%7D/g' 2>/dev/null)
    $BB wget -q -T 5 -O /dev/null "${BASE}/env_line?n=${LINE}&d=${ENCODED}" 2>/dev/null || true
    LINE=$((LINE+1))
done < "$OUT"

$BB wget -q -T 5 -O /dev/null "${BASE}/done?status=ok&lines=${LINE}" 2>/dev/null || true
exit 0
