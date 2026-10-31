mod support;
use nibe_core::*;
use serde_json::{json, Value};
use std::{sync::Arc, time::Duration};
use support::Server;
use tokio::io::AsyncReadExt;

#[tokio::test]
async fn public_contract_over_real_http() {
    let server = Server::new("normal_heating", false).await;
    for path in [
        "/health",
        "/api/v1/status",
        "/api/v1/device",
        "/api/v1/device/identity",
        "/api/v1/measurements",
        "/api/v1/registers",
        "/api/v1/registers/catalog",
        "/api/v1/registers/discovered",
        "/api/v1/diagnostics",
        "/api/v1/discovery/status",
        "/openapi.json",
    ] {
        let (code, body) = server.request("GET", path, None).await;
        assert_eq!(code, 200, "{path}: {body}");
    }
    let (_, status) = server.request("GET", "/api/v1/status", None).await;
    assert_eq!(status["connection"], "connected");
    assert_eq!(status["stale"], false);
    assert_eq!(status["controls_enabled"], false);
    let (_, readings) = server.request("GET", "/api/v1/measurements", None).await;
    assert_eq!(readings[0]["value"], json!({"type":"decimal", "value":7.5}));
    let (_, catalog) = server
        .request("GET", "/api/v1/registers/catalog", None)
        .await;
    assert_eq!(catalog[0]["verification"], "synthetic");
    assert_eq!(catalog[0]["scale"], 0.1);
    let (code, discovery) = server.request("POST", "/api/v1/discovery/scan", None).await;
    assert_eq!(code, 200);
    assert_eq!(discovery["active_probing_supported"], false);
    assert_eq!(discovery["observed_count"], 5);
    let (code, error) = server
        .request(
            "POST",
            "/api/v1/controls/heating-offset",
            Some(json!({"offset":2})),
        )
        .await;
    assert_eq!(code, 403);
    assert_eq!(error["error"]["code"], "controls_disabled");
    let (code, _) = server
        .request("POST", "/api/v1/registers/1", Some(json!({"value":99})))
        .await;
    assert_eq!(code, 404);
    assert_eq!(server.request("POST", "/api/v1/status", None).await.0, 405);
}

#[tokio::test]
async fn invalid_commands_are_typed_and_never_queued() {
    let server = Server::new("normal_heating", true).await;
    for (path, payload) in [
        ("/api/v1/controls/heating-offset", json!({"offset":11})),
        ("/api/v1/controls/heating-offset", json!({"offset":128})),
        (
            "/api/v1/controls/heating-offset",
            json!({"offset":2, "register":40000}),
        ),
        ("/api/v1/controls/temporary-lux", json!({"mode":"boost"})),
        ("/api/v1/controls/temporary-lux", json!({"mode":4})),
        (
            "/api/v1/controls/temporary-lux",
            json!({"mode":"three_hours", "register":48132}),
        ),
    ] {
        let (status, error) = server.request("POST", path, Some(payload)).await;
        assert_eq!(status, 422);
        assert_eq!(error["error"]["code"], "validation");
    }
    let (status, error) = server
        .raw("POST", "/api/v1/controls/heating-offset", "{")
        .await;
    assert_eq!(status, 422);
    assert_eq!(error["error"]["code"], "validation");
    assert_eq!(server.gateway.snapshot().diagnostics.commands_accepted, 0);
}

#[tokio::test]
async fn authorizer_can_reject_commands_before_device_access() {
    struct DenyWrites;
    impl nibe_api::Authorizer for DenyWrites {
        fn authorize(
            &self,
            _: &axum::http::HeaderMap,
            capability: nibe_api::Capability,
        ) -> Result<(), ServiceError> {
            match capability {
                nibe_api::Capability::Read => Ok(()),
                _ => Err(ServiceError::new(
                    ErrorCode::Forbidden,
                    "test policy denies writes",
                )),
            }
        }
    }
    let server = Server::authorized("normal_heating", true, Arc::new(DenyWrites)).await;
    assert_eq!(server.request("GET", "/api/v1/status", None).await.0, 200);
    let (code, body) = server
        .request(
            "POST",
            "/api/v1/controls/heating-offset",
            Some(json!({"offset":1})),
        )
        .await;
    assert_eq!(code, 403);
    assert_eq!(body["error"]["code"], "forbidden");
    assert_eq!(server.gateway.snapshot().diagnostics.commands_accepted, 0);
}

#[tokio::test]
async fn register_operations_need_the_hardware_adapter_and_validate_their_body() {
    let server = Server::new("normal_heating", true).await;
    let path = "/api/v1/controls/register-write";
    let (code, body) = server
        .request(
            "POST",
            path,
            Some(json!({"register": 47049, "value": 47.0})),
        )
        .await;
    assert_eq!(code, 501, "{body}");
    assert_eq!(body["error"]["code"], "unsupported");
    let (code, _) = server
        .request("POST", path, Some(json!({"register": 47049, "bogus": 1})))
        .await;
    assert_eq!(code, 422);
    let (code, body) = server
        .request(
            "POST",
            "/api/v1/controls/register-read",
            Some(json!({"register": 47049})),
        )
        .await;
    assert_eq!(code, 501, "{body}");
    let (code, _) = server
        .request(
            "POST",
            "/api/v1/controls/register-poll",
            Some(json!({"register": 47049, "mode": "slow"})),
        )
        .await;
    assert_eq!(code, 501);
    assert_eq!(server.gateway.snapshot().diagnostics.commands_accepted, 0);
}

#[tokio::test]
async fn sse_delivers_json_notifications_over_tcp() {
    let server = Server::new("normal_heating", false).await;
    let mut stream = server.sse().await;
    let mut response = String::new();
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let mut buffer = [0; 4096];
            let n = stream.read(&mut buffer).await.unwrap();
            assert_ne!(n, 0);
            response.push_str(std::str::from_utf8(&buffer[..n]).unwrap());
            if response.contains("event: state") && response.contains("data: {") {
                break;
            }
        }
    })
    .await
    .unwrap();
    assert!(response.contains("text/event-stream"));
    assert!(response.contains("\"kind\":\"state\""));
}

