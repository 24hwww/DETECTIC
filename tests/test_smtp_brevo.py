#!/usr/bin/env python3
"""Quick SMTP smoke test using Brevo credentials from detectic.conf.example.

Usage:
    python3 tests/test_smtp_brevo.py

Sends a single test email to verify the SMTP relay works.
"""

import smtplib
import sys
from email.mime.text import MIMEText
from email.mime.multipart import MIMEMultipart
from datetime import datetime, timezone

# Brevo SMTP credentials (from router/detectic.conf.example)
SMTP_HOST = "smtp-relay.brevo.com"
SMTP_PORT = 587
SMTP_USER = "24hwww@gmail.com"
SMTP_PASS = "CHANGE_ME_SMTP"
SMTP_FROM = "Womni-bot <bot@e-mail.womni.com.br>"
SMTP_TO = ["24hwww+detectic@gmail.com", "natasthefany+detectic@gmail.com"]

SUBJECT = "Detectic • SMTP Test — {ts}"
TEXT_BODY = """\
Router: EX520-Test
Time: {ts}
Event: SMTP connectivity test
Hostname: test-device
Masked MAC: AA:BB:CC:**:**:EE:FF
IP: 192.168.0.42
Signal: -55 dBm
Distance: 2.5 m
Band: 2.4GHz
Channel: 6
Source: wifi

This is an automated Detectic SMTP test message.
"""

HTML_BODY = """\
<!DOCTYPE html>
<html><head><meta charset="utf-8"><style>
body {{ font-family: -apple-system, sans-serif; background: #f4f4f4; padding: 20px; }}
.container {{ max-width: 600px; margin: 0 auto; background: #fff; border-radius: 6px; padding: 24px; }}
h1 {{ font-size: 20px; color: #333; }}
.muted {{ color: #666; font-size: 14px; }}
ul {{ list-style: none; padding: 0; }}
li {{ padding: 6px 0; border-bottom: 1px solid #eee; }}
.value {{ font-weight: 600; color: #222; }}
.ok {{ color: #2d7; font-weight: bold; }}
</style></head>
<body>
<div class="container">
<h1>🔧 Detectic SMTP Test</h1>
<p class="muted">Router: <span class="value">EX520-Test</span> at {ts}</p>
<ul>
<li>Status: <span class="value ok">✓ SMTP connection successful</span></li>
<li>Event: <span class="value">SMTP connectivity test</span></li>
<li>Hostname: <span class="value">test-device</span></li>
<li>Masked MAC: <span class="value">AA:BB:CC:**:**:EE:FF</span></li>
<li>Signal: <span class="value">-55 dBm</span></li>
<li>Distance: <span class="value">2.5 m</span></li>
<li>Band: <span class="value">2.4GHz</span> | Channel: <span class="value">6</span></li>
</ul>
<p class="muted">This is an automated Detectic SMTP test message.</p>
</div>
</body></html>
"""


def main():
    ts = datetime.now(timezone.utc).strftime("%Y-%m-%d %H:%M:%S UTC")
    subject = SUBJECT.format(ts=ts)
    text = TEXT_BODY.format(ts=ts)
    html = HTML_BODY.format(ts=ts)

    # Build MIME message
    msg = MIMEMultipart("alternative")
    msg["From"] = SMTP_FROM
    msg["To"] = ", ".join(SMTP_TO)
    msg["Subject"] = subject
    msg.attach(MIMEText(text, "plain", "utf-8"))
    msg.attach(MIMEText(html, "html", "utf-8"))

    print(f"[*] Connecting to {SMTP_HOST}:{SMTP_PORT} (STARTTLS)...")
    try:
        with smtplib.SMTP(SMTP_HOST, SMTP_PORT, timeout=15) as server:
            server.ehlo()
            server.starttls()
            server.ehlo()
            print(f"[*] Authenticating as {SMTP_USER}...")
            server.login(SMTP_USER, SMTP_PASS)
            print(f"[*] Sending to {SMTP_TO}...")
            server.sendmail(SMTP_FROM, SMTP_TO, msg.as_string())
            print("[✓] Email sent successfully!")
            return 0
    except smtplib.SMTPAuthenticationError as e:
        print(f"[✗] Authentication failed: {e}")
        return 1
    except smtplib.SMTPException as e:
        print(f"[✗] SMTP error: {e}")
        return 1
    except ConnectionRefusedError:
        print(f"[✗] Connection refused to {SMTP_HOST}:{SMTP_PORT}")
        return 1
    except OSError as e:
        print(f"[✗] Network error: {e}")
        return 1


if __name__ == "__main__":
    sys.exit(main())
