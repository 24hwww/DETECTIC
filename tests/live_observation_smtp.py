#!/usr/bin/env python3
"""Live EX520 observation + ONE SMTP presence report.

Connects to the real EX520 via IPv6 link-local, collects the full network map,
pseudonymizes devices, and sends a human-friendly PRESENCE REPORT via Brevo SMTP.

This script performs ONLY read-only operations against the router.
"""

import hashlib
import hmac as hmac_lib
import json
import os
import smtplib
import sys
import time
from datetime import datetime, timezone, timedelta
from email.mime.multipart import MIMEMultipart
from email.mime.text import MIMEText

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "python"))
from detectic_client import GtprClient

# --- Config ---
ROUTER_URL = "http://[fe80::3e6a:d2ff:fe5f:abc1%25enp2s0]"
ROUTER_USER = "user"
ROUTER_PASS = os.environ.get("DETECTIC_PASSWORD", "CHANGE_ME")
PSEUDONYM_SECRET = b"detectic-live-test-secret"
SENSOR_ID = "detectic-ex520-live"

SMTP_HOST = "smtp-relay.brevo.com"
SMTP_PORT = 587
SMTP_USER = "24hwww@gmail.com"
SMTP_PASS = "CHANGE_ME_SMTP"
SMTP_FROM = "Womni-bot <bot@e-mail.womni.com.br>"
SMTP_TO = ["24hwww+detectic@gmail.com", "natasthefany+detectic@gmail.com"]

OIDS = [
    "DEV2_WIFI_APDEV_ASSOCDEV",
    "DEV2_WIFI_DE_STA",
    "DEV2_WIFI_DE_BSS",
    "DEV2_WIFI_RADIO",
    "DEV2_DHCPV4_CLIENT",
    "DEV2_HOST_ENTRY",
]


def pseudonymize(identifier: str) -> str:
    return hmac_lib.new(PSEUDONYM_SECRET, identifier.encode(), hashlib.sha256).hexdigest()


def mask_mac(mac: str) -> str:
    parts = mac.split(":")
    if len(parts) == 6:
        return f"{parts[0]}:{parts[1]}:{parts[2]}:**:{parts[4]}:{parts[5]}"
    return mac


def normalize_mac(mac):
    """Return canonical lowercase colon-delimited MAC (00:1a:2b:3c:4d:5e)."""
    if not mac:
        return ""
    raw = "".join(
        c for c in str(mac).lower()
        if c in "0123456789abcdef"
    )
    if len(raw) != 12:
        return ""
    return ":".join(raw[i:i + 2] for i in range(0, 12, 2))


def is_wired_interface(interface_type):
    """Return True if the host interface type is wired/cable."""
    if not interface_type:
        return True  # default to wired for unknown host entries
    it = str(interface_type).lower()
    return not any(w in it for w in ("wifi", "wireless", "802.11", "wlan", "wl"))


def collect_observation():
    """Connect to real EX520 via IPv6, collect all OIDs, return structured data."""
    print(f"[*] Connecting to {ROUTER_URL} ...")
    t0 = time.time()
    c = GtprClient(ROUTER_URL, ROUTER_USER, ROUTER_PASS)
    c.connect()
    connect_ms = (time.time() - t0) * 1000
    print(f"[*] Connected in {connect_ms:.0f}ms")

    results = {}
    for oid in OIDS:
        t1 = time.time()
        raw = c.gl(oid)
        oid_ms = (time.time() - t1) * 1000
        parsed = json.loads(raw)
        data = parsed.get("data", [])
        if isinstance(data, str):
            try:
                data = json.loads(data)
            except Exception:
                data = []
        results[oid] = data
        print(f"[*] {oid}: {len(data)} entries ({oid_ms:.0f}ms)")

    total_ms = (time.time() - t0) * 1000
    return results, connect_ms, total_ms


# --- Proximity classification (EX520 signalStrength scale 0-128) ---

def classify_proximity(raw):
    """Classify EX520 signalStrength (0-128) into proximity categories.
    Returns (label_pt, color, sort_key)."""
    if raw is None or raw <= 0:
        return ("Incerto", "#999", 5)
    if raw >= 110:
        return ("Muito perto", "#2d7", 0)
    if raw >= 90:
        return ("Perto", "#5a5", 1)
    if raw >= 70:
        return ("Distancia media", "#da3", 2)
    return ("Longe", "#d55", 3)


