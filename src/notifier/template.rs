use crate::events::EventKind;
use crate::notifier::{DetectionEvent, SmtpConfig};

pub fn mask_mac(mac: &str) -> String {
    let parts: Vec<&str> = mac.split(':').collect();
    if parts.len() == 6 {
        format!(
            "{}:{}:{}:**:**:{}:{}",
            parts[0], parts[1], parts[2], parts[4], parts[5]
        )
    } else if mac.len() >= 12 {
        format!("{}:**:**:{}", &mac[..6], &mac[mac.len() - 4..])
    } else {
        mac.to_string()
    }
}

pub fn rcpi_to_dbm(rcpi: u32) -> i32 {
    if rcpi == 0 || rcpi > 127 {
        return -1;
    }
    (-110.0 + (rcpi as f32 * 30.0) / 127.0).round() as i32
}

fn fmt_option(o: &Option<String>) -> String {
    o.clone().unwrap_or_else(|| "N/A".into())
}

fn fmt_option_u8(o: Option<u8>) -> String {
    o.map(|v| v.to_string())
        .unwrap_or_else(|| "N/A".into())
}

/// Proximity icon for HTML display.
fn prox_icon_html(label: &str) -> String {
    match label {
        "Muito perto" => "&#128205;&#65039; Muito perto".into(),
        "Perto" => "&#128205; Perto".into(),
        "Distancia media" => "&#128207; Distancia media".into(),
        "Longe" => "&#128208; Longe".into(),
        "Cabo" => "&#128268; Cabo".into(),
        _ => "&#10067; Incerto".into(),
    }
}

/// Proximity color.
fn prox_color(label: &str) -> &str {
    match label {
        "Muito perto" => "#2d7",
        "Perto" => "#5a5",
        "Distancia media" => "#da3",
        "Longe" => "#d55",
        "Cabo" => "#888",
        _ => "#999",
    }
}

pub struct EmailTemplate;

