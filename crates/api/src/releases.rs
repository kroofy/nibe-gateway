//! Update discovery from GitHub Releases.
//!
//! GitHub is only a transport. A downloaded release goes to the updater exactly like a manual upload, so the
//! updater's Ed25519 signature, target and compatibility checks remain the only trust decision. Nothing is
//! contacted in the background: the admin presses "Check for updates", and staging is a second explicit step.
//! HTTPS goes through the image's `curl` (CA-verified, HTTPS-only redirects), which keeps a TLS stack out of the
//! gateway binary. Strings from GitHub are never echoed: only a parsed version, a size, a timestamp and a URL
//! that must point at this repository's release page reach the UI.
use serde::{Deserialize, Serialize};
use std::{
    io::Read,
    process::{Command, Stdio},
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use utoipa::ToSchema;

/// Repository the official releases are published to; `--update-repository none` disables the check.
pub const DEFAULT_REPOSITORY: &str = "kroofy/nibe-gateway";
const METADATA_LIMIT: usize = 512 * 1024;
const MIN_CHECK_INTERVAL: Duration = Duration::from_secs(15);

/// A failed HTTPS request. Deliberately carries no detail: nothing from the network is echoed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FetchError;

/// Source of HTTPS bodies; the production implementation runs `curl`, tests inject canned responses.
pub trait Fetch: Send + Sync {
    /// GET `url`, following HTTPS-only redirects, failing on HTTP errors or a body over `limit` bytes.
    /// `progress` receives the number of bytes received so far.
    fn get(
        &self,
        url: &str,
        accept: &str,
        limit: usize,
        timeout: Duration,
        progress: &dyn Fn(u64),
    ) -> Result<Vec<u8>, FetchError>;
}

pub struct Curl;
impl Fetch for Curl {
    fn get(
        &self,
        url: &str,
        accept: &str,
        limit: usize,
        timeout: Duration,
        progress: &dyn Fn(u64),
    ) -> Result<Vec<u8>, FetchError> {
        if !url.starts_with("https://") {
            return Err(FetchError);
        }
        let mut child = Command::new("curl")
            .args(["--silent", "--fail", "--location", "--max-redirs", "5"])
            .args(["--proto", "=https", "--proto-redir", "=https", "--tlsv1.2"])
            .args(["--connect-timeout", "15", "--max-time"])
            .arg(timeout.as_secs().max(1).to_string())
            .arg("--max-filesize")
            .arg(limit.to_string())
            .arg("--header")
            .arg(format!("Accept: {accept}"))
            .arg("--user-agent")
            .arg(concat!("nibe-gateway/", env!("CARGO_PKG_VERSION")))
            // `--` ends options, so a URL can never be read as one.
            .arg("--")
            .arg(url)
            .env_remove("CURL_HOME")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| FetchError)?;
        let mut stdout = child
            .stdout
            .take()
            .ok_or(FetchError)?
            .take(limit as u64 + 1);
        let mut body = Vec::with_capacity(limit.min(METADATA_LIMIT));
        let mut chunk = [0u8; 64 * 1024];
        let read = loop {
            match stdout.read(&mut chunk) {
                Ok(0) => break Ok(()),
                Ok(n) => {
                    body.extend_from_slice(&chunk[..n]);
                    progress(body.len() as u64);
                }
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(e) => break Err(e),
            }
        };
        if read.is_err() || body.len() > limit {
            let _ = child.kill();
            let _ = child.wait();
            return Err(FetchError);
        }
        match child.wait() {
            Ok(status) if status.success() => Ok(body),
            _ => Err(FetchError),
        }
    }
}

#[derive(Clone, Serialize, ToSchema)]
pub struct RemoteRelease {
    /// Semantic version from the release tag (`v` prefix removed).
    pub version: String,
    /// Release page on GitHub (always below `https://github.com/<repository>/releases/`).
    pub url: String,
    pub published_at: Option<String>,
    /// Size in bytes of the signed release asset.
    pub asset_size: u64,
    /// True when this version is newer than the running gateway.
    pub newer: bool,
}

