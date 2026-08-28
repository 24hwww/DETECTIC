#!/bin/sh
# Detectic hardened bootstrap for EX520V.
# Runs as root from /usr/bin/phoenix.sh.
#
# This script downloads the Detectic package, verifies SHA-256 checksums,
# reassembles the binary, and starts the sensor.  It never executes an
# unverified binary and never modifies stock firmware.
#
# The binary is split into 1 MiB parts (detectic.aa, .ab, .ac, ...).  The
# manifest.json lists every part and the full-binary checksum.  This script
# downloads every part that appears in the manifest, verifies it, and then
# concatenates them in sorted order.

# Survive SIGHUP + SIGTERM after phoenix/cos exits.
trap '' 1 15

export PATH=$PATH:/bin:/usr/bin:/sbin:/usr/sbin
BB=/bin/busybox

# Remove stale backend environment inherited from a previous phoenix run.
unset DETECTIC_BACKEND_URL DETECTIC_UPLOAD_URL DETECTIC_BACKEND_TOKEN 2>/dev/null

# Base URL and directories.
BASE="${DETECTIC_PACKAGE_URL:-http://192.168.0.27:8080}"
DIR="/var/run/misc/misc_rw/detectic"
TMPDIR="/var/tmp/detectic"
TMPPKG="/var/tmp/detectic_pkg"
LOG="$DIR/autostart.log"

# Uptime helper (no date(1) available).
up() { read u _ < /proc/uptime; echo "$u"; }

log() { echo "[$(up)] $*" >> "$LOG" 2>/dev/null; }

err() {
    log "ERROR: $*"
    echo "ERROR: $*" 1>&2
    # Best-effort failure callback; never abort the phoenix loop.
    # Replace spaces with underscores so the URL stays valid.
    _reason="$(echo "$*" | $BB tr ' ' '_')"
    $BB wget -q -T 5 -O /dev/null \
        "${BASE}/done?status=fail&reason=${_reason}" 2>/dev/null || true
    # Also ship the last 20 log lines as env_line entries (GET works on more
    # BusyBox wget builds than --post-file).
    if [ -f "$LOG" ]; then
        _n=0
        $BB tail -n 20 "$LOG" 2>/dev/null | while IFS= read -r _line; do
            _enc="$(echo "$_line" | $BB tr ' ' '_' | $BB head -c 300)"
            $BB wget -q -T 5 -O /dev/null \
                "${BASE}/env_line?n=${_n}&d=${_enc}" 2>/dev/null || true
            _n=$((_n + 1))
        done
    fi
    exit 0
}

# --- Bounded autostart log ---
if [ -f "$LOG" ]; then
    $BB tail -c 51200 "$LOG" > "$LOG.tmp" 2>/dev/null
    $BB rm -f "$LOG" 2>/dev/null
    $BB cp "$LOG.tmp" "$LOG" 2>/dev/null || true
    $BB rm -f "$LOG.tmp" 2>/dev/null || true
fi

# --- Prepare directories ---
$BB rm -rf "$TMPPKG" 2>/dev/null || true
$BB mkdir -p "$DIR" "$TMPPKG" "$TMPDIR" 2>/dev/null || \
    err "cannot_create_dirs"
$BB chmod 700 "$DIR" 2>/dev/null || log "chmod_dir_failed"
cd "$TMPPKG" 2>/dev/null || err "cannot_cd_tmppkg"

# --- Download helper with retry-like single attempt ---
fetch() {
    _url="$1"
    _out="$2"
    _timeout="${3:-60}"
    if ! $BB wget -q -T "$_timeout" -O "$_out" "$_url" 2>/dev/null; then
        return 1
    fi
    if [ ! -s "$_out" ]; then
        return 1
    fi
    return 0
}

