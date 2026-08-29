#!/bin/sh
# Check if Lifemote config is in userconfig binary
trap '' 1
export PATH=$PATH:/bin:/usr/bin:/sbin:/usr/sbin
BB=/bin/busybox
BASE="http://192.168.0.27:8080"
OUT=/var/tmp/userconfig_check.txt

{
echo "USERCONFIG_HEX_LIFEMOTE:"
# Search for "lifemote" in hex in the userconfig
$BB xxd /var/run/misc/misc_rw/0x00300000 2>/dev/null | $BB grep -i 'life\|phoen\|agent' | head -10 || echo "(none)"
echo ""
echo "USERCONFIG_GREP_LIFEMOTE:"
# Search for the URL we set
$BB grep -c '192.168.0.27' /var/run/misc/misc_rw/0x00300000 2>/dev/null || echo "0"
echo ""
echo "USERCONFIG_GREP_PHOENIX:"
$BB grep -c 'phoenix' /var/run/misc/misc_rw/0x00300000 2>/dev/null || echo "0"
echo ""
echo "USERCONFIG_SIZE:"
$BB wc -c /var/run/misc/misc_rw/0x00300000 2>/dev/null
echo ""
echo "USERCONFIG_MD5:"
$BB md5sum /var/run/misc/misc_rw/0x00300000 2>/dev/null || echo "(none)"
echo ""
echo "MISC_RW_BAK_USERCONFIG:"
$BB ls -la /var/run/misc/misc_rw_bak/0x003C0000 2>/dev/null || echo "(none)"
echo ""
echo "MISC_RW_BAK_USERCONFIG_GREP:"
$BB grep -c '192.168.0.27' /var/run/misc/misc_rw_bak/0x003C0000 2>/dev/null || echo "0"
echo ""
echo "MISC_RW_BAK_USERCONFIG_MD5:"
$BB md5sum /var/run/misc/misc_rw_bak/0x003C0000 2>/dev/null || echo "(none)"
echo ""
echo "ALL_MISC_RW_FILES:"
$BB find /var/run/misc/misc_rw/ -type f 2>/dev/null | head -20
echo ""
echo "ALL_MISC_RW_BAK_FILES:"
$BB find /var/run/misc/misc_rw_bak/ -type f 2>/dev/null | head -20
echo ""
echo "BOOTSTART_LOG:"
$BB cat /var/run/misc/misc_rw/detectic/autostart.log 2>/dev/null || echo "(none)"
echo ""
echo "DETECTIC_LOG_FIRST_LINES:"
$BB head -10 /var/run/misc/misc_rw/detectic/detectic.log 2>/dev/null || echo "(none)"
echo ""
echo "DETECTIC_LOG_LAST_LINES:"
$BB tail -10 /var/run/misc/misc_rw/detectic/detectic.log 2>/dev/null || echo "(none)"
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
