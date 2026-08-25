# PHASE13.6_CONFIG_EXECUTION_AUDIT.md

## Config formats
XML backupcfg, BBA config, TR-069 data model

## Findings
PROVEN-OFFLINE: backupcfg can enable Telnet/SSH → fixed binary launch
PROVEN-OFFLINE: data model apply handlers launch dropbear/telnetd
UNKNOWN: config field pointing to user-controlled executable path

No evidence of arbitrary command field.

Fixed executable pattern:
config → enable=true → /usr/sbin/telnetd

Arbitrary executable pattern:
config → path=/var/run/misc/... → exec(path)
Not proven.

Classification:
PROVEN-OFFLINE: fixed executable launch via config
UNKNOWN: arbitrary executable launch