# --- SHA-256 verification helper ---
# Computes SHA-256 of FILE and compares it to the expected value in CSUM_FILE.
verify_sha256() {
    _file="$1"
    _csum_file="$2"
    _name="$3"

    if [ ! -f "$_file" ]; then
        err "verify_missing_$_name"
    fi
    if [ ! -f "$_csum_file" ]; then
        err "verify_missing_checksum_$_name"
    fi

    _expected="$($BB cat "$_csum_file" 2>/dev/null | $BB awk 'NR==1{print; exit}' 2>/dev/null)"

    # Try to hash with available tools.  The EX520V BusyBox sha256sum applet
    # is listed but does not produce output, so fall back to /usr/sbin/openssl.
    _got=""
    _raw_got=""
    _openssl_test="$($BB which openssl 2>/dev/null)"
    if [ -n "$_openssl_test" ] && [ -x "$_openssl_test" ]; then
        _raw_got="$($_openssl_test dgst -sha256 "$_file" 2>/dev/null)"
        _got="$(echo "$_raw_got" | $BB awk '{print $NF}' 2>/dev/null)"
    fi
    if [ -z "$_got" ]; then
        _raw_got="$($BB sha256sum "$_file" 2>/dev/null)"
        _got="$(echo "$_raw_got" | $BB awk '{print $1}' 2>/dev/null)"
    fi
    [ -z "$_got" ] && _got="$(echo "$_raw_got" | $BB cut -d' ' -f1 2>/dev/null)"

    log "verify_debug_$_name expected_len=${#_expected} got_len=${#_got}"

    if [ -z "$_expected" ] || [ -z "$_got" ]; then
        log "verify_empty_hash_details expected='$_expected' raw_got='$_raw_got' got='$_got'"
        # Include diagnostics in the done callback URL (truncated to keep it valid).
        _details="$(echo "exp=${_expected}:got=${_got}:raw=${_raw_got}" | $BB tr ' ' '_' | $BB head -c 200)"
        err "verify_empty_hash_$_name:$_details"
    fi

    if [ "$_expected" != "$_got" ]; then
        log "sha256 mismatch $_name expected=$_expected got=$_got"
        err "verify_sha256_mismatch_$_name"
    fi

    log "sha256 ok $_name $_got"
}

# --- Download and verify package metadata ---
log "bootstart start base=$BASE version=$(cat "$DIR/version" 2>/dev/null || echo unknown)"
log "busybox_applets=$($BB --list 2>/dev/null | $BB tr '\n' ',' | $BB head -c 500)"

fetch "${BASE}/manifest.json"        "manifest.json"        10 || err "download_manifest"
fetch "${BASE}/detectic.sha256"      "detectic.sha256"      10 || err "download_bin_csum"
fetch "${BASE}/version"              "version"              10 || err "download_version"

# --- Validate manifest matches the expected version and part hashes ---
# Use a portable, BusyBox-safe parser: grab values from the JSON file.
MANIFEST_VERSION="$($BB sed -n 's/.*"version"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' manifest.json)"
MANIFEST_FULL="$($BB sed -n 's/.*"detectic"[[:space:]]*:[[:space:]]*"\([0-9a-fA-F]*\)".*/\1/p' manifest.json)"

[ -n "$MANIFEST_VERSION" ] || err "manifest_version_missing"
[ -n "$MANIFEST_FULL" ]    || err "manifest_full_hash_missing"

# The downloaded "version" file must match the manifest (self-consistency).
EXPECTED_VERSION="$($BB cat version 2>/dev/null)"
[ -n "$EXPECTED_VERSION" ] || err "version_file_empty"
if [ "$MANIFEST_VERSION" != "$EXPECTED_VERSION" ]; then
    log "manifest_version_mismatch expected=$EXPECTED_VERSION got=$MANIFEST_VERSION"
    err "manifest_version_mismatch"
fi

# Extract the list of split parts from the manifest.  Parts are keys of the
# form "detectic.XX" where XX is a lowercase suffix (aa, ab, ac, ...).
# The manifest is a compact single-line JSON, so expand commas to newlines
# so the sed pattern can match one entry at a time.
PARTS="$($BB tr ',' '\n' < manifest.json | $BB sed -n 's/.*"\(detectic\.[a-z][a-z]*\)"[[:space:]]*:[[:space:]]*"\([0-9a-fA-F]*\)".*/\1 \2/p')"
[ -n "$PARTS" ] || err "manifest_parts_missing"

# Build an ordered list of part names (aa, ab, ...) and a map name->hash.
PART_NAMES=""
PART_LIST_FILE="$TMPPKG/.part_list"
$BB rm -f "$PART_LIST_FILE" 2>/dev/null

