//! Auth gateway — a lightweight HTTP/WebSocket reverse proxy that runs on the
//! host, receives requests from agents inside the sandbox, swaps the dummy API
//! key for the real credential (API key or OAuth token), and forwards upstream.
//!
//! For API key auth: forwards to api.openai.com as-is.
//! For OAuth auth: rewrites to chatgpt.com/backend-api (the OAuth token is a
//! ChatGPT session token, not an API platform token).
//!
//! Supports both HTTP (for regular API calls) and WebSocket (for streaming
//! responses — Codex uses `ws://` for the Responses API).

use crate::db::Database;
use crate::oauth;
use anyhow::{Context, Result};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use futures_util::{SinkExt, StreamExt};
use http_body_util::{combinators::BoxBody, BodyExt, Full, StreamBody};
use hyper::body::{Bytes, Frame};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use reqwest::Client;
use std::convert::Infallible;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio_tungstenite::tungstenite;

/// Configuration for starting an auth gateway.
pub struct AuthGatewayConfig {
    /// Database for credential lookup and OAuth refresh.
    pub db: Arc<Database>,
    /// Which secret env var to look up (e.g. "OPENAI_API_KEY").
    pub secret_env_var: String,
    /// Upstream base URL for API key mode (e.g. "https://api.openai.com").
    pub upstream_base: String,
}

/// Running auth gateway handle. Drop or call `stop()` to shut down.
pub struct AuthGateway {
    /// The host port the gateway is listening on.
    pub host_port: u16,
    shutdown_tx: Option<oneshot::Sender<()>>,
}

impl AuthGateway {
    /// Start the auth gateway on an ephemeral port.
    pub async fn start(config: AuthGatewayConfig) -> Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .context("auth gateway: failed to bind")?;
        let host_port = listener.local_addr()?.port();

        let (shutdown_tx, mut shutdown_rx) = oneshot::channel::<()>();

        // Use native-tls for upstream HTTP connections — rustls gets flagged by
        // Cloudflare's JA3/JA4 TLS fingerprinting on chatgpt.com.
        let client = Client::builder()
            .use_native_tls()
            .user_agent("codex-cli/0.117.0")
            .build()
            .context("auth gateway: failed to build HTTP client")?;

        let state = Arc::new(GatewayState {
            db: config.db,
            secret_env_var: config.secret_env_var,
            upstream_base: config.upstream_base,
            client,
        });

        tokio::spawn(async move {
            loop {
                let (stream, _addr) = tokio::select! {
                    result = listener.accept() => {
                        match result {
                            Ok(conn) => conn,
                            Err(e) => {
                                eprintln!("[auth_gateway] accept error: {e}");
                                continue;
                            }
                        }
                    }
                    _ = &mut shutdown_rx => break,
                };

                let state = state.clone();
                tokio::spawn(async move {
                    let io = TokioIo::new(stream);
                    let svc = service_fn(move |req| {
                        let state = state.clone();
                        async move { handle_request(req, &state).await }
                    });
                    if let Err(e) = http1::Builder::new()
                        .serve_connection(io, svc)
                        .with_upgrades()
                        .await
                    {
                        eprintln!("[auth_gateway] connection error: {e}");
                    }
                });
            }
        });

        eprintln!("[auth_gateway] listening on 127.0.0.1:{host_port}");

        Ok(AuthGateway {
            host_port,
            shutdown_tx: Some(shutdown_tx),
        })
    }

    /// Gracefully stop the gateway.
    pub fn stop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
    }
}

impl Drop for AuthGateway {
    fn drop(&mut self) {
        self.stop();
    }
}

struct GatewayState {
    db: Arc<Database>,
    secret_env_var: String,
    upstream_base: String,
    client: Client,
}

type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// Resolve upstream URL and credential for a request.
struct ResolvedUpstream {
    url: String,
    credential: String,
    is_oauth: bool,
    account_id: Option<String>,
}

