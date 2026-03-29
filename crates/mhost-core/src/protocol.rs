use serde::{Deserialize, Serialize};
use serde_json::Value;

// ---------------------------------------------------------------------------
// RpcRequest
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcRequest {
    pub jsonrpc: String,
    pub id: u64,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

impl RpcRequest {
    pub fn new(id: u64, method: impl Into<String>, params: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            method: method.into(),
            params,
        }
    }
}

// ---------------------------------------------------------------------------
// RpcResponse
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcResponse {
    pub jsonrpc: String,
    pub id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

impl RpcResponse {
    pub fn success(id: u64, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn error(id: u64, error: RpcError) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(error),
        }
    }
}

// ---------------------------------------------------------------------------
// RpcError
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl RpcError {
    pub fn new(code: i32, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }
}

// ---------------------------------------------------------------------------
// RpcEvent
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcEvent {
    pub jsonrpc: String,
    pub method: String,
    pub params: Value,
}

impl RpcEvent {
    pub fn new(method: impl Into<String>, params: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            method: method.into(),
            params,
        }
    }
}

// ---------------------------------------------------------------------------
// Method constants
// ---------------------------------------------------------------------------

pub mod methods {
    pub const PROCESS_START: &str = "process.start";
    pub const PROCESS_STOP: &str = "process.stop";
    pub const PROCESS_RESTART: &str = "process.restart";
    pub const PROCESS_DELETE: &str = "process.delete";
    pub const PROCESS_LIST: &str = "process.list";
    pub const PROCESS_INFO: &str = "process.info";
    pub const PROCESS_ENV: &str = "process.env";
    pub const PROCESS_SCALE: &str = "process.scale";
    pub const PROCESS_SAVE: &str = "process.save";
    pub const PROCESS_RESURRECT: &str = "process.resurrect";

    pub const DAEMON_PING: &str = "daemon.ping";
    pub const DAEMON_KILL: &str = "daemon.kill";
    pub const DAEMON_VERSION: &str = "daemon.version";

    pub const LOG_TAIL: &str = "log.tail";
    pub const LOG_FLUSH: &str = "log.flush";

    pub const EVENT_LOG: &str = "event.log";
    pub const EVENT_STATUS: &str = "event.status";

    pub const HEALTH_STATUS: &str = "health.status";
    pub const GROUP_START: &str = "group.start";
    pub const GROUP_STOP: &str = "group.stop";
    pub const GROUP_LIST: &str = "group.list";
    pub const PROCESS_CLUSTER: &str = "process.cluster";

    pub const LOG_SEARCH: &str = "log.search";
    pub const LOG_COUNT_BY: &str = "log.count_by";
    pub const METRICS_SHOW: &str = "metrics.show";
    pub const METRICS_HISTORY: &str = "metrics.history";
    pub const METRICS_START_PROMETHEUS: &str = "metrics.start_prometheus";
    pub const NOTIFY_TEST: &str = "notify.test";

    pub const DEPLOY_EXECUTE: &str = "deploy.execute";
    pub const DEPLOY_ROLLBACK: &str = "deploy.rollback";
    pub const PROXY_LIST: &str = "proxy.list";
    pub const PROXY_RELOAD: &str = "proxy.reload";
}

// ---------------------------------------------------------------------------
// Error codes
// ---------------------------------------------------------------------------

pub mod error_codes {
    pub const PROCESS_NOT_FOUND: i32 = -32000;
    pub const PROCESS_ALREADY_RUNNING: i32 = -32001;
    pub const INVALID_CONFIG: i32 = -32002;
    pub const SPAWN_FAILED: i32 = -32003;
    pub const INTERNAL_ERROR: i32 = -32603;
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // -- Request serialization roundtrip ------------------------------------

    #[test]
    fn test_request_serialization_roundtrip() {
        let req = RpcRequest::new(
            1,
            methods::PROCESS_START,
            json!({"name": "api-server"}),
        );
        let json_str = serde_json::to_string(&req).expect("serialize");
        let decoded: RpcRequest = serde_json::from_str(&json_str).expect("deserialize");
        assert_eq!(decoded.jsonrpc, "2.0");
        assert_eq!(decoded.id, 1);
        assert_eq!(decoded.method, methods::PROCESS_START);
        assert_eq!(decoded.params["name"], "api-server");
    }

    // -- Success response has no "error" key --------------------------------

    #[test]
    fn test_response_success_no_error_key() {
        let resp = RpcResponse::success(42, json!({"status": "ok"}));
        let json_str = serde_json::to_string(&resp).expect("serialize");
        let v: Value = serde_json::from_str(&json_str).expect("parse");
        assert!(v.get("result").is_some(), "result key must be present");
        assert!(v.get("error").is_none(), "error key must be absent");
    }