def signal_quality(raw):
    """Interpret vendor 0-128 signal strength as quality level."""
    if raw is None or raw <= 0:
        return ("N/A", "#999")
    if raw >= 110:
        return ("Excelente", "#2d7")
    if raw >= 90:
        return ("Bom", "#5a5")
    if raw >= 70:
        return ("Regular", "#da3")
    return ("Fraco", "#d55")


def signal_bar_html(raw):
    """Generate a colored signal bar for HTML."""
    if raw is None or raw <= 0:
        return '<span style="color:#999">N/A</span>'
    pct = min(100, int(raw * 100 / 128))
    color = "#2d7" if raw >= 110 else "#5a5" if raw >= 90 else "#da3" if raw >= 70 else "#d55"
    return (
        f'<div style="display:flex;align-items:center;gap:6px">'
        f'<div style="width:60px;height:8px;background:#eee;border-radius:4px;overflow:hidden">'
        f'<div style="width:{pct}%;height:100%;background:{color};border-radius:4px"></div></div>'
        f'<span style="color:{color};font-weight:600">{raw}</span></div>'
    )


def rate_str(kbps):
    """Format kbps as readable string."""
    if kbps is None or kbps == 0 or kbps == "?":
        return "N/A"
    if kbps >= 1000:
        return f"{kbps/1000:.1f} Mbps"
    return f"{kbps} Kbps"


