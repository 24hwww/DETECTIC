# PHASE13.6_INTEGRATION_OPTIONS.md

A. Original firmware + external launcher → PROVEN-OFFLINE, safe, deployable
B. Original firmware + legitimate config hook → UNKNOWN, no arbitrary exec proven
C. Original firmware + legitimate plugin → UNKNOWN
D. Vendor-supported extension → UNKNOWN
E. Vendor-signed modified firmware → UNKNOWN, no signing path
F. User-signed firmware → UNKNOWN
G. Rebuilt firmware with valid signing → UNKNOWN
H. Signature bypass → PROHIBITED

Scoring:
A: feasibility high, security high, risk none
B-H: feasibility unknown to impossible, risk medium to critical

Recommendation: A is only proven safe deployable.
