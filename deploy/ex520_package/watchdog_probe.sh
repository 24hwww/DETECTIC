#!/bin/sh
CB="http://192.168.0.27:8080"
BB=/bin/busybox
$BB echo "===== /dev/watchdog =====" > /tmp/wd.txt
$BB ls -la /dev/watchdog 2>&1 >> /tmp/wd.txt || $BB echo "(missing)" >> /tmp/wd.txt
$BB echo "===== /sbin/watchdog =====" >> /tmp/wd.txt
$BB ls -la /sbin/watchdog 2>&1 >> /tmp/wd.txt || $BB echo "(missing)" >> /tmp/wd.txt
$BB echo "===== /sbin/hotplug =====" >> /tmp/wd.txt
$BB ls -la /sbin/hotplug 2>&1 >> /tmp/wd.txt || $BB echo "(missing)" >> /tmp/wd.txt
$BB echo "===== /etc/hotplug2.rules =====" >> /tmp/wd.txt
$BB cat /etc/hotplug2.rules >> /tmp/wd.txt
$BB echo "===== /etc/inittab =====" >> /tmp/wd.txt
$BB cat /etc/inittab >> /tmp/wd.txt
$BB echo "===== /etc/init.d/rcS last 30 =====" >> /tmp/wd.txt
$BB tail -30 /etc/init.d/rcS >> /tmp/wd.txt
/usr/sbin/curl -m 10 -T /tmp/wd.txt "${CB}/probe_log?tag=watchdog_probe" 2>/dev/null || true
