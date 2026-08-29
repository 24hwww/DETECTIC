//! Minimal CWMP (TR-069) SOAP handler for self-bootstrap.
//!
//! Handles the subset of CWMP RPCs needed for the EX520 autonomous persistence
//! loop.  No external XML library — all parsing is string-based to keep the
//! on-router binary tiny.
//!
//! Supported methods (Phase 1):
//!   - Inform → InformResponse
//!
//! Phase 5 will add:
//!   - SetParameterValues → SetParameterValuesResponse
//!   - GetParameterValues → GetParameterValuesResponse

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Parsed CWMP request from the EX520 stock cwmp client.
#[derive(Debug, Clone)]
pub struct CwmpRequest {
    /// The `cwmp:ID` header value (session correlation).
    pub id: String,
    /// The CWMP RPC method name (e.g. "Inform", "SetParameterValues").
    pub method: CwmpMethod,
    /// Raw body for deep parsing.
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CwmpMethod {
    Inform,
    GetParameterValues,
    SetParameterValues,
    Reboot,
    TransferComplete,
    GetRPCMethods,
    Unknown(String),
}

/// A CWMP SOAP response to send back to the stock cwmp client.
#[derive(Debug)]
pub struct CwmpResponse {
    pub status: u16,
    pub content_type: &'static str,
    pub body: Vec<u8>,
}

// ---------------------------------------------------------------------------
// SOAP envelope builder (namespace-aware, matches EX520 cwmp expectations)
// ---------------------------------------------------------------------------

/// CWMP SOAP envelope namespace used by the EX520 stock cwmp client.
/// The client sends with `xmlns:cwmp="urn:dslforum-org:cwmp-1-0"` and expects
/// responses in the same namespace.
const SOAP_ENVELOPE_OPEN: &str = "\
<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
<SOAP-ENV:Envelope\
 xmlns:SOAP-ENV=\"http://schemas.xmlsoap.org/soap/envelope/\"\
 SOAP-ENV:encodingStyle=\"http://schemas.xmlsoap.org/soap/encoding/\"\
 xmlns:SOAP-ENC=\"http://schemas.xmlsoap.org/soap/encoding/\"\
 xmlns:xsd=\"http://www.w3.org/2001/XMLSchema\"\
 xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\"\
 xmlns:cwmp=\"urn:dslforum-org:cwmp-1-0\">\
<SOAP-ENV:Header>\
<cwmp:ID SOAP-ENV:mustUnderstand=\"1\">";

const SOAP_ENVELOPE_MID: &str = "</cwmp:ID>\
</SOAP-ENV:Header>\
<SOAP-ENV:Body>";

const SOAP_ENVELOPE_CLOSE: &str = "\
</SOAP-ENV:Body>\
</SOAP-ENV:Envelope>";

fn soap_envelope(cid: &str, body: &str) -> String {
    format!(
        "{}{}{}{}{}",
        SOAP_ENVELOPE_OPEN,
        escape_xml(cid),
        SOAP_ENVELOPE_MID,
        body,
        SOAP_ENVELOPE_CLOSE
    )
}

fn escape_xml(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '&' => "&amp;".to_string(),
            '<' => "&lt;".to_string(),
            '>' => "&gt;".to_string(),
            '"' => "&quot;".to_string(),
            '\'' => "&apos;".to_string(),
            c => c.to_string(),
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Response builders
// ---------------------------------------------------------------------------

fn inform_response(cid: &str) -> CwmpResponse {
    let body = soap_envelope(
        cid,
        "<cwmp:InformResponse><MaxEnvelopes>1</MaxEnvelopes></cwmp:InformResponse>",
    );
    CwmpResponse {
        status: 200,
        content_type: "text/xml; charset=utf-8",
        body: body.into_bytes(),
    }
}

fn fault_response(cid: &str, code: i32, message: &str) -> CwmpResponse {
    let body = soap_envelope(
        cid,
        &format!(
            "<SOAP-ENV:Fault>\
<faultcode>Client</faultcode>\
<faultstring>CWMP fault</faultstring>\
<detail>\
<cwmp:Fault>\
<FaultCode>{}</FaultCode>\
<FaultString>{}</FaultString>\
</cwmp:Fault>\
</detail>\
</SOAP-ENV:Fault>",
            code,
            escape_xml(message)
        ),
    );
    CwmpResponse {
        status: 200,
        content_type: "text/xml; charset=utf-8",
        body: body.into_bytes(),
    }
}

fn empty_200() -> CwmpResponse {
    CwmpResponse {
        status: 200,
        content_type: "text/xml; charset=utf-8",
        body: vec![],
    }
}

// ---------------------------------------------------------------------------
// SOAP parser (namespace-tolerant, string-based)
// ---------------------------------------------------------------------------

/// Extract text content between an XML tag, tolerating namespace prefixes.
/// Handles `<cwmp:Foo>...</cwmp:Foo>` and `<Foo>...</Foo>` and any prefix.
fn extract_tag<'a>(xml: &'a str, local_name: &str) -> Option<&'a str> {
    // Try with common prefixes
    for prefix in &["cwmp:", "SOAP-ENV:", "SOAP-ENC:", ""] {
        let open = format!("<{}{}>", prefix, local_name);
        let close = format!("</{}{}>", prefix, local_name);
        if let Some(start) = xml.find(&open) {
            let content_start = start + open.len();
            if let Some(end) = xml[content_start..].find(&close) {
                return Some(&xml[content_start..content_start + end]);
            }
        }
    }
    None
}

