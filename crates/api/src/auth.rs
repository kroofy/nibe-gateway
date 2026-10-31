//! Local admin authentication. HTTP concerns stay out of the hardware-independent core.
use super::*;
use argon2::{
    password_hash::{PasswordHash, SaltString},
    Argon2, Params, PasswordHasher, PasswordVerifier,
};
use nibe_persistence::{random_token, Store};
use std::{
    collections::{HashMap, VecDeque},
    sync::Mutex,
    time::Instant,
};
use subtle::ConstantTimeEq;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use zeroize::Zeroizing;

pub const SESSION_SECONDS: u64 = 43_200;
const MAX_SESSIONS: usize = 32;
#[derive(Serialize, Deserialize, ToSchema)]
pub struct SessionStatus {
    pub authenticated: bool,
    pub setup_required: bool,
}
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PasswordRequest {
    pub password: String,
}
#[derive(Serialize, ToSchema)]
pub struct ErrorDetail {
    pub code: String,
    pub message: String,
}
#[derive(Serialize, ToSchema)]
pub struct AuthErrorBody {
    pub error: ErrorDetail,
}
#[derive(Debug)]
pub struct AuthError {
    status: StatusCode,
    code: &'static str,
    message: &'static str,
    retry_after: u64,
}
impl AuthError {
    pub fn unauthorized() -> Self {
        Self::new(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "admin session or initial setup proof required",
        )
    }
    pub fn forbidden() -> Self {
        Self::new(
            StatusCode::FORBIDDEN,
            "forbidden",
            "same-origin request header required",
        )
    }
    /// Admin session is valid but no active diagnostic lease authorizes this.
    pub fn lease_required() -> Self {
        Self::new(
            StatusCode::FORBIDDEN,
            "forbidden",
            "active diagnostic lease required",
        )
    }
    pub fn validation() -> Self {
        Self::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "validation",
            "invalid request fields",
        )
    }
    pub fn unavailable() -> Self {
        Self::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "unavailable",
            "operation unavailable; configuration retained",
        )
    }
    pub fn conflict() -> Self {
        Self::new(StatusCode::CONFLICT, "conflict", "admin already configured")
    }
    fn backoff(seconds: u64) -> Self {
        let mut error = Self::busy();
        error.retry_after = seconds.max(1);
        error
    }
    pub fn busy() -> Self {
        Self::new(
            StatusCode::TOO_MANY_REQUESTS,
            "busy",
            "too many attempts; retry after the indicated delay",
        )
    }
    pub fn new(status: StatusCode, code: &'static str, message: &'static str) -> Self {
        Self {
            status,
            code,
            message,
            retry_after: 60,
        }
    }
}
impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let mut response = (
            self.status,
            Json(AuthErrorBody {
                error: ErrorDetail {
                    code: self.code.into(),
                    message: self.message.into(),
                },
            }),
        )
            .into_response();
        if self.status == StatusCode::TOO_MANY_REQUESTS {
            response
                .headers_mut()
                .insert("retry-after", self.retry_after.to_string().parse().unwrap());
        }
        response
    }
}
pub fn json_body<T>(json: Result<Json<T>, JsonRejection>) -> Result<T, AuthError> {
    json.map(|j| j.0).map_err(|_| AuthError::validation())
}
struct ClientBucket {
    attempts: u32,
    last: Instant,
    blocked_until: Instant,
}
struct Memory {
    sessions: HashMap<String, Instant>,
    attempts: VecDeque<Instant>,
    clients: HashMap<std::net::IpAddr, ClientBucket>,
    debug_until: Option<Instant>,
    /// API key id -> last use (ms since epoch). RAM only: persisting it would write per request (ADR 0110).
    key_last_used: HashMap<String, u64>,
}
pub struct Auth {
    store: Arc<Store>,
    memory: Mutex<Memory>,
    work: Arc<Semaphore>,
    secure_cookie: bool,
}
impl Auth {
    pub fn new(store: Arc<Store>, secure_cookie: bool) -> Self {
        Self {
            store,
            memory: Mutex::new(Memory {
                sessions: HashMap::new(),
                attempts: VecDeque::new(),
                clients: HashMap::new(),
                debug_until: None,
                key_last_used: HashMap::new(),
            }),
            work: Arc::new(Semaphore::new(1)),
            secure_cookie,
        }
    }
    pub fn store(&self) -> Arc<Store> {
        self.store.clone()
    }
    pub fn set_debug(&self, seconds: u64) {
        self.memory.lock().unwrap().debug_until = if seconds == 0 {
            None
        } else {
            Some(Instant::now() + Duration::from_secs(seconds))
        };
    }
    pub fn debug_remaining(&self) -> u64 {
        self.memory
            .lock()
            .unwrap()
            .debug_until
            .map(|t| t.saturating_duration_since(Instant::now()).as_secs())
            .unwrap_or(0)
    }
    pub fn debug_enabled(&self) -> bool {
        self.memory
            .lock()
            .unwrap()
            .debug_until
            .is_some_and(|t| t > Instant::now())
    }
    pub fn setup_required(&self) -> bool {
        self.store.config().admin_password_hash.is_none()
    }
    fn session_token(headers: &HeaderMap) -> Option<&str> {
        headers
            .get_all("cookie")
            .iter()
            .filter_map(|h| h.to_str().ok())
            .flat_map(|h| h.split(';'))
            .filter_map(|part| part.trim().split_once('='))
            .find(|(name, _)| *name == "nibe_session")
            .map(|(_, value)| value)
    }
    pub fn authenticated(&self, headers: &HeaderMap) -> bool {
        let Some(token) = Self::session_token(headers) else {
            return false;
        };
        if token.len() != 64 {
            return false;
        }
        let mut memory = self.memory.lock().unwrap();
        memory.sessions.retain(|_, expiry| *expiry > Instant::now());
        memory.sessions.contains_key(token) && !self.setup_required()
    }
    pub fn status(&self, headers: &HeaderMap) -> SessionStatus {
        SessionStatus {
            authenticated: self.authenticated(headers),
            setup_required: self.setup_required(),
        }
    }
    pub fn provisioning(&self, headers: &HeaderMap) -> bool {
        if self.authenticated(headers) {
            return true;
        }
        if !self.setup_required() {
            return false;
        }
        let Some(token) = headers.get("x-setup-token").and_then(|h| h.to_str().ok()) else {
            return false;
        };
        let expected = self.store.setup_secrets();
        bool::from(token.as_bytes().ct_eq(expected.setup_token.as_bytes()))
    }
    pub fn permit(&self) -> Result<OwnedSemaphorePermit, AuthError> {
        self.work
            .clone()
            .try_acquire_owned()
            .map_err(|_| AuthError::busy())
    }
    fn attempt(&self, client: std::net::IpAddr) -> Result<(), AuthError> {
        self.attempt_at(client, Instant::now())
    }
    fn attempt_at(&self, client: std::net::IpAddr, now: Instant) -> Result<(), AuthError> {
        let mut memory = self.memory.lock().unwrap();
        memory
            .clients
            .retain(|_, c| now.saturating_duration_since(c.last) < Duration::from_secs(1800));
        while memory
            .attempts
            .front()
            .is_some_and(|t| now.saturating_duration_since(*t) >= Duration::from_secs(60))
        {
            memory.attempts.pop_front();
        }
        // Rejected attempts never consume global capacity or extend a client's ban.
        if let Some(bucket) = memory
            .clients
            .get(&client)
            .filter(|c| c.blocked_until > now)
        {
            return Err(AuthError::backoff(
                bucket
                    .blocked_until
                    .duration_since(now)
                    .as_secs()
                    .saturating_add(1),
            ));
        }
        if memory.attempts.len() >= 60 {
            return Err(AuthError::busy());
        }
        if memory.clients.len() >= 1024 && !memory.clients.contains_key(&client) {
            return Err(AuthError::busy());
        }
        let bucket = memory.clients.entry(client).or_insert(ClientBucket {
            attempts: 0,
            last: now,
            blocked_until: now,
        });
        if now.saturating_duration_since(bucket.last) >= Duration::from_secs(1800) {
            bucket.attempts = 0;
        }
        bucket.attempts += 1;
        bucket.last = now;
        if bucket.attempts >= 5 {
            // Five initial guesses, then exponentially slower individual retries.
            let seconds = 60u64.saturating_mul(1 << (bucket.attempts - 5).min(4));
            bucket.blocked_until = now + Duration::from_secs(seconds);
        }
        memory.attempts.push_back(now);
        Ok(())
    }
    fn cookie(&self, token: &str, max_age: u64) -> String {
        format!(
            "nibe_session={token}; HttpOnly; SameSite=Strict; Path=/; Max-Age={max_age}{}",
            if self.secure_cookie { "; Secure" } else { "" }
        )
    }
    fn issue(&self, headers: &HeaderMap) -> Response {
        let token = random_token(32);
        let mut memory = self.memory.lock().unwrap();
        if let Some(previous) = Self::session_token(headers) {
            memory.sessions.remove(previous);
        }
        memory.sessions.retain(|_, expiry| *expiry > Instant::now());
        if memory.sessions.len() >= MAX_SESSIONS {
            if let Some(oldest) = memory
                .sessions
                .iter()
                .min_by_key(|(_, expiry)| **expiry)
                .map(|(token, _)| token.clone())
            {
                memory.sessions.remove(&oldest);
            }
        }
        memory.sessions.insert(
            token.clone(),
            Instant::now() + Duration::from_secs(SESSION_SECONDS),
        );
        let mut response = Json(SessionStatus {
            authenticated: true,
            setup_required: false,
        })
        .into_response();
        response.headers_mut().insert(
            "set-cookie",
            self.cookie(&token, SESSION_SECONDS).parse().unwrap(),
        );
        response
    }
    pub fn invalidate_all(&self) {
        let mut memory = self.memory.lock().unwrap();
        memory.sessions.clear();
        memory.debug_until = None;
    }
    pub fn logout(&self, headers: &HeaderMap) -> Response {
        self.set_debug(0);
        if let Some(token) = Self::session_token(headers) {
            self.memory.lock().unwrap().sessions.remove(token);
        }
        let mut response = Json(SessionStatus {
            authenticated: false,
            setup_required: self.setup_required(),
        })
        .into_response();
        response
            .headers_mut()
            .insert("set-cookie", self.cookie("", 0).parse().unwrap());
        response
    }
    pub async fn setup(
        &self,
        headers: &HeaderMap,
        password: String,
    ) -> Result<Response, AuthError> {
        let _permit = self.permit()?;
        if !self.setup_required() {
            return Err(AuthError::conflict());
        }
        if !self.provisioning(headers) {
            return Err(AuthError::unauthorized());
        }
        let password = Zeroizing::new(password);
        if password.chars().count() < 12 || password.len() > 256 {
            return Err(AuthError::validation());
        }
        self.attempt(std::net::Ipv4Addr::LOCALHOST.into())?;
        let store = self.store.clone();
        low_priority(move || {
            let salt = SaltString::encode_b64(random_token(16).as_bytes())
                .map_err(|_| AuthError::unavailable())?;
            let hash = argon()
                .hash_password(password.as_bytes(), &salt)
                .map_err(|_| AuthError::unavailable())?
                .to_string();
            store
                .update(|config| config.admin_password_hash = Some(hash))
                .map_err(|_| AuthError::unavailable())
        })
        .await
        .map_err(|_| AuthError::unavailable())??;
        Ok(self.issue(headers))
    }
    /// Change the admin password after verifying the current one. Ends all sessions and revokes all
    /// API keys, so a leaked key cannot outlive the password it was created under (ADR 0110).
    pub async fn change_password(&self, current: String, new: String) -> Result<(), AuthError> {
        let _permit = self.permit()?;
        let current = Zeroizing::new(current);
        let new = Zeroizing::new(new);
        if new.chars().count() < 12 || new.len() > 256 || current.len() > 256 {
            return Err(AuthError::validation());
        }
        let hash = self
            .store
            .config()
            .admin_password_hash
            .ok_or_else(AuthError::unauthorized)?;
        let store = self.store.clone();
        low_priority(move || {
            let valid = PasswordHash::new(&hash)
                .map(|h| argon().verify_password(current.as_bytes(), &h).is_ok())
                .unwrap_or(false);
            if !valid {
                return Err(AuthError::unauthorized());
            }
            let salt = SaltString::encode_b64(random_token(16).as_bytes())
                .map_err(|_| AuthError::unavailable())?;
            let next = argon()
                .hash_password(new.as_bytes(), &salt)
                .map_err(|_| AuthError::unavailable())?
                .to_string();
            store
                .update(|config| {
                    config.admin_password_hash = Some(next);
                    config.api_keys.clear();
                })
                .map_err(|_| AuthError::unavailable())
        })
        .await
        .map_err(|_| AuthError::unavailable())??;
        self.invalidate_all();
        Ok(())
    }
    /// Set the admin password from the boot-partition provisioning file (ADR 0108). Physical access to
    /// the card is owner access, so this also replaces an existing password and ends all sessions.
    pub async fn provision_password(&self, password: String) -> Result<(), AuthError> {
        let _permit = self.permit()?;
        let password = Zeroizing::new(password);
        if password.chars().count() < 12 || password.len() > 256 {
            return Err(AuthError::validation());
        }
        let store = self.store.clone();
        low_priority(move || {
            let salt = SaltString::encode_b64(random_token(16).as_bytes())
                .map_err(|_| AuthError::unavailable())?;
            let hash = argon()
                .hash_password(password.as_bytes(), &salt)
                .map_err(|_| AuthError::unavailable())?
                .to_string();
            store
                .update(|config| config.admin_password_hash = Some(hash))
                .map_err(|_| AuthError::unavailable())
        })
        .await
        .map_err(|_| AuthError::unavailable())??;
        self.invalidate_all();
        Ok(())
    }
    pub async fn login(
        &self,
        headers: &HeaderMap,
        password: String,
    ) -> Result<Response, AuthError> {
        self.login_from(headers, password, std::net::Ipv4Addr::LOCALHOST.into())
            .await
    }
    pub async fn login_from(
        &self,
        headers: &HeaderMap,
        password: String,
        client: std::net::IpAddr,
    ) -> Result<Response, AuthError> {
        let password = Zeroizing::new(password);
        if password.len() > 256 {
            return Err(AuthError::validation());
        }
        self.attempt(client)?;
        let _permit = self.permit()?;
        let hash = self
            .store
            .config()
            .admin_password_hash
            .ok_or_else(AuthError::unauthorized)?;
        let valid = low_priority(move || {
            PasswordHash::new(&hash)
                .map(|hash| argon().verify_password(password.as_bytes(), &hash).is_ok())
                .unwrap_or(false)
        })
        .await
        .map_err(|_| AuthError::unavailable())?;
        if !valid {
            return Err(AuthError::unauthorized());
        }
        self.memory.lock().unwrap().clients.remove(&client);
        Ok(self.issue(headers))
    }
}
/// Runs password hashing on its own short-lived thread at the lowest CPU priority. On the single-core
/// Pi Zero a ~1 s Argon2 hash otherwise delays the serial thread's ACKs past the pump's deadline.
/// The thread exits afterwards, so no pooled thread is left deprioritised.
async fn low_priority<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> Result<T, tokio::sync::oneshot::error::RecvError> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    // If the thread cannot start, `tx` is dropped and the caller gets an error ("unavailable").
    let _ = std::thread::Builder::new()
        .name("argon2".into())
        .spawn(move || {
            #[cfg(target_os = "linux")]
            // SAFETY: setpriority on the calling thread (who = 0) with a constant; no pointers involved.
            unsafe {
                libc::setpriority(libc::PRIO_PROCESS, 0, 19);
            }
            let _ = tx.send(work());
        });
    rx.await
}
fn argon() -> Argon2<'static> {
    Argon2::new(
        argon2::Algorithm::Argon2id,
        argon2::Version::V0x13,
        Params::new(19_456, 2, 1, None).expect("fixed argon2 parameters"),
    )
}
impl Auth {
    /// Scope of a valid `Authorization: Bearer nibe_…` API key, if any (ADR 0110).
    pub fn api_scope(&self, headers: &HeaderMap) -> Option<nibe_persistence::ApiScope> {
        self.api_key(headers).map(|key| key.scope)
    }
    fn api_key(&self, headers: &HeaderMap) -> Option<nibe_persistence::ApiKey> {
        let token = headers
            .get("authorization")?
            .to_str()
            .ok()?
            .strip_prefix("Bearer ")?;
        let hash = crate::access::hash_token(token)?;
        let keys = self.store.config().api_keys;
        let key = keys
            .iter()
            .find(|k| bool::from(k.hash.as_bytes().ct_eq(hash.as_bytes())))?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as u64);
        self.memory
            .lock()
            .unwrap()
            .key_last_used
            .insert(key.id.clone(), now);
        Some(key.clone())
    }
    pub fn key_last_used(&self, id: &str) -> Option<u64> {
        self.memory.lock().unwrap().key_last_used.get(id).copied()
    }
}
impl Authorizer for Auth {
    fn authorize(&self, headers: &HeaderMap, capability: Capability) -> Result<(), ServiceError> {
        let by_key = match capability {
            Capability::Read => self.api_scope(headers).is_some(),
            // Probes only read catalog registers (ADR 0101); control keys may start them (ADR 0110).
            Capability::Control | Capability::Discover => {
                self.api_scope(headers) == Some(nibe_persistence::ApiScope::Control)
            }
            Capability::Debug => false,
        };
        if by_key {
            return Ok(());
        }
        if self.authenticated(headers)
            && (!matches!(capability, Capability::Debug) || self.debug_enabled())
        {
            Ok(())
        } else {
            Err(ServiceError::new(
                ErrorCode::Forbidden,
                "admin session required",
            ))
        }
    }
    fn principal(&self, headers: &HeaderMap) -> String {
        self.api_key(headers)
            .map_or_else(|| "admin".into(), |key| key.name)
    }
}