#[derive(Clone, Serialize, ToSchema)]
pub struct RemoteStatus {
    /// False when the gateway was started with `--update-repository none`.
    pub enabled: bool,
    pub repository: Option<String>,
    pub current_version: String,
    /// Milliseconds since the Unix epoch of the last completed check, if any.
    pub checked_at_ms: Option<u64>,
    /// Latest published release with a signed gateway asset, from the last successful check.
    pub latest: Option<RemoteRelease>,
    /// Fixed error code of the last check: `unreachable`, `no_release`, `no_asset` or `invalid_release`.
    pub error: Option<String>,
    /// A download of the latest release is in progress.
    pub downloading: bool,
    /// Progress of a running "download and stage", if any.
    pub staging: Option<StageProgress>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum StagePhase {
    /// Receiving the signed release from the release server.
    Downloading,
    /// Downloaded; the updater is verifying and staging it.
    Verifying,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, ToSchema)]
pub struct StageProgress {
    pub phase: StagePhase,
    pub received_bytes: u64,
    pub total_bytes: u64,
}

/// Keeps the staging slot (and its progress) until the caller has handed the release to the updater.
pub struct StagingGuard {
    releases: Arc<Releases>,
    _slot: tokio::sync::OwnedMutexGuard<()>,
}
impl Drop for StagingGuard {
    fn drop(&mut self) {
        *self.releases.progress.lock().unwrap() = None;
    }
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct StageRequest {
    /// Version shown by the last check; staging fails if a different release has been published since.
    pub version: String,
}

struct Checked {
    at: Instant,
    at_ms: u64,
    latest: Option<(RemoteRelease, String)>,
    error: Option<&'static str>,
}

pub struct Releases {
    repository: Option<String>,
    current: semver::Version,
    fetch: Arc<dyn Fetch>,
    last: Mutex<Option<Checked>>,
    download: Arc<tokio::sync::Mutex<()>>,
    progress: Mutex<Option<StageProgress>>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum StageError {
    Disabled,
    Busy,
    /// No check, or the requested version is not the latest checked one or not newer.
    NotOffered,
    Download,
}

/// `owner/name` with GitHub's allowed characters; nothing that could change the URL's meaning.
pub fn valid_repository(value: &str) -> bool {
    let ok = |part: &str| {
        !part.is_empty()
            && part.len() <= 100
            && part != "."
            && part != ".."
            && part
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
    };
    matches!(value.split_once('/'), Some((owner, name)) if ok(owner) && ok(name) && !name.contains('/'))
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

impl Releases {
    pub fn new(repository: Option<String>, fetch: Arc<dyn Fetch>) -> Self {
        Self {
            repository: repository.filter(|r| valid_repository(r)),
            current: semver::Version::parse(env!("CARGO_PKG_VERSION")).expect("workspace version"),
            fetch,
            last: Mutex::new(None),
            download: Arc::new(tokio::sync::Mutex::new(())),
            progress: Mutex::new(None),
        }
    }
    #[cfg(test)]
    fn with_version(mut self, version: &str) -> Self {
        self.current = semver::Version::parse(version).unwrap();
        self
    }

    pub fn status(&self) -> RemoteStatus {
        let last = self.last.lock().unwrap();
        RemoteStatus {
            enabled: self.repository.is_some(),
            repository: self.repository.clone(),
            current_version: self.current.to_string(),
            checked_at_ms: last.as_ref().map(|c| c.at_ms),
            latest: last
                .as_ref()
                .and_then(|c| c.latest.as_ref().map(|l| l.0.clone())),
            error: last.as_ref().and_then(|c| c.error.map(String::from)),
            downloading: self.download.try_lock().is_err(),
            staging: *self.progress.lock().unwrap(),
        }
    }

    /// Ask GitHub for the latest release. Repeated presses within a few seconds reuse the last answer, which keeps
    /// the gateway far below GitHub's anonymous limit of 60 requests per hour.
    pub async fn check(self: &Arc<Self>) -> RemoteStatus {
        let Some(repository) = self.repository.clone() else {
            return self.status();
        };
        let recent = matches!(&*self.last.lock().unwrap(), Some(last) if last.at.elapsed() < MIN_CHECK_INTERVAL);
        if recent {
            return self.status();
        }
        let this = self.clone();
        let url = format!("https://api.github.com/repos/{repository}/releases/latest");
        let result = tokio::task::spawn_blocking(move || {
            this.fetch.get(
                &url,
                "application/vnd.github+json",
                METADATA_LIMIT,
                Duration::from_secs(30),
                &|_| {},
            )
        })
        .await
        .unwrap_or(Err(FetchError));
        let checked = match result {
            Err(FetchError) => Checked {
                at: Instant::now(),
                at_ms: now_ms(),
                latest: None,
                error: Some("unreachable"),
            },
            Ok(body) => match self.parse(&repository, &body) {
                Ok(latest) => Checked {
                    at: Instant::now(),
                    at_ms: now_ms(),
                    latest: Some(latest),
                    error: None,
                },
                Err(code) => Checked {
                    at: Instant::now(),
                    at_ms: now_ms(),
                    latest: None,
                    error: Some(code),
                },
            },
        };
        *self.last.lock().unwrap() = Some(checked);
        self.status()
    }

    /// Release metadata → shown release plus its asset download URL.
    fn parse(
        &self,
        repository: &str,
        body: &[u8],
    ) -> Result<(RemoteRelease, String), &'static str> {
        #[derive(Deserialize)]
        struct Asset {
            name: String,
            size: u64,
            browser_download_url: String,
        }
        #[derive(Deserialize)]
        struct Release {
            tag_name: String,
            html_url: String,
            published_at: Option<String>,
            #[serde(default)]
            draft: bool,
            #[serde(default)]
            prerelease: bool,
            #[serde(default)]
            assets: Vec<Asset>,
        }
        let release: Release = serde_json::from_slice(body).map_err(|_| "invalid_release")?;
        if release.draft || release.prerelease {
            return Err("no_release");
        }
        let version = semver::Version::parse(
            release
                .tag_name
                .strip_prefix('v')
                .unwrap_or(&release.tag_name),
        )
        .map_err(|_| "invalid_release")?;
        let page = format!("https://github.com/{repository}/releases/");
        let download = format!("https://github.com/{repository}/releases/download/");
        if !release.html_url.starts_with(&page)
            || !release.html_url[page.len()..]
                .bytes()
                .all(|b| b.is_ascii_graphic())
        {
            return Err("invalid_release");
        }
        let name = format!("nibe-gateway-{version}.tar");
        let asset = release
            .assets
            .into_iter()
            .find(|a| a.name == name)
            .ok_or("no_asset")?;
        if asset.size == 0
            || asset.size > nibe_updater::artifact::MAX_ARTIFACT as u64
            || !asset.browser_download_url.starts_with(&download)
            || !asset.browser_download_url.ends_with(&format!("/{name}"))
        {
            return Err("invalid_release");
        }
        // Only the ISO-8601 shape survives; anything else from GitHub is dropped rather than echoed.
        let published_at = release.published_at.filter(|t| {
            t.len() <= 32
                && t.bytes()
                    .all(|b| b.is_ascii_digit() || b"-:TZ.+".contains(&b))
        });
        Ok((
            RemoteRelease {
                newer: version.cmp_precedence(&self.current).is_gt(),
                version: version.to_string(),
                url: release.html_url,
                published_at,
                asset_size: asset.size,
            },
            asset.browser_download_url,
        ))
    }

    /// Download the signed tar of the release shown by the last check. The caller hands it to the updater and
    /// drops the guard afterwards; until then the status reports the `verifying` phase.
    pub async fn download(
        self: &Arc<Self>,
        version: &str,
    ) -> Result<(Vec<u8>, StagingGuard), StageError> {
        if self.repository.is_none() {
            return Err(StageError::Disabled);
        }
        let slot = self
            .download
            .clone()
            .try_lock_owned()
            .map_err(|_| StageError::Busy)?;
        let (release, url) = self
            .last
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|c| c.latest.clone())
            .ok_or(StageError::NotOffered)?;
        // Only an upgrade is offered here; downgrades stay a deliberate manual upload with confirmation.
        if release.version != version || !release.newer {
            return Err(StageError::NotOffered);
        }
        let guard = StagingGuard {
            releases: self.clone(),
            _slot: slot,
        };
        let total_bytes = release.asset_size;
        self.set_progress(StagePhase::Downloading, 0, total_bytes);
        let this = self.clone();
        let body = tokio::task::spawn_blocking(move || {
            this.fetch.get(
                &url,
                "application/octet-stream",
                total_bytes as usize,
                Duration::from_secs(600),
                &|received| this.set_progress(StagePhase::Downloading, received, total_bytes),
            )
        })
        .await
        .unwrap_or(Err(FetchError))
        .map_err(|_| StageError::Download)?;
        if body.len() as u64 != total_bytes {
            return Err(StageError::Download);
        }
        self.set_progress(StagePhase::Verifying, total_bytes, total_bytes);
        Ok((body, guard))
    }