    // -- Error response has no "result" key ---------------------------------

    #[test]
    fn test_response_error_no_result_key() {
        let rpc_err = RpcError::new(error_codes::PROCESS_NOT_FOUND, "process not found");
        let resp = RpcResponse::error(7, rpc_err);
        let json_str = serde_json::to_string(&resp).expect("serialize");
        let v: Value = serde_json::from_str(&json_str).expect("parse");
        assert!(v.get("error").is_some(), "error key must be present");
        assert!(v.get("result").is_none(), "result key must be absent");
    }

    // -- Event serialization ------------------------------------------------

    #[test]
    fn test_event_serialization() {
        let event = RpcEvent::new(methods::EVENT_STATUS, json!({"id": "abc", "status": "online"}));
        let json_str = serde_json::to_string(&event).expect("serialize");
        let decoded: RpcEvent = serde_json::from_str(&json_str).expect("deserialize");
        assert_eq!(decoded.jsonrpc, "2.0");
        assert_eq!(decoded.method, methods::EVENT_STATUS);
        assert_eq!(decoded.params["status"], "online");
    }

    // -- Request with no params defaults to Null ----------------------------

    #[test]
    fn test_request_no_params_defaults_to_null() {
        let json_str = r#"{"jsonrpc":"2.0","id":3,"method":"daemon.ping"}"#;
        let req: RpcRequest = serde_json::from_str(json_str).expect("deserialize");
        assert_eq!(req.params, Value::Null);
    }

    // -- RpcResponse with both result and error None (deserialize only) -----

    #[test]
    fn test_response_both_result_and_error_none() {
        // A response with neither result nor error is technically valid to
        // deserialize (optional fields default to None).
        let json_str = r#"{"jsonrpc":"2.0","id":99}"#;
        let resp: RpcResponse = serde_json::from_str(json_str).expect("deserialize");
        assert_eq!(resp.id, 99);
        assert!(resp.result.is_none());
        assert!(resp.error.is_none());
    }

    // -- Multiple distinct request IDs --------------------------------------

    #[test]
    fn test_multiple_request_ids_are_distinct() {
        let req1 = RpcRequest::new(1, methods::PROCESS_START, json!({}));
        let req2 = RpcRequest::new(2, methods::PROCESS_STOP, json!({}));
        let req3 = RpcRequest::new(100, methods::DAEMON_PING, json!({}));

        assert_ne!(req1.id, req2.id);
        assert_ne!(req2.id, req3.id);
        assert_ne!(req1.id, req3.id);
    }

    // -- methods module has expected constant values ------------------------

    #[test]
    fn test_methods_constants() {
        assert_eq!(methods::PROCESS_START, "process.start");
        assert_eq!(methods::PROCESS_STOP, "process.stop");
        assert_eq!(methods::PROCESS_RESTART, "process.restart");
        assert_eq!(methods::PROCESS_DELETE, "process.delete");
        assert_eq!(methods::PROCESS_LIST, "process.list");
        assert_eq!(methods::PROCESS_INFO, "process.info");
        assert_eq!(methods::DAEMON_PING, "daemon.ping");
        assert_eq!(methods::DAEMON_KILL, "daemon.kill");
        assert_eq!(methods::DAEMON_VERSION, "daemon.version");
        assert_eq!(methods::LOG_TAIL, "log.tail");
        assert_eq!(methods::LOG_FLUSH, "log.flush");
        assert_eq!(methods::HEALTH_STATUS, "health.status");
        assert_eq!(methods::GROUP_START, "group.start");
        assert_eq!(methods::GROUP_STOP, "group.stop");
        assert_eq!(methods::GROUP_LIST, "group.list");
        assert_eq!(methods::NOTIFY_TEST, "notify.test");
        assert_eq!(methods::DEPLOY_EXECUTE, "deploy.execute");
        assert_eq!(methods::DEPLOY_ROLLBACK, "deploy.rollback");
    }

    // -- error_codes module has expected values -----------------------------

    #[test]
    fn test_error_codes_values() {
        assert_eq!(error_codes::PROCESS_NOT_FOUND, -32000);
        assert_eq!(error_codes::PROCESS_ALREADY_RUNNING, -32001);
        assert_eq!(error_codes::INVALID_CONFIG, -32002);
        assert_eq!(error_codes::SPAWN_FAILED, -32003);
        assert_eq!(error_codes::INTERNAL_ERROR, -32603);
    }
}