impl EmailTemplate {
    pub fn render(event: &DetectionEvent, config: &SmtpConfig) -> (String, String, String) {
        let subject = match event.kind {
            EventKind::DeviceJoined => format!(
                "Detectic \u{2022} {} dispositivos detectados ({} conectados)",
                event.total_devices, event.connected_count
            ),
            EventKind::DeviceLeft => format!(
                "Detectic \u{2022} {} dispositivos detectados ({} conectados)",
                event.total_devices, event.connected_count
            ),
            EventKind::DeviceUpdated => format!(
                "Detectic \u{2022} {} dispositivos detectados ({} conectados)",
                event.total_devices, event.connected_count
            ),
        };

        let masked_mac = event
            .mac
            .as_deref()
            .map(mask_mac)
            .unwrap_or_else(|| "N/A".into());
        let hostname = fmt_option(&event.hostname);
        let ip = fmt_option(&event.ip);
        let band = fmt_option(&event.band);
        let channel = fmt_option_u8(event.channel);

        let (rssi_text, rcpi_text) = if let Some(rssi) = event.rssi_dbm {
            (format!("{rssi} dBm"), "N/A".into())
        } else if let Some(rcpi) = event.rcpi {
            let dbm = rcpi_to_dbm(rcpi);
            if dbm == -1 {
                ("N/A".into(), rcpi.to_string())
            } else {
                (format!("{dbm} dBm"), rcpi.to_string())
            }
        } else {
            ("N/A".into(), "N/A".into())
        };

        let ts = chrono::DateTime::from_timestamp(event.captured_at, 0)
            .map(|d| d.format("%d/%m/%Y %H:%M").to_string())
            .unwrap_or_else(|| event.captured_at.to_string());

        let conn_status = if event.connected {
            "Conectado"
        } else {
            "Detectado"
        };
        let conn_color = if event.connected { "#2d7" } else { "#7b3fa0" };
        let conn_dot = if event.connected {
            "&#9679;"
        } else {
            "&#9679;"
        };

        let activity = if event.active {
            "Presente"
        } else {
            "Ausente"
        };
        let act_color = if event.active { "#2d7" } else { "#999" };

        let prox_label = &event.proximity;
        let prox_c = prox_color(prox_label);
        let prox_display = prox_icon_html(prox_label);

        let qual_label = &event.signal_quality;

        // ─── Plain text ───
        let text = format!(
            "=====================================\n\
              DETECTIC - Relatorio de Presenca\n\
              TP-Link EX520V\n\
            =====================================\n\n\
            Data: {ts}\n\
            Sensor: {router}\n\n\
            -------------------------------------\n\
              RESUMO\n\
            -------------------------------------\n\
            {total} dispositivos detectados\n\
              {connected} conectados ao EX520\n\
              {not_connected} nao conectados\n\n\
            -------------------------------------\n\
              DISPOSITIVO\n\
            -------------------------------------\n\
            Hostname:    {hostname}\n\
            MAC:         {mac}\n\
            IP:          {ip}\n\
            Conexao:     {conn}\n\
            Presenca:    {activity}\n\
            Signal:      {rssi} (RCPI {rcpi})\n\
            Qualidade:   {qual}\n\
            Proximidade: {prox}\n\
            Banda:       {band}\n\
            Canal:       {channel}\n\n\
            -------------------------------------\n\
              PRIVACIDAD\n\
            -------------------------------------\n\
            MACs enmascaradas. Sem identificadores reais.\n\
            Modificacoes ao router: NENHUMA.\n",
            ts = ts,
            router = config.router_name,
            total = event.total_devices,
            connected = event.connected_count,
            not_connected = event.not_connected_count,
            hostname = hostname,
            mac = masked_mac,
            ip = ip,
            conn = conn_status,
            activity = activity,
            rssi = rssi_text,
            rcpi = rcpi_text,
            qual = qual_label,
            prox = prox_label,
            band = band,
            channel = channel,
        );

        // ─── HTML ───
        let html = format!(
            "<!DOCTYPE html>
<html><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">
<style>
body {{ font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; background: #f0f2f5; padding: 20px; margin: 0; }}
.c {{ max-width: 700px; margin: 0 auto; background: #fff; border-radius: 12px; overflow: hidden; box-shadow: 0 2px 12px rgba(0,0,0,0.08); }}
.header {{ background: linear-gradient(135deg, #1a5276, #2e86c1); color: #fff; padding: 24px 28px; }}
.header h1 {{ margin: 0; font-size: 22px; font-weight: 600; }}
.header .sub {{ color: rgba(255,255,255,0.75); font-size: 13px; margin-top: 4px; }}
.body {{ padding: 24px 28px; }}
.dash {{ background: #f8f9fa; border-radius: 10px; padding: 20px; margin: 0 0 20px 0; text-align: center; }}
.dash .big {{ font-size: 36px; font-weight: 700; color: #2e86c1; margin: 0; }}
.dash .sub-counts {{ display: flex; justify-content: center; gap: 28px; margin-top: 12px; font-size: 14px; }}
.dash .sub-counts .item {{ display: flex; align-items: center; gap: 5px; }}
.dash .dot {{ width: 10px; height: 10px; border-radius: 50%; display: inline-block; }}
.dash .dot-green {{ background: #2d7; }}
.dash .dot-purple {{ background: #7b3fa0; }}
.section {{ margin: 20px 0; }}
.section h3 {{ font-size: 15px; color: #333; border-bottom: 2px solid #2e86c1; padding-bottom: 6px; margin-bottom: 12px; }}
.card {{ background: #f8f9fa; border-radius: 8px; padding: 16px; }}
.card .row {{ display: flex; justify-content: space-between; padding: 4px 0; font-size: 13px; }}
.card .label {{ color: #666; }}
.card .value {{ font-weight: 600; color: #222; }}
.footer {{ background: #f8f9fa; padding: 14px 28px; border-top: 1px solid #eee; font-size: 11px; color: #999; text-align: center; }}
.tech {{ font-size: 11px; color: #aaa; margin-top: 16px; }}
</style></head><body>
<div class=\"c\">
  <div class=\"header\">
    <h1>DETECTIC &mdash; Relatorio de Presenca</h1>
    <div class=\"sub\">{router} &middot; {ts}</div>
  </div>
  <div class=\"body\">
    <div class=\"dash\">
      <p class=\"big\">{total} dispositivos detectados</p>
      <div class=\"sub-counts\">
        <div class=\"item\"><span class=\"dot dot-green\"></span> {connected} conectados</div>
        <div class=\"item\"><span class=\"dot dot-purple\"></span> {not_connected} nao conectados</div>
      </div>
    </div>

    <div class=\"section\">
      <h3>&#128241; Dispositivo Detectado</h3>
      <div class=\"card\">
        <div class=\"row\"><span class=\"label\">Hostname</span><span class=\"value\">{hostname}</span></div>
        <div class=\"row\"><span class=\"label\">MAC</span><span class=\"value\">{mac}</span></div>
        <div class=\"row\"><span class=\"label\">IP</span><span class=\"value\">{ip}</span></div>
        <div class=\"row\"><span class=\"label\">Conexao</span><span class=\"value\" style=\"color:{conn_c}\">{conn_dot} {conn}</span></div>
        <div class=\"row\"><span class=\"label\">Presenca</span><span class=\"value\" style=\"color:{act_c}\">{activity}</span></div>
        <div class=\"row\"><span class=\"label\">Signal</span><span class=\"value\">{rssi} (RCPI {rcpi})</span></div>
        <div class=\"row\"><span class=\"label\">Qualidade</span><span class=\"value\">{qual}</span></div>
        <div class=\"row\"><span class=\"label\">Proximidade</span><span class=\"value\" style=\"color:{prox_c}\">{prox_display}</span></div>
        <div class=\"row\"><span class=\"label\">Banda</span><span class=\"value\">{band}</span></div>
        <div class=\"row\"><span class=\"label\">Canal</span><span class=\"value\">{channel}</span></div>
      </div>
    </div>

    <div class=\"tech\">
      Proximidade: estimativa baseada em signal strength. Nao e distancia exata.<br>
      <b>Conexao:</b> <span style=\"color:#2d7\">&#9679;</span> Conectado = associado ao EX520 &middot;
      <span style=\"color:#7b3fa0\">&#9679;</span> Detectado = detectado por RF mas nao associado
    </div>
  </div>
  <div class=\"footer\">
    Privacidade: MACs enmascaradas (HMAC-SHA256). Sem identificadores reais.<br>
    Modificacoes ao router: NENHUMA. Somente leitura via API GTPR/GDPR.
  </div>
</div></body></html>",
            router = config.router_name,
            ts = ts,
            total = event.total_devices,
            connected = event.connected_count,
            not_connected = event.not_connected_count,
            hostname = hostname,
            mac = masked_mac,
            ip = ip,
            conn_c = conn_color,
            conn_dot = conn_dot,
            conn = conn_status,
            act_c = act_color,
            activity = activity,
            rssi = rssi_text,
            rcpi = rcpi_text,
            qual = qual_label,
            prox_c = prox_c,
            prox_display = prox_display,
            band = band,
            channel = channel,
        );

        (subject, text, html)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_mac_correctly() {
        assert_eq!(mask_mac("AA:BB:CC:DD:EE:FF"), "AA:BB:CC:**:**:EE:FF");
    }

    #[test]
    fn rcpi_converts_to_dbm() {
        assert_eq!(rcpi_to_dbm(104), -85);
        assert_eq!(rcpi_to_dbm(0), -1);
        assert_eq!(rcpi_to_dbm(200), -1);
    }

    #[test]
    fn text_contains_masked_mac() {
        let event = DetectionEvent {
            captured_at: 1_000,
            kind: EventKind::DeviceJoined,
            pseudonym: "p1".into(),
            changed_fields: vec![],
            hostname: Some("phone".into()),
            ip: Some("10.0.0.1".into()),
            mac: Some("AA:BB:CC:DD:EE:FF".into()),
            rssi_dbm: None,
            rcpi: Some(104),
            band: Some("2.4G".into()),
            channel: Some(6),
            source: Some("wifi".into()),
            distance_m: Some(2.5),
            connected: true,
            active: true,
            proximity: "Perto".into(),
            signal_quality: "Bom".into(),
            total_devices: 5,
            connected_count: 5,
            not_connected_count: 0,
        };
        let cfg = SmtpConfig {
            router_name: "EX520".into(),
            ..SmtpConfig::default()
        };
        let (subject, text, html) = EmailTemplate::render(&event, &cfg);
        assert!(subject.contains("5 dispositivos"));
        assert!(text.contains("EX520"));
        assert!(text.contains("AA:BB:CC:**:**:EE:FF"));
        assert!(text.contains("Conectado"));
        assert!(text.contains("Perto"));
        assert!(html.contains("2.4G"));
        assert!(html.contains("Conectado"));
        assert!(html.contains("Relatorio de Presenca"));
    }

    #[test]
    fn html_shows_not_connected_device() {
        let event = DetectionEvent {
            captured_at: 2_000,
            kind: EventKind::DeviceJoined,
            pseudonym: "p2".into(),
            changed_fields: vec![],
            hostname: Some("unknown-phone".into()),
            ip: None,
            mac: Some("11:22:33:44:55:66".into()),
            rssi_dbm: Some(-50),
            rcpi: None,
            band: Some("5G".into()),
            channel: Some(36),
            source: Some("wifi".into()),
            distance_m: None,
            connected: false,
            active: true,
            proximity: "Muito perto".into(),
            signal_quality: "Excelente".into(),
            total_devices: 3,
            connected_count: 2,
            not_connected_count: 1,
        };
        let cfg = SmtpConfig {
            router_name: "EX520".into(),
            ..SmtpConfig::default()
        };
        let (subject, text, html) = EmailTemplate::render(&event, &cfg);
        assert!(subject.contains("3 dispositivos"));
        assert!(subject.contains("2 conectados"));
        assert!(text.contains("Detectado"));
        assert!(text.contains("1 nao conectados"));
        assert!(text.contains("Muito perto"));
        assert!(html.contains("Detectado"));
        assert!(html.contains("7b3fa0")); // purple color
    }
}
