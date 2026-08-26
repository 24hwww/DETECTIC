//! Minimal HTTP client supporting IPv6 link-local scope IDs.
//!
//! The `ureq` crate (v2) cannot connect to IPv6 link-local addresses with
//! scope IDs (e.g. `fe80::...%enp2s0`) because the `url` crate treats the
//! `%scope` suffix as part of the hex address and rejects it.
//!
//! This module provides a tiny HTTP/1.1 client that:
//!   - Parses `http://[IPv6%scope]:port/path` correctly
//!   - Creates `SocketAddrV6` with the proper scope/interface index
//!   - Sends/receives plain HTTP (POST/GET) with headers and body
//!   - Handles `Set-Cookie` / `Cookie` manually
//!
//! Only the operations needed by the GTPR/GDPR protocol are implemented.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{IpAddr, Ipv6Addr, SocketAddr, SocketAddrV6, TcpStream, ToSocketAddrs};
use std::time::Duration;

/// Parsed URL with IPv6 scope support.
#[derive(Debug, Clone)]
pub struct ParsedUrl {
    pub host: String,
    pub port: u16,
    pub path: String,
    /// If this is an IPv6 link-local address, the scope/interface name.
    pub scope: Option<String>,
}

impl ParsedUrl {
    /// Parse a URL like `http://[fe80::1%enp2s0]:80/path` or `http://192.168.0.1/path`.
    pub fn parse(url: &str) -> Result<Self, String> {
        let rest = url
            .strip_prefix("http://")
            .or_else(|| url.strip_prefix("https://"))
            .ok_or_else(|| format!("not an http URL: {}", url))?;

        let (host_port, path) = match rest.find('/') {
            Some(i) => (&rest[..i], &rest[i..]),
            None => (rest, "/"),
        };

        let (host_str, port, scope) = if host_port.starts_with('[') {
            // IPv6: [addr] or [addr%scope] or [addr]:port or [addr%scope]:port
            let close = host_port
                .find(']')
                .ok_or_else(|| format!("unclosed bracket in host: {}", host_port))?;
            let ipv6_part = &host_port[1..close];
            let after = &host_port[close + 1..];

            // Handle both %25 (URL-encoded %) and raw % scope separators
            let (addr, scope) = if let Some(si) = ipv6_part.find("%25") {
                (&ipv6_part[..si], Some(ipv6_part[si + 3..].to_string()))
            } else if let Some(si) = ipv6_part.find('%') {
                (&ipv6_part[..si], Some(ipv6_part[si + 1..].to_string()))
            } else {
                (ipv6_part, None)
            };

            let port = if after.starts_with(':') {
                after[1..]
                    .parse::<u16>()
                    .map_err(|e| format!("bad port: {}", e))?
            } else {
                80
            };

            (addr.to_string(), port, scope)
        } else {
            // IPv4 or hostname
            let (h, p) = if let Some(ci) = host_port.rfind(':') {
                let port: u16 = host_port[ci + 1..]
                    .parse()
                    .map_err(|e| format!("bad port: {}", e))?;
                (&host_port[..ci], port)
            } else {
                (host_port, 80u16)
            };
            (h.to_string(), p, None)
        };

        Ok(ParsedUrl {
            host: host_str,
            port,
            path: path.to_string(),
            scope,
        })
    }

    /// Resolve to a SocketAddr, using scope ID for IPv6 link-local.
    fn to_socket_addr(&self) -> Result<SocketAddr, String> {
        // Try parsing as IPv6 first
        if let Ok(ip) = self.host.parse::<Ipv6Addr>() {
            if ip.is_loopback() {
                return Ok(SocketAddr::new(IpAddr::V6(ip), self.port));
            }
            if ip.is_unicast_link_local() {
                let scope = self
                    .scope
                    .as_deref()
                    .ok_or("IPv6 link-local address requires a scope ID (e.g. %enp2s0)")?;
                let ifindex = interface_index(scope)?;
                return Ok(SocketAddr::V6(SocketAddrV6::new(
                    ip, self.port, 0, ifindex,
                )));
            }
            return Ok(SocketAddr::V6(SocketAddrV6::new(ip, self.port, 0, 0)));
        }

        // Try as IPv4
        if let Ok(ip) = self.host.parse::<std::net::Ipv4Addr>() {
            return Ok(SocketAddr::new(IpAddr::V4(ip), self.port));
        }

        // DNS resolution
        let mut addrs = format!("{}:{}", self.host, self.port)
            .to_socket_addrs()
            .map_err(|e| format!("DNS resolution failed: {}", e))?;
        addrs
            .next()
            .ok_or_else(|| format!("no addresses resolved for {}", self.host))
    }
}