#[test]
fn openapi_snapshot() {
    let actual = serde_json::to_value(nibe_api::api_document()).unwrap();
    let name = if cfg!(feature = "dev") {
        "openapi.dev.json"
    } else {
        "openapi.json"
    };
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/contract")
        .join(name);
    if std::env::var_os("UPDATE_OPENAPI").is_some() {
        std::fs::write(
            &path,
            format!("{}\n", serde_json::to_string_pretty(&actual).unwrap()),
        )
        .unwrap();
    }
    let expected: Value = serde_json::from_slice(&std::fs::read(path).expect(
        "OpenAPI snapshot missing; run UPDATE_OPENAPI=1 cargo test -p nibe-api openapi_snapshot",
    ))
    .unwrap();
    assert_eq!(
        actual, expected,
        "API changed; review diff and explicitly regenerate snapshots"
    );
}

#[cfg(not(feature = "dev"))]
#[tokio::test]
async fn production_build_has_no_debug_routes_or_schemas() {
    let server = Server::new("normal_heating", true).await;
    for path in [
        "/__dev/scenario",
        "/__dev/time/advance",
        "/__dev/fault",
        "/__dev/reset",
        "/__dev/commands",
    ] {
        assert_eq!(server.request("POST", path, Some(json!({}))).await.0, 404);
    }
    let document = serde_json::to_string(&nibe_api::api_document()).unwrap();
    assert!(!document.contains("/__dev/"));
    assert!(!document.contains("DevAction"));
    assert!(!server.gateway.snapshot().controls_enabled);
}

/// ADR 0060: while no admin exists, a client on the open setup AP needs no setup token; every other
/// client still does, and nothing replaces the admin session once an admin exists.
#[tokio::test]
async fn setup_ap_clients_need_no_setup_token_until_an_admin_exists() {
    let csrf = [("X-Requested-With", "nibe-gateway")];
    let password = r#"{"password":"a-long-admin-password"}"#;

    let phone = Server::unclaimed_from("192.168.4.23:50000").await;
    let (_, status, _) = phone
        .raw_headers("GET", "/api/v1/setup/status", "", &[])
        .await;
    assert_eq!(status["network_state"], "ap");
    assert_eq!(status["setup_token_required"], false);
    assert_eq!(
        phone
            .raw_headers("GET", "/api/v1/network/scan", "", &csrf)
            .await
            .0,
        200
    );
    // CSRF still applies to the tokenless path.
    assert_eq!(
        phone
            .raw_headers("POST", "/api/v1/setup/admin", password, &[])
            .await
            .0,
        403
    );
    let (code, _, headers) = phone
        .raw_headers("POST", "/api/v1/setup/admin", password, &csrf)
        .await;
    assert_eq!(code, 200);
    assert!(headers.contains_key("set-cookie"));
    // Once the admin exists, neither the AP nor the token grants provisioning.
    let token = phone.store.as_ref().unwrap().setup_secrets().setup_token;
    let with_token = [
        ("X-Requested-With", "nibe-gateway"),
        ("X-Setup-Token", token.as_str()),
    ];
    for headers in [&csrf[..], &with_token[..]] {
        assert_eq!(
            phone
                .raw_headers("GET", "/api/v1/network/scan", "", headers)
                .await
                .0,
            401
        );
    }
    assert_eq!(
        phone
            .raw_headers("POST", "/api/v1/setup/admin", password, &with_token)
            .await
            .0,
        401
    );
    assert_eq!(
        phone
            .raw_headers("GET", "/api/v1/setup/status", "", &[])
            .await
            .1["setup_token_required"],
        false
    );

    // A device on the home LAN still needs the token.
    let lan = Server::unclaimed_from("192.168.1.23:50000").await;
    assert_eq!(
        lan.raw_headers("GET", "/api/v1/setup/status", "", &[])
            .await
            .1["setup_token_required"],
        true
    );
    assert_eq!(
        lan.raw_headers("GET", "/api/v1/network/scan", "", &csrf)
            .await
            .0,
        401
    );
    assert_eq!(
        lan.raw_headers("POST", "/api/v1/setup/admin", password, &csrf)
            .await
            .0,
        401
    );
    let token = lan.store.as_ref().unwrap().setup_secrets().setup_token;
    let with_token = [
        ("X-Requested-With", "nibe-gateway"),
        ("X-Setup-Token", token.as_str()),
    ];
    assert_eq!(
        lan.raw_headers("POST", "/api/v1/setup/admin", password, &with_token)
            .await
            .0,
        200
    );

    // A home LAN that also uses 192.168.4.0/24: once the station is up, the AP is down and the
    // token is required again.
    let overlap = Server::unclaimed_from("192.168.4.23:50000").await;
    let good = r#"{"ssid":"Home WiFi","password":"correct-password"}"#;
    assert_eq!(
        overlap
            .raw_headers("POST", "/api/v1/network/connect", good, &csrf)
            .await
            .1["state"],
        "connected"
    );
    assert_eq!(
        overlap
            .raw_headers("GET", "/api/v1/setup/status", "", &[])
            .await
            .1["setup_token_required"],
        true
    );
    assert_eq!(
        overlap
            .raw_headers("POST", "/api/v1/setup/admin", password, &csrf)
            .await
            .0,
        401
    );
}

