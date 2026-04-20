use serde::{Deserialize, Serialize};

/// JSON-RPC 2.0 version string — a hard constant on every envelope.
pub const JSONRPC_VERSION: &str = "2.0";

/// Request-response id. `u64` because we generate them monotonically client-side.
pub type RequestId = u64;

/// Standard JSON-RPC error codes.
pub mod error_code {
    pub const PARSE_ERROR: i32 = -32700;
    pub const INVALID_REQUEST: i32 = -32600;
    pub const METHOD_NOT_FOUND: i32 = -32601;
    pub const INVALID_PARAMS: i32 = -32602;
    pub const INTERNAL_ERROR: i32 = -32603;

    // Application-defined codes (JSON-RPC reserves -32000 to -32099 for
    // server errors; application codes should be outside that range).
    pub const PERMISSION_DENIED: i32 = 1001;
    pub const NOT_FOUND: i32 = 1002;
    pub const VERSION_MISMATCH: i32 = 1003;
    pub const AUTH_REQUIRED: i32 = 1004;
    pub const AUTH_INVALID: i32 = 1005;
    pub const PAIRING_REJECTED: i32 = 1006;
}

/// A JSON-RPC 2.0 error object.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl RpcError {
    pub fn new(code: i32, message: impl Into<String>) -> Self {
        Self { code, message: message.into(), data: None }
    }

    pub fn with_data(mut self, data: serde_json::Value) -> Self {
        self.data = Some(data);
        self
    }

    pub fn method_not_found(method: &str) -> Self {
        Self::new(error_code::METHOD_NOT_FOUND, format!("method not found: {method}"))
    }

    pub fn invalid_params(msg: impl Into<String>) -> Self {
        Self::new(error_code::INVALID_PARAMS, msg)
    }

    pub fn internal(msg: impl Into<String>) -> Self {
        Self::new(error_code::INTERNAL_ERROR, msg)
    }
}

/// A JSON-RPC request: has an `id`, expects a matching response.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Request {
    pub jsonrpc: String,
    pub id: RequestId,
    pub method: String,
    #[serde(default = "default_params")]
    pub params: serde_json::Value,
}

/// A JSON-RPC response: has an `id` matching a prior request, and either
/// `result` (success) or `error` (failure) — exactly one.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Response {
    pub jsonrpc: String,
    pub id: RequestId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

/// A JSON-RPC notification: fire-and-forget, no `id`, no response expected.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Notification {
    pub jsonrpc: String,
    pub method: String,
    #[serde(default = "default_params")]
    pub params: serde_json::Value,
}

fn default_params() -> serde_json::Value {
    serde_json::Value::Null
}

/// Any incoming JSON-RPC message — request, response, or notification.
#[derive(Debug, Clone, PartialEq)]
pub enum Message {
    Request(Request),
    Response(Response),
    Notification(Notification),
}

impl Request {
    pub fn new(id: RequestId, method: impl Into<String>, params: serde_json::Value) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION.to_string(),
            id,
            method: method.into(),
            params,
        }
    }
}

impl Response {
    pub fn success(id: RequestId, result: serde_json::Value) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION.to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn error(id: RequestId, error: RpcError) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION.to_string(),
            id,
            result: None,
            error: Some(error),
        }
    }
}

impl Notification {
    pub fn new(method: impl Into<String>, params: serde_json::Value) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION.to_string(),
            method: method.into(),
            params,
        }
    }
}

/// Errors from decoding a JSON-RPC envelope off the wire.
#[derive(Debug, thiserror::Error)]
pub enum DecodeError {
    #[error("invalid json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("missing jsonrpc field")]
    MissingJsonRpc,
    #[error("wrong jsonrpc version: expected \"2.0\", got {0:?}")]
    WrongVersion(String),
    #[error("malformed message: could not classify as request, response, or notification")]
    Malformed,
}