# shellcheck disable=SC3011
set -- $PARTS
while [ $# -ge 2 ]; do
    _name="$1"
    _hash="$2"
    shift 2
    case "$_name" in
        detectic.??)
            ;;
        *)
            continue
            ;;
    esac
    # Verify hash is 64 hex chars.
    [ "${#_hash}" -eq 64 ] || continue
    log "part_parsed name=$_name hash_len=${#_hash} hash_prefix=${_hash%${_hash#????????}}"
    # Download the checksum file for this part (required by verify_sha256).
    fetch "${BASE}/${_name}.sha256" "${_name}.sha256" 10 || err "download_csum_${_name}"
    _dl="$($BB cat "${_name}.sha256" 2>/dev/null | $BB awk 'NR==1{print; exit}')"
    log "csum_downloaded name=$_name dl_len=${#_dl}"
    # Write the expected hash into the checksum file so verify_sha256 matches.
    # Use echo instead of printf; the BusyBox/ash printf built-in on this
    # firmware does not expand "%s\n" correctly for long hex strings.
    echo "$_hash" > "${_name}.sha256"
    _written="$($BB cat "${_name}.sha256" 2>/dev/null | $BB awk 'NR==1{print; exit}')"
    log "csum_written name=$_name written_len=${#_written}"
    PART_NAMES="$PART_NAMES $_name"
    echo "$_name" >> "$PART_LIST_FILE"
done

[ -n "$PART_NAMES" ] || err "no_valid_parts_in_manifest"

log "manifest_ok version=$MANIFEST_VERSION parts=[$PART_NAMES]"

# --- Download and verify binary parts ---
for _name in $PART_NAMES; do
    fetch "${BASE}/$_name" "$_name" 180 || err "download_${_name}"
    verify_sha256 "$_name" "${_name}.sha256" "$_name"
    $BB cp "$_name" "$TMPDIR/$_name" 2>/dev/null || err "copy_${_name}_vartmp"
done

# --- Download launcher and env (small files, optional env) ---
fetch "${BASE}/launcher.sh"          "launcher.sh"          30 || err "download_launcher"
$BB chmod +x "launcher.sh"

fetch "${BASE}/detectic.env"         "detectic.env"         15
if [ -f "detectic.env" ]; then
    # Detectic.env must be owner-only readable.
    $BB chmod 600 "detectic.env"
fi

# --- Copy persistent files into misc_rw ---
$BB cp "launcher.sh"   "$DIR/launcher.sh"   2>/dev/null || log "copy_launcher_failed"
$BB cp "version"       "$DIR/version"       2>/dev/null || log "copy_version_failed"
$BB cp "manifest.json" "$DIR/manifest.json" 2>/dev/null || log "copy_manifest_failed"
if [ -f "detectic.env" ]; then
    # Remove stale env files before writing new ones.
    $BB rm -f "$DIR/detectic.env" "$TMPDIR/detectic.env" "$DIR/.env" "$TMPDIR/.env" 2>/dev/null || true
    $BB cp "detectic.env" "$DIR/detectic.env" 2>/dev/null || log "copy_env_failed"
    $BB cp "detectic.env" "$DIR/.env" 2>/dev/null || log "copy_dotenv_failed"
    # Also copy to /var/tmp/detectic/ because launcher.sh prefers that path.
    $BB cp "detectic.env" "$TMPDIR/detectic.env" 2>/dev/null || log "copy_env_tmp_failed"
    $BB cp "detectic.env" "$TMPDIR/.env" 2>/dev/null || log "copy_dotenv_tmp_failed"
    # Ensure the persisted copy is also 600.
    $BB chmod 600 "$DIR/detectic.env" "$DIR/.env" 2>/dev/null || true
    $BB chmod 600 "$TMPDIR/detectic.env" "$TMPDIR/.env" 2>/dev/null || true
    # Diagnostic: send the BACKEND_URL line from the env file.
    _be_line="$($BB grep '^DETECTIC_BACKEND_URL=' detectic.env 2>/dev/null | $BB head -c 200)"
    _enc="$(echo "$_be_line" | $BB tr ' ' '_')"
    $BB wget -q -T 5 -O /dev/null "${BASE}/env_line?n=70&d=env_file_${_enc}" 2>/dev/null || true
    # Also send what's actually in the copied file on the router.
    _be_line2="$($BB grep '^DETECTIC_BACKEND_URL=' "$TMPDIR/detectic.env" 2>/dev/null | $BB head -c 200)"
    _enc="$(echo "$_be_line2" | $BB tr ' ' '_')"
    $BB wget -q -T 5 -O /dev/null "${BASE}/env_line?n=69&d=env_tmpdir_${_enc}" 2>/dev/null || true