#[tokio::test]
async fn provisioning_auth_and_reset_over_real_http() {
    let server = Server::unclaimed().await;
    let json_header = [("X-Requested-With", "nibe-gateway")];
    let token = server.store.as_ref().unwrap().setup_secrets().setup_token;
    let setup_headers = [
        ("X-Requested-With", "nibe-gateway"),
        ("X-Setup-Token", token.as_str()),
    ];
    for (method, path) in [
        ("GET", "/api/v1/device"),
        ("GET", "/api/v1/events"),
        ("POST", "/api/v1/discovery/scan"),
        ("POST", "/api/v1/controls/heating-offset"),
        ("GET", "/api/v1/network/scan"),
        ("POST", "/__dev/scenario"),
    ] {
        assert_eq!(
            server.raw_headers(method, path, "{}", &json_header).await.0,
            401,
            "{path}"
        );
    }
    let (_, setup, _) = server
        .raw_headers("GET", "/api/v1/setup/status", "", &[])
        .await;
    assert_eq!(setup["network_state"], "ap");
    // Loopback is not the setup AP: the token stays required (ADR 0060).
    assert_eq!(setup["setup_token_required"], true);
    assert!(!setup.to_string().contains(&token));
    let bad = r#"{"ssid":"Home WiFi","password":"wrong"}"#;
    assert_eq!(
        server
            .raw_headers("POST", "/api/v1/network/connect", bad, &setup_headers)
            .await
            .0,
        422
    );
    assert_eq!(
        server
            .raw_headers("GET", "/api/v1/network/status", "", &setup_headers)
            .await
            .1["state"],
        "ap"
    );
    let good = r#"{"ssid":"Home WiFi","password":"correct-password"}"#;
    assert_eq!(
        server
            .raw_headers("POST", "/api/v1/network/connect", good, &setup_headers)
            .await
            .1["state"],
        "connected"
    );
    let password = r#"{"password":"a-long-admin-password"}"#;
    assert_eq!(
        server
            .raw_headers("POST", "/api/v1/setup/admin", password, &[])
            .await
            .0,
        403
    );
    let (code, _, headers) = server
        .raw_headers("POST", "/api/v1/setup/admin", password, &setup_headers)
        .await;
    assert_eq!(code, 200);
    let cookie = headers["set-cookie"]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap();
    assert!(headers["set-cookie"]
        .to_str()
        .unwrap()
        .contains("HttpOnly; SameSite=Strict"));
    let session_headers = [("X-Requested-With", "nibe-gateway"), ("Cookie", cookie)];
    assert_eq!(
        server
            .raw_headers("GET", "/api/v1/device", "", &session_headers)
            .await
            .0,
        200
    );
    assert_eq!(
        server
            .raw_headers("GET", "/api/v1/network/status", "", &setup_headers)
            .await
            .0,
        401
    );
    assert_eq!(
        server
            .raw_headers("POST", "/api/v1/setup/admin", password, &session_headers)
            .await
            .0,
        409
    );
    let hostile_headers = [
        ("X-Requested-With", "nibe-gateway"),
        ("Cookie", cookie),
        ("Origin", "https://attacker.invalid"),
    ];
    assert_eq!(
        server
            .raw_headers("POST", "/api/v1/discovery/scan", "", &hostile_headers)
            .await
            .0,
        403
    );
    assert_eq!(
        server
            .raw_headers("POST", "/api/v1/network/recovery", "", &session_headers)
            .await
            .1["state"],
        "recovery_ap"
    );
    assert_eq!(
        server
            .raw_headers(
                "POST",
                "/api/v1/setup/factory-reset",
                r#"{"confirmation":"wrong"}"#,
                &session_headers
            )
            .await
            .0,
        422
    );
    assert_eq!(
        server
            .raw_headers("POST", "/api/v1/auth/logout", "", &session_headers)
            .await
            .0,
        200
    );
    assert_eq!(
        server
            .raw_headers("GET", "/api/v1/device", "", &session_headers)
            .await
            .0,
        401
    );
    let (code, _, headers) = server
        .raw_headers("POST", "/api/v1/auth/login", password, &json_header)
        .await;
    assert_eq!(code, 200);
    let cookie = headers["set-cookie"]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap();
    let session_headers = [("X-Requested-With", "nibe-gateway"), ("Cookie", cookie)];
    assert_eq!(
        server
            .raw_headers(
                "POST",
                "/api/v1/setup/factory-reset",
                r#"{"confirmation":"ERASE"}"#,
                &session_headers
            )
            .await
            .0,
        200
    );
    assert_eq!(
        server
            .raw_headers("GET", "/api/v1/device", "", &session_headers)
            .await
            .0,
        401
    );
    assert_eq!(
        server
            .raw_headers("GET", "/api/v1/network/status", "", &setup_headers)
            .await
            .0,
        401
    );
    assert_eq!(
        server
            .raw_headers("GET", "/api/v1/setup/status", "", &[])
            .await
            .1["network_state"],
        "ap"
    );
    assert!(server.store.as_ref().unwrap().config().network.is_none());
}

#[tokio::test]
async fn revoking_a_session_closes_an_existing_sse_stream() {
    let server = Server::new("normal_heating", false).await;
    let mut stream = server.sse().await;
    let mut first = [0; 2048];
    let n = tokio::time::timeout(Duration::from_secs(1), stream.read(&mut first))
        .await
        .unwrap()
        .unwrap();
    assert!(String::from_utf8_lossy(&first[..n]).contains("200 OK"));
    server.auth.as_ref().unwrap().invalidate_all();
    let mut remaining = Vec::new();
    tokio::time::timeout(Duration::from_secs(2), stream.read_to_end(&mut remaining))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(server.request("GET", "/api/v1/device", None).await.0, 401);
}

