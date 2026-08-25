//! Minimal IPv6 link-local HTTP proxy for the EX520.
//!
//! The `ureq` crate cannot connect to IPv6 link-local addresses with scope IDs
//! (e.g. `fe80::...%25enp2s0`). This proxy:
//!   1. Listens on 127.0.0.1:18200
//!   2. Forwards every request to the real EX520 via IPv6 link-local
//!   3. Returns the response as-is
//!
//! Usage:
//!   cargo run --example ipv6_proxy
//!   # then in another terminal:
//!   DETECTIC_PASSWORD=CHANGE_ME DETECTIC_SECRET=dummy \
//!     ./target/release/detectic --url http://127.0.0.1:18200 --user user map

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv6Addr, SocketAddrV6, TcpListener, TcpStream};

const EX520_V6: Ipv6Addr = Ipv6Addr::new(0xfe80, 0, 0, 0, 0x3e6a, 0xd2ff, 0xfe5f, 0xabc1);
const IFINDEX: u32 = 2; // enp2s0

fn main() {
    let listener = TcpListener::bind("127.0.0.1:18200").expect("bind");
    eprintln!("[proxy] listening on http://127.0.0.1:18200 -> [fe80::3e6a:d2ff:fe5f:abc1%enp2s0]");

    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        std::thread::spawn(move || handle(stream));
    }
}

fn handle(mut client: TcpStream) {
    let client_clone = client.try_clone().unwrap();
    let mut reader = BufReader::new(client_clone);

    // Read the request line: METHOD /path HTTP/1.1
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).is_err() {
        return;
    }
    let parts: Vec<&str> = request_line.trim().split_whitespace().collect();
    if parts.len() < 2 {
        return;
    }
    let method = parts[0];
    let path = parts[1];

    // Read headers
    let mut content_length: usize = 0;
    let mut headers_raw = Vec::new();
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).is_err() {
            return;
        }
        let trimmed = line.trim().to_string();
        if trimmed.is_empty() {
            break;
        }
        if let Some(val) = trimmed.strip_prefix("Content-Length:") {
            content_length = val.trim().parse().unwrap_or(0);
        }
        headers_raw.push(trimmed);
    }

    // Read body if present
    let mut body = vec![0u8; content_length];
    if content_length > 0 {
        if reader.read_exact(&mut body).is_err() {
            return;
        }
    }

    // Connect to EX520 via IPv6 link-local with scope ID
    let addr = SocketAddrV6::new(EX520_V6, 80, 0, IFINDEX);
    let Ok(mut upstream) = TcpStream::connect_timeout(
        &std::net::SocketAddr::V6(addr),
        std::time::Duration::from_secs(10),
    ) else {
        let resp = b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\n\r\n";
        let _ = client.write_all(resp);
        return;
    };

    upstream
        .set_read_timeout(Some(std::time::Duration::from_secs(30)))
        .ok();

    // Rebuild the request for the upstream (rewrite headers)
    let v6_host = "[fe80::3e6a:d2ff:fe5f:abc1]";
    let mut upstream_req = format!("{} {} HTTP/1.1\r\n", method, path);
    for h in &headers_raw {
        if h.to_ascii_lowercase().starts_with("host:") {
            upstream_req.push_str(&format!("Host: {}\r\n", v6_host));
        } else if h.to_ascii_lowercase().starts_with("referer:") {
            upstream_req.push_str(&format!("Referer: http://{}/\r\n", v6_host));
        } else if h.to_ascii_lowercase().starts_with("origin:") {
            upstream_req.push_str(&format!("Origin: http://{}\r\n", v6_host));
        } else {
            upstream_req.push_str(&format!("{}\r\n", h));
        }
    }
    upstream_req.push_str("\r\n");

    let _ = upstream.write_all(upstream_req.as_bytes());
    if !body.is_empty() {
        let _ = upstream.write_all(&body);
    }

    // Proxy the response back to the client
    let mut buf = [0u8; 8192];
    loop {
        match upstream.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                if client.write_all(&buf[..n]).is_err() {
                    break;
                }
            }
            Err(_) => break,
        }
    }
}
