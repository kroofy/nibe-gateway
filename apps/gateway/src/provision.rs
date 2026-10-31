//! Applies `state/provision.json` written by `nibe-provision.service` from the boot partition (ADR 0060).
use nibe_api::auth::Auth;
use nibe_network::Provisioner;
use serde::Deserialize;
use std::{path::PathBuf, sync::Arc};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Wifi {
    ssid: String,
    #[serde(default)]
    password: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Provision {
    wifi: Option<Wifi>,
    admin_password: Option<String>,
}

/// Reads and deletes the file before applying it, so secrets never outlive one attempt. Wi-Fi uses the
/// normal connect transaction: a failure leaves the setup AP running exactly as a failed UI attempt does.
pub async fn apply(data_dir: PathBuf, auth: Arc<Auth>, network: Arc<Provisioner>) {
    let path = data_dir.join("provision.json");
    let Ok(bytes) = std::fs::read(&path) else {
        return;
    };
    if let Err(error) = std::fs::remove_file(&path) {
        tracing::error!(%error, "cannot delete provision.json; not applying it");
        return;
    }
    let provision: Provision = match serde_json::from_slice(&bytes) {
        Ok(p) => p,
        Err(error) => {
            tracing::error!(%error, "invalid provision.json ignored");
            return;
        }
    };
    if let Some(password) = provision.admin_password {
        match auth.provision_password(password).await {
            Ok(()) => tracing::info!("admin password provisioned"),
            Err(_) => tracing::error!("admin password provisioning failed"),
        }
    }
    if let Some(wifi) = provision.wifi {
        match network.connect(wifi.ssid, wifi.password).await {
            Ok(status) => tracing::info!(state = ?status.state, "Wi-Fi provisioned"),
            Err(error) => {
                tracing::error!(%error, "Wi-Fi provisioning failed; setup AP stays available")
            }
        }
    }
}
