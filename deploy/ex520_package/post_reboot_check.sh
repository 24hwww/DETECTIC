#!/bin/sh
# Check if Lifemote config persisted after reboot
trap '' 1
export PATH=$PATH:/bin:/usr/bin:/sbin:/usr/sbin
BB=/bin/busybox
BASE="http://192.168.0.27:8080"
OUT=/var/tmp/post_reboot_check.txt

{
echo "POST_REBOOT_UPTIME:"
$BB cat /proc/uptime 2>/dev/null
echo ""
echo "CLEAN_TEST_MARKER:"
$BB cat /var/run/misc/misc_rw/detectic/clean_test_marker.txt 2>/dev/null || echo "(none)"
echo ""
echo "AUTOSTART_LOG:"
$BB cat /var/run/misc/misc_rw/detectic/autostart.log 2>/dev/null || echo "(none)"
echo ""
echo "PS_PHOENIX:"
$BB ps 2>/dev/null | $BB grep -i phoenix || echo "(none)"
echo ""
echo "PS_LIFEMOTE:"
$BB ps 2>/dev/null | $BB grep -i lifemote || echo "(none)"
echo ""
echo "LIFEMOTE_DAEMON:"
$BB ls -la /tmp/lifemote_cpe_daemon.sh 2>/dev/null || echo "(none)"
echo ""
echo "COS_PID:"
$BB ps 2>/dev/null | $BB grep ' cos$' || $BB ps 2>/dev/null | $BB grep ' cos ' || echo "(none)"
echo ""
echo "USERCONFIG_GREP_CLEAN_TEST:"
$BB grep -c 'clean_test' /var/run/misc/misc_rw/0x00300000 2>/dev/null || echo "0"
echo ""
echo "USERCONFIG_GREP_192_168:"
$BB grep -c '192.168.0.27' /var/run/misc/misc_rw/0x00300000 2>/dev/null || echo "0"
echo ""
echo "USERCONFIG_GREP_LIFEMOTE:"
$BB grep -c 'lifemote' /var/run/misc/misc_rw/0x00300000 2>/dev/null || echo "0"
echo ""
echo "USERCONFIG_GREP_ENABLE:"
$BB grep -c 'enable' /var/run/misc/misc_rw/0x00300000 2>/dev/null || echo "0"
echo ""
echo "MISC_RW_DETECTIC_LS:"
$BB ls -la /var/run/misc/misc_rw/detectic/ 2>/dev/null || echo "(none)"
echo ""
echo "ALL_MISC_RW:"
$BB ls -la /var/run/misc/misc_rw/ 2>/dev/null | head -20
echo ""
echo "NETSTAT_TCP_8787:"
$BB netstat -tlnp 2>/dev/null | $BB grep 8787 || echo "(none)"
echo ""
echo "NETSTAT_UDP_5353:"
$BB netstat -ulnp 2>/dev/null | $BB grep 5353 || echo "(none)"
} > "$OUT" 2>&1

# Upload each line
LINE=0
while IFS= read -r line; do
    ENCODED=$($BB echo -n "$line" | $BB sed 's/ /%20/g;s/&/%26/g;s/=/%3D/g;s/\//%2F/g;s/:/%3A/g;s/</%3C/g;s/>/%3E/g;s/|/%7C/g;s/(/%28/g;s/)/%29/g;s/;/%3B/g;s/"/%22/g;s/{/%7B/g;s/}/%7D/g' 2>/dev/null)
    $BB wget -q -T 5 -O /dev/null "${BASE}/env_line?n=${LINE}&d=${ENCODED}" 2>/dev/null || true
    LINE=$((LINE+1))
done < "$OUT"

$BB wget -q -T 5 -O /dev/null "${BASE}/done?status=post_reboot_check&lines=${LINE}" 2>/dev/null || true
exit 0