impl ResolvedUpstream {
    /// Apply auth headers to a hyper HeaderMap (used by WebSocket path).
    fn apply_to_header_map(&self, headers: &mut hyper::header::HeaderMap) {
        headers.insert(
            "Authorization",
            format!("Bearer {}", self.credential).parse().unwrap(),
        );
        if self.is_oauth {
            if let Some(ref acct_id) = self.account_id {
                headers.insert("chatgpt-account-id", acct_id.parse().unwrap());
            }
            headers.insert("OpenAI-Beta", "responses=experimental".parse().unwrap());
            headers.insert("originator", "codex_cli_rs".parse().unwrap());
        }
    }

    /// Apply auth headers to a reqwest RequestBuilder (used by HTTP path).
    fn apply_to_request(&self, mut builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        builder = builder.header("Authorization", format!("Bearer {}", self.credential));
        if self.is_oauth {
            if let Some(ref acct_id) = self.account_id {
                builder = builder.header("chatgpt-account-id", acct_id.as_str());
            }
            builder = builder.header("OpenAI-Beta", "responses=experimental");
            builder = builder.header("originator", "codex_cli_rs");
        }
        builder
    }
}

async fn resolve_upstream(
    state: &GatewayState,
    path_and_query: &str,
) -> Result<ResolvedUpstream> {
    let auth_method = state
        .db
        .get_secret_auth_method(&state.secret_env_var)
        .unwrap_or_else(|_| "api_key".into());

    let is_oauth = auth_method == "oauth";

    if is_oauth {
        let db = state.db.clone();
        let env_var = state.secret_env_var.clone();
        if let Err(e) = oauth::refresh_if_needed(&db, &env_var).await {
            eprintln!("[auth_gateway] oauth refresh failed: {e}");
        }
    }

    let credential = state
        .db
        .get_secret_value(&state.secret_env_var)?
        .context("auth gateway: secret not found in vault")?;

    let account_id = if is_oauth {
        state
            .db
            .get_oauth_id_token(&state.secret_env_var)
            .ok()
            .flatten()
            .and_then(|jwt| extract_jwt_claim(&jwt, "chatgpt_account_id"))
    } else {
        None
    };

    // API key → api.openai.com/v1 as-is
    // OAuth   → chatgpt.com/backend-api/codex (strip /v1 prefix,
    //           and /v1/codex prefix to avoid double /codex/)
    let url = if is_oauth {
        let path = path_and_query
            .strip_prefix("/v1/codex")
            .or_else(|| path_and_query.strip_prefix("/v1"))
            .unwrap_or(path_and_query);
        format!("https://chatgpt.com/backend-api/codex{path}")
    } else {
        format!("{}{}", state.upstream_base, path_and_query)
    };

    Ok(ResolvedUpstream {
        url,
        credential,
        is_oauth,
        account_id,
    })
}

fn is_websocket_upgrade(req: &Request<hyper::body::Incoming>) -> bool {
    req.headers()
        .get(hyper::header::UPGRADE)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.eq_ignore_ascii_case("websocket"))
        .unwrap_or(false)
}

async fn handle_request(
    req: Request<hyper::body::Incoming>,
    state: &GatewayState,
) -> Result<Response<BoxBody<Bytes, BoxError>>, Infallible> {
    if is_websocket_upgrade(&req) {
        match handle_websocket(req, state).await {
            Ok(resp) => Ok(resp),
            Err(e) => {
                eprintln!("[auth_gateway] websocket error: {e}");
                let body = Full::new(Bytes::from(format!("auth gateway ws error: {e}")))
                    .map_err(|e| -> BoxError { Box::new(e) })
                    .boxed();
                Ok(Response::builder()
                    .status(StatusCode::BAD_GATEWAY)
                    .body(body)
                    .unwrap())
            }
        }
    } else {
        match forward_http(req, state).await {
            Ok(resp) => Ok(resp),
            Err(e) => {
                eprintln!("[auth_gateway] http error: {e}");
                let body = Full::new(Bytes::from(format!("auth gateway error: {e}")))
                    .map_err(|e| -> BoxError { Box::new(e) })
                    .boxed();
                Ok(Response::builder()
                    .status(StatusCode::BAD_GATEWAY)
                    .body(body)
                    .unwrap())
            }
        }
    }
}

// --- WebSocket proxy ---

