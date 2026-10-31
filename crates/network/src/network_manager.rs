//! NetworkManager system D-Bus backend. Never invokes nmcli or logs D-Bus errors:
//! NetworkManager error text can contain SSIDs or credentials.
use super::*;
use std::{collections::HashMap, time::Duration};
use zbus::{
    zvariant::{OwnedObjectPath, OwnedValue, Value},
    Connection, Proxy,
};
const NM: &str = "org.freedesktop.NetworkManager";
const ROOT: &str = "/org/freedesktop/NetworkManager";
const SETTINGS: &str = "/org/freedesktop/NetworkManager/Settings";
type Settings = HashMap<String, HashMap<String, OwnedValue>>;
fn value(v: impl Into<Value<'static>>) -> OwnedValue {
    v.into().try_into().expect("owned D-Bus value")
}
fn unavailable(_: impl std::fmt::Display) -> NetworkError {
    NetworkError::Unavailable
}
async fn bounded<T>(
    seconds: u64,
    operation: impl Future<Output = NetworkResult<T>>,
) -> NetworkResult<T> {
    tokio::time::timeout(Duration::from_secs(seconds), operation)
        .await
        .map_err(unavailable)?
}
pub struct NetworkManager {
    bus: Connection,
    device: OwnedObjectPath,
    interface: String,
    pending: Option<(OwnedObjectPath, OwnedObjectPath)>,
}
impl NetworkManager {
    /// Interface must be selected deliberately (normally wlan0); never choose Ethernet.
    /// Waits (bounded) for NetworkManager and a managed Wi-Fi device. At boot the gateway can start
    /// before NetworkManager manages wlan0; failing then made systemd restart the gateway and left
    /// the saved station unrestored (HP-30, 2026-10-25).
    pub async fn new(interface: &str) -> NetworkResult<Self> {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(90);
        loop {
            match Self::connect_device(interface).await {
                Ok(backend) => return Ok(backend),
                Err(error) if tokio::time::Instant::now() >= deadline => return Err(error),
                Err(_) => tokio::time::sleep(Duration::from_secs(2)).await,
            }
        }
    }
    async fn connect_device(interface: &str) -> NetworkResult<Self> {
        bounded(10, async {
            let bus = Connection::system().await.map_err(unavailable)?;
            let manager = Proxy::new(&bus, NM, ROOT, NM).await.map_err(unavailable)?;
            let device: OwnedObjectPath = manager
                .call("GetDeviceByIpIface", &(interface,))
                .await
                .map_err(unavailable)?;
            let proxy = Proxy::new(
                &bus,
                NM,
                device.as_str(),
                "org.freedesktop.NetworkManager.Device",
            )
            .await
            .map_err(unavailable)?;
            let kind: u32 = proxy
                .get_property("DeviceType")
                .await
                .map_err(unavailable)?;
            if kind != 2 {
                return Err(NetworkError::Unavailable);
            }
            // 10 = NM_DEVICE_STATE_UNMANAGED: NetworkManager has not taken the device over yet.
            let state: u32 = proxy.get_property("State").await.map_err(unavailable)?;
            if state <= 10 {
                return Err(NetworkError::Unavailable);
            }
            drop(proxy);
            drop(manager);
            Ok(Self {
                bus,
                device,
                interface: interface.into(),
                pending: None,
            })
        })
        .await
    }
    async fn proxy<'a>(&'a self, path: &'a str, interface: &'a str) -> NetworkResult<Proxy<'a>> {
        Proxy::new(&self.bus, NM, path, interface)
            .await
            .map_err(unavailable)
    }
    async fn activated(&self, active: &OwnedObjectPath) -> NetworkResult<()> {
        let proxy = self
            .proxy(
                active.as_str(),
                "org.freedesktop.NetworkManager.Connection.Active",
            )
            .await?;
        loop {
            let state: u32 = proxy.get_property("State").await.map_err(unavailable)?;
            match state {
                2 => {
                    // Device activated means association + IP configuration, not internet access.
                    let device = self
                        .proxy(
                            self.device.as_str(),
                            "org.freedesktop.NetworkManager.Device",
                        )
                        .await?;
                    let state: u32 = device.get_property("State").await.map_err(unavailable)?;
                    if state == 100 {
                        return Ok(());
                    }
                }
                3 | 4 => return Err(NetworkError::InvalidCredentials),
                _ => {}
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }
    fn settings(&self, ssid: &str, password: &str, ap: bool) -> Settings {
        let mut settings = Settings::new();
        settings.insert(
            "connection".into(),
            HashMap::from([
                (
                    "id".into(),
                    value(if ap {
                        "nibe-gateway-setup".into()
                    } else {
                        format!("nibe-gateway-station-{ssid}")
                    }),
                ),
                ("type".into(), value("802-11-wireless")),
                // Enables settings.modify.own without broad modify.system rights.
                (
                    "permissions".into(),
                    value(vec!["user:nibe-gateway:".to_string()]),
                ),
                ("interface-name".into(), value(self.interface.clone())),
                // Setup AP is always explicit and memory-only. Station reconnect is allowed.
                ("autoconnect".into(), value(!ap)),
            ]),
        );
        settings.insert(
            "802-11-wireless".into(),
            HashMap::from([
                ("ssid".into(), value(ssid.as_bytes().to_vec())),
                (
                    "mode".into(),
                    value(if ap { "ap" } else { "infrastructure" }),
                ),
                ("band".into(), value("bg")),
            ]),
        );
        if !password.is_empty() {
            settings.insert(
                "802-11-wireless-security".into(),
                HashMap::from([
                    ("key-mgmt".into(), value("wpa-psk")),
                    ("psk".into(), value(password.to_owned())),
                    ("proto".into(), value(vec!["rsn".to_string()])),
                    ("pairwise".into(), value(vec!["ccmp".to_string()])),
                ]),
            );
        }
        let mut ipv4 =
            HashMap::from([("method".into(), value(if ap { "shared" } else { "auto" }))]);
        if ap {
            let address = HashMap::from([
                ("address".to_string(), value("192.168.4.1")),
                ("prefix".into(), value(24u32)),
            ]);
            ipv4.insert("address-data".into(), value(vec![address]));
            // The setup subnet must not become the gateway's default route.
            ipv4.insert("never-default".into(), value(true));
        }
        settings.insert("ipv4".into(), ipv4);
        settings.insert(
            "ipv6".into(),
            HashMap::from([("method".into(), value("disabled"))]),
        );
        settings
    }
    async fn add(&self, settings: Settings) -> NetworkResult<(OwnedObjectPath, OwnedObjectPath)> {
        let manager = self.proxy(ROOT, NM).await?;
        let root = OwnedObjectPath::try_from("/").unwrap();
        let options = HashMap::from([("persist", value("memory"))]);
        let (path, active, _): (
            OwnedObjectPath,
            OwnedObjectPath,
            HashMap<String, OwnedValue>,
        ) = manager
            .call(
                "AddAndActivateConnection2",
                &(settings, &self.device, root, options),
            )
            .await
            .map_err(unavailable)?;
        self.activated(&active).await?;
        Ok((path, active))
    }
    async fn profiles(&self) -> NetworkResult<Vec<(OwnedObjectPath, String)>> {
        let settings = self
            .proxy(SETTINGS, "org.freedesktop.NetworkManager.Settings")
            .await?;
        let paths: Vec<OwnedObjectPath> = settings
            .call("ListConnections", &())
            .await
            .map_err(unavailable)?;
        let mut result = Vec::new();
        for path in paths {
            let proxy = self
                .proxy(
                    path.as_str(),
                    "org.freedesktop.NetworkManager.Settings.Connection",
                )
                .await?;
            let data: Settings = proxy.call("GetSettings", &()).await.map_err(unavailable)?;
            if let Some(connection) = data.get("connection") {
                let id = connection.get("id").and_then(|v| <&str>::try_from(v).ok());
                let iface = connection
                    .get("interface-name")
                    .and_then(|v| <&str>::try_from(v).ok());
                if let Some(id) = id.filter(|id| {
                    id.starts_with("nibe-gateway-") && iface == Some(self.interface.as_str())
                }) {
                    result.push((path.clone(), id.into()));
                }
            }
        }
        Ok(result)
    }
    async fn delete(&self, path: &OwnedObjectPath) -> NetworkResult<()> {
        self.proxy(
            path.as_str(),
            "org.freedesktop.NetworkManager.Settings.Connection",
        )
        .await?
        .call::<_, _, ()>("Delete", &())
        .await
        .map_err(unavailable)
    }
}
impl Network for NetworkManager {
    fn ip_addresses(&mut self) -> Operation<'_, Vec<String>> {
        Box::pin(bounded(3, async move {
            let device = self
                .proxy(
                    self.device.as_str(),
                    "org.freedesktop.NetworkManager.Device",
                )
                .await?;
            let mut addresses = vec![];
            for (property, interface) in [
                ("Ip4Config", "org.freedesktop.NetworkManager.IP4Config"),
                ("Ip6Config", "org.freedesktop.NetworkManager.IP6Config"),
            ] {
                let path: OwnedObjectPath =
                    device.get_property(property).await.map_err(unavailable)?;
                if path.as_str() == "/" {
                    continue;
                }
                let config = self.proxy(path.as_str(), interface).await?;
                let data: Vec<HashMap<String, OwnedValue>> = config
                    .get_property("AddressData")
                    .await
                    .map_err(unavailable)?;
                for item in data {
                    if let Some(address) =
                        item.get("address").and_then(|v| <&str>::try_from(v).ok())
                    {
                        if let Ok(ip) = address.parse::<std::net::IpAddr>() {
                            addresses.push(ip.to_string());
                        }
                    }
                }
            }
            addresses.sort();
            addresses.dedup();
            Ok(addresses)
        }))
    }
    fn simulated(&self) -> bool {
        false
    }
    fn scan(&mut self) -> Operation<'_, Vec<AccessPoint>> {
        Box::pin(bounded(12, async move {
            let wireless = self
                .proxy(
                    self.device.as_str(),
                    "org.freedesktop.NetworkManager.Device.Wireless",
                )
                .await?;
            // AP drivers may refuse an active scan; the cached list is still useful.
            let previous: i64 = wireless
                .get_property("LastScan")
                .await
                .map_err(unavailable)?;
            let requested = wireless
                .call::<_, _, ()>("RequestScan", &(HashMap::<String, OwnedValue>::new(),))
                .await
                .is_ok();
            if requested {
                let _ = tokio::time::timeout(Duration::from_secs(5), async {
                    loop {
                        let current: i64 = wireless
                            .get_property("LastScan")
                            .await
                            .map_err(unavailable)?;
                        if current > previous {
                            return Ok::<(), NetworkError>(());
                        }
                        tokio::time::sleep(Duration::from_millis(200)).await;
                    }
                })
                .await;
            }
            let paths: Vec<OwnedObjectPath> = wireless
                .call("GetAllAccessPoints", &())
                .await
                .map_err(unavailable)?;
            let mut networks = Vec::<AccessPoint>::new();
            for path in paths.into_iter().take(128) {
                let ap = self
                    .proxy(path.as_str(), "org.freedesktop.NetworkManager.AccessPoint")
                    .await?;
                let bytes: Vec<u8> = ap.get_property("Ssid").await.map_err(unavailable)?;
                let Ok(ssid) = String::from_utf8(bytes) else {
                    continue;
                };
                if ssid.is_empty() || networks.iter().any(|ap| ap.ssid == ssid) {
                    continue;
                }
                let signal: u8 = ap.get_property("Strength").await.map_err(unavailable)?;
                let flags: u32 = ap.get_property("Flags").await.map_err(unavailable)?;
                networks.push(AccessPoint {
                    ssid,
                    signal,
                    secured: flags & 1 != 0,
                });
            }
            networks.sort_by_key(|n| std::cmp::Reverse(n.signal));
            Ok(networks)
        }))
    }
    fn start_ap<'a>(&'a mut self, secrets: &'a SetupSecrets) -> Operation<'a, ()> {
        Box::pin(async move {
            if secrets.ap_password.len() < 12 {
                return Err(NetworkError::Unavailable);
            }
            let checkpoint = bounded(5, async {
                self.proxy(ROOT, NM)
                    .await?
                    .call::<_, _, OwnedObjectPath>(
                        "CheckpointCreate",
                        &(vec![self.device.clone()], 60u32, 2u32),
                    )
                    .await
                    .map_err(unavailable)
            })
            .await?;
            let result = bounded(
                40,
                self.add(self.settings(&secrets.ssid(), &secrets.ap_password, true)),
            )
            .await;
            match result {
                Ok((active, _)) => {
                    bounded(5, async {
                        self.proxy(ROOT, NM)
                            .await?
                            .call::<_, _, ()>("CheckpointDestroy", &(&checkpoint,))
                            .await
                            .map_err(unavailable)
                    })
                    .await?;
                    // Cleanup is best effort *after* activation/commit; it cannot turn a
                    // committed switch into a reported transaction failure.
                    let _ = bounded(3, async {
                        for (old, id) in self.profiles().await? {
                            if old != active && id == "nibe-gateway-setup" {
                                let _ = self.delete(&old).await;
                            }
                        }
                        Ok(())
                    })
                    .await;
                    Ok(())
                }
                Err(error) => {
                    let _ = bounded(5, async {
                        self.proxy(ROOT, NM)
                            .await?
                            .call::<_, _, HashMap<String, u32>>(
                                "CheckpointRollback",
                                &(checkpoint,),
                            )
                            .await
                            .map_err(unavailable)
                    })
                    .await;
                    Err(error)
                }
            }
        })
    }
    fn connect<'a>(&'a mut self, ssid: &'a str, password: &'a str) -> Operation<'a, ()> {
        Box::pin(async move {
            let checkpoint = bounded(5, async {
                self.proxy(ROOT, NM)
                    .await?
                    .call::<_, _, OwnedObjectPath>(
                        "CheckpointCreate",
                        &(vec![self.device.clone()], 60u32, 2u32),
                    )
                    .await
                    .map_err(unavailable)
            })
            .await?;
            let result = bounded(40, self.add(self.settings(ssid, password, false))).await;
            match result {
                Ok((path, _)) => {
                    self.pending = Some((checkpoint, path));
                    Ok(())
                }
                Err(error) => {
                    let _ = bounded(5, async {
                        self.proxy(ROOT, NM)
                            .await?
                            .call::<_, _, HashMap<String, u32>>(
                                "CheckpointRollback",
                                &(checkpoint,),
                            )
                            .await
                            .map_err(unavailable)
                    })
                    .await;
                    Err(error)
                }
            }
        })
    }
    fn finish(&mut self, commit: bool) -> Operation<'_, ()> {
        Box::pin(async move {
            let Some((checkpoint, path)) = self.pending.clone() else {
                return Ok(());
            };
            if commit {
                bounded(5, async {
                    self.proxy(
                        path.as_str(),
                        "org.freedesktop.NetworkManager.Settings.Connection",
                    )
                    .await?
                    .call::<_, _, ()>("Save", &())
                    .await
                    .map_err(unavailable)
                })
                .await?;
                bounded(5, async {
                    self.proxy(ROOT, NM)
                        .await?
                        .call::<_, _, ()>("CheckpointDestroy", &(checkpoint,))
                        .await
                        .map_err(unavailable)
                })
                .await?;
                self.pending = None;
                let _ = bounded(3, async {
                    for (old, _) in self.profiles().await? {
                        if old != path {
                            let _ = self.delete(&old).await;
                        }
                    }
                    Ok(())
                })
                .await;
                Ok(())
            } else {
                self.pending = None;
                bounded(5, async {
                    let result = self
                        .proxy(ROOT, NM)
                        .await?
                        .call::<_, _, HashMap<String, u32>>("CheckpointRollback", &(checkpoint,))
                        .await
                        .map_err(unavailable)?;
                    if result.values().any(|code| *code != 0) {
                        return Err(NetworkError::Unavailable);
                    }
                    Ok(())
                })
                .await
            }
        })
    }
    fn restore<'a>(&'a mut self, config: &'a NetworkConfig) -> Operation<'a, ()> {
        Box::pin(bounded(40, async move {
            let id = format!("nibe-gateway-station-{}", config.ssid);
            let path = self
                .profiles()
                .await?
                .into_iter()
                .find(|(_, name)| *name == id)
                .map(|(p, _)| p)
                .ok_or(NetworkError::Unavailable)?;
            let active: OwnedObjectPath = self
                .proxy(ROOT, NM)
                .await?
                .call(
                    "ActivateConnection",
                    &(path, &self.device, OwnedObjectPath::try_from("/").unwrap()),
                )
                .await
                .map_err(unavailable)?;
            self.activated(&active).await
        }))
    }
    fn connected<'a>(&'a mut self, ssid: &'a str) -> Operation<'a, bool> {
        Box::pin(bounded(5, async move {
            let proxy = self
                .proxy(
                    self.device.as_str(),
                    "org.freedesktop.NetworkManager.Device",
                )
                .await?;
            let state: u32 = proxy.get_property("State").await.map_err(unavailable)?;
            let wifi = self
                .proxy(
                    self.device.as_str(),
                    "org.freedesktop.NetworkManager.Device.Wireless",
                )
                .await?;
            let mode: u32 = wifi.get_property("Mode").await.map_err(unavailable)?;
            if state != 100 || mode != 2 {
                return Ok(false);
            }
            let active: OwnedObjectPath = wifi
                .get_property("ActiveAccessPoint")
                .await
                .map_err(unavailable)?;
            if active.as_str() == "/" {
                return Ok(false);
            }
            let access_point = self
                .proxy(
                    active.as_str(),
                    "org.freedesktop.NetworkManager.AccessPoint",
                )
                .await?;
            let current: Vec<u8> = access_point
                .get_property("Ssid")
                .await
                .map_err(unavailable)?;
            Ok(current == ssid.as_bytes())
        }))
    }
    fn forget(&mut self) -> Operation<'_, ()> {
        Box::pin(bounded(15, async move {
            for (path, _) in self.profiles().await? {
                self.delete(&path).await?;
            }
            Ok(())
        }))
    }
}
