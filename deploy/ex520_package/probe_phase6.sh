#!/bin/sh
# Phase 6 probe: investigate alternatives for Wi-Fi data on EX520
# - Read nrd.conf
# - Try iwpriv/iwlist with correct interface names (rai0, rax0, apclii0, apclix0)
# - Read nrd's IPC socket info
# - Try to send a message to /var/tmp/45
# - Check /proc files for station info
# - Read map.conf, tpVendorConf.json

trap '' 1
export PATH=$PATH:/bin:/usr/bin:/sbin:/usr/sbin
BB=/bin/busybox

BASE="http://192.168.0.27:8080"
TMPDIR="/var/tmp/phase6_probe"
OUT="$TMPDIR/output.txt"

$BB mkdir -p "$TMPDIR" 2>/dev/null
$BB rm -f "$OUT" 2>/dev/null

{
echo "NRD_CONF:"
$BB cat /var/tmp/nrd.conf 2>/dev/null || echo "(none)"
echo ""
echo "IWCONFIG_ALL:"
$BB iwconfig 2>/dev/null | head -60 || echo "(failed)"
echo ""
echo "IWLIST_RAI0_SCAN:"
$BB iwlist rai0 scan 2>/dev/null | head -40 || echo "(failed)"
echo ""
echo "IWLIST_RAX0_SCAN:"
$BB iwlist rax0 scan 2>/dev/null | head -40 || echo "(failed)"
echo ""
echo "IWPRIV_RAI0:"
$BB iwpriv rai0 2>/dev/null | head -60 || echo "(failed)"
echo ""
echo "IWPRIV_RAX0:"
$BB iwpriv rax0 2>/dev/null | head -60 || echo "(failed)"
echo ""
echo "IWPRIV_RAI0_STAT:"
$BB iwpriv rai0 stat 2>/dev/null | head -40 || echo "(failed)"
echo ""
echo "IWPRIV_RAX0_STAT:"
$BB iwpriv rax0 stat 2>/dev/null | head -40 || echo "(failed)"
echo ""
echo "IWPRIV_RAI0_SHOW:"
$BB iwpriv rai0 show 2>/dev/null | head -40 || echo "(failed)"
echo ""
echo "IWPRIV_RAX0_SHOW:"
$BB iwpriv rax0 show 2>/dev/null | head -40 || echo "(failed)"
echo ""
echo "IWLIST_RAI0_ASSOC:"
$BB iwlist rai0 assoc 2>/dev/null || echo "(failed)"
echo ""
echo "IWLIST_RAX0_ASSOC:"
$BB iwlist rax0 assoc 2>/dev/null || echo "(failed)"
echo ""
echo "IWLIST_APCLII0_SCAN:"
$BB iwlist apclii0 scan 2>/dev/null | head -30 || echo "(failed)"
echo ""
echo "IWLIST_APCLIX0_SCAN:"
$BB iwlist apclix0 scan 2>/dev/null | head -30 || echo "(failed)"
echo ""
echo "PROC_NET_WIRELESS:"
$BB cat /proc/net/wireless 2>/dev/null
echo ""
echo "MAP_CONF:"
$BB cat /var/tmp/map.conf 2>/dev/null | head -80 || echo "(none)"
echo ""
echo "TP_VENDOR_CONF:"
$BB cat /var/tmp/tpVendorConf.json 2>/dev/null || echo "(none)"
echo ""
echo "TP_AGENT_CONF:"
$BB cat /var/tmp/tpAgentConf.json 2>/dev/null || echo "(none)"
echo ""
echo "EASYMESH_AL_MAC:"
$BB cat /var/tmp/easyMesh_agent_AL_MAC 2>/dev/null || echo "(none)"
echo ""
echo "EASYMESH_WORKMODE:"
$BB cat /var/tmp/easyMesh_workmode 2>/dev/null || echo "(none)"
echo ""
echo "STAINFO:"
$BB cat /tmp/ai_roaming/ar_pat/staInfo 2>/dev/null || echo "(none)"
echo ""
echo "LS_AI_ROAMING:"
$BB ls -la /tmp/ai_roaming/ 2>/dev/null || echo "(none)"
$BB ls -la /tmp/ai_roaming/ar_pat/ 2>/dev/null || echo "(none2)"
echo ""
echo "PROC_NET_ARP:"
$BB cat /proc/net/arp 2>/dev/null
echo ""
echo "CLIENT_LINK_PREFER:"
$BB cat /var/tmp/clientLinkPreferInfo 2>/dev/null || echo "(none)"
echo ""
echo "ALL_IFACES:"
$BB cat /proc/net/dev 2>/dev/null
echo ""
echo "LS_PROC_NRD:"
$BB ls -la /proc/2743/fd/ 2>/dev/null | head -30 || echo "(none)"
echo ""
echo "PROC_NRD_NETLINK:"
$BB ls -la /proc/2743/fd/ 2>/dev/null | $BB grep socket | head -10 || echo "(none)"
} > "$OUT" 2>&1

# Upload each line via GET request
LINE=0
while IFS= read -r line; do
    ENCODED=$($BB echo -n "$line" | $BB sed 's/ /%20/g;s/&/%26/g;s/=/%3D/g;s/\//%2F/g;s/:/%3A/g;s/</%3C/g;s/>/%3E/g;s/|/%7C/g;s/(/%28/g;s/)/%29/g;s/;/%3B/g;s/"/%22/g;s/{/%7B/g;s/}/%7D/g' 2>/dev/null)
    $BB wget -q -T 5 -O /dev/null "${BASE}/env_line?n=${LINE}&d=${ENCODED}" 2>/dev/null || true
    LINE=$((LINE+1))
done < "$OUT"

$BB wget -q -T 5 -O /dev/null "${BASE}/done?status=ok&lines=${LINE}" 2>/dev/null || true
exit 0