fi

# --- Stop any running instance and remove old binary ---
# First, aggressively kill ANY detectic process (old launcher may lack aggressive kill).
for _proc in /proc/[0-9]*/cmdline; do
    if $BB grep -ql detectic "$_proc" 2>/dev/null; then
        _spid="$($BB echo "$_proc" | $BB sed 's|/proc/||;s|/cmdline||')"
        $BB kill -9 "$_spid" 2>/dev/null || true
    fi
done
$BB sleep 1
$BB sh "$DIR/launcher.sh" stop 2>/var/tmp/launcher.trace || true
$BB rm -f "$TMPDIR/detectic" "$TMPDIR/detectic.tmp" 2>/dev/null || true

# --- Reassemble parts in the order they appeared in the manifest ---
# The manifest is built in sorted order, and BusyBox on this firmware does
# not include the `sort` applet, so rely on the parsed order.
SORTED_PARTS="$($BB cat "$PART_LIST_FILE" 2>/dev/null)"
[ -n "$SORTED_PARTS" ] || err "part_sort_failed"

$BB rm -f "$TMPDIR/detectic.tmp" 2>/dev/null
for _name in $SORTED_PARTS; do
    $BB cat "$TMPDIR/$_name" >> "$TMPDIR/detectic.tmp" 2>/dev/null || err "reassemble_failed_${_name}"
done

verify_sha256 "$TMPDIR/detectic.tmp" "detectic.sha256" "full"

log "bin_info old_sha=$($BB openssl dgst -sha256 "$TMPDIR/detectic" 2>/dev/null | $BB awk '{print $NF}') new_sha=$($BB openssl dgst -sha256 "$TMPDIR/detectic.tmp" 2>/dev/null | $BB awk '{print $NF}')"

$BB chmod +x "$TMPDIR/detectic.tmp"

# BusyBox on the EX520V does not include `mv` (the `mv` applet is missing),
# so use cp+rm instead.  Remove the old binary first so `cp` can replace an
# in-use executable (avoids "text file busy" on some kernels).
$BB rm -f "$TMPDIR/detectic" 2>/dev/null || true
$BB cp -f "$TMPDIR/detectic.tmp" "$TMPDIR/detectic" 2>/dev/null || \
    err "copy_detectic_to_runtime_failed"
$BB rm -f "$TMPDIR/detectic.tmp" 2>/dev/null || true

# --- Cleanup download cache ---
$BB rm -rf "$TMPPKG"

# --- Quick binary test: run the `version` subcommand to verify it executes ---
_test_out="$($TMPDIR/detectic version 2>&1)" || true
_enc="$(echo "$_test_out" | $BB tr ' ' '_' | $BB head -c 300)"
$BB wget -q -T 5 -O /dev/null "${BASE}/env_line?n=99&d=bin_test_${_enc}" 2>/dev/null || true

# --- Start sensor in background (survives bootstart exit) ---
( trap '' 1 2 15; $BB sh "$DIR/launcher.sh" start 2>/var/tmp/launcher.trace >> "$LOG" 2>&1 ) &
ret=$?
$BB sleep 5

# Check if the sensor process is still alive after 5 seconds
_sensor_pid="$($BB cat "$DIR/detectic.pid" 2>/dev/null || echo none)"
if [ "$_sensor_pid" != "none" ] && $BB kill -0 "$_sensor_pid" 2>/dev/null; then
    _enc="sensor_alive_pid=${_sensor_pid}"
    $BB wget -q -T 5 -O /dev/null "${BASE}/env_line?n=98&d=${_enc}" 2>/dev/null || true
    _port_check="$($BB netstat -tln 2>/dev/null | $BB grep 8787 || echo not_listening)"
    _enc="$(echo "$_port_check" | $BB tr ' ' '_' | $BB head -c 300)"
    $BB wget -q -T 5 -O /dev/null "${BASE}/env_line?n=97&d=port_${_enc}" 2>/dev/null || true