    fn set_progress(&self, phase: StagePhase, received_bytes: u64, total_bytes: u64) {
        *self.progress.lock().unwrap() = Some(StageProgress {
            phase,
            received_bytes,
            total_bytes,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[derive(Default)]
    struct Canned {
        bodies: HashMap<String, Vec<u8>>,
        calls: Mutex<Vec<String>>,
    }
    impl Fetch for Canned {
        fn get(
            &self,
            url: &str,
            _: &str,
            limit: usize,
            _: Duration,
            progress: &dyn Fn(u64),
        ) -> Result<Vec<u8>, FetchError> {
            self.calls.lock().unwrap().push(url.into());
            if let Some(body) = self.bodies.get(url) {
                progress(body.len() as u64 / 2);
            }
            self.bodies
                .get(url)
                .filter(|b| b.len() <= limit)
                .cloned()
                .ok_or(FetchError)
        }
    }
    async fn body(r: &Arc<Releases>, version: &str) -> Result<Vec<u8>, StageError> {
        r.download(version).await.map(|(body, _guard)| body)
    }
    const LATEST: &str = "https://api.github.com/repos/o/r/releases/latest";
    const ASSET: &str = "https://github.com/o/r/releases/download/v0.2.0/nibe-gateway-0.2.0.tar";
    fn release(tag: &str, asset_url: &str, size: usize) -> Vec<u8> {
        serde_json::json!({
            "tag_name": tag,
            "html_url": format!("https://github.com/o/r/releases/tag/{tag}"),
            "published_at": "2026-10-31T10:00:00Z",
            "body": "<script>alert(1)</script>",
            "assets": [{"name": asset_url.rsplit('/').next().unwrap(), "size": size, "browser_download_url": asset_url}]
        })
        .to_string()
        .into_bytes()
    }
    fn releases(bodies: &[(&str, Vec<u8>)]) -> (Arc<Releases>, Arc<Canned>) {
        let fetch = Arc::new(Canned {
            bodies: bodies
                .iter()
                .map(|(u, b)| (u.to_string(), b.clone()))
                .collect(),
            ..Default::default()
        });
        let r = Releases::new(Some("o/r".into()), fetch.clone()).with_version("0.1.25");
        (Arc::new(r), fetch)
    }

    #[test]
    fn repository_names_cannot_alter_the_url() {
        assert!(valid_repository("kroofy/nibe-gateway"));
        for bad in [
            "kroofy", "a/b/c", "../x", "a/..", "a b/c", "a/c?x=1", "a/c#", "/c", "a/", "a/c\n",
        ] {
            assert!(!valid_repository(bad), "{bad:?}");
        }
    }

    #[tokio::test]
    async fn newer_release_is_offered_and_downloaded_once_checked() {
        let (r, fetch) = releases(&[
            (LATEST, release("v0.2.0", ASSET, 4)),
            (ASSET, b"tar!".to_vec()),
        ]);
        assert_eq!(
            body(&r, "0.2.0").await,
            Err(StageError::NotOffered),
            "no check yet"
        );
        let status = r.check().await;
        let latest = status.latest.unwrap();
        assert_eq!(
            (latest.version.as_str(), latest.newer, latest.asset_size),
            ("0.2.0", true, 4)
        );
        assert_eq!(status.error, None);
        assert!(
            !serde_json::to_string(&r.status())
                .unwrap()
                .contains("script"),
            "release notes are never echoed"
        );
        assert_eq!(body(&r, "0.1.99").await, Err(StageError::NotOffered));
        let (tar, guard) = r.download("0.2.0").await.unwrap();
        assert_eq!(tar, b"tar!");
        let staging = r.status().staging.unwrap();
        assert_eq!(
            (staging.phase, staging.received_bytes, staging.total_bytes),
            (StagePhase::Verifying, 4, 4)
        );
        assert!(r.status().downloading);
        assert_eq!(
            body(&r, "0.2.0").await,
            Err(StageError::Busy),
            "one staging at a time"
        );
        drop(guard);
        assert!(r.status().staging.is_none() && !r.status().downloading);
        r.check().await;
        assert_eq!(
            fetch
                .calls
                .lock()
                .unwrap()
                .iter()
                .filter(|u| *u == LATEST)
                .count(),
            1,
            "rapid re-checks are cached"
        );
    }

    #[tokio::test]
    async fn same_or_older_release_is_shown_but_not_staged() {
        let asset = "https://github.com/o/r/releases/download/v0.1.25/nibe-gateway-0.1.25.tar";
        let (r, _) = releases(&[
            (LATEST, release("v0.1.25", asset, 4)),
            (asset, b"tar!".to_vec()),
        ]);
        assert!(!r.check().await.latest.unwrap().newer);
        assert_eq!(body(&r, "0.1.25").await, Err(StageError::NotOffered));
    }

    #[tokio::test]
    async fn foreign_urls_bad_tags_and_missing_assets_are_rejected() {
        for (body, code) in [
            (
                release(
                    "v0.2.0",
                    "https://evil.example/releases/download/v0.2.0/nibe-gateway-0.2.0.tar",
                    4,
                ),
                "invalid_release",
            ),
            (
                release(
                    "v0.2.0",
                    "https://github.com/other/r/releases/download/v0.2.0/nibe-gateway-0.2.0.tar",
                    4,
                ),
                "invalid_release",
            ),
            (release("latest", ASSET, 4), "invalid_release"),
            (
                release("v0.2.0", ASSET, nibe_updater::artifact::MAX_ARTIFACT + 1),
                "invalid_release",
            ),
            (
                release(
                    "v0.2.0",
                    "https://github.com/o/r/releases/download/v0.2.0/nibe-os.tar",
                    4,
                ),
                "no_asset",
            ),
            (b"not json".to_vec(), "invalid_release"),
        ] {
            let (r, _) = releases(&[(LATEST, body)]);
            let status = r.check().await;
            assert_eq!(status.error.as_deref(), Some(code));
            assert!(status.latest.is_none());
        }
        let (r, _) = releases(&[]);
        assert_eq!(r.check().await.error.as_deref(), Some("unreachable"));
    }

    #[tokio::test]
    async fn truncated_download_and_disabled_source_fail_closed() {
        let (r, _) = releases(&[
            (LATEST, release("v0.2.0", ASSET, 5)),
            (ASSET, b"tar!".to_vec()),
        ]);
        r.check().await;
        assert_eq!(body(&r, "0.2.0").await, Err(StageError::Download));
        let off = Arc::new(Releases::new(None, Arc::new(Canned::default())));
        assert!(!off.check().await.enabled);
        assert_eq!(body(&off, "0.2.0").await, Err(StageError::Disabled));
    }
}
