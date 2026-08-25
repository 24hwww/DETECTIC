#!/bin/sh
# router_agent.sh — Persistent bidirectional agent for EX520
# Deployed via phoenix.sh from host.
# Runs as root on the router.
# 
# Architecture:
#   1. Creates a persistent command listener on port 9999
#   2. Host sends commands via HTTP POST to router:9999/exec
#   3. Agent executes and returns results
#   4. Agent periodically polls host for commands (backup channel)
#
# This script is downloaded and executed by phoenix.sh

export PATH=$PATH:/bin:/usr/bin:/sbin:/usr/sbin
BB=/bin/busybox

HOST_IP="__HOST_IP__"
HOST_PORT="__HOST_PORT__"
AGENT_PORT=9999
POLL_INTERVAL=30
LOG=/var/tmp/router_agent.log
CMD_LOG=/var/tmp/router_commands.log

up() { read u _ < /proc/uptime; echo "$u"; }
log() { echo "[$(up)] $*" >> "$LOG" 2>/dev/null; }

# Ensure log is bounded
if [ -f "$LOG" ]; then
    $BB tail -c 32768 "$LOG" > "$LOG.tmp" 2>/dev/null
    $BB mv "$LOG.tmp" "$LOG" 2>/dev/null
fi

log "router_agent starting HOST=$HOST_IP:$HOST_PORT"

# --- Create a minimal HTTP listener using busybox ---
# busybox httpd doesn't support POST well, so we use a different approach:
# Use a FIFO + while loop for command processing.

mkdir -p /var/tmp/agent 2>/dev/null
MKFIFO=/var/tmp/agent/cmd_fifo
RESULT_DIR=/var/tmp/agent/results
mkdir -p "$RESULT_DIR" 2>/dev/null

# Clean up
rm -f "$MKFIFO" 2>/dev/null
mkfifo "$MKFIFO" 2>/dev/null

log "FIFO created: $MKFIFO"

# --- Function to execute a command and store result ---
exec_cmd() {
    local cmd_id="$1"
    local cmd="$2"
    local result_file="$RESULT_DIR/${cmd_id}.txt"
    
    log "executing cmd_id=$cmd_id: $cmd"
    
    # Execute command
    OUTPUT=$(eval "$cmd" 2>&1)
    RC=$?
    
    # Store result
    echo "cmd_id=$cmd_id" > "$result_file"
    echo "rc=$RC" >> "$result_file"
    echo "timestamp=$(date)" >> "$result_file"
    echo "---" >> "$result_file"
    echo "$OUTPUT" >> "$result_file"
    
    # Send result to host
    $BB wget -q -T 10 -O /dev/null \
        --post-file="$result_file" \
        "http://${HOST_IP}:${HOST_PORT}/result/${cmd_id}" 2>/dev/null || \
    curl -s -m 10 -X POST \
        -d @"$result_file" \
        "http://${HOST_IP}:${HOST_PORT}/result/${cmd_id}" 2>/dev/null || true
    
    log "cmd_id=$cmd_id completed rc=$RC"
    
    # Clean up result file after sending
    rm -f "$result_file" 2>/dev/null
}

# --- Poll host for commands (backup channel) ---
poll_host() {
    while true; do
        # Fetch pending command from host
        CMD_RESPONSE=$($BB wget -q -T 10 -O - \
            "http://${HOST_IP}:${HOST_PORT}/poll?agent=router&since=$(date +%s)" 2>/dev/null || \
            curl -s -m 10 "http://${HOST_IP}:${HOST_PORT}/poll?agent=router" 2>/dev/null || echo "")
        
        if [ -n "$CMD_RESPONSE" ] && [ "$CMD_RESPONSE" != "" ]; then
            # Parse command from response
            CMD_ID=$(echo "$CMD_RESPONSE" | $BB grep -o 'cmd_id=[^ ]*' | $BB cut -d= -f2 || echo "")
            CMD_BODY=$(echo "$CMD_RESPONSE" | $BB grep -o 'cmd=[^ ]*' | $BB cut -d= -f2- || echo "")
            
            if [ -n "$CMD_ID" ] && [ -n "$CMD_BODY" ]; then
                exec_cmd "$CMD_ID" "$CMD_BODY"
            fi
        fi
        
        $BB sleep "$POLL_INTERVAL"
    done
}

# --- Main loop ---
# Start polling in background
poll_host &
POLL_PID=$!
log "Poll loop started PID=$POLL_PID"

# Keep agent alive
trap "log 'agent shutting down'; kill $POLL_PID 2>/dev/null; exit 0" 1 2 3 15

# Write PID file
echo $$ > /var/tmp/router_agent.pid 2>/dev/null

log "router_agent running PID=$$"

# Wait forever (or until killed)
while true; do
    $BB sleep 60
    # Check if poll is still alive
    if ! kill -0 $POLL_PID 2>/dev/null; then
        log "poll loop died, restarting"
        poll_host &
        POLL_PID=$!
    fi
done