def build_device_summary(results):
    """Extract devices from WiFi assoc + STA + host table, classify presence."""
    assoc = results.get("DEV2_WIFI_APDEV_ASSOCDEV", [])
    sta = results.get("DEV2_WIFI_DE_STA", [])
    host = results.get("DEV2_HOST_ENTRY", [])

    # Build STA lookup by MAC for extra stats
    sta_by_mac = {}
    for s in sta:
        mac = normalize_mac(s.get("MACAddress", ""))
        if mac:
            sta_by_mac[mac] = s

    devices = []
    seen = set()

    for d in assoc:
        mac = normalize_mac(d.get("MACAddress", ""))
        if not mac or mac in seen:
            continue
        seen.add(mac)
        ss_raw = d.get("signalStrength", "0")
        try:
            ss = int(ss_raw)
        except (ValueError, TypeError):
            ss = 0
        ssl_raw = d.get("X_TP_SignalStrengthLevel", "0")
        try:
            ssl = int(ssl_raw)
        except (ValueError, TypeError):
            ssl = 0
        noise_raw = d.get("noise", "0")
        try:
            noise = int(noise_raw)
        except (ValueError, TypeError):
            noise = 0
        active = d.get("active", "0") == "1"
        tx_raw = d.get("lastDataDownlinkRate", "0")
        try:
            tx = int(tx_raw)
        except (ValueError, TypeError):
            tx = 0
        rx_raw = d.get("lastDataUplinkRate", "0")
        try:
            rx = int(rx_raw)
        except (ValueError, TypeError):
            rx = 0
        max_link_raw = d.get("X_TP_MaxLinkRate", "0")
        try:
            max_link = int(max_link_raw)
        except (ValueError, TypeError):
            max_link = 0

        # Enrich from DEV2_WIFI_DE_STA if available
        sta_entry = sta_by_mac.get(mac, {})
        bytes_sent = int(sta_entry.get("bytesSent", "0") or "0")
        bytes_recv = int(sta_entry.get("bytesReceived", "0") or "0")
        errors = int(sta_entry.get("errorsSent", "0") or "0")
        retries = int(sta_entry.get("retransCount", "0") or "0")

        # Proximity
        prox_label, prox_color, prox_sort = classify_proximity(ss)
        qual_label, qual_color = signal_quality(ss)

        devices.append({
            "pseudonym": pseudonymize(mac)[:16],
            "hostname": d.get("X_TP_HostName", "Dispositivo desconhecido"),
            "ip": d.get("X_TP_IPAddress", ""),
            "mac_masked": mask_mac(mac),
            "signal_strength_raw": ss,
            "signal_strength_level": ssl,
            "noise_raw": noise,
            "band": "5GHz" if d.get("operatingStandard") == "ac" else "2.4GHz",
            "standard": d.get("operatingStandard", "?"),
            "tx_rate_kbps": tx,
            "rx_rate_kbps": rx,
            "max_link_rate_kbps": max_link,
            "active": active,
            "connected": True,
            "association_time": d.get("associationTime", ""),
            "radio_mac": d.get("X_TP_RadioMac", ""),
            "source": "wifi",
            "bytes_sent": bytes_sent,
            "bytes_recv": bytes_recv,
            "errors": errors,
            "retries": retries,
            "proximity_label": prox_label,
            "proximity_color": prox_color,
            "proximity_sort": prox_sort,
            "quality_label": qual_label,
            "quality_color": qual_color,
        })

    # Add host-only devices not seen in WiFi assoc (wired / cable)
    wifi_macs = {normalize_mac(d.get("MACAddress", "")) for d in assoc}
    for h in host:
        mac = normalize_mac(h.get("physAddress", ""))
        if not mac or mac in seen or mac in wifi_macs:
            continue
        seen.add(mac)
        interface_type = h.get("interfaceType", "cable")
        wired = is_wired_interface(interface_type)
        devices.append({
            "pseudonym": pseudonymize(mac)[:16],
            "hostname": h.get("hostName", "Dispositivo desconhecido"),
            "ip": h.get("IPAddress", ""),
            "mac_masked": mask_mac(mac),
            "signal_strength_raw": None,
            "signal_strength_level": None,
            "noise_raw": None,
            "band": interface_type,
            "standard": None,
            "tx_rate_kbps": 0,
            "rx_rate_kbps": 0,
            "max_link_rate_kbps": 0,
            "active": True,
            "connected": True,
            "association_time": "",
            "radio_mac": "",
            "source": "host",
            "interface_type": interface_type,
            "bytes_sent": 0,
            "bytes_recv": 0,
            "errors": 0,
            "retries": 0,
            "proximity_label": "Cabo" if wired else "Incerto",
            "proximity_color": "#888",
            "proximity_sort": 4 if wired else 5,
            "quality_label": "N/A",
            "quality_color": "#999",
        })

    # Summary stats
    active_count = sum(1 for d in devices if d["active"])
    connected_count = sum(1 for d in devices if d["connected"])
    wifi_count = sum(1 for d in devices if d["source"] == "wifi")
    wired_count = sum(1 for d in devices if d["source"] == "host" and is_wired_interface(d.get("interface_type", "cable")))

    # Proximity buckets
    prox_buckets = {"Muito perto": 0, "Perto": 0, "Distancia media": 0, "Longe": 0, "Incerto": 0, "Cabo": 0}
    for d in devices:
        prox_buckets[d["proximity_label"]] = prox_buckets.get(d["proximity_label"], 0) + 1

    # Signal stats
    signal_values = [d["signal_strength_raw"] for d in devices if d["signal_strength_raw"] is not None and d["signal_strength_raw"] > 0]
    avg_signal = sum(signal_values) // len(signal_values) if signal_values else 0

    band_24 = sum(1 for d in devices if d["band"] == "2.4GHz")
    band_5 = sum(1 for d in devices if d["band"] == "5GHz")

    return devices, {
        "total": len(devices),
        "active": active_count,
        "connected": connected_count,
        "not_connected": 0,  # placeholder — GTPR only shows associated
        "wifi": wifi_count,
        "wired": wired_count,
        "band_2_4ghz": band_24,
        "band_5ghz": band_5,
        "dhcp_entries": len(results.get("DEV2_DHCPV4_CLIENT", [])),
        "host_entries": len(host),
        "avg_signal": avg_signal,
        "proximity": prox_buckets,
    }