/// Get the interface index (ifindex) for a network interface name.
fn interface_index(name: &str) -> Result<u32, String> {
    // Read /sys/class/net/<name>/ifindex
    let path = format!("/sys/class/net/{}/ifindex", name);
    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("cannot read {}: {}", path, e))?;
    content
        .trim()
        .parse::<u32>()
        .map_err(|e| format!("bad ifindex for {}: {}", name, e))
}

/// An HTTP response.
#[derive(Debug)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: String,
}

/// Minimal HTTP client for GTPR/GDPR.
pub struct HttpClient {
    connect_timeout: Duration,
    read_timeout: Duration,
}

impl HttpClient {
    pub fn new(connect_timeout: Duration, read_timeout: Duration) -> Self {
        Self {
            connect_timeout,
            read_timeout,
        }
    }

    /// Send an HTTP POST request.
    pub fn post(
        &self,
        url: &str,
        headers: &[(&str, &str)],
        body: &str,
    ) -> Result<HttpResponse, String> {
        self.request("POST", url, headers, Some(body))
    }

    /// Send an HTTP GET request.
    pub fn get(&self, url: &str, headers: &[(&str, &str)]) -> Result<HttpResponse, String> {
        self.request("GET", url, headers, None)
    }

    fn request(
        &self,
        method: &str,
        url: &str,
        extra_headers: &[(&str, &str)],
        body: Option<&str>,
    ) -> Result<HttpResponse, String> {
        let parsed = ParsedUrl::parse(url)?;
        let addr = parsed.to_socket_addr()?;

        let mut stream =
            TcpStream::connect_timeout(&addr, self.connect_timeout).map_err(|e| {
                format!(
                    "Connection Failed: Connect error: {} (os error {})",
                    e,
                    e.raw_os_error().unwrap_or(0)
                )
            })?;
        stream
            .set_read_timeout(Some(self.read_timeout))
            .map_err(|e| format!("set_read_timeout: {}", e))?;

        // Build the HTTP request
        let host_header = if let Some(ref scope) = parsed.scope {
            format!("[{}%{}]", parsed.host, scope)
        } else if parsed.host.parse::<Ipv6Addr>().is_ok() {
            format!("[{}]", parsed.host)
        } else {
            parsed.host.clone()
        };

        let mut request = format!(
            "{} {} HTTP/1.1\r\nHost: {}:{}\r\nConnection: close\r\n",
            method, parsed.path, host_header, parsed.port
        );

        for (k, v) in extra_headers {
            request.push_str(&format!("{}: {}\r\n", k, v));
        }

        if let Some(body) = body {
            request.push_str(&format!("Content-Length: {}\r\n", body.len()));
        }

        request.push_str("\r\n");
        if let Some(body) = body {
            request.push_str(body);
        }

        // Send
        stream
            .write_all(request.as_bytes())
            .map_err(|e| format!("write: {}", e))?;

        // Read response
        let mut reader = BufReader::new(&stream);
        let mut status_line = String::new();
        reader
            .read_line(&mut status_line)
            .map_err(|e| format!("read status: {}", e))?;

        // Some firmware endpoints (e.g. GTPR so) return a bare content-length
        // line followed by that many bytes instead of a full HTTP response.
        if let Ok(len) = status_line.trim().parse::<usize>() {
            if len == 0 {
                return Ok(HttpResponse {
                    status: 200,
                    headers: HashMap::new(),
                    body: String::new(),
                });
            }
            let mut body_buf = vec![0u8; len];
            reader
                .read_exact(&mut body_buf)
                .map_err(|e| format!("read body after bare length: {}", e))?;
            return Ok(HttpResponse {
                status: 200,
                headers: HashMap::new(),
                body: String::from_utf8_lossy(&body_buf).to_string(),
            });
        }

        let status = parse_status(&status_line)?;

        // Read headers
        let mut headers = HashMap::new();
        let mut content_length: usize = 0;
        loop {
            let mut line = String::new();
            reader
                .read_line(&mut line)
                .map_err(|e| format!("read header: {}", e))?;
            let trimmed = line.trim().to_string();
            if trimmed.is_empty() {
                break;
            }
            if let Some((k, v)) = trimmed.split_once(':') {
                let k = k.trim().to_lowercase();
                let v = v.trim().to_string();
                if k == "content-length" {
                    content_length = v.parse().unwrap_or(0);
                }
                // Store original-case header for Set-Cookie
                let orig_k = trimmed.split_once(':').unwrap().0.trim().to_string();
                headers.insert(orig_k, v);
            }
        }

        // Read body — handle both Content-Length and Transfer-Encoding: chunked
        let is_chunked = headers
            .get("Transfer-Encoding")
            .map(|v| v.to_lowercase().contains("chunked"))
            .unwrap_or(false);

        let body_str = if is_chunked {
            read_chunked_body(&mut reader)?
        } else if content_length > 0 {
            let mut body_buf = vec![0u8; content_length];
            reader
                .read_exact(&mut body_buf)
                .map_err(|e| format!("read body: {}", e))?;
            String::from_utf8_lossy(&body_buf).to_string()
        } else {
            String::new()
        };

        Ok(HttpResponse {
            status,
            headers,
            body: body_str,
        })
    }
}

