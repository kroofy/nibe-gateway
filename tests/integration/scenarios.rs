#[path = "../contract/support/mod.rs"]
mod support;
use nibe_core::*;
use serde_json::json;
use std::time::Duration;
use support::Server;

#[tokio::test]
async fn scenario_fixtures_are_observable_through_same_http_api() {
    for scenario in nibe_simulator::scenarios() {
        let server = Server::new(&scenario.name, false).await;
        let (_, state) = server.request("GET", "/api/v1/device", None).await;
        match scenario.fault {
            nibe_simulator::Fault::Disconnect
            | nibe_simulator::Fault::Malformed
            | nibe_simulator::Fault::Timeout => {
                assert_eq!(state["stale"], true, "{}", scenario.name);
                assert_eq!(state["connection"], "disconnected");
                assert!(state["diagnostics"]["transport_errors"].as_u64().unwrap() > 0);
            }
            _ => {
                assert_eq!(
                    state["measurements"][0]["value"]["value"],
                    json!(scenario.outdoor)
                );
                assert_eq!(
                    state["network"],
                    serde_json::to_value(scenario.network).unwrap()
                );
                assert_eq!(
                    state["stale"],
                    scenario.fault == nibe_simulator::Fault::Stale
                );
            }
        }
    }
}

#[tokio::test]
async fn slow_event_consumer_does_not_stop_device_updates() {
    let server = Server::new("normal_heating", false).await;
    let mut receiver = server.gateway.subscribe(); // deliberately do not consume
    let before = server.gateway.snapshot().diagnostics.polls;
    tokio::time::timeout(Duration::from_secs(2), async {
        while server.gateway.snapshot().diagnostics.polls < before + 8 {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .unwrap();
    assert!(matches!(
        receiver.recv().await,
        Err(tokio::sync::broadcast::error::RecvError::Lagged(_))
    ));
    assert_eq!(server.request("GET", "/api/v1/status", None).await.0, 200);
}

#[tokio::test]
async fn saturated_adapter_queue_and_timeouts_leave_http_cache_responsive() {
    let server = Server::new("timeout", false).await;
    let mut requests = Vec::new();
    for _ in 0..12 {
        let gateway = server.gateway.clone();
        requests.push(tokio::spawn(async move { gateway.discover().await }));
    }
    let (code, status) = tokio::time::timeout(
        Duration::from_millis(100),
        server.request("GET", "/api/v1/status", None),
    )
    .await
    .expect("HTTP cache must not wait for adapter");
    assert_eq!(code, 200);
    assert_eq!(status["stale"], true);
    let mut busy = 0;
    let mut timeouts = 0;
    for task in requests {
        match task.await.unwrap().unwrap_err().code {
            ErrorCode::Busy => busy += 1,
            ErrorCode::Timeout => timeouts += 1,
            other => panic!("unexpected error: {other:?}"),
        }
    }
    assert!(busy > 0);
    assert!(timeouts > 0);
}

#[cfg(feature = "dev")]
#[tokio::test]
async fn dev_time_fault_recovery_and_command_history() {
    let server = Server::new("normal_heating", true).await;
    assert_eq!(server.request("GET", "/__dev/commands", None).await.0, 403);
    server.auth.as_ref().unwrap().set_debug(300);
    let (code, _) = server
        .request(
            "POST",
            "/api/v1/controls/heating-offset",
            Some(json!({"offset":3})),
        )
        .await;
    assert_eq!(code, 200);
    let (_, history) = server.request("GET", "/__dev/commands", None).await;
    assert_eq!(history[0]["command"]["offset"], 3);
    let (code, _) = server
        .request("POST", "/__dev/fault", Some(json!({"kind":"stale"})))
        .await;
    assert_eq!(code, 200);
    let (_, stale) = server
        .request(
            "POST",
            "/__dev/time/advance",
            Some(json!({"milliseconds":6000})),
        )
        .await;
    assert_eq!(stale["stale"], true);
    assert_eq!(stale["measurements"][0]["age_ms"], 6000);
    let (_, broken) = server
        .request("POST", "/__dev/fault", Some(json!({"kind":"malformed"})))
        .await;
    assert!(broken["diagnostics"]["malformed_frames"].as_u64().unwrap() > 0);
    assert_eq!(broken["stale"], true);
    let (_, recovered) = server
        .request("POST", "/__dev/fault", Some(json!({"kind":"none"})))
        .await;
    assert_eq!(recovered["stale"], false);
    assert_eq!(recovered["measurements"][3]["value"]["value"], 3);
    server
        .request(
            "POST",
            "/__dev/scenario",
            Some(json!({"name":"rejected_write"})),
        )
        .await;
    let (code, rejected) = server
        .request(
            "POST",
            "/api/v1/controls/heating-offset",
            Some(json!({"offset":2})),
        )
        .await;
    assert_eq!(code, 409);
    assert_eq!(rejected["error"]["code"], "rejected");
    let (_, reset) = server.request("POST", "/__dev/reset", None).await;
    assert_eq!(reset["stale"], false);
    let (_, history) = server.request("GET", "/__dev/commands", None).await;
    assert!(history.as_array().unwrap().is_empty());
    assert_eq!(
        server
            .request(
                "POST",
                "/__dev/scenario",
                Some(json!({"name":"nonexistent"}))
            )
            .await
            .0,
        422
    );
    assert_eq!(
        server
            .request(
                "POST",
                "/__dev/time/advance",
                Some(json!({"milliseconds":86_400_001_u64}))
            )
            .await
            .0,
        422
    );
}
