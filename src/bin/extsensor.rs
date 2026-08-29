//! extsensor — external RF probe sensor (low-latency motion detector).
//!
//! Captures 802.11 management frames on a monitor-mode interface via AF_PACKET,
//! parses the radiotap header (rate / channel / RSSI) and Probe Requests (MAC,
//! SSID, supported rates, HT/VHT/HE, randomized flag), pseudonymizes the MAC,
//! and emits `rf.probe_detected` events to the Detectic backend over HTTPS with
//! the standard HMAC contract (reusing [`crate::event_transport::HttpEventTransport`]).
//!
//! This is the TRUE low-latency motion path the EX520 cannot provide: it detects
//! ANY Wi-Fi device (associated or not) in real time, per frame, with RSSI.
//!
//! Build (host with a monitor-mode adapter):
//!   cargo build --release --bin extsensor --features tls
//! Run (example):
//!   sudo --preserve-env=DETECTIC_SECRET ./target/release/extsensor \
//!     --iface wlan0mon --sensor-id ex520-ext-001 \
//!     --backend-url https://detectic.24hwww.workers.dev \
//!     --secret "$DETECTIC_SECRET"

use clap::Parser;
use detectic::crypto::pseudonymize;
use detectic::event_transport::{HttpEventTransport, ReliableQueue};
use detectic::proximity::ProximityConfig;
use detectic::rf_presence::{RfPresenceSensor, RfProbe};
use detectic::temporal::TemporalConfig;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Ethernet protocol for 802.11 (in network byte order).
const ETH_P_802_11: u16 = 0x0019;

#[derive(Parser, Debug)]
#[command(name = "extsensor", version, about = "Detectic external monitor-mode probe sensor")]
struct Args {
    /// Monitor-mode interface to sniff (e.g. wlan0mon).
    #[arg(long)]
    iface: String,
    /// Sensor id reported to the backend.
    #[arg(long, env = "DETECTIC_SENSOR_ID", default_value = "ext-001")]
    sensor_id: String,
    /// Base backend URL (HTTPS). The event endpoint is appended automatically.
    #[arg(long, env = "DETECTIC_BACKEND_URL", default_value = "https://detectic.24hwww.workers.dev")]
    backend_url: String,
    /// HMAC secret for pseudonymization + request signing (never logged).
    #[arg(long, env = "DETECTIC_SECRET")]
    secret: String,
    /// Only report probes seen on this band ("2.4GHz"|"5GHz"). Optional.
    #[arg(long)]
    band: Option<String>,
    /// Optional EX520 sensor /probes endpoint to also POST each observation to
    /// (so the EX520 http_dashboard can show motion-detected devices).
    #[arg(long, env = "DETECTIC_PROBES_URL")]
    probes_url: Option<String>,
    /// Flush the batch to the backend at most this often (ms). Default 250.
    #[arg(long, default_value_t = 250)]
    flush_ms: u64,
}

fn main() {
    let args = Args::parse();
    if args.secret.is_empty() {
        eprintln!("error: set DETECTIC_SECRET (HMAC secret, never hardcode)");
        std::process::exit(2);
    }
    let ifindex = ifindex(&args.iface)
        .unwrap_or_else(|| {
            eprintln!("error: cannot resolve ifindex for '{}'", args.iface);
            std::process::exit(2);
        });
    let fd = unsafe { open_packet_socket(ifindex) }.unwrap_or_else(|e| {
        eprintln!("error: open AF_PACKET socket: {e}");
        std::process::exit(2);
    });

    let mut transport = HttpEventTransport::new(
        &args.backend_url,
        &args.sensor_id,
        args.secret.as_bytes(),
        Duration::from_secs(10),
    );
    let mut queue = ReliableQueue::default();
    let mut presence = RfPresenceSensor::new(
        &args.sensor_id,
        TemporalConfig {
            polls_to_absent: 4,
            signal_delta_threshold: 5,
            ..Default::default()
        },
        ProximityConfig::default(),
    );
    let mut seq: u64 = 0;
    let mut probe_buf: Vec<serde_json::Value> = Vec::new();
    let mut last_flush = std::time::Instant::now();
    let mut last_tick = std::time::Instant::now();
    let tick_interval = Duration::from_millis(presence.tick_interval_ms());
    let mut buf = vec![0u8; 65536];

    println!(
        "extsensor listening on iface={} band={:?} backend={}",
        args.iface, args.band, args.backend_url
    );

    loop {
        let n = unsafe { recv_packet(fd, &mut buf) };
        if n <= 0 {
            std::thread::sleep(Duration::from_millis(2));
        } else if let Some(probe) = parse_probe(&buf[..n as usize]) {
            if args.band.as_ref().map(|b| *b == probe.band).unwrap_or(true) {
                let pseudo = pseudonymize(args.secret.as_bytes(), &probe.mac);
                let ts = now_secs();
                if args.probes_url.is_some() {
                    probe_buf.push(probe_json(&pseudo, &probe));
                }
                let rf = RfProbe {
                    device_id: pseudo,
                    timestamp: ts,
                    band: probe.band,
                    channel: probe.channel,
                    frequency_mhz: probe.frequency_mhz,
                    rssi_dbm: probe.rssi_dbm.map(|r| r as i64),
                    per_chain_rssi: probe.per_chain_rssi,
                    ssid: probe.ssid,
                    ht_vht_he: probe.ht_vht_he,
                    supported_rates: probe.supported_rates,
                    vendor_ies: probe.vendor_ies,
                    randomized: probe.randomized,
                    confidence: 0.5,
                };
                let events = presence.observe(&rf, ts);
                queue.submit(events);
            }
        }

        // Periodic tick so the presence engine can emit absence transitions.
        if last_tick.elapsed() >= tick_interval {
            let ts = now_secs();
            queue.submit(presence.tick(ts));
            last_tick = std::time::Instant::now();
        }

        // Opportunistic flush so a quiet channel still drains.
        if last_flush.elapsed().as_millis() as u64 >= args.flush_ms
            && (queue.pending_len() > 0 || !probe_buf.is_empty())
        {
            seq = flush(&mut transport, &mut queue, &mut probe_buf, &args.probes_url, seq);
            last_flush = std::time::Instant::now();
        }
    }
}