/// Extract the `cwmp:ID` value from the SOAP header.
fn extract_cwmp_id(body: &str) -> String {
    // Try standard cwmp:ID
    if let Some(id) = extract_tag(body, "ID") {
        return id.trim().to_string();
    }
    // Fallback: regex-like search for cwmp:ID>...</
    if let Some(start) = body.find("cwmp:ID") {
        let after = &body[start..];
        if let Some(gt) = after.find('>') {
            let content = &after[gt + 1..];
            if let Some(end) = content.find('<') {
                return content[..end].trim().to_string();
            }
        }
    }
    "0".to_string()
}

/// Detect the CWMP method from the SOAP body.
fn detect_method(body: &str) -> CwmpMethod {
    // Check for known method tags (with or without cwmp: prefix)
    let methods = [
        ("Inform", CwmpMethod::Inform),
        ("GetParameterValues", CwmpMethod::GetParameterValues),
        ("SetParameterValues", CwmpMethod::SetParameterValues),
        ("Reboot", CwmpMethod::Reboot),
        ("TransferComplete", CwmpMethod::TransferComplete),
        ("GetRPCMethods", CwmpMethod::GetRPCMethods),
    ];

    for (name, variant) in &methods {
        // Check with cwmp: prefix (both open and self-closing tags)
        if body.contains(&format!("cwmp:{}>", name))
            || body.contains(&format!("cwmp:{} ", name))
            || body.contains(&format!("cwmp:{}/>", name))
        {
            return variant.clone();
        }
        // Check without prefix (some clients may omit it)
        if body.contains(&format!("<{}>", name))
            || body.contains(&format!("<{} ", name))
            || body.contains(&format!("<{}/>", name))
        {
            return variant.clone();
        }
    }

    // Extract the first element inside SOAP-ENV:Body for unknown methods
    if let Some(body_start) = body.find("<SOAP-ENV:Body>") {
        let after_body = &body[body_start + 15..];
        if let Some(tag_start) = after_body.find('<') {
            let tag = &after_body[tag_start + 1..];
            let end_chars = ['>', ' ', '/'];
            if let Some(end) = tag.find(|c: char| end_chars.contains(&c)) {
                return CwmpMethod::Unknown(tag[..end].to_string());
            }
        }
    }

    // Last resort: find first XML tag in the body
    if let Some(tag_start) = body.find('<') {
        let tag = &body[tag_start + 1..];
        let end_chars = ['>', ' ', '/'];
        if let Some(end) = tag.find(|c: char| end_chars.contains(&c)) {
            let name = &tag[..end];
            // Strip namespace prefix (e.g. "cwmp:" -> "FooBar")
            if let Some(colon) = name.rfind(':') {
                return CwmpMethod::Unknown(name[colon + 1..].to_string());
            }
            return CwmpMethod::Unknown(name.to_string());
        }
    }

    CwmpMethod::Unknown("empty".to_string())
}

