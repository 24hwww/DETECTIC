use serde_json::json;
use std::env;
use std::io::Write;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tungstenite::protocol::CloseFrame;
use tungstenite::{client::connect, Message};
use url::Url;

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

fn log(label: &str, msg: &str) {
    println!("[{}] {} {}", now_ms(), label, msg);
    std::io::stdout().flush().ok();
}

fn as_text(m: Message) -> String {
    match m {
        Message::Text(s) => s,
        Message::Close(_) => "__close__".into(),
        _ => format!("{:?}", m),
    }
}

fn main() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    let url = env::var("WSS_URL")
        .unwrap_or_else(|_| {
            "wss://detectic.24hwww.workers.dev/ws?role=sensor&sensor_id=ex520-001".into()
        })
        .parse::<Url>()
        .expect("WSS_URL must be a valid URL");

    let start = Instant::now();
    let (mut socket, response) = connect(url.as_str()).expect("WSS connect failed");
    let open = start.elapsed();
    log("WSS_OPEN", &format!("status={:?} rtt_ms={:.2}", response.status(), open.as_millis()));

    // Server automatically sends hello_ack upon connection.
    let msg = as_text(socket.read().expect("read failed"));
    log("HELLO_ACK_AUTO", &format!("elapsed_ms={} payload={}", start.elapsed().as_millis(), msg));

    // 1. Client-initiated hello round-trip.
    let t0 = Instant::now();
    socket
        .send(Message::Text(json!({"type":"hello","protocol":1}).to_string()))
        .expect("send hello failed");
    let m1 = as_text(socket.read().expect("read hello ack failed"));
    let m2 = as_text(socket.read().expect("read command failed"));
    let hello_rtt = t0.elapsed();
    log("HELLO_RTT", &format!("{:.2} ms ack={} next={}", hello_rtt.as_millis(), m1, m2));

    // 2. Cloudflare-pushed command -> EX520 reply.
    //    m2 is the server's 'command' message (e.g. GET_STATUS).
    let t1 = Instant::now();
    socket
        .send(Message::Text(
            json!({"type":"command_ack","command":"GET_STATUS","protocol":1}).to_string(),
        ))
        .expect("send command_ack failed");
    let m3 = as_text(socket.read().expect("read command_ack failed"));
    let cmd_rtt = t1.elapsed();
    log("COMMAND_RTT", &format!("{:.2} ms server_push={} ack={}", cmd_rtt.as_millis(), m2, m3));

    // 3. Ping/pong round-trip.
    let t2 = Instant::now();
    socket
        .send(Message::Text(json!({"type":"ping","client_time":now_ms()}).to_string()))
        .expect("send ping failed");
    let m4 = as_text(socket.read().expect("read pong failed"));
    let ping_rtt = t2.elapsed();
    log("PING_RTT", &format!("{:.2} ms -> {}", ping_rtt.as_millis(), m4));

    // 4. Test echo.
    let t3 = Instant::now();
    socket
        .send(Message::Text(json!({"type":"test"}).to_string()))
        .expect("send test failed");
    let m5 = as_text(socket.read().expect("read test ack failed"));
    let test_rtt = t3.elapsed();
    log("TEST_RTT", &format!("{:.2} ms -> {}", test_rtt.as_millis(), m5));

    // 5. Event upload and ack.
    let t4 = Instant::now();
    socket
        .send(Message::Text(
            json!({
                "type": "event",
                "protocol": 1,
                "event_id": "evt-wss-probe-001",
                "sensor_id": "ex520-001",
                "observed_at": now_ms(),
                "payload": {
                    "device_id": "probe-device",
                    "rssi": -42,
                    "connection_state": "unknown"
                }
            })
            .to_string(),
        ))
        .expect("send event failed");
    let m6 = as_text(socket.read().expect("read event ack failed"));
    let event_rtt = t4.elapsed();
    log("EVENT_RTT", &format!("{:.2} ms -> {}", event_rtt.as_millis(), m6));

    // Give other clients a moment to receive broadcast, then close
    std::thread::sleep(Duration::from_millis(500));
    socket
        .close(Some(CloseFrame {
            code: tungstenite::protocol::frame::coding::CloseCode::Normal,
            reason: std::borrow::Cow::Borrowed("probe done"),
        }))
        .expect("close failed");

    while socket.read().is_ok() {}

    log("DONE", "WSS capability test completed");
}
