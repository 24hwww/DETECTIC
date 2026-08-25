#!/usr/bin/env python3
"""EX520 Detectic email notification daemon.

This is a lightweight, non-blocking host-side endpoint that the router's
`launcher.sh` calls after Detectic starts and every 5 minutes. It sends
SMTP notifications using `DETECTIC_SMTP_*` environment variables.

Email is strictly observational: failures are logged locally and never
block or crash the sensor.
"""
import os
import smtplib
import threading
import time
import urllib.parse
from email.message import EmailMessage
from http.server import BaseHTTPRequestHandler, HTTPServer

HOST = os.environ.get("DETECTIC_EMAILD_HOST", "192.168.0.27")
PORT = int(os.environ.get("DETECTIC_EMAILD_PORT", "8081"))

ENABLED = (
    os.environ.get("DETECTIC_EMAIL_ENABLED", os.environ.get("SMTP_ENABLED", "0")).lower()
    in ("1", "true", "yes")
)
SMTP_HOST = os.environ.get("DETECTIC_SMTP_HOST", "")
SMTP_PORT = int(os.environ.get("DETECTIC_SMTP_PORT", "587"))
SMTP_USER = os.environ.get("DETECTIC_SMTP_USER", "")
SMTP_PASSWORD = os.environ.get("DETECTIC_SMTP_PASSWORD", "")

# Accept both DETECTIC_SMTP_STARTTLS and DETECTIC_SMTP_TLS.
# Brevo style: DETECTIC_SMTP_TLS=starttls (or true/on/1).
_tls = os.environ.get("DETECTIC_SMTP_STARTTLS", os.environ.get("DETECTIC_SMTP_TLS", "")).lower()
SMTP_STARTTLS = _tls in ("1", "true", "yes", "starttls")

FROM = os.environ.get("DETECTIC_EMAIL_FROM") or os.environ.get("DETECTIC_SMTP_FROM", "detectic@example.com")
TO = os.environ.get("DETECTIC_EMAIL_TO") or os.environ.get("DETECTIC_SMTP_TO", "")
ROUTER_ID = os.environ.get("DETECTIC_ROUTER_ID", "EX520")


def log(msg):
    ts = time.strftime("%Y-%m-%dT%H:%M:%S%z")
    print(f"{ts} {msg}", flush=True)


def send_smtp(subject, body):
    """Best-effort SMTP send. Returns without raising."""
    if not ENABLED:
        log("email disabled; not sending")
        return
    if not (SMTP_HOST and TO):
        log("email enabled but SMTP_HOST or TO not configured; skipping")
        return

    msg = EmailMessage()
    msg["From"] = FROM
    msg["To"] = TO
    msg["Subject"] = subject
    msg.set_content(body)

    try:
        if SMTP_PORT == 465:
            server = smtplib.SMTP_SSL(SMTP_HOST, SMTP_PORT, timeout=10)
        else:
            server = smtplib.SMTP(SMTP_HOST, SMTP_PORT, timeout=10)
            if SMTP_STARTTLS:
                server.starttls()
        if SMTP_USER:
            server.login(SMTP_USER, SMTP_PASSWORD)
        server.send_message(msg)
        server.quit()
        log(f"email sent: {subject}")
    except Exception as e:
        log(f"email notification failed: {e}")


def send_startup(qs):
    up = qs.get("up", "unknown")
    version = qs.get("version", "unknown")
    pid = qs.get("pid", "unknown")
    status = qs.get("status", "running")
    subject = f"[DETECTIC] {ROUTER_ID} sensor started — {version}"
    body = (
        f"DETECTIC startup notification\n"
        f"Router:      {ROUTER_ID}\n"
        f"Version:     {version}\n"
        f"PID:         {pid}\n"
        f"Uptime:      {up}s\n"
        f"Status:      {status}\n"
        f"Timestamp:   {time.strftime('%Y-%m-%dT%H:%M:%S%z')}\n"
    )
    threading.Thread(target=send_smtp, args=(subject, body), daemon=True).start()


def send_report(qs):
    up = qs.get("up", "unknown")
    version = qs.get("version", "unknown")
    pid = qs.get("pid", "unknown")
    devices = qs.get("devices", "unavailable")
    interval = qs.get("interval", "300")
    subject = f"[DETECTIC] {ROUTER_ID} report — {version}"
    body = (
        f"DETECTIC periodic report\n"
        f"Router:      {ROUTER_ID}\n"
        f"Version:     {version}\n"
        f"PID:         {pid}\n"
        f"Uptime:      {up}s\n"
        f"Interval:    {interval}s\n"
        f"Devices:     {devices}\n"
        f"Timestamp:   {time.strftime('%Y-%m-%dT%H:%M:%S%z')}\n"
    )
    threading.Thread(target=send_smtp, args=(subject, body), daemon=True).start()


def send_event(qs):
    """Send a single event notification from the router sensor."""
    event = qs.get("event", "unknown")
    device = qs.get("device", "unknown")
    hostname = qs.get("hostname", "unknown")
    band = qs.get("band", "unknown")
    signal = qs.get("signal", "N/A")
    ts = qs.get("timestamp", time.strftime('%Y-%m-%dT%H:%M:%S%z'))
    emoji = {
        "connected": "🟢",
        "disconnected": "🔴",
        "unassociated_in_range": "🟣",
        "unassociated_left": "⚪",
    }.get(event, "⚫")
    subject = f"[DETECTIC {ROUTER_ID}] {emoji} {event}: {device[:12]}"
    body = (
        f"DETECTIC event notification\n"
        f"Router:      {ROUTER_ID}\n"
        f"Event:       {event}\n"
        f"Device:      {device}\n"
        f"Hostname:    {hostname}\n"
        f"Band:        {band}\n"
        f"Signal:      {signal}\n"
        f"Timestamp:   {ts}\n"
    )
    threading.Thread(target=send_smtp, args=(subject, body), daemon=True).start()


class EmaildHandler(BaseHTTPRequestHandler):
    def log_message(self, fmt, *args):
        # avoid duplicating http.server output; we log the events we care about
        pass

    def do_GET(self):
        parsed = urllib.parse.urlparse(self.path)
        if parsed.path != "/email":
            self.send_response(404)
            self.end_headers()
            return

        qs = urllib.parse.parse_qs(parsed.query)
        qs = {k: v[0] for k, v in qs.items()}
        mtype = qs.get("type", "")

        if mtype == "startup":
            send_startup(qs)
            log(f"startup notification requested up={qs.get('up')} version={qs.get('version')} pid={qs.get('pid')}")
        elif mtype == "report":
            send_report(qs)
            log(f"report notification requested up={qs.get('up')} devices={qs.get('devices')}")
        elif mtype == "event":
            send_event(qs)
            log(f"event notification requested event={qs.get('event')} device={qs.get('device')}")
        else:
            log(f"unknown email type: {mtype}")

        self.send_response(200)
        self.end_headers()
        self.wfile.write(b"ok\n")


def main():
    log(f"emaild starting on {HOST}:{PORT} enabled={ENABLED}")
    server = HTTPServer((HOST, PORT), EmaildHandler)
    server.serve_forever()


if __name__ == "__main__":
    main()