/// Read an HTTP/1.1 chunked transfer-encoded body.
fn read_chunked_body<R: Read>(reader: &mut BufReader<R>) -> Result<String, String> {
    let mut body = Vec::new();
    loop {
        let mut size_line = String::new();
        reader
            .read_line(&mut size_line)
            .map_err(|e| format!("chunked read size: {}", e))?;
        let size_str = size_line.trim();
        let chunk_size = usize::from_str_radix(size_str, 16)
            .map_err(|e| format!("bad chunk size '{}': {}", size_str, e))?;
        if chunk_size == 0 {
            // Read trailing \r\n after final chunk
            let mut trail = String::new();
            let _ = reader.read_line(&mut trail);
            break;
        }
        let mut chunk = vec![0u8; chunk_size];
        reader
            .read_exact(&mut chunk)
            .map_err(|e| format!("chunked read data: {}", e))?;
        body.extend_from_slice(&chunk);
        // Read trailing \r\n after chunk data
        let mut crlf = String::new();
        reader
            .read_line(&mut crlf)
            .map_err(|e| format!("chunked read crlf: {}", e))?;
    }
    Ok(String::from_utf8_lossy(&body).to_string())
}

fn parse_status(line: &str) -> Result<u16, String> {
    // "HTTP/1.1 200 OK\r\n" or a bare numeric status (e.g. "40\r\n").
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.is_empty() {
        return Err(format!("bad status line: {}", line.trim()));
    }
    let code = if parts.len() >= 2 { parts[1] } else { parts[0] };
    code.parse::<u16>()
        .map_err(|e| format!("bad status code: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_ipv4_url() {
        let u = ParsedUrl::parse("http://192.168.0.1:8080/path").unwrap();
        assert_eq!(u.host, "192.168.0.1");
        assert_eq!(u.port, 8080);
        assert_eq!(u.path, "/path");
        assert!(u.scope.is_none());
    }

    #[test]
    fn parse_ipv6_global() {
        let u = ParsedUrl::parse("http://[2001:db8::1]/path").unwrap();
        assert_eq!(u.host, "2001:db8::1");
        assert_eq!(u.port, 80);
        assert_eq!(u.path, "/path");
        assert!(u.scope.is_none());
    }

    #[test]
    fn parse_ipv6_link_local_with_scope() {
        let u = ParsedUrl::parse("http://[fe80::3e6a:d2ff:fe5f:abc1%25enp2s0]/cgi/getGDPRParm")
            .unwrap();
        assert_eq!(u.host, "fe80::3e6a:d2ff:fe5f:abc1");
        assert_eq!(u.port, 80);
        assert_eq!(u.path, "/cgi/getGDPRParm");
        assert_eq!(u.scope.as_deref(), Some("enp2s0"));
    }

    #[test]
    fn parse_ipv6_link_local_raw_scope() {
        // Also accept the unencoded % form
        let u = ParsedUrl::parse("http://[fe80::1%25wlan0]:80/test").unwrap();
        assert_eq!(u.host, "fe80::1");
        assert_eq!(u.port, 80);
        assert_eq!(u.path, "/test");
        assert_eq!(u.scope.as_deref(), Some("wlan0"));
    }

    #[test]
    fn parse_default_port() {
        let u = ParsedUrl::parse("http://192.168.0.1/").unwrap();
        assert_eq!(u.port, 80);
    }

    #[test]
    fn parse_no_path() {
        let u = ParsedUrl::parse("http://192.168.0.1").unwrap();
        assert_eq!(u.path, "/");
    }

    #[test]
    fn parse_ipv6_no_scope_fails_resolving() {
        let u = ParsedUrl::parse("http://[fe80::1]/test").unwrap();
        assert!(u.to_socket_addr().is_err());
    }

    #[test]
    fn parse_ipv4_resolves() {
        let u = ParsedUrl::parse("http://127.0.0.1:18099/test").unwrap();
        let addr = u.to_socket_addr().unwrap();
        assert_eq!(addr.to_string(), "127.0.0.1:18099");
    }
}