#[tokio::test]
async fn websocket_streams_readings_and_enforces_session_and_origin() {
    use futures_util::StreamExt;
    use tokio_tungstenite::tungstenite::{client::IntoClientRequest, Message};
    let server = Server::new("normal_heating", false).await;
    let connect = |cookie: Option<String>, origin: Option<String>| {
        let address = server.address;
        async move {
            let mut request = format!("ws://{address}/api/v1/stream")
                .into_client_request()
                .unwrap();
            if let Some(cookie) = cookie {
                request
                    .headers_mut()
                    .insert("cookie", cookie.parse().unwrap());
            }
            if let Some(origin) = origin {
                request
                    .headers_mut()
                    .insert("origin", origin.parse().unwrap());
            }
            let tcp = tokio::net::TcpStream::connect(address).await.unwrap();
            tokio_tungstenite::client_async(request, tcp).await
        }
    };
    assert!(connect(None, None).await.is_err(), "no session, no stream");
    let cookie = server.cookie.clone();
    assert!(
        connect(cookie.clone(), Some("https://evil.example".into()))
            .await
            .is_err(),
        "cross-origin upgrade must be rejected"
    );
    let (mut socket, _) = connect(cookie, Some(format!("http://{}", server.address)))
        .await
        .unwrap();
    let mut next = async || -> serde_json::Value {
        loop {
            match tokio::time::timeout(Duration::from_secs(2), socket.next())
                .await
                .unwrap()
                .unwrap()
                .unwrap()
            {
                Message::Text(text) => return serde_json::from_str(&text).unwrap(),
                _ => continue,
            }
        }
    };
    let hello = next().await;
    assert_eq!(hello["type"], "hello");
    assert_eq!(hello["mode"], "simulated");
    let reading = next().await;
    assert_eq!(reading["type"], "reading");
    assert!(reading["id"].is_u64() && reading["observed_at_ms"].is_u64());
    server.auth.as_ref().unwrap().invalidate_all();
    let closed = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            match socket.next().await {
                None | Some(Err(_)) | Some(Ok(Message::Close(_))) => break,
                Some(Ok(_)) => continue,
            }
        }
    })
    .await;
    assert!(closed.is_ok(), "revoked session must close the stream");
}

#[tokio::test]
async fn api_keys_reach_only_their_scope_and_die_with_the_password() {
    let server = Server::new("normal_heating", false).await;
    let (code, read) = server
        .request(
            "POST",
            "/api/v1/access/api-keys",
            Some(serde_json::json!({"name": "monitor", "scope": "read"})),
        )
        .await;
    assert_eq!(code, 200, "{read}");
    let token = read["token"].as_str().unwrap().to_string();
    assert!(token.starts_with("nibe_") && token.len() == 69);
    let (_, list) = server.request("GET", "/api/v1/access/api-keys", None).await;
    assert_eq!(list[0]["name"], "monitor");
    assert!(
        list[0].get("token").is_none() && list[0].get("hash").is_none(),
        "never listed"
    );
    let bearer = |method: &'static str, path: &'static str, token: String| {
        let address = server.address;
        async move {
            let mut stream = tokio::net::TcpStream::connect(address).await.unwrap();
            let body = if method == "POST" {
                "{\"offset\":1}"
            } else {
                ""
            };
            let request = format!(
                "{method} {path} HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            tokio::io::AsyncWriteExt::write_all(&mut stream, request.as_bytes())
                .await
                .unwrap();
            let mut response = String::new();
            tokio::io::AsyncReadExt::read_to_string(&mut stream, &mut response)
                .await
                .unwrap();
            response
                .split_whitespace()
                .nth(1)
                .unwrap()
                .parse::<u16>()
                .unwrap()
        }
    };
    assert_eq!(bearer("GET", "/api/v1/device", token.clone()).await, 200);
    assert_eq!(
        bearer("GET", "/api/v1/registers/explorer", token.clone()).await,
        404,
        "simulated has no explorer, but the key passed the guard"
    );
    assert_eq!(
        bearer("POST", "/api/v1/controls/heating-offset", token.clone()).await,
        403,
        "read key cannot control"
    );
    assert_eq!(
        bearer("GET", "/api/v1/access/api-keys", token.clone()).await,
        403,
        "keys never manage keys"
    );
    assert_eq!(
        bearer("GET", "/api/v1/integrations/mqtt", token.clone()).await,
        403,
        "keys never see MQTT settings"
    );
    assert_eq!(
        bearer("GET", "/api/v1/device", format!("nibe_{}", "0".repeat(64))).await,
        401
    );
    let (_, control) = server
        .request(
            "POST",
            "/api/v1/access/api-keys",
            Some(serde_json::json!({"name": "ha", "scope": "control"})),
        )
        .await;
    let control = control["token"].as_str().unwrap().to_string();
    // The guard admits the control key; the simulated test device then reports controls disabled.
    let auth = format!("Bearer {control}");
    let (code, body, _) = server
        .raw_headers(
            "POST",
            "/api/v1/controls/heating-offset",
            "{\"offset\":1}",
            &[
                ("Authorization", auth.as_str()),
                ("Content-Type", "application/json"),
            ],
        )
        .await;
    assert_eq!(
        (code, body["error"]["code"].as_str()),
        (403, Some("controls_disabled")),
        "{body}"
    );
    let id = list[0]["id"].as_str().unwrap().to_string();
    let path: &'static str = Box::leak(format!("/api/v1/access/api-keys/{id}").into_boxed_str());
    assert_eq!(server.request("DELETE", path, None).await.0, 200);
    assert_eq!(bearer("GET", "/api/v1/device", token).await, 401, "revoked");
    let (code, _) = server
        .request("POST", "/api/v1/access/password", Some(serde_json::json!({"current_password": "contract-admin-password", "new_password": "a-new-password-123"})))
        .await;
    assert_eq!(code, 200);
    assert_eq!(
        bearer("GET", "/api/v1/device", control).await,
        401,
        "password change revokes keys"
    );
}

#[tokio::test]
async fn mqtt_settings_are_admin_only_validated_and_never_return_the_password() {
    let server = Server::new("normal_heating", false).await;
    let (code, view) = server
        .request("GET", "/api/v1/integrations/mqtt", None)
        .await;
    assert_eq!(code, 200, "{view}");
    assert_eq!(
        (view["enabled"].as_bool(), view["status"]["state"].as_str()),
        (Some(false), Some("disabled"))
    );
    assert_eq!(view["base_topic"], "nibe/nibe_1234");
    let settings = |host: &str, prefix: &str, password: Value| {
        serde_json::json!({
            "enabled": true, "host": host, "port": 1, "username": "ha", "password": password,
            "topic_prefix": prefix, "discovery": true, "discovery_prefix": "homeassistant",
            "allow_commands": false
        })
    };
    for bad in [
        settings("bad host", "nibe", Value::Null),
        settings("broker.lan", "nibe/#", Value::Null),
    ] {
        let (code, error) = server
            .request("POST", "/api/v1/integrations/mqtt", Some(bad))
            .await;
        assert_eq!(
            (code, error["error"]["code"].as_str()),
            (422, Some("validation"))
        );
    }
    // Port 1 on localhost refuses: the bridge reports an error, the settings are kept.
    let (code, view) = server
        .request(
            "POST",
            "/api/v1/integrations/mqtt",
            Some(settings("127.0.0.1", "nibe", Value::from("s3cret-broker"))),
        )
        .await;
    assert_eq!(code, 200, "{view}");
    assert_eq!(view["password_set"], true);
    assert!(
        !view.to_string().contains("s3cret-broker"),
        "never returned"
    );
    // Null keeps the stored password; an empty string clears it.
    let (_, view) = server
        .request(
            "POST",
            "/api/v1/integrations/mqtt",
            Some(settings("127.0.0.1", "nibe", Value::Null)),
        )
        .await;
    assert_eq!(view["password_set"], true);
    let (_, view) = server
        .request(
            "POST",
            "/api/v1/integrations/mqtt",
            Some(settings("127.0.0.1", "nibe", Value::from(""))),
        )
        .await;
    assert_eq!(view["password_set"], false);
}

#[tokio::test]
async fn history_records_polled_readings_and_validates_queries() {
    let server = Server::new("normal_heating", false).await;
    let (code, status) = server.request("GET", "/api/v1/history/status", None).await;
    assert_eq!(
        (code, status["enabled"].as_bool()),
        (200, Some(true)),
        "{status}"
    );
    // The recorder samples every 2 s; give it a moment, then flush via a query loop on status.
    let mut written = 0;
    for _ in 0..40 {
        tokio::time::sleep(Duration::from_millis(250)).await;
        let (_, s) = server.request("GET", "/api/v1/history/status", None).await;
        written = s["buffered"].as_u64().unwrap_or(0) + s["samples_written"].as_u64().unwrap_or(0);
        if written > 0 {
            break;
        }
    }
    assert!(written > 0, "recorder buffered the simulated readings");
    for bad in [
        "/api/v1/history",
        "/api/v1/history?registers=a",
        "/api/v1/history?registers=1,2,3,4,5,6,7,8,9",
        "/api/v1/history?registers=1&from=10&to=5",
        "/api/v1/history?registers=1&resolution=5m",
    ] {
        let (code, error) = server.request("GET", bad, None).await;
        assert_eq!(
            (code, error["error"]["code"].as_str()),
            (422, Some("validation")),
            "{bad}"
        );
    }
    // The simulator's frozen clock sits in 2023; query that window.
    let (code, body) = server
        .request(
            "GET",
            "/api/v1/history?registers=1,2&from=1600000000000&to=1800000000000&resolution=1h",
            None,
        )
        .await;
    assert_eq!(code, 200, "{body}");
    assert_eq!(body["series"].as_array().unwrap().len(), 2);
    let cookie = server.cookie.clone().unwrap();
    let mut stream = tokio::net::TcpStream::connect(server.address)
        .await
        .unwrap();
    tokio::io::AsyncWriteExt::write_all(
        &mut stream,
        format!("GET /api/v1/history/export.csv?registers=1 HTTP/1.1\r\nHost: localhost\r\nCookie: {cookie}\r\nConnection: close\r\n\r\n").as_bytes(),
    )
    .await
    .unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).await.unwrap();
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    assert!(response.contains("text/csv") && response.contains("time_utc,register,name"));
}