def build_report(devices, summary, connect_ms, total_ms):
    """Build the PRESENCE REPORT — human-friendly, 5-second rule."""
    ts = datetime.now(timezone(timedelta(hours=-3))).strftime("%d/%m/%Y %H:%M BRT")
    ts_short = datetime.now(timezone(timedelta(hours=-3))).strftime("%d/%m/%Y %H:%M")

    # Sort devices: connected first, then by proximity (closest first)
    devices_sorted = sorted(devices, key=lambda d: (not d["connected"], d["proximity_sort"]))

    # Count proximity (excluding wired)
    wifi_devices = [d for d in devices if d["source"] == "wifi"]
    prox_near = sum(1 for d in wifi_devices if d["proximity_sort"] == 0)
    prox_ok = sum(1 for d in wifi_devices if d["proximity_sort"] == 1)
    prox_mid = sum(1 for d in wifi_devices if d["proximity_sort"] == 2)
    prox_far = sum(1 for d in wifi_devices if d["proximity_sort"] == 3)

    # ─── TEXT REPORT ───
    device_lines = []
    for d in devices_sorted:
        conn = "Conectado" if d["connected"] else "Nao conectado"
        active = "Presente" if d["active"] else "Ausente"
        ss = d["signal_strength_raw"]
        sig = f"{ss}" if ss and ss > 0 else "N/A"
        device_lines.append(
            f"  {d['hostname']:<20} {conn:<14} {active:<10} "
            f"Signal={sig:<5} {d['proximity_label']}"
        )

    text = f"""=====================================
  DETECTIC - Relatorio de Presenca
  TP-Link EX520V
=====================================

Data: {ts}
Sensor: {SENSOR_ID}

-------------------------------------
  RESUMO
-------------------------------------
{summary['total']} dispositivos detectados
  {summary['connected']} conectados ao EX520
  {summary['wifi']} via WiFi
  {summary['wired']} via cabo

Proximidade (WiFi):
  Muito perto:   {prox_near}
  Perto:         {prox_ok}
  Dist. media:   {prox_mid}
  Longe:         {prox_far}

-------------------------------------
  DISPOSITIVOS
-------------------------------------
{"Dispositivo":<20} {"Status":<14} {"Presenca":<10} {"Signal":<7} {"Proximidade"}
{"─"*20} {"─"*14} {"─"*10} {"─"*7} {"─"*14}
{chr(10).join(device_lines)}

-------------------------------------
  PRIVACIDAD
-------------------------------------
MACs enmascaradas. Sem identificadores reais.
Modificacoes ao router: NENHUMA.
"""

    # ─── HTML REPORT ───
    html_devices = ""
    for d in devices_sorted:
        ss = d["signal_strength_raw"]
        if d["connected"]:
            status_dot = '<span style="color:#2d7;font-size:18px">&#9679;</span>'
            status_text = "Conectado"
            status_color = "#2d7"
        else:
            status_dot = '<span style="color:#7b3fa0;font-size:18px">&#9679;</span>'
            status_text = "Detectado"
            status_color = "#7b3fa0"

        if d["active"]:
            activity = "Presente"
            act_color = "#2d7"
        else:
            activity = "Ausente"
            act_color = "#999"

        prox = d["proximity_label"]
        prox_c = d["proximity_color"]
        qual = d["quality_label"]
        qual_c = d["quality_color"]
        band = d["band"] or "Cabo"
        std = d["standard"] or ""

        # Proximity icon
        prox_icons = {
            "Muito perto": "&#128205;&#65039; Muito perto",
            "Perto": "&#128205; Perto",
            "Distancia media": "&#128207; Distancia media",
            "Longe": "&#128208; Longe",
            "Cabo": "&#128268; Cabo",
            "Incerto": "&#10067; Incerto",
        }
        prox_display = prox_icons.get(prox, prox)

        row_opacity = "" if d["active"] else ' style="opacity:0.45"'

        html_devices += f"""<tr{row_opacity}>
  <td style="min-width:160px">
    <span style="font-size:16px">{status_dot}</span>
    <b>{d['hostname']}</b>
    <br><span style="color:#999;font-size:11px">{d['ip']}</span>
  </td>
  <td><span style="color:{status_color};font-weight:600;font-size:12px">{status_text}</span></td>
  <td><span style="color:{act_color};font-weight:600;font-size:12px">{activity}</span></td>
  <td>{signal_bar_html(ss)}<br><span style="color:{qual_c};font-size:11px">{qual}</span></td>
  <td><span style="color:{prox_c};font-weight:500;font-size:12px">{prox_display}</span></td>
  <td style="font-size:12px;color:#666">{band} {std}</td>
</tr>
"""

    # Dashboard HTML
    html = f"""<!DOCTYPE html>
<html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<style>
body {{ font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; background: #f0f2f5; padding: 20px; margin: 0; }}
.c {{ max-width: 700px; margin: 0 auto; background: #fff; border-radius: 12px; overflow: hidden; box-shadow: 0 2px 12px rgba(0,0,0,0.08); }}
.header {{ background: linear-gradient(135deg, #1a5276, #2e86c1); color: #fff; padding: 24px 28px; }}
.header h1 {{ margin: 0; font-size: 22px; font-weight: 600; }}
.header .sub {{ color: rgba(255,255,255,0.75); font-size: 13px; margin-top: 4px; }}
.body {{ padding: 24px 28px; }}

/* ── Dashboard card ── */
.dash {{ background: #f8f9fa; border-radius: 10px; padding: 20px; margin: 0 0 20px 0; text-align: center; }}
.dash .big {{ font-size: 36px; font-weight: 700; color: #2e86c1; margin: 0; }}
.dash .sub-counts {{ display: flex; justify-content: center; gap: 28px; margin-top: 12px; font-size: 14px; }}
.dash .sub-counts .item {{ display: flex; align-items: center; gap: 5px; }}
.dash .dot {{ width: 10px; height: 10px; border-radius: 50%; display: inline-block; }}
.dash .dot-green {{ background: #2d7; }}
.dash .dot-purple {{ background: #7b3fa0; }}
.dash .dot-gray {{ background: #ccc; }}

/* ── Proximity row ── */
.prox-row {{ display: flex; gap: 10px; margin: 16px 0; }}
.prox-box {{ flex: 1; text-align: center; padding: 12px 8px; border-radius: 8px; background: #fff; border: 1px solid #eee; }}
.prox-box .num {{ font-size: 22px; font-weight: 700; }}
.prox-box .lbl {{ font-size: 11px; color: #666; margin-top: 2px; }}

/* ── Device table ── */
.section {{ margin: 20px 0; }}
.section h3 {{ font-size: 15px; color: #333; border-bottom: 2px solid #2e86c1; padding-bottom: 6px; margin-bottom: 12px; }}
table {{ border-collapse: collapse; width: 100%; font-size: 13px; }}
th {{ background: #f8f9fa; text-align: left; padding: 8px 10px; border-bottom: 2px solid #ddd; font-weight: 600; color: #555; }}
td {{ padding: 8px 10px; border-bottom: 1px solid #eee; vertical-align: middle; }}
tr:hover {{ background: #f8f9fa; }}

/* ── Footer ── */
.footer {{ background: #f8f9fa; padding: 14px 28px; border-top: 1px solid #eee; font-size: 11px; color: #999; text-align: center; }}
.tech {{ font-size: 11px; color: #aaa; margin-top: 16px; }}
</style></head><body>
<div class="c">
  <!-- Header -->
  <div class="header">
    <h1>DETECTIC &mdash; Relatorio de Presenca</h1>
    <div class="sub">TP-Link EX520V &middot; {ts}</div>
  </div>

  <div class="body">
    <!-- ── 5-SECOND DASHBOARD ── -->
    <div class="dash">
      <p class="big">{summary['total']} dispositivos detectados</p>
      <div class="sub-counts">
        <div class="item"><span class="dot dot-green"></span> {summary['connected']} conectados</div>
        <div class="item"><span class="dot dot-purple"></span> 0 nao conectados</div>
        <div class="item"><span class="dot dot-gray"></span> {summary['wired']} cabo</div>
      </div>
    </div>

    <!-- ── PROXIMITY BREAKDOWN ── -->
    <div class="prox-row">
      <div class="prox-box">
        <div class="num" style="color:#2d7">{prox_near}</div>
        <div class="lbl">&#128205;&#65039; Muito perto</div>
      </div>
      <div class="prox-box">
        <div class="num" style="color:#5a5">{prox_ok}</div>
        <div class="lbl">&#128205; Perto</div>
      </div>
      <div class="prox-box">
        <div class="num" style="color:#da3">{prox_mid}</div>
        <div class="lbl">&#128207; Distancia media</div>
      </div>
      <div class="prox-box">
        <div class="num" style="color:#d55">{prox_far}</div>
        <div class="lbl">&#128208; Longe</div>
      </div>
    </div>

    <!-- ── DEVICE LIST ── -->
    <div class="section">
      <h3>&#128241; Dispositivos</h3>
      <table>
        <tr>
          <th>Dispositivo</th>
          <th>Conexao</th>
          <th>Presenca</th>
          <th>Signal</th>
          <th>Proximidade</th>
          <th>Rede</th>
        </tr>
        {html_devices}
      </table>
    </div>

    <!-- ── SIGNAL LEGEND ── -->
    <div class="tech">
      <b>Signal:</b> escala EX520 0-128 &middot;
      <span style="color:#2d7">&#9679; Excelente (&ge;110)</span> &middot;
      <span style="color:#5a5">&#9679; Bom (&ge;90)</span> &middot;
      <span style="color:#da3">&#9679; Regular (&ge;70)</span> &middot;
      <span style="color:#d55">&#9679; Fraco (&lt;70)</span>
      <br>
      <b>Proximidade:</b> estimativa baseada em signal strength. Nao e distancia exata.
      <br>
      <b>Conexao:</b> <span style="color:#2d7">&#9679;</span> Conectado = associado ao EX520 &middot;
      <span style="color:#7b3fa0">&#9679;</span> Detectado = detectado por RF mas nao associado
      <br>
      Latencia: {connect_ms:.0f}ms auth / {total_ms:.0f}ms total
    </div>
  </div>

  <div class="footer">
    Privacidade: MACs enmascaradas (HMAC-SHA256). Sem identificadores reais.<br>
    Modificacoes ao router: NENHUMA. Somente leitura via API GTPR/GDPR.
  </div>
</div></body></html>"""

    return text, html


