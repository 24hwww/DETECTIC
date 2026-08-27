//! Minimal mDNS responder for `detectic.local`.
//!
//! This is a pure-Rust, dependency-free implementation that answers mDNS
//! A-record queries for `detectic.local` and `_http._tcp.local` PTR/SRV/TXT
//! queries on the multicast address `224.0.0.251:5353`.
//!
//! It runs in a background thread and is designed to be small enough for the
//! on-router aarch64-musl build.

use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};
use std::thread;
use std::time::Duration;

/// Well-known mDNS multicast IPv4 address and port.
const MDNS_ADDR: Ipv4Addr = Ipv4Addr::new(224, 0, 0, 251);
const MDNS_PORT: u16 = 5353;
const MDNS_TTL: u32 = 120;

/// DNS record type constants.
const TYPE_A: u16 = 1;
const TYPE_PTR: u16 = 12;
const TYPE_TXT: u16 = 16;
const TYPE_SRV: u16 = 33;
const CLASS_IN: u16 = 1;
const CLASS_FLUSH: u16 = 0x8000;

/// A tiny DNS packet cursor.
pub struct MdnsResponder {
    socket: UdpSocket,
    hostname: String,
    ip: Ipv4Addr,
    port: u16,
    txt: Vec<String>,
}

impl MdnsResponder {
    /// Spawn a responder on the given IPv4 address.  `hostname` is the bare
    /// name, e.g. `detectic`.  The `.local` suffix is added automatically.
    pub fn spawn(
        hostname: impl Into<String>,
        ip: Ipv4Addr,
        port: u16,
        txt: Vec<String>,
    ) -> Result<(), String> {
        let hostname = hostname.into();
        let bind_addr = SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, MDNS_PORT);
        let socket = UdpSocket::bind(bind_addr).map_err(|e| format!("mdns bind error: {e}"))?;

        // Join the mDNS multicast group on the interface that owns the
        // advertised IPv4 address.  Using UNSPECIFIED can select the WAN/default
        // route on the EX520V and cause LAN clients to miss the responder.
        if let Err(e) = socket.join_multicast_v4(&MDNS_ADDR, &ip) {
            return Err(format!("mdns join_multicast_v4 error: {e}"));
        }

        // Set a generous receive timeout so the thread can be shut down
        // implicitly when the process exits.
        let _ = socket.set_read_timeout(Some(Duration::from_secs(1)));

        let responder = MdnsResponder {
            socket,
            hostname,
            ip,
            port,
            txt,
        };

        thread::Builder::new()
            .name("mdns-responder".into())
            .spawn(move || responder.run())
            .map_err(|e| format!("mdns thread spawn error: {e}"))?;