/// Parse EventCodes from an Inform body.
/// Returns a list of (code, description) tuples.
pub fn parse_event_codes(body: &str) -> Vec<String> {
    let mut codes = Vec::new();
    let mut pos = 0;
    while let Some(start) = body[pos..].find("<EventCode>") {
        let abs_start = pos + start + 11; // len("<EventCode>")
        if let Some(end) = body[abs_start..].find("</EventCode>") {
            codes.push(body[abs_start..abs_start + end].trim().to_string());
            pos = abs_start + end + 12; // len("</EventCode>")
        } else {
            break;
        }
    }
    codes
}

/// Parse DeviceId from an Inform body.
pub fn parse_device_id(body: &str) -> DeviceId {
    DeviceId {
        manufacturer: extract_tag(body, "Manufacturer").unwrap_or("").to_string(),
        oui: extract_tag(body, "OUI").unwrap_or("").to_string(),
        product_class: extract_tag(body, "ProductClass").unwrap_or("").to_string(),
        serial_number: extract_tag(body, "SerialNumber").unwrap_or("").to_string(),
    }
}

#[derive(Debug, Clone, Default)]
pub struct DeviceId {
    pub manufacturer: String,
    pub oui: String,
    pub product_class: String,
    pub serial_number: String,
}

/// Parse ParameterValueStruct entries from a SetParameterValues body.
pub fn parse_set_parameter_values(body: &str) -> Vec<ParameterBinding> {
    let mut params = Vec::new();
    let mut pos = 0;
    while let Some(start) = body[pos..].find("<ParameterValueStruct>") {
        let abs_start = pos + start + 22;
        if let Some(end) = body[abs_start..].find("</ParameterValueStruct>") {
            let block = &body[abs_start..abs_start + end];
            let name = extract_tag(block, "Name").unwrap_or("").to_string();
            // Value may have xsi:type attribute — extract just the text content
            let value = if let Some(vstart) = block.find("<Value") {
                let after_tag = &block[vstart..];
                if let Some(gt) = after_tag.find('>') {
                    let content = &after_tag[gt + 1..];
                    if let Some(clt) = content.find('<') {
                        content[..clt].trim().to_string()
                    } else {
                        String::new()
                    }
                } else {
                    String::new()
                }
            } else {
                extract_tag(block, "Value").unwrap_or("").to_string()
            };
            if !name.is_empty() {
                params.push(ParameterBinding { name, value });
            }
            pos = abs_start + end + 22;
        } else {
            break;
        }
    }
    params
}

#[derive(Debug, Clone)]
pub struct ParameterBinding {
    pub name: String,
    pub value: String,
}

/// Parse ParameterNames from a GetParameterValues body.
pub fn parse_get_parameter_names(body: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut pos = 0;
    while let Some(start) = body[pos..].find("<string>") {
        let abs_start = pos + start + 8;
        if let Some(end) = body[abs_start..].find("</string>") {
            names.push(body[abs_start..abs_start + end].trim().to_string());
            pos = abs_start + end + 9;
        } else {
            break;
        }
    }
    names
}

// ---------------------------------------------------------------------------
// Main request handler
// ---------------------------------------------------------------------------