#[tokio::test]
async fn poll_choices_apply_at_once_and_are_saved_in_the_settings_file() {
    let server = Server::with_replay_explorer().await;
    let poll = |register: u16, mode: &str| json!({"register": register, "mode": mode});
    let (code, lists) = server
        .request(
            "POST",
            "/api/v1/controls/register-poll",
            Some(poll(47049, "slow")),
        )
        .await;
    assert_eq!(code, 200, "{lists}");
    assert_eq!(lists["slow"], json!([47049]));
    assert_eq!(lists["overrides"], json!({"47049": "slow"}));
    let (code, lists) = server
        .request(
            "POST",
            "/api/v1/controls/register-poll",
            Some(poll(40004, "fast")),
        )
        .await;
    assert_eq!(code, 200, "{lists}");
    assert_eq!(lists["fast"], json!([40004]));
    // The adapter runs the new lists, and the settings file holds only the deviations.
    let (_, polls) = server.request("GET", "/api/v1/registers/polls", None).await;
    assert_eq!(
        (polls["fast"].clone(), polls["slow"].clone()),
        (json!([40004]), json!([47049]))
    );
    let saved = server.store.as_ref().unwrap().config().poll_overrides;
    assert_eq!(
        serde_json::to_value(&saved).unwrap(),
        json!({"40004": "fast", "47049": "slow"})
    );
    // Back to the configured mode (off here) removes the override.
    server
        .request(
            "POST",
            "/api/v1/controls/register-poll",
            Some(poll(47049, "off")),
        )
        .await;
    let saved = server.store.as_ref().unwrap().config().poll_overrides;
    assert_eq!(
        serde_json::to_value(&saved).unwrap(),
        json!({"40004": "fast"})
    );
    // Rules: catalog registers only, known modes only.
    let (code, _) = server
        .request(
            "POST",
            "/api/v1/controls/register-poll",
            Some(poll(1, "fast")),
        )
        .await;
    assert_eq!(code, 422);
    let (code, _) = server
        .request(
            "POST",
            "/api/v1/controls/register-poll",
            Some(poll(40004, "often")),
        )
        .await;
    assert_eq!(code, 422);
    // Replay never writes: open writes are hardware only.
    let (code, body) = server
        .request(
            "POST",
            "/api/v1/controls/register-write",
            Some(json!({"register": 47049, "raw": 470})),
        )
        .await;
    assert_eq!(code, 403, "{body}");
}

