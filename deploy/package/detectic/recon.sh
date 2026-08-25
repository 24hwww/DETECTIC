#!/bin/sh
# EX520 Live Reconnaissance Script
# Paste this into a Telnet/SSH session on the EX520
# Collects all information needed for Phase 1
# NO modifications, NO reboots, NO destructive actions

echo "=========================================="
echo "  EX520 LIVE RECONNAISSANCE"
echo "  $(date)"
echo "=========================================="
echo ""

echo "=== 1. USER / PERMISSIONS ==="
id 2>/dev/null || echo "id: not available"
whoami 2>/dev/null || echo "whoami: not available"
echo ""

echo "=== 2. ARCHITECTURE ==="
uname -a 2>/dev/null || echo "uname: not available"
uname -m 2>/dev/null || echo "uname -m: not available"
echo ""

echo "=== 3. KERNEL ==="
cat /proc/version 2>/dev/null || echo "/proc/version: not available"
echo ""

echo "=== 4. CPU ==="
cat /proc/cpuinfo 2>/dev/null | head -20 || echo "/proc/cpuinfo: not available"
echo ""

echo "=== 5. MEMORY ==="
free 2>/dev/null || echo "free: not available"
cat /proc/meminfo 2>/dev/null | head -10 || echo "/proc/meminfo: not available"
echo ""

echo "=== 6. DISK / STORAGE ==="
df -h 2>/dev/null || echo "df: not available"
echo ""

echo "=== 7. MOUNT POINTS ==="
mount 2>/dev/null || echo "mount: not available"
echo ""

echo "=== 8. MTD (flash partitions) ==="
cat /proc/mtd 2>/dev/null || echo "/proc/mtd: not available"
echo ""

echo "=== 9. WRITABLE FILESYSTEMS ==="
echo "--- Testing write access ---"
for dir in /var/run/misc/misc_rw /var/tmp /tmp /var/log /var/run /tmp; do
    if [ -d "$dir" ]; then
        testfile="${dir}/.recon_test_$$"
        if touch "$testfile" 2>/dev/null; then
            rm -f "$testfile" 2>/dev/null
            echo "WRITABLE: $dir"
        else
            echo "READ-ONLY: $dir"
        fi
    else
        echo "NOT FOUND: $dir"
    fi
done
echo ""

echo "=== 10. EXECUTABLE FILESYSTEMS ==="
echo "--- Checking noexec mount option ---"
mount 2>/dev/null | grep -E "misc_rw|var/tmp|tmp " | while read line; do
    echo "$line"
done
echo ""

echo "=== 11. MISC_RW DEEP INSPECTION ==="
MISCRW="/var/run/misc/misc_rw"
if [ -d "$MISCRW" ]; then
    echo "Directory: $MISCRW"
    ls -la "$MISCRW" 2>/dev/null
    echo ""
    echo "Disk usage:"
    du -sh "$MISCRW" 2>/dev/null
    echo ""
    echo "Free space:"
    df -h "$MISCRW" 2>/dev/null
    echo ""
    echo "File count:"
    find "$MISCRW" -type f 2>/dev/null | wc -l
    echo ""
    echo "All files:"
    find "$MISCRW" -type f 2>/dev/null
    echo ""
    echo "Test execute from misc_rw:"
    testfile="${MISCRW}/.exec_test_$$"
    echo '#!/bin/sh' > "$testfile" 2>/dev/null
    echo 'echo "EXEC_OK"' >> "$testfile" 2>/dev/null
    chmod +x "$testfile" 2>/dev/null
    result=$(sh "$testfile" 2>/dev/null)
    rm -f "$testfile" 2>/dev/null
    if [ "$result" = "EXEC_OK" ]; then
        echo "EXECUTABLE from misc_rw: YES"
    else
        echo "EXECUTABLE from misc_rw: NO or UNKNOWN"
    fi
else
    echo "MISC_RW NOT FOUND at $MISCRW"
fi
echo ""

echo "=== 12. RUNNING PROCESSES ==="
ps 2>/dev/null || echo "ps: not available"
echo ""

echo "=== 13. NETWORK INTERFACES ==="
ip addr 2>/dev/null || ifconfig 2>/dev/null || echo "ip/ifconfig: not available"
echo ""

echo "=== 14. WIFI INTERFACES ==="
iw dev 2>/dev/null || echo "iw: not available"
iwinfo 2>/dev/null || echo "iwinfo: not available"
echo ""

echo "=== 15. LISTENING PORTS ==="
netstat -tuln 2>/dev/null || echo "netstat: not available"
echo ""

echo "=== 16. AVAILABLE COMMANDS ==="
for cmd in ps kill mkdir chmod mv cp sha256sum df mount sync cat ls killall pidof pgrep nohup; do
    if which "$cmd" >/dev/null 2>&1; then
        echo "AVAILABLE: $cmd ($(which $cmd))"
    else
        echo "MISSING: $cmd"
    fi
done
echo ""

echo "=== 17. BUSYBOX COMMANDS ==="
busybox 2>/dev/null | head -30 || echo "busybox: not available or no help"
echo ""

echo "=== 18. INIT SYSTEM ==="
cat /proc/1/cmdline 2>/dev/null | tr '\0' ' ' || echo "/proc/1/cmdline: not available"
echo ""
ls -la /etc/init.d/ 2>/dev/null || echo "/etc/init.d: not available"
echo ""

echo "=== 19. COS DAEMON ==="
ps 2>/dev/null | grep -E "cos|httpd|dnsmasq|telnetd|dropbear" || echo "No known daemons found"
echo ""

echo "=== 20. TELNET / SSH STATUS ==="
echo "Telnet ports:"
netstat -tuln 2>/dev/null | grep -E ":23 |:2323 " || echo "  No telnet ports listening"
echo "SSH ports:"
netstat -tuln 2>/dev/null | grep -E ":22 |:2222 " || echo "  No SSH ports listening"
echo ""

echo "=== 21. STARTUP SCRIPTS ==="
cat /etc/init.d/rcS 2>/dev/null | head -50 || echo "rcS: not available"
echo ""

echo "=== 22. RC_HOOK ==="
ls -la /etc/rcS_hook/ 2>/dev/null || echo "rcS_hook: not available"
echo ""

echo "=== 23. UBI INFO ==="
ubinfo -a 2>/dev/null || echo "ubinfo: not available"
echo ""

echo "=========================================="
echo "  RECONNAISSANCE COMPLETE"
echo "  $(date)"
echo "=========================================="