/// Handle a CWMP POST request from the stock EX520 cwmp client.
///
/// Returns a `CwmpResponse` with the appropriate SOAP body.
pub fn handle_cwmp_request(body: &str) -> CwmpResponse {
    let id = extract_cwmp_id(body);
    let method = detect_method(body);

    eprintln!(
        "[CWMP RX] id={} method={:?} body_len={}",
        id,
        method,
        body.len()
    );

    match method {
        CwmpMethod::Inform => {
            let event_codes = parse_event_codes(body);
            let device = parse_device_id(body);
            eprintln!(
                "[CWMP INFORM] events={:?} device={}:{}",
                event_codes, device.oui, device.serial_number
            );
            let resp = inform_response(&id);
            eprintln!("[CWMP TX] InformResponse status=200");
            resp
        }
        CwmpMethod::GetParameterValues => {
            eprintln!("[CWMP GETPV] acknowledged (Phase 5)");
            // Phase 5: respond with actual parameter values
            // For now, return empty 200 to keep the session alive
            empty_200()
        }
        CwmpMethod::SetParameterValues => {
            let params = parse_set_parameter_values(body);
            eprintln!("[CWMP SETPV] params={:?}", params);
            // Phase 5: apply parameter changes and respond
            empty_200()
        }
        CwmpMethod::Reboot => {
            eprintln!("[CWMP REBOOT] acknowledged");
            empty_200()
        }
        CwmpMethod::TransferComplete => {
            eprintln!("[CWMP TRANSFER_COMPLETE] acknowledged");
            empty_200()
        }
        CwmpMethod::GetRPCMethods => {
            eprintln!("[CWMP GETRPCMETHODS] acknowledged");
            empty_200()
        }
        CwmpMethod::Unknown(name) => {
            eprintln!("[CWMP UNKNOWN] method={}", name);
            fault_response(&id, 9000, "Method not supported")
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_cwmp_id_standard() {
        let xml = r#"<SOAP-ENV:Envelope>
<SOAP-ENV:Header>
<cwmp:ID SOAP-ENV:mustUnderstand="1">12345</cwmp:ID>
</SOAP-ENV:Header>
<SOAP-ENV:Body>
<cwmp:Inform/>
</SOAP-ENV:Body>
</SOAP-ENV:Envelope>"#;
        assert_eq!(extract_cwmp_id(xml), "12345");
    }

    #[test]
    fn extract_cwmp_id_no_prefix() {
        let xml = r#"<Header><ID>99999</ID></Header>"#;
        assert_eq!(extract_cwmp_id(xml), "99999");
    }

    #[test]
    fn detect_method_inform() {
        let xml = r#"<cwmp:Inform>"#;
        assert_eq!(detect_method(xml), CwmpMethod::Inform);
    }

    #[test]
    fn detect_method_set_pv() {
        let xml = r#"<cwmp:SetParameterValues>"#;
        assert_eq!(detect_method(xml), CwmpMethod::SetParameterValues);
    }

    #[test]
    fn detect_method_reboot() {
        let xml = r#"<cwmp:Reboot>"#;
        assert_eq!(detect_method(xml), CwmpMethod::Reboot);
    }

    #[test]
    fn detect_method_unknown() {
        let xml = r#"<cwmp:FooBar/>"#;
        match detect_method(xml) {
            CwmpMethod::Unknown(name) => assert_eq!(name, "FooBar"),
            _ => panic!("expected Unknown"),
        }
    }

    #[test]
    fn parse_event_codes_standard() {
        let body = r#"<Event>
<EventCode>0 BOOTSTRAP</EventCode>
<EventCode>1 BOOT</EventCode>
</Event>"#;
        let codes = parse_event_codes(body);
        assert_eq!(codes, vec!["0 BOOTSTRAP", "1 BOOT"]);
    }

    #[test]
    fn parse_event_codes_empty() {
        let body = r#"<Event></Event>"#;
        let codes = parse_event_codes(body);
        assert!(codes.is_empty());
    }

    #[test]
    fn parse_device_id_fields() {
        let body = r#"<DeviceId>
<Manufacturer>TP-Link</Manufacturer>
<OUI>000000</OUI>
<ProductClass>EX520</ProductClass>
<SerialNumber>ABC123</SerialNumber>
</DeviceId>"#;
        let id = parse_device_id(body);
        assert_eq!(id.manufacturer, "TP-Link");
        assert_eq!(id.oui, "000000");
        assert_eq!(id.product_class, "EX520");
        assert_eq!(id.serial_number, "ABC123");
    }

    #[test]
    fn test_parse_set_parameter_values() {
        let body = r#"<SetParameterValues>
<ParameterList>
<ParameterValueStruct>
<Name>Device.X_TP_LIFEMOTE_EXT.LifemoteAgent.Enable</Name>
<Value xsi:type="xsd:boolean">1</Value>
</ParameterValueStruct>
<ParameterValueStruct>
<Name>Device.X_TP_LIFEMOTE_EXT.LifemoteAgent.URL</Name>
<Value xsi:type="xsd:string">http://192.168.0.1:8080/bootstart.sh</Value>
</ParameterValueStruct>
</ParameterList>
</SetParameterValues>"#;
        let params = super::parse_set_parameter_values(body);
        assert_eq!(params.len(), 2);
        assert_eq!(
            params[0].name,
            "Device.X_TP_LIFEMOTE_EXT.LifemoteAgent.Enable"
        );
        assert_eq!(params[0].value, "1");
        assert_eq!(params[1].name, "Device.X_TP_LIFEMOTE_EXT.LifemoteAgent.URL");
        assert_eq!(params[1].value, "http://192.168.0.1:8080/bootstart.sh");
    }

    #[test]
    fn test_parse_get_parameter_names() {
        let body = r#"<GetParameterValues>
<ParameterNames>
<string>Device.X_TP_LIFEMOTE_EXT.LifemoteAgent.Enable</string>
<string>Device.X_TP_LIFEMOTE_EXT.LifemoteAgent.URL</string>
</ParameterNames>
</GetParameterValues>"#;
        let names = super::parse_get_parameter_names(body);
        assert_eq!(names.len(), 2);
        assert_eq!(names[0], "Device.X_TP_LIFEMOTE_EXT.LifemoteAgent.Enable");
        assert_eq!(names[1], "Device.X_TP_LIFEMOTE_EXT.LifemoteAgent.URL");
    }

    #[test]
    fn inform_response_valid_soap() {
        let resp = inform_response("test-123");
        assert_eq!(resp.status, 200);
        let body = String::from_utf8_lossy(&resp.body);
        assert!(body.contains("test-123"));
        assert!(body.contains("InformResponse"));
        assert!(body.contains("MaxEnvelopes"));
        assert!(body.contains("urn:dslforum-org:cwmp-1-0"));
    }

    #[test]
    fn fault_response_valid_soap() {
        let resp = fault_response("err-1", 9000, "not supported");
        assert_eq!(resp.status, 200);
        let body = String::from_utf8_lossy(&resp.body);
        assert!(body.contains("9000"));
        assert!(body.contains("not supported"));
        assert!(body.contains("Fault"));
    }

    #[test]
    fn handle_inform_request() {
        let body = r#"<?xml version="1.0" encoding="UTF-8"?>
<SOAP-ENV:Envelope xmlns:SOAP-ENV="http://schemas.xmlsoap.org/soap/envelope/" xmlns:cwmp="urn:dslforum-org:cwmp-1-0">
<SOAP-ENV:Header>
<cwmp:ID SOAP-ENV:mustUnderstand="1">INFORM-001</cwmp:ID>
</SOAP-ENV:Header>
<SOAP-ENV:Body>
<cwmp:Inform>
<DeviceId>
<Manufacturer>TP-Link</Manufacturer>
<OUI>3C6AD2</OUI>
<ProductClass>EX520</ProductClass>
<SerialNumber>123456</SerialNumber>
</DeviceId>
<Event>
<EventCode>1 BOOT</EventCode>
</Event>
</cwmp:Inform>
</SOAP-ENV:Body>
</SOAP-ENV:Envelope>"#;
        let resp = handle_cwmp_request(body);
        assert_eq!(resp.status, 200);
        let body_str = String::from_utf8_lossy(&resp.body);
        assert!(body_str.contains("INFORM-001"));
        assert!(body_str.contains("InformResponse"));
    }

    #[test]
    fn handle_empty_body() {
        let resp = handle_cwmp_request("");
        assert_eq!(resp.status, 200);
    }

    #[test]
    fn escape_xml_special_chars() {
        assert_eq!(escape_xml("a&b"), "a&amp;b");
        assert_eq!(escape_xml("<tag>"), "&lt;tag&gt;");
        assert_eq!(escape_xml("a\"b"), "a&quot;b");
    }

    #[test]
    fn soap_envelope_structure() {
        let env = soap_envelope("id1", "<body/>");
        assert!(env.starts_with("<?xml"));
        assert!(env.contains("id1"));
        assert!(env.contains("<body/>"));
        assert!(env.ends_with("</SOAP-ENV:Envelope>"));
    }
}