async fn handle_websocket(
    req: Request<hyper::body::Incoming>,
    state: &GatewayState,
) -> Result<Response<BoxBody<Bytes, BoxError>>> {
    let path_and_query = req
        .uri()
        .path_and_query()
        .map(|pq| pq.as_str())
        .unwrap_or("/")
        .to_string();

    let upstream = resolve_upstream(state, &path_and_query).await?;

    // Build the upstream WebSocket URL (wss://)
    let ws_url = upstream.url.replacen("https://", "wss://", 1);

    // Let tungstenite build the base request with proper WS handshake headers,
    // then add our auth headers on top.
    use tungstenite::client::IntoClientRequest;
    let mut ws_request = ws_url
        .as_str()
        .into_client_request()
        .context("failed to build ws request")?;

    upstream.apply_to_header_map(ws_request.headers_mut());
    ws_request.headers_mut().insert("User-Agent", "codex-cli/0.117.0".parse().unwrap());

    // Copy WebSocket subprotocol if present
    if let Some(proto) = req.headers().get("sec-websocket-protocol") {
        ws_request.headers_mut().insert("Sec-WebSocket-Protocol", proto.clone());
    }

    eprintln!("[auth_gateway] WebSocket connecting to {ws_url}");

    // Connect to upstream WebSocket (uses native-tls via the feature flag)
    let (upstream_ws, ws_resp) = match tokio_tungstenite::connect_async(ws_request).await {
        Ok(pair) => pair,
        Err(e) => {
            eprintln!("[auth_gateway] WebSocket connect error: {e:?}");
            anyhow::bail!("upstream WebSocket connection failed: {e}");
        }
    };

    eprintln!(
        "[auth_gateway] WebSocket connected to upstream (status: {})",
        ws_resp.status()
    );

    // Accept the client-side WebSocket upgrade via hyper
    // We send back a 101 and then get the upgraded IO stream
    let (parts, _body) = req.into_parts();
    let mut req_for_upgrade = Request::from_parts(parts, ());

    // Spawn the relay task after the upgrade completes
    let upgrade_fut = hyper::upgrade::on(&mut req_for_upgrade);

    // Build the 101 Switching Protocols response
    let resp = Response::builder()
        .status(StatusCode::SWITCHING_PROTOCOLS)
        .header(hyper::header::UPGRADE, "websocket")
        .header(hyper::header::CONNECTION, "Upgrade")
        // Generate a valid Sec-WebSocket-Accept from the client's key
        .header(
            "Sec-WebSocket-Accept",
            compute_ws_accept(
                req_for_upgrade
                    .headers()
                    .get("Sec-WebSocket-Key")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or(""),
            ),
        )
        .body(
            Full::new(Bytes::new())
                .map_err(|e| -> BoxError { Box::new(e) })
                .boxed(),
        )
        .unwrap();

    // Spawn the bidirectional relay
    tokio::spawn(async move {
        let upgraded = match upgrade_fut.await {
            Ok(u) => u,
            Err(e) => {
                eprintln!("[auth_gateway] ws upgrade failed: {e}");
                return;
            }
        };

        let client_ws = tokio_tungstenite::WebSocketStream::from_raw_socket(
            TokioIo::new(upgraded),
            tungstenite::protocol::Role::Server,
            None,
        )
        .await;

        let (mut client_tx, mut client_rx) = client_ws.split();
        let (mut upstream_tx, mut upstream_rx) = upstream_ws.split();

        // Relay frames bidirectionally
        let client_to_upstream = async {
            while let Some(msg) = client_rx.next().await {
                match msg {
                    Ok(m) => {
                        if m.is_close() {
                            let _ = upstream_tx.close().await;
                            break;
                        }
                        if upstream_tx.send(m).await.is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        };

        let upstream_to_client = async {
            while let Some(msg) = upstream_rx.next().await {
                match msg {
                    Ok(m) => {
                        if m.is_close() {
                            let _ = client_tx.close().await;
                            break;
                        }
                        if client_tx.send(m).await.is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        };

        tokio::select! {
            _ = client_to_upstream => {},
            _ = upstream_to_client => {},
        }

        eprintln!("[auth_gateway] WebSocket relay ended");
    });

    Ok(resp)
}

/// Compute `Sec-WebSocket-Accept` from `Sec-WebSocket-Key` per RFC 6455.
fn compute_ws_accept(key: &str) -> String {
    tungstenite::handshake::derive_accept_key(key.as_bytes())
}

// --- HTTP proxy ---

async fn forward_http(
    req: Request<hyper::body::Incoming>,
    state: &GatewayState,
) -> Result<Response<BoxBody<Bytes, BoxError>>> {
    let path_and_query = req
        .uri()
        .path_and_query()
        .map(|pq| pq.as_str())
        .unwrap_or("/")
        .to_string();

    let upstream = resolve_upstream(state, &path_and_query).await?;

    // Build the upstream request
    let method = req.method().clone();
    eprintln!("[auth_gateway] {method} {path_and_query} -> {}", upstream.url);
    let mut builder = state.client.request(method, &upstream.url);

    // Copy headers, stripping Authorization and Host
    for (name, value) in req.headers() {
        if name == hyper::header::AUTHORIZATION || name == hyper::header::HOST {
            continue;
        }
        if let Ok(v) = value.to_str() {
            builder = builder.header(name.as_str(), v);
        }
    }

    builder = upstream.apply_to_request(builder);

    // Collect the request body
    let body_bytes = req
        .into_body()
        .collect()
        .await
        .map(|collected| collected.to_bytes())?;
    eprintln!("[auth_gateway] body: {} bytes", body_bytes.len());
    if !body_bytes.is_empty() {
        builder = builder.body(body_bytes);
    }

    // Send and stream the response back
    let upstream_resp = builder.send().await.context("upstream request failed")?;

    let status = upstream_resp.status();
    let mut resp_builder = Response::builder().status(status.as_u16());

    for (name, value) in upstream_resp.headers() {
        resp_builder = resp_builder.header(name, value);
    }

    // For errors, log the response body for debugging
    if !status.is_success() {
        let err_body = upstream_resp.bytes().await.unwrap_or_default();
        let err_text = String::from_utf8_lossy(&err_body);
        eprintln!("[auth_gateway] upstream {status}: {err_text}");
        let body = Full::new(err_body)
            .map_err(|e| -> BoxError { Box::new(e) })
            .boxed();
        return Ok(resp_builder.body(body).unwrap());
    }

    let byte_stream = upstream_resp.bytes_stream();
    let stream_body = StreamBody::new(byte_stream.map(
        |result: Result<Bytes, reqwest::Error>| -> Result<Frame<Bytes>, BoxError> {
            result.map(Frame::data).map_err(|e| Box::new(e) as BoxError)
        },
    ));
    let boxed_body = BodyExt::boxed(stream_body);

    Ok(resp_builder.body(boxed_body).unwrap())
}

/// Extract a single claim from the `https://api.openai.com/auth` object in a JWT.
fn extract_jwt_claim(jwt: &str, claim: &str) -> Option<String> {
    let payload = jwt.split('.').nth(1)?;
    let bytes = URL_SAFE_NO_PAD.decode(payload).ok()?;
    let json: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    json.get("https://api.openai.com/auth")?
        .get(claim)?
        .as_str()
        .map(|s| s.to_string())
}

/// Build a minimal unsigned JWT containing just the chatgpt_account_id claim.
/// Pi's `openai-codex-responses` API type parses the apiKey as a JWT to extract
/// the account ID. This gives it a parseable token without exposing the real
/// OAuth credentials.
pub fn build_stub_jwt(db: &Database, secret_env_var: &str) -> Option<String> {
    let id_token = db.get_oauth_id_token(secret_env_var).ok()??;
    let account_id = extract_jwt_claim(&id_token, "chatgpt_account_id")?;

    let header = URL_SAFE_NO_PAD.encode(b"{\"alg\":\"none\",\"typ\":\"JWT\"}");
    let payload_json = serde_json::json!({
        "https://api.openai.com/auth": {
            "chatgpt_account_id": account_id
        }
    });
    let payload = URL_SAFE_NO_PAD.encode(payload_json.to_string().as_bytes());
    Some(format!("{header}.{payload}."))
}
