# PHASE13_AUTOSTART_SURFACES

Investigated surfaces:
1. /etc/init.d/rcS — ro, hardcoded
2. /etc/rcS_hook — ro, path hardcoded in /bin/rcsHook
3. Hotplug.d — ro scripts
4. cos supervisor — proprietary, no service registration API found
5. Config apply handlers — can launch dropbear/telnetd, fixed binaries
6. misc_rw — writable but no execution trigger

Conclusion:
No legitimate firmware-integrated autostart surface exists without modifying rootfs or rebuilding firmware.

All autostart mechanisms require firmware modification.

External launcher remains the only firmware-independent method.