fn flush(
    t: &mut HttpEventTransport,
    queue: &mut ReliableQueue,
    probe_buf: &mut Vec<serde_json::Value>,
    probes_url: &Option<String>,
    seq: u64,
) -> u64 {
    if !probe_buf.is_empty() {
        if let Some(url) = probes_url {
            let endpoint = format!("{}/probes", url.trim_end_matches('/'));
            let body = serde_json::to_vec(&probe_buf).unwrap_or_default();
            let _ = ureq::post(&endpoint)
                .set("Content-Type", "application/json")
                .send_bytes(&body)
                .map(|r| println!("probes_post status={}", r.status()));
        }
        probe_buf.clear();
    }
    let report = queue.flush(t);
    if report.sent > 0 {
        println!("flushed {} canonical events (kept={})", report.sent, report.kept);
    }
    seq
}

fn probe_json(pseudo: &str, probe: &Probe) -> serde_json::Value {
    serde_json::json!({
        "device_id": pseudo,
        "rssi": probe.rssi_dbm,
        "rssi_dbm": probe.rssi_dbm,
        "band": probe.band,
        "channel": probe.channel,
        "frequency_mhz": probe.frequency_mhz,
        "ssid": probe.ssid,
        "per_chain_rssi": probe.per_chain_rssi,
        "randomized": probe.randomized,
    })
}

#[derive(Debug)]
struct Probe {
    mac: String,
    band: String,
    channel: Option<u8>,
    frequency_mhz: Option<u32>,
    rssi_dbm: Option<i32>,
    per_chain_rssi: Vec<i64>,
    ssid: Option<String>,
    ht_vht_he: Option<String>,
    supported_rates: Vec<String>,
    vendor_ies: Vec<String>,
    randomized: bool,
}

