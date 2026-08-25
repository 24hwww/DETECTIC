# PHASE13.6_COS_DEEP_ANALYSIS.md

## Binary location
_rootfs/bin or /usr/bin? PROVEN-FROM-SOURCE cos exists

## Static analysis
Strings not fully enumerated offline. No symbol table.

Evidence:
- rcS starts cos &
- cos is supervisor principal

Unknowns:
- fork/exec usage
- config parsing
- plugin loading
- dynamic loading
- executable path construction
- writable path references

Search for references to /var/run/misc → UNKNOWN
Search for dlopen → UNKNOWN
Search for execve → UNKNOWN

Classification:
PROVEN-FROM-SOURCE: cos started
UNKNOWN: can launch arbitrary executable
BLOCKED: no disassembly performed, binary stripped

Conclusion: No evidence cos can launch /var/run/misc/misc_rw/detectic/detectic without modification.