        Ok(())
    }

    /// Run the responder loop until the process exits.
    fn run(&self) {
        let mut buf = [0u8; 1500];
        let local = format!("{}.", self.hostname);

        loop {
            match self.socket.recv_from(&mut buf) {
                Ok((len, from)) => {
                    if let Some(q) = parse_dns_question(&buf[..len], &local) {
                        if let Err(e) = self.respond(q, from) {
                            crate::logging::warn(&format!("mdns_response_error err={e}"));
                        }
                    }
                }
                Err(_) => {
                    // Timeout or error; loop and try again. This makes the
                    // thread periodically wake up without consuming CPU.
                }
            }
        }
    }

    fn respond(&self, question: Question, _from: SocketAddr) -> Result<(), std::io::Error> {
        let mut pkt = Vec::with_capacity(512);

        // DNS header: id=0, flags, qdcount=0, ancount, nscount=0, arcount=0
        // bits: response=1, authoritative=1, recdesired=0
        pkt.extend_from_slice(&[0x00, 0x00]); // id
        pkt.extend_from_slice(&[0x84, 0x00]); // flags: response, authoritative
        pkt.extend_from_slice(&[0x00, 0x00]); // qdcount

        // Build answer section.
        let mut answers = Vec::new();
        match question {
            Question::A => {
                // detectic.local A <ip>
                encode_name(&mut answers, &format!("{}.local", self.hostname));
                answers.extend_from_slice(&TYPE_A.to_be_bytes());
                answers.extend_from_slice(&(CLASS_IN | CLASS_FLUSH).to_be_bytes());
                answers.extend_from_slice(&MDNS_TTL.to_be_bytes());
                answers.extend_from_slice(&0x0004u16.to_be_bytes()); // rdlength
                answers.push(self.ip.octets()[0]);
                answers.push(self.ip.octets()[1]);
                answers.push(self.ip.octets()[2]);
                answers.push(self.ip.octets()[3]);
            }
            Question::Ptr => {
                // _http._tcp.local PTR detectic._http._tcp.local
                encode_name(&mut answers, "_http._tcp.local");
                answers.extend_from_slice(&TYPE_PTR.to_be_bytes());
                answers.extend_from_slice(&CLASS_IN.to_be_bytes());
                answers.extend_from_slice(&MDNS_TTL.to_be_bytes());
                let target = format!("{}._http._tcp.local", self.hostname);
                let mut rd = Vec::new();
                encode_name(&mut rd, &target);
                answers.extend_from_slice(&(rd.len() as u16).to_be_bytes());
                answers.extend_from_slice(&rd);

                // Also include SRV and TXT for the target (additional records)
                let addl = self.build_srv_txt(&target);
                answers.extend_from_slice(&addl);
            }
            Question::Srv(name) => {
                answers.extend_from_slice(&self.build_srv_txt(&name));
            }
            Question::Txt(name) => {
                answers.extend_from_slice(&self.build_txt(&name));
            }
        }

        pkt.extend_from_slice(&(1u16).to_be_bytes()); // ancount
        pkt.extend_from_slice(&[0x00, 0x00]); // nscount
        pkt.extend_from_slice(&[0x00, 0x00]); // arcount
        pkt.extend_from_slice(&answers);

        let dest = SocketAddr::V4(SocketAddrV4::new(MDNS_ADDR, MDNS_PORT));
        self.socket.send_to(&pkt, dest)?;
        Ok(())
    }

    fn build_srv_txt(&self, target: &str) -> Vec<u8> {
        let mut out = Vec::new();

        // SRV record
        encode_name(&mut out, target);
        out.extend_from_slice(&TYPE_SRV.to_be_bytes());
        out.extend_from_slice(&(CLASS_IN | CLASS_FLUSH).to_be_bytes());
        out.extend_from_slice(&MDNS_TTL.to_be_bytes());
        let mut rd = Vec::new();
        rd.extend_from_slice(&0u16.to_be_bytes()); // priority
        rd.extend_from_slice(&0u16.to_be_bytes()); // weight
        rd.extend_from_slice(&self.port.to_be_bytes()); // port
        encode_name(&mut rd, &format!("{}.local", self.hostname));
        out.extend_from_slice(&(rd.len() as u16).to_be_bytes());
        out.extend_from_slice(&rd);

        // TXT record
        out.extend_from_slice(&self.build_txt(target));
        out
    }

    fn build_txt(&self, target: &str) -> Vec<u8> {
        let mut out = Vec::new();
        encode_name(&mut out, target);
        out.extend_from_slice(&TYPE_TXT.to_be_bytes());
        out.extend_from_slice(&(CLASS_IN | CLASS_FLUSH).to_be_bytes());
        out.extend_from_slice(&MDNS_TTL.to_be_bytes());
        let mut rd = Vec::new();
        for txt in &self.txt {
            rd.push(txt.len() as u8);
            rd.extend_from_slice(txt.as_bytes());
        }
        out.extend_from_slice(&(rd.len() as u16).to_be_bytes());
        out.extend_from_slice(&rd);
        out
    }
}

#[derive(Debug, Clone)]
enum Question {
    A,
    Ptr,
    Srv(String),
    Txt(String),
}

/// Parse an incoming mDNS packet looking for questions that match our hostname.
fn parse_dns_question(packet: &[u8], hostname: &str) -> Option<Question> {
    if packet.len() < 12 {
        return None;
    }
    // flags
    let flags = u16::from_be_bytes([packet[2], packet[3]]);
    if flags & 0x8000 != 0 {
        // This is a response, not a query.
        return None;
    }

    let qdcount = u16::from_be_bytes([packet[4], packet[5]]) as usize;
    let mut offset: usize = 12;

    for _ in 0..qdcount {
        let name_start = offset;
        while offset < packet.len() {
            let len = packet[offset] as usize;
            if len == 0 {
                // End of this name; offset will be set after the qtype/qclass read.
                break;
            }
            if len & 0xc0 == 0xc0 {
                // Compression pointer; skip 2 bytes.
                offset += 2;
                break;
            }
            offset += len + 1;
        }
        let name = decode_name(packet, name_start)?;

        // After the qname ends, qtype and qclass follow on the next two bytes.
        offset += 1; // skip the zero-length terminator we already consumed
        let qtype = u16::from_be_bytes([packet[offset], packet[offset + 1]]);
        // qclass at offset+2/3

        let lower = name.to_lowercase();
        let a_name = format!("{}.local", hostname);
        let a_name_dot = format!("{}.local.", hostname);
        let svc_name = format!("{}._http._tcp.local", hostname);
        let svc_name_dot = format!("{}._http._tcp.local.", hostname);

        if lower == a_name || lower == a_name_dot {
            match qtype {
                TYPE_A => return Some(Question::A),
                TYPE_SRV => return Some(Question::Srv(name)),
                TYPE_TXT => return Some(Question::Txt(name)),
                _ => {}
            }
        } else if lower == "_http._tcp.local" || lower == "_http._tcp.local." {
            if qtype == TYPE_PTR {
                return Some(Question::Ptr);
            }
        } else if lower == svc_name || lower == svc_name_dot {
            match qtype {
                TYPE_PTR => return Some(Question::Ptr),
                TYPE_SRV => return Some(Question::Srv(name)),
                TYPE_TXT => return Some(Question::Txt(name)),
                _ => {}
            }
        }

        offset += 4;
    }

    None
}