// ---------------------------------------------------------------- radiotap ---
/// Best-effort radiotap header parse. Returns the 802.11 frame slice and the
/// useful signal fields, or `None` if the frame cannot be understood.
fn parse_probe(buf: &[u8]) -> Option<Probe> {
    if buf.len() < 8 {
        return None;
    }
    let kind = buf[0];
    if kind != 0 {
        // Not a radiotap header (e.g. a cooked capture) — drop.
        return None;
    }
    let hlen = u16::from_le_bytes([buf[2], buf[3]]) as usize;
    if hlen < 8 || hlen > buf.len() {
        return None;
    }
    // Parse the present bitmap (one or more u32 words, last word has bit31 clear).
    let mut present: Vec<u32> = Vec::new();
    let mut idx = 4usize;
    loop {
        if idx + 4 > hlen {
            return None;
        }
        let w = u32::from_le_bytes([buf[idx], buf[idx + 1], buf[idx + 2], buf[idx + 3]]);
        present.push(w);
        idx += 4;
        if w & 0x8000_0000 == 0 {
            break;
        }
    }

    // Field size (bytes) per radiotap bit index 0..=21. Any higher present bit is
    // considered unknown → we bail to keep the field offsets trustworthy.
    const SIZES: [usize; 22] = [
        8, 1, 1, 4, 2, 1, 1, 2, 2, 2, 1, 1, 1, 1, 2, 2, 1, 1, 4, 3, 8, 12,
    ];
    let mut cursor = idx;
    let mut freq: Option<u32> = None;
    let mut signal_dbm: Option<i32> = None;
    let mut signal_db: Option<i32> = None;
    for (wi, w) in present.iter().enumerate() {
        for bit in 0u32..32 {
            if w & (1 << bit) == 0 {
                continue;
            }
            let g = wi * 32 + bit as usize;
            if g >= SIZES.len() {
                return None; // unsupported field: cannot trust the offsets.
            }
            let size = SIZES[g];
            if cursor + size > hlen {
                return None;
            }
            match g {
                3 => {
                    freq = Some(u16::from_le_bytes([buf[cursor], buf[cursor + 1]]) as u32);
                }
                5 => signal_dbm = Some(buf[cursor] as i8 as i32),
                12 => signal_db = Some(buf[cursor] as i32),
                _ => {}
            }
            cursor += size;
        }
    }

    // The 802.11 frame begins right after the radiotap header.
    let wire = &buf[hlen..];
    if wire.len() < 24 {
        return None;
    }
    let fc = wire[0];
    let ftype = (fc >> 2) & 0x3;
    let subtype = (fc >> 4) & 0xf;
    if ftype != 0 {
        // Not management → not a probe/assoc/deauth frame we track.
        return None;
    }
    if subtype != 4 && subtype != 0 && subtype != 2 && subtype != 10 && subtype != 12 {
        return None;
    }

    let mac = format_mac(&wire[10..16]);
    let randomized = wire[10] & 0x02 != 0;
    let rssi_dbm = signal_dbm.or(signal_db);
    let frequency_mhz = freq;
    let band = match freq {
        Some(f) if f < 2500 => "2.4GHz".to_string(),
        Some(f) if f >= 4900 => "5GHz".to_string(),
        _ => "unknown".to_string(),
    };
    let channel = freq.and_then(freq_to_channel);

    // Management frame body: tagged parameters begin after the 24-byte header.
    let body = if wire.len() > 24 { &wire[24..] } else { &[] };
    let (ssid, supported_rates, ht_vht_he, vendor_ies) = parse_ies(body);

    Some(Probe {
        mac,
        band,
        channel,
        frequency_mhz,
        rssi_dbm,
        per_chain_rssi: rssi_dbm.map(|r| vec![r as i64]).unwrap_or_default(),
        ssid,
        ht_vht_he,
        supported_rates,
        vendor_ies,
        randomized,
    })
}

fn freq_to_channel(freq: u32) -> Option<u8> {
    match freq {
        2407..=2484 => Some(((freq - 2407) / 5) as u8),
        4900..=5895 => Some(((freq - 5000) / 5) as u8),
        _ => None,
    }
}

/// Parse 802.11 tagged parameters (information elements).
fn parse_ies(body: &[u8]) -> (Option<String>, Vec<String>, Option<String>, Vec<String>) {
    let mut ssid = None;
    let mut rates = Vec::new();
    let mut ht = false;
    let mut vht = false;
    let mut he = false;
    let mut vendor = Vec::new();
    let mut i = 0usize;
    while i + 2 <= body.len() {
        let id = body[i];
        let len = body[i + 1] as usize;
        if i + 2 + len > body.len() {
            break;
        }
        let data = &body[i + 2..i + 2 + len];
        match id {
            0 => {
                if len > 0 && !data.iter().all(|&b| b == 0) {
                    ssid = Some(String::from_utf8_lossy(data).to_string());
                }
            }
            1 => {
                for &r in data.iter().take(len) {
                    let m = r & 0x7f;
                    let kbps = (m as u32) * 500;
                    rates.push(format!("{}k", kbps));
                }
            }
            45 => ht = true,
            191 => vht = true,
            255 => {
                // Vendor-specific or HE capabilities; keep a short fingerprint.
                vendor.push(format!("{:02x}", data.first().copied().unwrap_or(0)));
                if data.len() >= 3 && (data[0] == 0x00 || data[0] == 0xff) {
                    he = true;
                }
            }
            _ => {}
        }
        i += 2 + len;
    }
    let flag = match (he, vht, ht) {
        (true, _, _) => Some("HE".to_string()),
        (_, true, _) => Some("VHT".to_string()),
        (_, _, true) => Some("HT".to_string()),
        _ => None,
    };
    (ssid, rates, flag, vendor)
}

