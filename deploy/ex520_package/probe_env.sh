#!/bin/sh
# Environment probe for EX520 - collects wireless tool output and sends via GET
# Runs as root from /usr/bin/phoenix.sh

trap '' 1
export PATH=$PATH:/bin:/usr/bin:/sbin:/usr/sbin
BB=/bin/busybox

BASE="http://192.168.0.27:8080"
TMPDIR="/var/tmp/env_probe"
OUT="$TMPDIR/output.txt"

$BB mkdir -p "$TMPDIR" 2>/dev/null
$BB rm -f "$OUT" 2>/dev/null

# Collect all environment info
{
echo "PROC_NET_WIRELESS:"
$BB cat /proc/net/wireless 2>/dev/null || echo "(none)"
echo ""
echo "IWLIST_RA0:"
$BB iwlist ra0 scan 2>/dev/null | head -40 || echo "(failed)"
echo ""
echo "IWLIST_RAI0:"
$BB iwlist rai0 scan 2>/dev/null | head -40 || echo "(failed)"
echo ""
echo "IWPRIV_RA0_STAT:"
$BB iwpriv ra0 stat 2>/dev/null | head -40 || echo "(failed)"
echo ""
echo "IWPRIV_RAI0_STAT:"
$BB iwpriv rai0 stat 2>/dev/null | head -40 || echo "(failed)"
echo ""
echo "IWCONFIG_RA0:"
$BB iwconfig ra0 2>/dev/null | head -20 || echo "(failed)"
echo ""
echo "IWCONFIG_RAI0:"
$BB iwconfig rai0 2>/dev/null | head -20 || echo "(failed)"
echo ""
echo "STAINFO:"
$BB cat /tmp/ai_roaming/ar_pat/staInfo 2>/dev/null | head -30 || echo "(none)"
echo ""
echo "VAR_TMP_LS:"
$BB ls -la /var/tmp/ 2>/dev/null
echo ""
echo "VAR_TMP_45:"
$BB ls -la /var/tmp/45 2>/dev/null || echo "(none)"
echo ""
echo "PS_NRD:"
$BB ps 2>/dev/null | $BB grep -i nrd || echo "(none)"
echo ""
echo "PROC_NET_NETLINK:"
$BB cat /proc/net/netlink 2>/dev/null || echo "(none)"
echo ""
echo "IFCONFIG:"
$BB ifconfig 2>/dev/null | head -40
echo ""
echo "IWLIST_RA0_ASSOC:"
$BB iwlist ra0 assoc 2>/dev/null | head -30 || echo "(failed)"
echo ""
echo "IWLIST_RAI0_ASSOC:"
$BB iwlist rai0 assoc 2>/dev/null | head -30 || echo "(failed)"
echo ""
echo "IWPRIV_RA0_GET_STA_INFO:"
$BB iwpriv ra0 get_sta_info 2>/dev/null | head -30 || echo "(failed)"
echo ""
echo "IWPRIV_RAI0_GET_STA_INFO:"
$BB iwpriv rai0 get_sta_info 2>/dev/null | head -30 || echo "(failed)"
echo ""
echo "PROC_NET_ARP:"
$BB cat /proc/net/arp 2>/dev/null || echo "(none)"
echo ""
echo "DHCP_LEASES:"
$BB cat /tmp/dhcp.leases 2>/dev/null | head -20 || echo "(none)"
$BB cat /var/lib/misc/dhcp.leases 2>/dev/null | head -20 || echo "(none2)"
} > "$OUT" 2>&1

# Upload each line via GET request with line number
LINE=0
while IFS= read -r line; do
    # URL-encode the line (basic encoding)
    ENCODED=$($BB echo -n "$line" | $BB sed 's/ /%20/g;s/&/%26/g;s/=/%3D/g;s/\//%2F/g;s/:/%3A/g;s/</%3C/g;s/>/%3E/g;s/|/%7C/g;s/(/%28/g;s/)/%29/g;s/;/%3B/g' 2>/dev/null)
    $BB wget -q -T 5 -O /dev/null "${BASE}/env_line?n=${LINE}&d=${ENCODED}" 2>/dev/null || true
    LINE=$((LINE+1))
done < "$OUT"

# Signal completion
$BB wget -q -T 5 -O /dev/null "${BASE}/done?status=ok&lines=${LINE}" 2>/dev/null || true
exit 0
