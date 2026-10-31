mod config;
mod provision;
use config::Config;
use nibe_api::{auth::Auth, provisioning::ProvisioningState};
use nibe_core::{start_device, Clock, Device, ServiceConfig, SystemClock};
use nibe_device_f730::{
    catalog::Catalog, decode::WordOrder, start_adapter, transport::Capture, AdapterConfig,
    TransportConfig,
};
use nibe_network::{Network, Provisioner, SimulatedNetwork};
use nibe_persistence::Store;
use nibe_simulator::{SimClock, SimulatedDevice};
use std::{error::Error, sync::Arc};
use tower_http::services::{ServeDir, ServeFile};

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> Result<(), Box<dyn Error>> {
    nibe_api::record_process_start();
    tracing::debug!(sqlite = nibe_history::sqlite_version(), "history backend");
    let base_filter = std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into());
    let (filter, reload) =
        tracing_subscriber::reload::Layer::new(tracing_subscriber::EnvFilter::new(&base_filter));
    use tracing_subscriber::prelude::*;
    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer())
        .init();
    let verbose_enabled = std::sync::atomic::AtomicBool::new(false);
    let verbose: Arc<dyn Fn(bool) + Send + Sync> = Arc::new(move |enabled| {
        if verbose_enabled.swap(enabled, std::sync::atomic::Ordering::SeqCst) == enabled {
            return;
        }
        let directives = if enabled {
            format!("{base_filter},nibe_gateway=debug,nibe_core=debug,nibe_api=debug")
        } else {
            base_filter.clone()
        };
        let _ = reload.reload(tracing_subscriber::EnvFilter::new(directives));
    });
    let Some(config) = Config::load()? else {
        return Ok(());
    };
    config.validate()?;
    let (device, clock, f730): (Box<dyn Device>, Arc<dyn Clock>, _) = if config.device
        == "simulated"
    {
        let clock = Arc::new(SimClock::default());
        clock.freeze(config.freeze_time);
        if config.freeze_time {
            clock.reset();
        }
        (
            Box::new(SimulatedDevice::new(clock.clone(), &config.scenario)?),
            clock,
            None,
        )
    } else {
        let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
        let catalog = match &config.catalog {
            Some(path) => Catalog::load(&std::fs::read(path)?)?,
            None => Catalog::bundled(),
        };
        let transport = if config.device == "replay" {
            TransportConfig::Replay(match &config.capture {
                Some(path) => Capture::open(path)?,
                None => Capture::bundled(),
            })
        } else {
            TransportConfig::Hardware(config.serial.clone())
        };
        let word_order = match config.word_swap.as_str() {
            "0" => WordOrder::HighWordFirst,
            "1" => WordOrder::LowWordFirst,
            _ if config.device == "replay" && config.capture.is_none() => WordOrder::LowWordFirst,
            _ => WordOrder::Unknown,
        };
        let (device, handle) = start_adapter(
            AdapterConfig {
                transport,
                catalog,
                word_order,
                capture_path: config.record_capture.clone(),
                wire: config.wire.clone(),
                hardware_reads: config.allow_hardware_reads,
                poll_registers: config.poll_registers.clone(),
                slow_poll_registers: config.slow_poll_registers.clone(),
                slow_poll_interval_s: config.slow_poll_interval_s,
                alarm_reset: nibe_device_f730::writes::AlarmResetPolicy {
                    manual: config.allow_alarm_reset,
                    auto_251: config.alarm_auto_reset,
                },
                writable_registers: if config.allow_hardware_writes {
                    config.writable_registers.clone()
                } else {
                    vec![]
                },
                open_writes: config.open_writes_active(),
            },
            clock.clone(),
        )
        .await?;
        (Box::new(device), clock, Some(handle))
    };
    let service_config = ServiceConfig {
        stale_after_ms: config.stale_after_ms,
        allow_simulated_controls: config.allow_simulated_controls,
        hardware_controls: config.open_writes_active()
            || config.allow_hardware_writes
            || config.allow_alarm_reset
            || config.alarm_auto_reset,
        ..ServiceConfig::default()
    };
    let (gateway, service) = start_device(device, clock, service_config);
    // Start wire servicing before awaiting potentially slow radio transitions.
    let identity = device_identity();
    let store = Arc::new(Store::open(config.data_dir(), &identity)?);
    let backend: Box<dyn Network> = match config.network_backend() {
        "network-manager" => {
            #[cfg(feature = "network-manager")]
            {
                Box::new(
                    nibe_network::network_manager::NetworkManager::new(&config.network_interface)
                        .await?,
                )
            }
            #[cfg(not(feature = "network-manager"))]
            {
                return Err("NetworkManager feature is disabled".into());
            }
        }
        _ => Box::<SimulatedNetwork>::default(),
    };
    let network = Arc::new(Provisioner::new(backend, store.clone()).await?);
    // The saved station can appear after start (NetworkManager still starting, Wi-Fi outage).
    network.spawn_restore_retries(std::time::Duration::from_secs(30));
    if config.recover_network {
        network.recovery().await?;
    }
    // Starts with any lease persisted before a restart; a lapsed one is restored right away (ADR 0123).
    let leases = nibe_api::lease::Leases::start(gateway.clone(), Some(store.clone()));
    // Audit log of every change (ADR 0128), next to the settings on the data partition.
    let audit = Arc::new(nibe_persistence::audit::AuditLog::open(&config.data_dir()));
    leases.set_audit(audit.clone());
    let pump_writes = f730.clone().map(|handle| {
        nibe_api::audit::follow_pump_writes(handle, audit.clone(), gateway.shutdown_receiver())
    });
    let auth = Arc::new(Auth::new(store, config.secure_cookie));
    tokio::spawn(provision::apply(
        config.data_dir(),
        auth.clone(),
        network.clone(),
    ));
    let maintenance = nibe_api::maintenance::router(nibe_api::maintenance::Maintenance {
        auth: auth.clone(),
        gateway: gateway.clone(),
        socket: std::env::var_os("NIBE_UPDATER_SOCKET")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| "/run/nibe-updater/control.sock".into()),
        system_socket: std::env::var_os("NIBE_SYSTEM_SOCKET")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| "/run/nibe-system/control.sock".into()),
        capture: config.record_capture.clone(),
        verbose,
        releases: Arc::new(nibe_api::releases::Releases::new(
            config.update_repository.clone(),
            Arc::new(nibe_api::releases::Curl),
        )),
    });
    let simulated_controls = config.device == "simulated" && config.allow_simulated_controls;
    let writable = |id| {
        config.open_writes_active()
            || (config.allow_hardware_writes && config.writable_registers.contains(&id))
    };
    // Register reads, writes and poll choices by number, shared by the UI, HTTP and MQTT (ADR 0125, 0126).
    let registers = f730.clone().map(|handle| {
        nibe_api::registers::Registers::new(
            handle,
            gateway.clone(),
            Some(auth.store()),
            config.poll_registers.clone(),
            config.slow_poll_registers.clone(),
            Some(leases.clone()),
        )
    });
    if let Some(registers) = &registers {
        // Poll choices saved in the settings file; a failure leaves the configured lists running.
        if let Err(error) = registers.restore_polls().await {
            tracing::warn!(%error, "saved poll choices not applied");
        }
    }
    let mqtt = nibe_mqtt::start(
        gateway.clone(),
        nibe_mqtt::Identity {
            device_id: nibe_mqtt::device_id(&identity),
            gateway_version: env!("CARGO_PKG_VERSION").into(),
            audit: Some(audit.clone()),
        },
        nibe_mqtt::Controls {
            heating_offset: simulated_controls || writable(47011),
            temporary_lux: simulated_controls || writable(48132),
            alarm_reset: simulated_controls || config.allow_alarm_reset || config.alarm_auto_reset,
        },
        auth.store().config().mqtt,
        registers
            .clone()
            .map(|r| r as Arc<dyn nibe_mqtt::RegisterAccess>),
    );
    // History on the persistent data partition (ADR 0114); a failure leaves the gateway running without it.
    let history = match nibe_history::History::open(
        nibe_history::Config::new(config.data_dir().join("history")),
        nibe_history::clock_synced,
    ) {
        Ok(history) => {
            nibe_history::spawn_recorder(gateway.clone(), history.clone());
            Some(history)
        }
        Err(error) => {
            tracing::warn!(%error, "history disabled");
            None
        }
    };
    let mut app = nibe_api::provisioning::router_with_f730(
        gateway,
        ProvisioningState {
            auth: auth.clone(),
            network,
            identity,
            mqtt: Some(mqtt),
            history,
            leases: Some(leases),
            registers,
            audit: Some(audit.clone()),
        },
        f730.clone(),
    )
    .merge(maintenance);
    if let Some(dir) = config
        .admin_dir
        .filter(|dir| dir.join("index.html").is_file())
    {
        // API fallbacks remain JSON; SPA fallback applies only to non-API paths.
        let assets = ServeDir::new(&dir).fallback(ServeFile::new(dir.join("index.html")));
        app = app.fallback_service(axum::routing::get_service(assets));
        // Reserve unknown API/debug routes so missing API paths never return the SPA.
        app = app
            .route("/api/{*path}", axum::routing::any(api_not_found))
            .route("/__dev/{*path}", axum::routing::any(api_not_found));
    }
    let listener = tokio::net::TcpListener::bind(config.bind).await?;
    tracing::info!(address = %listener.local_addr()?, device = %config.device, dev = cfg!(feature = "dev"), "gateway listening");
    let stop = service.shutdown_trigger();
    let fatal_watch = f730.clone();
    // Outside every router, so changes on all of them are recorded (ADR 0128).
    let app = app.layer(axum::middleware::from_fn_with_state(
        nibe_api::audit::AuditLayer {
            log: audit.clone(),
            auth,
        },
        nibe_api::audit::record,
    ));
    let app = app.layer(axum::middleware::from_fn(nibe_api::security_headers));
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(async move {
        tokio::select! {
            _ = shutdown_signal() => {},
            _ = async { match fatal_watch { Some(handle) => handle.wait_fatal().await,
                None => std::future::pending::<()>().await } } => {}
        }
        stop();
    })
    .await?;
    service.shutdown().await;
    // Record pump outcomes still waiting for a read-back, then write the last entries before exit.
    if let Some(task) = pump_writes {
        let _ = tokio::time::timeout(std::time::Duration::from_secs(2), task).await;
    }
    audit.flush();
    if let Some(handle) = f730 {
        let fatal = handle.wire_health().device_fatal;
        handle.shutdown_and_wait().await?;
        if fatal {
            return Err("wire task exhausted UART recovery; systemd should restart gateway".into());
        }
    }
    Ok(())
}
async fn api_not_found() -> impl axum::response::IntoResponse {
    (
        axum::http::StatusCode::NOT_FOUND,
        axum::Json(nibe_api::ApiErrorBody {
            error: nibe_core::ServiceError::new(
                nibe_core::ErrorCode::Unsupported,
                "route not found",
            ),
        }),
    )
}
async fn shutdown_signal() {
    #[cfg(unix)]
    {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("install SIGTERM handler");
        tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = terminate.recv() => {} }
    }
    #[cfg(not(unix))]
    let _ = tokio::signal::ctrl_c().await;
}

fn device_identity() -> String {
    for path in [
        "/sys/firmware/devicetree/base/serial-number",
        "/sys/class/net/wlan0/address",
        "/etc/machine-id",
    ] {
        if let Ok(value) = std::fs::read_to_string(path) {
            let value = value.trim_matches(char::from(0)).trim();
            if !value.is_empty() {
                return value.into();
            }
        }
    }
    "simulated".into()
}