fn format_mac(b: &[u8]) -> String {
    b.iter()
        .take(6)
        .map(|x| format!("{x:02x}"))
        .collect::<Vec<_>>()
        .join(":")
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

// ------------------------------------------------------ AF_PACKET (libc) ----
unsafe fn open_packet_socket(ifindex: i32) -> std::io::Result<i32> {
    let fd = libc::socket(libc::AF_PACKET, libc::SOCK_RAW, ETH_P_802_11.to_be() as i32);
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    let flags = libc::fcntl(fd, libc::F_GETFL, 0);
    if flags < 0 || libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) < 0 {
        let _ = libc::close(fd);
        return Err(std::io::Error::last_os_error());
    }
    let mut sll: libc::sockaddr_ll = std::mem::zeroed();
    sll.sll_family = libc::AF_PACKET as u16;
    sll.sll_protocol = ETH_P_802_11.to_be();
    sll.sll_ifindex = ifindex;
    let addr_len = std::mem::size_of::<libc::sockaddr_ll>() as libc::socklen_t;
    let rc = libc::bind(
        fd,
        &sll as *const libc::sockaddr_ll as *const libc::sockaddr,
        addr_len,
    );
    if rc != 0 {
        let e = std::io::Error::last_os_error();
        libc::close(fd);
        return Err(e);
    }
    Ok(fd)
}

fn ifindex(name: &str) -> Option<i32> {
    unsafe {
        let cstr = std::ffi::CString::new(name).ok()?;
        let idx = libc::if_nametoindex(cstr.as_ptr());
        if idx == 0 {
            None
        } else {
            Some(idx as i32)
        }
    }
}

unsafe fn recv_packet(fd: i32, buf: &mut [u8]) -> isize {
    libc::recv(fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len(), 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a radiotap header (rate, channel, signal) + an 802.11 Probe Request.
    fn probe_frame(mac: [u8; 6]) -> Vec<u8> {
        let mut b = Vec::new();
        // radiotap
        b.push(0); // version
        b.push(0); // pad
        let hlen: u16 = 8 + 1 + 4 + 1; // header + rate + channel + signal
        b.extend_from_slice(&hlen.to_le_bytes());
        let present: u32 = (1 << 2) | (1 << 3) | (1 << 5);
        b.extend_from_slice(&present.to_le_bytes());
        b.push(12); // rate (12 * 500kbps = 6 Mbps)
        b.extend_from_slice(&2412u16.to_le_bytes()); // channel freq (2.4 GHz ch1)
        b.extend_from_slice(&0u16.to_le_bytes()); // channel flags
        b.push(0x80u8); // signal dBm = -128 (0x80 as i8 = -128)
        // 802.11 mgmt header
        b.push(0x40); // fc: type 0, subtype 4 (Probe Request)
        b.push(0x00); // flags
        b.extend_from_slice(&[0; 2]); // duration
        b.extend_from_slice(&[0xff; 6]); // addr1 (broadcast)
        b.extend_from_slice(&mac); // addr2 (transmitter)
        b.extend_from_slice(&[0xff; 6]); // addr3
        b.extend_from_slice(&[0, 0]); // seq
        // body: tagged params
        b.push(0); // SSID id
        b.push(4); // len
        b.extend_from_slice(b"HOME");
        b.push(1); // Supported Rates
        b.push(3);
        b.extend_from_slice(&[0x02, 0x04, 0x0b]);
        b
    }

    #[test]
    fn parses_probe_request_and_fields() {
        let mac = [0x02u8, 0xaa, 0xbb, 0xcc, 0xdd, 0xee];
        let p = parse_probe(&probe_frame(mac)).expect("should parse probe");
        assert_eq!(p.mac, "02:aa:bb:cc:dd:ee");
        assert!(p.randomized, "locally-administered bit should flag randomized");
        assert_eq!(p.band, "2.4GHz");
        assert_eq!(p.frequency_mhz, Some(2412));
        assert_eq!(p.channel, Some(1));
        assert_eq!(p.rssi_dbm, Some(-128));
        assert_eq!(p.ssid.as_deref(), Some("HOME"));
        assert!(!p.supported_rates.is_empty());
        assert!(p.per_chain_rssi.contains(&-128));
    }

    #[test]
    fn rejects_non_management_frames() {
        // A data frame (type 2) must be dropped.
        let mut b = probe_frame([0x02, 1, 2, 3, 4, 5]);
        let hlen = u16::from_le_bytes([b[2], b[3]]) as usize;
        b[hlen] = 0x08; // first 802.11 byte: type 2 (data)
        assert!(parse_probe(&b).is_none());
    }

    #[test]
    fn rejects_non_radiotap() {
        let b = vec![0xffu8; 32]; // not radiotap (version != 0)
        assert!(parse_probe(&b).is_none());
    }
}