#[tokio::test]
async fn register_notes_are_admin_only_validated_and_shown_in_the_explorer() {
    let server = Server::with_replay_explorer().await;
    let (code, explorer) = server
        .request("GET", "/api/v1/registers/explorer", None)
        .await;
    assert_eq!(code, 200, "{explorer}");
    assert_eq!(explorer["register_notes"], json!({}));
    let notes = json!({"notes": [
        {"register": 43108, "relevance": "relevant", "display_verified": true,
         "label": "active fan speed step", "source": "owner test 2026-10-26"},
        {"register": 41256, "relevance": "unmaintained", "note": "always 0", "source": "survey"}
    ]});
    let (code, saved) = server
        .request("PUT", "/api/v1/registers/notes", Some(notes.clone()))
        .await;
    assert_eq!(code, 200, "{saved}");
    assert_eq!(
        saved["register_notes"]["43108"]["label"],
        "active fan speed step"
    );
    assert!(
        saved["register_notes"]["43108"]["timestamp_ms"]
            .as_u64()
            .unwrap()
            > 0
    );
    // Merge keeps 43108 and adds 40004; replace with an empty list clears.
    let (_, merged) = server
        .request(
            "PUT",
            "/api/v1/registers/notes",
            Some(json!({"mode": "merge", "notes": [{"register": 40004, "relevance": "relevant", "source": "s"}]})),
        )
        .await;
    assert_eq!(merged["register_notes"].as_object().unwrap().len(), 3);
    assert_eq!(
        merged["register_notes"]["43108"]["label"],
        "active fan speed step"
    );
    let (_, explorer) = server
        .request("GET", "/api/v1/registers/explorer", None)
        .await;
    assert_eq!(
        explorer["register_notes"]["41256"]["relevance"],
        "unmaintained"
    );
    assert_eq!(
        explorer["register_notes"]["43108"]["display_verified"],
        true
    );
    let catalog_entry = explorer["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == 43108)
        .unwrap();
    assert_eq!(
        (
            catalog_entry["verification"].as_str(),
            catalog_entry["writable"].as_bool(),
            catalog_entry["definition"]["write_allowed"].as_bool(),
            catalog_entry["definition"]["unit"].is_null(),
        ),
        (Some("unverified"), Some(false), Some(false), true),
        "notes never change the catalog or grant writes"
    );
    for bad in [
        json!({"notes": [{"register": 1, "relevance": "relevant", "source": "s", "write_allowed": true}]}),
        json!({"notes": [{"register": 1, "relevance": "maybe", "source": "s"}]}),
        json!({"notes": [{"register": 1, "relevance": "relevant", "source": ""}]}),
        json!({"notes": [{"register": 1, "relevance": "relevant", "source": "s", "note": "x".repeat(257)}]}),
        json!({"notes": [{"register": 1, "relevance": "relevant", "source": "s"}, {"register": 1, "relevance": "absent", "source": "s"}]}),
        json!({"mode": "append", "notes": []}),
    ] {
        let (code, error) = server
            .request("PUT", "/api/v1/registers/notes", Some(bad.clone()))
            .await;
        assert_eq!(
            (code, error["error"]["code"].as_str()),
            (422, Some("validation")),
            "{bad}"
        );
    }
    // A full set for one pump fits (2048 notes with long text is ~600 KiB, so use 600 short ones).
    let many: Vec<Value> = (0..600)
        .map(|i| json!({"register": 40000 + i, "relevance": "absent", "source": "register-survey-2026-10-25 class b", "note": "x".repeat(200)}))
        .collect();
    let (code, saved) = server
        .request(
            "PUT",
            "/api/v1/registers/notes",
            Some(json!({"notes": many})),
        )
        .await;
    assert_eq!(code, 200, "{saved}");
    // Size bound: a body above 256 KiB is refused before parsing.
    let huge = format!("{{\"notes\":[],\"pad\":\"{}\"}}", "x".repeat(300 * 1024));
    let cookie = server.cookie.clone().unwrap();
    let (code, _, _) = server
        .raw_headers(
            "PUT",
            "/api/v1/registers/notes",
            &huge,
            &[
                ("X-Requested-With", "nibe-gateway"),
                ("Cookie", cookie.as_str()),
            ],
        )
        .await;
    assert_eq!(code, 413);
    // Same session/CSRF guard as the other admin routes.
    let (code, _, _) = server
        .raw_headers(
            "PUT",
            "/api/v1/registers/notes",
            &notes.to_string(),
            &[("Cookie", cookie.as_str())],
        )
        .await;
    assert_eq!(code, 403, "CSRF header required");
    let (code, _, _) = server
        .raw_headers(
            "PUT",
            "/api/v1/registers/notes",
            &notes.to_string(),
            &[("X-Requested-With", "nibe-gateway")],
        )
        .await;
    assert_eq!(code, 401, "no session");
    // API keys read notes through the explorer but can never write them, whatever the scope.
    for scope in ["read", "control"] {
        let (_, key) = server
            .request(
                "POST",
                "/api/v1/access/api-keys",
                Some(json!({"name": scope, "scope": scope})),
            )
            .await;
        let auth = format!("Bearer {}", key["token"].as_str().unwrap());
        let (code, body, _) = server
            .raw_headers(
                "PUT",
                "/api/v1/registers/notes",
                &notes.to_string(),
                &[("Authorization", auth.as_str())],
            )
            .await;
        assert_eq!(code, 403, "{scope} key: {body}");
        let (code, body, _) = server
            .raw_headers(
                "GET",
                "/api/v1/registers/explorer",
                "",
                &[("Authorization", auth.as_str())],
            )
            .await;
        assert_eq!(code, 200);
        assert_eq!(body["register_notes"]["43108"]["relevance"], "relevant");
    }
    let (_, cleared) = server
        .request(
            "PUT",
            "/api/v1/registers/notes",
            Some(json!({"mode": "replace", "notes": []})),
        )
        .await;
    assert_eq!(cleared["register_notes"], json!({}));
    assert!(server.store.as_ref().unwrap().register_notes().is_empty());
}

#[cfg(feature = "dev")]
async fn wait_for(server: &Server, what: &str, condition: impl Fn(&Value) -> bool) -> Value {
    let mut last = Value::Null;
    for _ in 0..200 {
        last = server.request("GET", "/api/v1/status", None).await.1;
        if condition(&last) {
            return last;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("timed out waiting for {what}: {last}");
}
#[cfg(feature = "dev")]
fn offset_reading(server: &Server) -> Option<f64> {
    server
        .gateway
        .snapshot()
        .measurements
        .iter()
        .find(|m| m.name == "heating_offset")
        .and_then(|m| match m.value {
            Some(RegisterValue::Integer(v)) => Some(v as f64),
            Some(RegisterValue::Decimal(v)) => Some(v),
            _ => None,
        })
}

/// ADR 0123: a lapsed lease restores the offset read back before the first leased write; renewal postpones it.
#[cfg(feature = "dev")]
#[tokio::test]
async fn heating_offset_lease_renewal_and_restore() {
    let server = Server::new("normal_heating", true).await;
    let offset =
        |value: Value| server.request("POST", "/api/v1/controls/heating-offset", Some(value));
    // The owner's normal offset, set without a lease, becomes the baseline.
    assert_eq!(offset(json!({"offset":-2})).await.0, 200);
    server.clock.advance(1).unwrap();
    for _ in 0..200 {
        if offset_reading(&server) == Some(-2.0) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let (_, status) = server.request("GET", "/api/v1/status", None).await;
    assert_eq!(
        status["heating_offset_lease"],
        json!({"heating_offset":null,"remaining_s":null,"last_restore":null})
    );
    let (code, receipt) = offset(json!({"offset":-10,"lease_seconds":60})).await;
    assert_eq!(code, 200, "{receipt}");
    assert!(receipt["detail"].as_str().unwrap().contains("lease 60 s"));
    let (_, status) = server.request("GET", "/api/v1/status", None).await;
    let lease = &status["heating_offset_lease"];
    assert_eq!(lease["heating_offset"]["holder"], "admin");
    assert_eq!(lease["heating_offset"]["offset"], -10);
    assert_eq!(lease["heating_offset"]["baseline"], -2);
    assert_eq!(lease["remaining_s"], 60);
    // Renewal before expiry keeps the original baseline and pushes the expiry out.
    server.clock.advance(50_000).unwrap();
    assert_eq!(offset(json!({"offset":-8,"lease_seconds":60})).await.0, 200);
    server.clock.advance(50_000).unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    let (_, status) = server.request("GET", "/api/v1/status", None).await;
    assert_eq!(
        status["heating_offset_lease"]["heating_offset"]["offset"],
        -8
    );
    assert_eq!(
        status["heating_offset_lease"]["heating_offset"]["baseline"],
        -2
    );
    assert_eq!(status["heating_offset_lease"]["remaining_s"], 10);
    assert!(status["heating_offset_lease"]["last_restore"].is_null());
    assert_eq!(offset_reading(&server), Some(-8.0));
    // No renewal: the baseline comes back through the same named control.
    server.clock.advance(10_000).unwrap();
    for _ in 0..200 {
        if offset_reading(&server) == Some(-2.0) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    server.clock.advance(1).unwrap();
    let status = wait_for(&server, "the restore", |s| {
        !s["heating_offset_lease"]["last_restore"].is_null()
    })
    .await;
    let lease = &status["heating_offset_lease"];
    assert!(lease["heating_offset"].is_null());
    assert_eq!(lease["last_restore"]["ok"], true);
    assert_eq!(lease["last_restore"]["baseline"], -2);
    assert_eq!(lease["last_restore"]["holder"], "admin");
    let history = server.gateway.command_history();
    assert_eq!(
        history.last().unwrap().command,
        Command::HeatingOffset { offset: -2 }
    );
    let stored = server.store.as_ref().unwrap().lease_state();
    assert!(stored.heating_offset.is_none());
    assert!(stored.last_restore.unwrap().ok);
    server.clock.advance(1).unwrap();
    for _ in 0..200 {
        if offset_reading(&server) == Some(-2.0) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("simulated offset not restored");
}

#[cfg(feature = "dev")]
#[tokio::test]
async fn heating_offset_lease_bounds_holder_and_unleased_writes() {
    let server = Server::new("normal_heating", true).await;
    for seconds in [0, 59, 3601, 86_400] {
        let (code, body) = server
            .request(
                "POST",
                "/api/v1/controls/heating-offset",
                Some(json!({"offset":1,"lease_seconds":seconds})),
            )
            .await;
        assert_eq!(code, 422, "{seconds}: {body}");
        assert_eq!(body["error"]["code"], "validation");
    }
    let (code, _) = server
        .request(
            "POST",
            "/api/v1/controls/heating-offset",
            Some(json!({"offset":1,"lease_seconds":-5})),
        )
        .await;
    assert_eq!(code, 422);
    assert_eq!(server.gateway.snapshot().diagnostics.commands_accepted, 0);
    // A control key's name is the holder.
    let (_, key) = server
        .request(
            "POST",
            "/api/v1/access/api-keys",
            Some(json!({"name": "neo", "scope": "control"})),
        )
        .await;
    let auth = format!("Bearer {}", key["token"].as_str().unwrap());
    let (code, body, _) = server
        .raw_headers(
            "POST",
            "/api/v1/controls/heating-offset",
            "{\"offset\":-10,\"lease_seconds\":3600}",
            &[("Authorization", auth.as_str())],
        )
        .await;
    assert_eq!(code, 200, "{body}");
    let (_, status) = server.request("GET", "/api/v1/status", None).await;
    assert_eq!(
        status["heating_offset_lease"]["heating_offset"]["holder"],
        "neo"
    );
    // A write without a lease behaves as before and ends the lease: no restore follows.
    let (code, receipt) = server
        .request(
            "POST",
            "/api/v1/controls/heating-offset",
            Some(json!({"offset":3})),
        )
        .await;
    assert_eq!(code, 200);
    assert!(receipt["detail"]
        .as_str()
        .unwrap()
        .contains("lease held by neo ended"));
    let (_, status) = server.request("GET", "/api/v1/status", None).await;
    assert!(status["heating_offset_lease"]["heating_offset"].is_null());
    server.clock.advance(3_601_000).unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    let (_, status) = server.request("GET", "/api/v1/status", None).await;
    assert!(status["heating_offset_lease"]["last_restore"].is_null());
    assert_eq!(
        server.gateway.command_history().last().unwrap().command,
        Command::HeatingOffset { offset: 3 }
    );
}

#[cfg(feature = "dev")]
#[tokio::test]
async fn leased_writes_need_a_lease_store() {
    struct AllowAll;
    impl nibe_api::Authorizer for AllowAll {
        fn authorize(
            &self,
            _: &axum::http::HeaderMap,
            _: nibe_api::Capability,
        ) -> Result<(), ServiceError> {
            Ok(())
        }
    }
    let server = Server::authorized("normal_heating", true, Arc::new(AllowAll)).await;
    let (code, body) = server
        .request(
            "POST",
            "/api/v1/controls/heating-offset",
            Some(json!({"offset":1,"lease_seconds":60})),
        )
        .await;
    assert_eq!(
        (code, body["error"]["code"].as_str()),
        (501, Some("unsupported"))
    );
    let (_, status) = server.request("GET", "/api/v1/status", None).await;
    assert!(status["heating_offset_lease"].is_null());
    // Unleased writes work as before.
    let (code, _) = server
        .request(
            "POST",
            "/api/v1/controls/heating-offset",
            Some(json!({"offset":1})),
        )
        .await;
    assert_eq!(code, 200);
}

#[tokio::test]
async fn audit_log_records_changes_with_actor_and_outcome_and_hides_secrets() {
    let server = Server::new("normal_heating", true).await;
    // A change by the admin, with a secret in the body.
    let (code, _) = server
        .request(
            "POST",
            "/api/v1/integrations/mqtt",
            Some(json!({
                "enabled": false, "host": "10.0.0.2", "port": 1883, "username": "ha",
                "password": "s3cret-broker", "topic_prefix": "nibe", "discovery": true,
                "discovery_prefix": "homeassistant", "allow_commands": false
            })),
        )
        .await;
    assert_eq!(code, 200);
    // A rejected change is recorded with the error.
    let (code, _) = server
        .request(
            "POST",
            "/api/v1/controls/heating-offset",
            Some(json!({"offset": 99})),
        )
        .await;
    assert_eq!(code, 422);
    // A change by an API key carries the key's name.
    let (_, key) = server
        .request(
            "POST",
            "/api/v1/access/api-keys",
            Some(json!({"name": "home-assistant", "scope": "control"})),
        )
        .await;
    let token = key["token"].as_str().unwrap().to_string();
    let authorization = format!("Bearer {token}");
    // Simulated controls exist only in dev builds; elsewhere the handler refuses: recorded as failed.
    let (key_code, body, _) = server
        .raw_headers(
            "POST",
            "/api/v1/controls/heating-offset",
            r#"{"offset": -2}"#,
            &[("Authorization", &authorization)],
        )
        .await;
    assert_eq!(
        key_code,
        if cfg!(feature = "dev") { 200 } else { 403 },
        "{body}"
    );
    // Not changes: a read, a login attempt and a refused (unauthenticated) request.
    server
        .request(
            "POST",
            "/api/v1/auth/login",
            Some(json!({"password": "wrong"})),
        )
        .await;
    server
        .raw_headers(
            "POST",
            "/api/v1/controls/heating-offset",
            r#"{"offset": 1}"#,
            &[("X-Requested-With", "nibe-gateway")],
        )
        .await;

    let (code, page) = server.request("GET", "/api/v1/audit", None).await;
    assert_eq!(code, 200, "{page}");
    let entries = page["entries"].as_array().unwrap();
    let summary: Vec<_> = entries
        .iter()
        .map(|e| {
            (
                e["actor"].as_str().unwrap(),
                e["action"].as_str().unwrap(),
                e["ok"].as_bool().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            ("home-assistant", "Heating offset", key_code == 200),
            ("admin", "API key created", true),
            ("admin", "Heating offset", false),
            ("admin", "MQTT settings changed", true),
        ],
        "{page}"
    );
    assert!(entries[2]["outcome"].as_str().unwrap().starts_with("422"));
    assert!(entries[0]["outcome"]
        .as_str()
        .unwrap()
        .starts_with(&key_code.to_string()));
    assert!(entries[0]["detail"].as_str().unwrap().contains("-2"));
    let text = page.to_string();
    assert!(
        !text.contains("s3cret-broker") && !text.contains(&token),
        "{text}"
    );
    assert!(entries[3]["detail"].as_str().unwrap().contains("10.0.0.2"));
    // Paging: older entries after the last seq.
    let last = entries[1]["seq"].as_u64().unwrap();
    let (_, older) = server
        .request("GET", &format!("/api/v1/audit?before={last}&limit=1"), None)
        .await;
    assert_eq!(older["entries"][0]["action"], "Heating offset");
    assert_eq!(older["more"], true);
    // API keys cannot read the audit log.
    let (code, _, _) = server
        .raw_headers(
            "GET",
            "/api/v1/audit",
            "",
            &[("Authorization", &authorization)],
        )
        .await;
    assert_eq!(code, 403);
}
