//! Integration test: use `RemoteClient` (the same code that builds to WASM)
//! against a real `RemoteServer` running `StubHandler` in the same process.

use anyhow::Result;
use iroh::{discovery::static_provider::StaticProvider, Endpoint};
use superhq_remote_client::RemoteClient;
use superhq_remote_host::{RemoteServer, StubHandler};
use superhq_remote_proto::{methods, PROTOCOL_VERSION};

async fn setup() -> Result<(RemoteServer, Endpoint)> {
    let server = RemoteServer::spawn(StubHandler::default()).await?;
    server.endpoint().online().await;
    let server_addr = server.endpoint().addr();
    let disco = StaticProvider::new();
    disco.add_endpoint_info(server_addr);
    let client_ep = Endpoint::builder().discovery(disco).bind().await?;
    Ok((server, client_ep))
}

/// Authenticate a `RemoteClient` by calling `session.hello` (no creds —
/// StubHandler accepts unauthed hellos). After this, data streams and
/// non-hello methods are allowed.
async fn ensure_hello(client: &RemoteClient) -> Result<()> {
    client
        .session_hello(methods::SessionHelloParams {
            protocol_version: PROTOCOL_VERSION,
            device_label: "test".into(),
            resume_token: None,
            auth: None,
        })
        .await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn client_hello_and_tabs() -> Result<()> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,iroh=warn".into()),
        )
        .try_init();

    let (server, client_ep) = setup().await?;
    let server_id = server.endpoint_id();

    let (client, _notifications) = RemoteClient::connect(&client_ep, server_id).await?;

    let hello = client
        .session_hello(methods::SessionHelloParams {
            protocol_version: PROTOCOL_VERSION,
            device_label: "test client".into(),
            resume_token: None,
            auth: None,
        })
        .await?;
    assert_eq!(hello.protocol_version, PROTOCOL_VERSION);
    assert!(!hello.session_id.is_empty());
    assert_eq!(hello.tabs.len(), 0);

    let tabs = client.tabs_list().await?;
    assert_eq!(tabs.len(), 0);

    client.close();
    server.shutdown().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn client_pty_echo() -> Result<()> {
    let (server, client_ep) = setup().await?;
    let server_id = server.endpoint_id();

    let (client, _notifications) = RemoteClient::connect(&client_ep, server_id).await?;
    ensure_hello(&client).await?;

    let attach = client
        .pty_attach(methods::PtyAttachParams { workspace_id: 1, tab_id: 7 })
        .await?;
    assert_eq!(attach.cols, 80);
    assert_eq!(attach.rows, 24);

    let (mut pty_send, mut pty_recv) = client
        .open_pty_stream(1, 7, attach.cols, attach.rows)
        .await?;

    let payload = b"hello from client\n";
    pty_send.write_all(payload).await?;
    pty_send.finish()?;

    let mut got = Vec::new();
    while got.len() < payload.len() {
        let mut tmp = [0u8; 1024];
        match pty_recv.read(&mut tmp).await? {
            None | Some(0) => break,
            Some(n) => got.extend_from_slice(&tmp[..n]),
        }
    }
    assert_eq!(got, payload);

    client.close();
    server.shutdown().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn client_unknown_method_returns_rpc_error() -> Result<()> {
    let (server, client_ep) = setup().await?;
    let server_id = server.endpoint_id();
    let (client, _notifications) = RemoteClient::connect(&client_ep, server_id).await?;
    ensure_hello(&client).await?;

    // Use the low-level `call` to invoke a method the server doesn't know.
    let result: Result<serde_json::Value, _> =
        client.call("not.a.method", serde_json::json!({})).await;
    match result {
        Err(superhq_remote_client::RpcCallError::Rpc(err)) => {
            assert_eq!(err.code, superhq_remote_proto::error_code::METHOD_NOT_FOUND);
        }
        other => panic!("expected Rpc error, got {other:?}"),
    }

    client.close();
    server.shutdown().await?;
    Ok(())
}