/// X-Requested-With forces a preflight for cross-origin fetch; the server exposes
/// no CORS permission. Also reject explicit cross-site browser metadata/Origin.
pub fn check_csrf(headers: &HeaderMap) -> Result<(), AuthError> {
    if headers
        .get("x-requested-with")
        .and_then(|h| h.to_str().ok())
        != Some("nibe-gateway")
    {
        return Err(AuthError::forbidden());
    }
    if headers
        .get("sec-fetch-site")
        .and_then(|h| h.to_str().ok())
        .is_some_and(|site| site != "same-origin" && site != "none")
    {
        return Err(AuthError::forbidden());
    }
    if let Some(origin) = headers.get("origin") {
        let origin = origin.to_str().map_err(|_| AuthError::forbidden())?;
        let host = headers
            .get("host")
            .and_then(|h| h.to_str().ok())
            .ok_or_else(AuthError::forbidden)?;
        if origin != format!("http://{host}") && origin != format!("https://{host}") {
            return Err(AuthError::forbidden());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn lan_attacker_cannot_exhaust_admin_bucket_or_spoof_source_headers() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::open(dir.path(), "1234").unwrap());
        let auth = Auth::new(store.clone(), false);
        let headers = HeaderMap::from_iter([(
            "x-setup-token".parse().unwrap(),
            store.setup_secrets().setup_token.parse().unwrap(),
        )]);
        auth.setup(&headers, "administrator-password".into())
            .await
            .unwrap();
        let attacker = "192.168.1.20".parse().unwrap();
        let admin = "192.168.1.30".parse().unwrap();
        for _ in 0..5 {
            assert_eq!(
                auth.login_from(&HeaderMap::new(), "wrong".into(), attacker)
                    .await
                    .err()
                    .unwrap()
                    .status,
                StatusCode::UNAUTHORIZED
            );
        }
        for _ in 0..1000 {
            assert!(auth.attempt(attacker).is_err());
        }
        let response = auth
            .login_from(&HeaderMap::new(), "administrator-password".into(), admin)
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let spoofed = HeaderMap::from_iter([(
            "x-forwarded-for".parse().unwrap(),
            "192.168.1.30".parse().unwrap(),
        )]);
        assert_eq!(
            auth.login_from(&spoofed, "administrator-password".into(), attacker)
                .await
                .err()
                .unwrap()
                .status,
            StatusCode::TOO_MANY_REQUESTS
        );
        let now = Instant::now() + Duration::from_secs(61);
        assert!(auth.attempt_at(attacker, now).is_ok());
        assert!(auth
            .attempt_at(attacker, now + Duration::from_secs(61))
            .is_err());
        assert!(auth.attempt_at(admin, now).is_ok());
    }
    #[test]
    fn global_guess_cap_bounds_many_sources_and_recovers() {
        let dir = tempfile::tempdir().unwrap();
        let auth = Auth::new(Arc::new(Store::open(dir.path(), "id").unwrap()), false);
        let now = Instant::now();
        for n in 1..=60 {
            assert!(auth
                .attempt_at(std::net::Ipv4Addr::new(192, 168, 1, n).into(), now)
                .is_ok());
        }
        assert!(auth
            .attempt_at("192.168.1.99".parse().unwrap(), now)
            .is_err());
        assert!(auth
            .attempt_at(
                "192.168.1.99".parse().unwrap(),
                now + Duration::from_secs(61)
            )
            .is_ok());
    }
    #[tokio::test]
    async fn setup_login_expiry_logout_limits_and_restart() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::open(dir.path(), "1234").unwrap());
        let auth = Auth::new(store.clone(), true);
        let mut headers = HeaderMap::new();
        assert!(auth
            .setup(&headers, "a-long-admin-password".into())
            .await
            .is_err());
        headers.insert(
            "x-setup-token",
            store.setup_secrets().setup_token.parse().unwrap(),
        );
        let response = auth
            .setup(&headers, "a-long-admin-password".into())
            .await
            .unwrap();
        let cookie = response.headers()["set-cookie"].to_str().unwrap();
        assert!(cookie.contains("Secure"));
        headers.insert("cookie", cookie.split(';').next().unwrap().parse().unwrap());
        assert!(auth.authenticated(&headers));
        assert!(auth.authorize(&headers, Capability::Debug).is_err());
        auth.set_debug(300);
        assert!(auth.authorize(&headers, Capability::Debug).is_ok());
        assert!(auth
            .authorize(&HeaderMap::new(), Capability::Debug)
            .is_err());
        auth.memory.lock().unwrap().debug_until = Some(Instant::now() - Duration::from_secs(1));
        assert!(auth.authorize(&headers, Capability::Debug).is_err());
        assert!(!auth.provisioning(&HeaderMap::from_iter([(
            "x-setup-token".parse().unwrap(),
            store.setup_secrets().setup_token.parse().unwrap()
        )])));
        assert!(store
            .config()
            .admin_password_hash
            .unwrap()
            .starts_with("$argon2id$"));
        auth.logout(&headers);
        assert!(!auth.authenticated(&headers));
        let response = auth
            .login(&headers, "a-long-admin-password".into())
            .await
            .unwrap();
        headers.insert(
            "cookie",
            response.headers()["set-cookie"]
                .to_str()
                .unwrap()
                .split(';')
                .next()
                .unwrap()
                .parse()
                .unwrap(),
        );
        for expiry in auth.memory.lock().unwrap().sessions.values_mut() {
            *expiry = Instant::now() - Duration::from_secs(1);
        }
        assert!(!auth.authenticated(&headers));
        for _ in 0..5 {
            assert_eq!(
                auth.login(&headers, "wrong".into())
                    .await
                    .err()
                    .unwrap()
                    .status,
                StatusCode::UNAUTHORIZED
            );
        }
        assert_eq!(
            auth.login(&headers, "wrong".into())
                .await
                .err()
                .unwrap()
                .status,
            StatusCode::TOO_MANY_REQUESTS
        );
        assert!(!Auth::new(store, false).authenticated(&headers));
    }
}