/// Parse one JSON-RPC message from a JSON string.
///
/// Strategy: parse as generic JSON, inspect which fields are present, and
/// deserialize into the specific variant. This avoids the ambiguity that
/// untagged serde enums run into with the JSON-RPC schema.
pub fn decode(text: &str) -> Result<Message, DecodeError> {
    let raw: serde_json::Value = serde_json::from_str(text)?;
    let obj = raw.as_object().ok_or(DecodeError::Malformed)?;

    let jsonrpc = obj
        .get("jsonrpc")
        .and_then(|v| v.as_str())
        .ok_or(DecodeError::MissingJsonRpc)?;
    if jsonrpc != JSONRPC_VERSION {
        return Err(DecodeError::WrongVersion(jsonrpc.to_string()));
    }

    let has_id = obj.contains_key("id");
    let has_method = obj.contains_key("method");

    match (has_id, has_method) {
        (true, true) => {
            let req: Request = serde_json::from_value(raw)?;
            Ok(Message::Request(req))
        }
        (true, false) => {
            let resp: Response = serde_json::from_value(raw)?;
            Ok(Message::Response(resp))
        }
        (false, true) => {
            let note: Notification = serde_json::from_value(raw)?;
            Ok(Message::Notification(note))
        }
        (false, false) => Err(DecodeError::Malformed),
    }
}

/// Serialize a message as a single JSON string (no trailing newline).
pub fn encode_request(req: &Request) -> Result<String, serde_json::Error> {
    serde_json::to_string(req)
}

pub fn encode_response(resp: &Response) -> Result<String, serde_json::Error> {
    serde_json::to_string(resp)
}

pub fn encode_notification(note: &Notification) -> Result<String, serde_json::Error> {
    serde_json::to_string(note)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn roundtrip_request() {
        let req = Request::new(42, "tabs.list", json!({}));
        let wire = encode_request(&req).unwrap();
        let decoded = decode(&wire).unwrap();
        match decoded {
            Message::Request(r) => assert_eq!(r, req),
            _ => panic!("expected request"),
        }
    }

    #[test]
    fn roundtrip_response_success() {
        let resp = Response::success(42, json!([{"tab_id": 1}]));
        let wire = encode_response(&resp).unwrap();
        let decoded = decode(&wire).unwrap();
        match decoded {
            Message::Response(r) => assert_eq!(r, resp),
            _ => panic!("expected response"),
        }
    }

    #[test]
    fn roundtrip_response_error() {
        let resp = Response::error(42, RpcError::method_not_found("foo.bar"));
        let wire = encode_response(&resp).unwrap();
        let decoded = decode(&wire).unwrap();
        match decoded {
            Message::Response(r) => assert_eq!(r, resp),
            _ => panic!("expected response"),
        }
    }

    #[test]
    fn roundtrip_notification() {
        let note = Notification::new("diff.file_changed", json!({"path": "x"}));
        let wire = encode_notification(&note).unwrap();
        let decoded = decode(&wire).unwrap();
        match decoded {
            Message::Notification(n) => assert_eq!(n, note),
            _ => panic!("expected notification"),
        }
    }

    #[test]
    fn rejects_wrong_version() {
        let wire = r#"{"jsonrpc":"1.0","id":1,"method":"foo","params":{}}"#;
        assert!(matches!(decode(wire), Err(DecodeError::WrongVersion(_))));
    }

    #[test]
    fn rejects_missing_version() {
        let wire = r#"{"id":1,"method":"foo","params":{}}"#;
        assert!(matches!(decode(wire), Err(DecodeError::MissingJsonRpc)));
    }

    #[test]
    fn rejects_malformed() {
        let wire = r#"{"jsonrpc":"2.0"}"#;
        assert!(matches!(decode(wire), Err(DecodeError::Malformed)));
    }

    #[test]
    fn default_params_is_null_when_absent() {
        let wire = r#"{"jsonrpc":"2.0","id":1,"method":"foo"}"#;
        let decoded = decode(wire).unwrap();
        match decoded {
            Message::Request(r) => assert_eq!(r.params, serde_json::Value::Null),
            _ => panic!("expected request"),
        }
    }
}