/// Decode a DNS name (handles only the simple label form, not compression).
fn decode_name(packet: &[u8], start: usize) -> Option<String> {
    let mut labels = Vec::new();
    let mut offset = start;
    let mut jumped = false;

    loop {
        if offset >= packet.len() {
            return None;
        }
        let len = packet[offset] as usize;
        if len == 0 {
            break;
        }
        if len & 0xc0 == 0xc0 {
            // Follow compression pointer once.
            if jumped {
                return None; // no nested compression
            }
            let pointer = u16::from_be_bytes([packet[offset], packet[offset + 1]]) & 0x3fff;
            offset = pointer as usize;
            jumped = true;
            continue;
        }
        if len > 63 {
            return None;
        }
        offset += 1;
        if offset + len > packet.len() {
            return None;
        }
        labels.push(String::from_utf8_lossy(&packet[offset..offset + len]).into_owned());
        offset += len;
    }

    Some(labels.join("."))
}

/// Encode a DNS domain name into labels.
fn encode_name(out: &mut Vec<u8>, name: &str) {
    for label in name.split('.') {
        let bytes = label.as_bytes();
        out.push(bytes.len() as u8);
        out.extend_from_slice(bytes);
    }
    out.push(0);
}

/// Attempt to determine a sensible IPv4 address to advertise.
pub fn guess_local_ipv4() -> Option<Ipv4Addr> {
    // If the user supplied an IPv4 in DETECTIC_URL, use it.
    if let Ok(url) = std::env::var("DETECTIC_URL") {
        if let Some(ip) = parse_ipv4_from_url(&url) {
            return Some(ip);
        }
    }
    // Last resort: the EX520 default management IP.
    Some(Ipv4Addr::new(192, 168, 0, 1))
}

fn parse_ipv4_from_url(url: &str) -> Option<Ipv4Addr> {
    // Very simple URL IPv4 parser: find the first IPv4-looking a.b.c.d.
    // Handles http://192.168.0.1 and http://192.168.0.1:8080.
    for part in url.split('/') {
        if let Some(ip) = part
            .split(':')
            .next()
            .and_then(|s| s.parse::<Ipv4Addr>().ok())
        {
            return Some(ip);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_encoding_roundtrip() {
        let mut buf = Vec::new();
        encode_name(&mut buf, "detectic.local");
        assert_eq!(decode_name(&buf, 0), Some("detectic.local".into()));
    }

    #[test]
    fn parse_a_query() {
        // Build a minimal A query for detectic.local.
        let mut pkt = vec![0u8; 12];
        pkt[2] = 0x01; // opcode query
        pkt[4] = 0x00;
        pkt[5] = 0x01; // 1 question
        encode_name(&mut pkt, "detectic.local");
        pkt.extend_from_slice(&TYPE_A.to_be_bytes());
        pkt.extend_from_slice(&CLASS_IN.to_be_bytes());

        let q = parse_dns_question(&pkt, "detectic").unwrap();
        assert!(matches!(q, Question::A));
    }

    #[test]
    fn parse_ptr_query() {
        let mut pkt = vec![0u8; 12];
        pkt[5] = 0x01;
        encode_name(&mut pkt, "_http._tcp.local");
        pkt.extend_from_slice(&TYPE_PTR.to_be_bytes());
        pkt.extend_from_slice(&CLASS_IN.to_be_bytes());

        let q = parse_dns_question(&pkt, "detectic").unwrap();
        assert!(matches!(q, Question::Ptr));
    }
}
