use std::time::Duration;

fn field(list: &str, key: &str) -> String {
    for seg in list.split(',') {
        let seg = seg.trim().trim_start_matches('{');
        if let Some(rest) = seg.strip_prefix(&format!("\"{}\":", key)) {
            return rest.trim_matches(|c| c == ' ' || c == '"').to_string();
        }
    }
    "?".into()
}

fn main() {
    let url = std::env::var("DETECTIC_URL").unwrap();
    let user = std::env::var("DETECTIC_USER").unwrap();
    let pass = std::env::var("DETECTIC_PASSWORD").unwrap();

    let mut t = None;
    for attempt in 1..=10u32 {
        let mut c = detectic::transport::GtprClient::new(&url, &user, &pass);
        match c.connect() {
            Ok(()) => { t = Some(c); break; }
            Err(e) => { eprintln!("[retry {attempt}] login: {e}"); std::thread::sleep(Duration::from_secs(4)); }
        }
    }
    let t = t.expect("login failed after retries");

    // Pre-state
    let list = t.gl("DEV2_ADT_WAN").unwrap();
    println!("PRE : v4={} ip={} gw={} v6={}", field(&list,"connStatusV4"), field(&list,"connIPv4Address"), field(&list,"connIPv4Gateway"), field(&list,"connStatusV6"));

    // DISCONNECT
    println!("{}", t.op_with_data("ACT_PPP_DISCONN", r#"{"stack":"1,0,0,0,0,0","pstack":"0,0,0,0,0,0"}"#).unwrap());
    let mut disconnected = false;
    for _ in 0..8 {
        std::thread::sleep(Duration::from_secs(4));
        let l = t.gl("DEV2_ADT_WAN").unwrap();
        let s = field(&l, "connStatusV4");
        println!("post-disc poll: v4={s}");
        if s != "Connected" { disconnected = true; break; }
    }
    println!("disconnected_observed={disconnected}");

    // CONNECT
    match t.op_with_data("ACT_PPP_CONN", r#"{"stack":"1,0,0,0,0,0","pstack":"0,0,0,0,0,0"}"#) {
        Ok(r) => println!("CONN resp: {r}"),
        Err(e) => eprintln!("CONN err: {e}"),
    }
    let mut final_v4 = String::new();
    for i in 0..36 {
        std::thread::sleep(Duration::from_secs(5));
        let l = match t.gl("DEV2_ADT_WAN") { Ok(l) => l, Err(e) => { eprintln!("poll err {e}"); continue; } };
        let s = field(&l, "connStatusV4");
        let ip = field(&l, "connIPv4Address");
        println!("reconnect poll{i}: v4={s} ip={ip}");
        if s == "Connected" && !ip.is_empty() && ip != "?" { final_v4 = ip; break; }
    }
    println!("FINAL_IP={final_v4}");
}
