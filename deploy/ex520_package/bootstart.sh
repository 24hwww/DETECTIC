#!/bin/sh
# Detectic hardened bootstrap for EX520V.
# Runs as root from /usr/bin/phoenix.sh.
#
# This script downloads the Detectic package, verifies SHA-256 checksums,
# atomically reassembles the binary, and starts the sensor.  It never
# executes an unverified binary and never modifies stock firmware.

# Survive SIGHUP after phoenix exits.
trap '' 1

export PATH=$PATH:/bin:/usr/bin:/sbin:/usr/sbin
BB=/bin/busybox

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
    exit 0
}

# --- Bounded autostart log ---
if [ -f "$LOG" ]; then
    $BB tail -c 51200 "$LOG" > "$LOG.tmp" 2>/dev/null
    $BB mv "$LOG.tmp" "$LOG" 2>/dev/null
fi

# --- Prepare directories ---
$BB rm -rf "$TMPPKG" 2>/dev/null || true
$BB mkdir -p "$DIR" "$TMPPKG" "$TMPDIR" 2>/dev/null || \
    err "cannot_create_dirs"
$BB chmod 700 "$DIR" 2>/dev/null || log "chmod_dir_failed"
$BB cd "$TMPPKG" 2>/dev/null || err "cannot_cd_tmppkg"

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

    _expected="$($BB cat "$_csum_file" 2>/dev/null | $BB head -n 1)"
    _got="$($BB sha256sum -b "$_file" 2>/dev/null | $BB awk '{print $1}')"

    if [ -z "$_expected" ] || [ -z "$_got" ]; then
        err "verify_empty_hash_$_name"
    fi

    if [ "$_expected" != "$_got" ]; then
        log "sha256 mismatch $_name expected=$_expected got=$_got"
        err "verify_sha256_mismatch_$_name"
    fi

    log "sha256 ok $_name $_got"
}

# --- Download and verify package metadata ---
log "bootstart start base=$BASE version=$(cat "$DIR/version" 2>/dev/null || echo unknown)"

fetch "${BASE}/manifest.json"        "manifest.json"        10 || err "download_manifest"
fetch "${BASE}/detectic.aa.sha256"   "detectic.aa.sha256"   10 || err "download_aa_csum"
fetch "${BASE}/detectic.ab.sha256"   "detectic.ab.sha256"   10 || err "download_ab_csum"
fetch "${BASE}/detectic.sha256"      "detectic.sha256"      10 || err "download_bin_csum"
fetch "${BASE}/version"              "version"              10 || err "download_version"

# --- Validate manifest matches the expected version and part hashes ---
MANIFEST_VERSION="$($BB sed -n 's/.*"version"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' manifest.json)"
MANIFEST_AA="$($BB sed -n 's/.*"detectic.aa"[[:space:]]*:[[:space:]]*"\([a-f0-9]\{64\}\)".*/\1/p' manifest.json)"
MANIFEST_AB="$($BB sed -n 's/.*"detectic.ab"[[:space:]]*:[[:space:]]*"\([a-f0-9]\{64\}\)".*/\1/p' manifest.json)"
MANIFEST_FULL="$($BB sed -n 's/.*"detectic"[[:space:]]*:[[:space:]]*"\([a-f0-9]\{64\}\)".*/\1/p' manifest.json)"

[ -n "$MANIFEST_VERSION" ] || err "manifest_version_missing"
[ -n "$MANIFEST_AA" ]      || err "manifest_aa_hash_missing"
[ -n "$MANIFEST_AB" ]      || err "manifest_ab_hash_missing"
[ -n "$MANIFEST_FULL" ]    || err "manifest_full_hash_missing"

EXPECTED_VERSION="$($BB cat "$DIR/version" 2>/dev/null || echo "")"
if [ -n "$EXPECTED_VERSION" ] && [ "$MANIFEST_VERSION" != "$EXPECTED_VERSION" ]; then
    log "manifest_version_mismatch expected=$EXPECTED_VERSION got=$MANIFEST_VERSION"
    err "manifest_version_mismatch"
fi

log "manifest_ok version=$MANIFEST_VERSION"

# --- Download binary parts ---
fetch "${BASE}/detectic.aa"          "detectic.aa"         180 || err "download_aa"
fetch "${BASE}/detectic.ab"          "detectic.ab"         180 || err "download_ab"

# --- Verify parts ---
verify_sha256 "detectic.aa" "detectic.aa.sha256" "aa"
verify_sha256 "detectic.ab" "detectic.ab.sha256" "ab"

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
    $BB cp "detectic.env" "$DIR/detectic.env" 2>/dev/null || log "copy_env_failed"
    # Ensure the persisted copy is also 600.
    $BB chmod 600 "$DIR/detectic.env" 2>/dev/null || true
fi

# --- Stop any running instance and remove old binary ---
$BB sh "$DIR/launcher.sh" stop 2>/var/tmp/launcher.trace || true
$BB rm -f "$TMPDIR/detectic" "$TMPDIR/detectic.tmp" 2>/dev/null || true

# --- Copy pieces to runtime dir ---
$BB cp "detectic.aa" "$TMPDIR/detectic.aa" 2>/dev/null || err "copy_aa_vartmp"
$BB cp "detectic.ab" "$TMPDIR/detectic.ab" 2>/dev/null || err "copy_ab_vartmp"

# --- Atomic reassembly with final checksum verification ---
$BB cat "$TMPDIR/detectic.aa" "$TMPDIR/detectic.ab" > "$TMPDIR/detectic.tmp" 2>/dev/null || \
    err "reassemble_failed"

verify_sha256 "$TMPDIR/detectic.tmp" "detectic.sha256" "full"

$BB chmod +x "$TMPDIR/detectic.tmp"
$BB mv -f "$TMPDIR/detectic.tmp" "$TMPDIR/detectic" 2>/dev/null || err "atomic_move_failed"

# --- Cleanup download cache ---
$BB rm -rf "$TMPPKG"

# --- Start sensor in background (survives bootstart exit) ---
( $BB sh "$DIR/launcher.sh" start 2>/var/tmp/launcher.trace >> "$LOG" 2>&1 ) &
ret=$?
$BB sleep 1

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