else
    _enc="sensor_dead_pid=${_sensor_pid}"
    $BB wget -q -T 5 -O /dev/null "${BASE}/env_line?n=98&d=${_enc}" 2>/dev/null || true
fi

# Dump last 15 lines of detectic.log via GET callbacks (POST doesn't work on this BusyBox).
$BB sleep 3
_n=80
$BB tail -n 15 "$DIR/detectic.log" 2>/dev/null | while IFS= read -r _line; do
    _enc="$(echo "$_line" | $BB tr ' ' '_' | $BB tr '\n' ' ' | $BB head -c 300)"
    $BB wget -q -T 5 -O /dev/null "${BASE}/env_line?n=${_n}&d=${_enc}" 2>/dev/null || true
    _n=$((_n + 1))
done

# Targeted grep for the sensor LIFECYCLE markers (start, server bind, exit,
# restart, panic). These are the lines that tell us whether the sensor ran,
# bound :8787, and whether the launcher restart loop is working.
# detectic.log persists across reboots, so match the current boot's uptime range
# (we only need the newest occurrences). n=120+ is collision-free.
_n=120
$BB grep -aE 'http_server_started|http_server_bind_failed|sensor_exited|restarted PID|restart_failed|launcher_exited|started PID|panic|Starting Detectic|service_started|service_stopped|watchdog_failed' "$DIR/detectic.log" 2>/dev/null | $BB tail -n 12 | while IFS= read -r _line; do
    _enc="$(echo "$_line" | $BB tr ' ' '_' | $BB tr '\n' ' ' | $BB head -c 300)"
    $BB wget -q -T 5 -O /dev/null "${BASE}/env_line?n=${_n}&d=${_enc}" 2>/dev/null || true
    _n=$((_n + 1))
done

# mDNS diagnostic: grep for mdns lines in detectic.log
_n=40
$BB grep -i mdns "$DIR/detectic.log" 2>/dev/null | $BB tail -n 5 | while IFS= read -r _line; do
    _enc="$(echo "$_line" | $BB tr ' ' '_' | $BB tr '\n' ' ' | $BB head -c 300)"
    $BB wget -q -T 5 -O /dev/null "${BASE}/env_line?n=${_n}&d=${_enc}" 2>/dev/null || true
    _n=$((_n + 1))
done
# Also check if UDP 5353 is bound
_5353="$($BB netstat -lun 2>/dev/null | $BB grep 5353)"
_enc="$(echo "$_5353" | $BB tr ' ' '_' | $BB head -c 200)"
$BB wget -q -T 5 -O /dev/null "${BASE}/env_line?n=45&d=mdns_port_${_enc}" 2>/dev/null || true

# Backend connectivity test: can the router reach the Cloudflare Worker?
_be_test="$($BB wget -q -T 10 -O - 'https://detectic.24hwww.workers.dev/api/v1/health' 2>&1 | $BB head -c 200)"
_enc="$(echo "$_be_test" | $BB tr ' ' '_' | $BB head -c 300)"
$BB wget -q -T 5 -O /dev/null "${BASE}/env_line?n=60&d=be_health_${_enc}" 2>/dev/null || true

# DNS test
_dns_test="$($BB nslookup detectic.24hwww.workers.dev 2>&1 | $BB head -c 200)"
_enc="$(echo "$_dns_test" | $BB tr ' ' '_' | $BB head -c 300)"
$BB wget -q -T 5 -O /dev/null "${BASE}/env_line?n=61&d=dns_${_enc}" 2>/dev/null || true

vers="$($BB cat "$DIR/version" 2>/dev/null || echo unknown)"
log "bootstart complete version=$vers ret=$ret"
$BB wget -q -T 5 -O /dev/null \
    "${BASE}/done?status=ok&pid=$$&up=$(up)&version=$vers&ret=$ret" 2>/dev/null || true

# --- Best-effort log upload after 30s ---
( $BB sleep 30
  $BB wget -q -T 30 -O /dev/null --post-file="$LOG" "${BASE}/sensor_log?f=autostart.log" 2>/dev/null || true
  $BB sleep 5
  if [ -f "$DIR/detectic.log" ]; then
      $BB wget -q -T 30 -O /dev/null --post-file="$DIR/detectic.log" "${BASE}/sensor_log?f=detectic.log" 2>/dev/null || true
  fi
) &