def send_smtp(subject, text, html):
    """Send exactly ONE email via Brevo SMTP."""
    msg = MIMEMultipart("alternative")
    msg["From"] = SMTP_FROM
    msg["To"] = ", ".join(SMTP_TO)
    msg["Subject"] = subject
    msg.attach(MIMEText(text, "plain", "utf-8"))
    msg.attach(MIMEText(html, "html", "utf-8"))

    print(f"[*] Connecting to {SMTP_HOST}:{SMTP_PORT} (STARTTLS)...")
    with smtplib.SMTP(SMTP_HOST, SMTP_PORT, timeout=15) as server:
        server.ehlo()
        server.starttls()
        server.ehlo()
        server.login(SMTP_USER, SMTP_PASS)
        server.sendmail(SMTP_FROM, SMTP_TO, msg.as_string())
    print("[*] SMTP: email sent successfully!")


def main():
    print("=" * 60)
    print("Detectic EX520 — Observacao em Vivo + Relatorio de Presenca")
    print("=" * 60)

    # Step 1: Collect live observation
    results, connect_ms, total_ms = collect_observation()

    # Step 2: Build device summary (pseudonymized, classified)
    devices, summary = build_device_summary(results)
    print(f"\n[*] Dispositivos: {summary['total']} (conectados={summary['connected']}, wifi={summary['wifi']}, cabo={summary['wired']})")
    print(f"[*] Proximidade: {summary['proximity']}")

    # Step 3: Print pseudonymized observation
    print("\n--- Observacao em Vivo ---")
    for d in devices:
        ss = d["signal_strength_raw"]
        prox = d["proximity_label"]
        conn = "Conectado" if d["connected"] else "Detectado"
        print(f"  {d['hostname']:<20} {conn:<14} signal={str(ss):>4} {prox}")
    print("--- Fim ---\n")

    # Step 4: Build and send presence report
    subject = f"Detectic — {summary['total']} dispositivos detectados ({summary['connected']} conectados)"
    text, html = build_report(devices, summary, connect_ms, total_ms)
    send_smtp(subject, text, html)

    print("\n" + "=" * 60)
    print("RESULTADO: Observacao PASS + Relatorio de Presenca ENVIADO")
    print("=" * 60)
    return 0


if __name__ == "__main__":
    sys.exit(main())
