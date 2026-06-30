//! CLI login_id polling store. Maps login_id → session result.
//!
//! # Flow
//! 1. `start` generates an OIDC state and calls [`LoginStore::issue`] to get
//!    a `login_id`. The CLI receives both the `auth_url` (to open in a
//!    browser) and the `login_id` (to poll with).
//! 2. The user authenticates at the OIDC provider. The provider redirects
//!    back to the callback with `?code=...&state=...`.
//! 3. `callback` looks up the login by state via
//!    [`LoginStore::find_by_state`], then calls [`LoginStore::complete`] (or
//!    [`LoginStore::deny`] on failure) to record the result.
//! 4. The CLI polls [`LoginStore::poll`] until the status is no longer
//!    `pending`. Complete results are one-shot (consumed on first read).
//!
//! # TTL
//! Pending logins expire after [`LOGIN_TTL_SECS`] (10 minutes). Expired
//! logins are returned as `expired` on the next poll and then cleaned up.
//!
//! # Concurrency
//! Two separate `Mutex`es protect the forward and reverse indexes. Lock
//! ordering is always `by_id` → `by_state` to prevent deadlocks.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// Pending login TTL: 10 minutes. CLI users authenticate in a browser;
/// after this window the login_id is considered abandoned.
pub(super) const LOGIN_TTL_SECS: i64 = 600;

/// Status of a CLI login attempt, returned by [`LoginStore::poll`].
#[derive(Clone, Debug)]
pub(super) enum LoginStatus {
    /// Still waiting for the user to authenticate in the browser.
    Pending,
    /// Authentication succeeded. Contains the MCM session token, the
    /// authenticated owner, and the session expiry (unix seconds).
    Complete {
        token: String,
        owner: String,
        expires_at_unix: i64,
    },
    /// Login request expired (user didn't authenticate within TTL).
    Expired,
    /// Authentication failed (invalid code, wrong issuer, bad token, etc.).
    Denied { reason: String },
}

/// A pending login request tracked by the store.
struct PendingLogin {
    /// The OIDC `state` parameter used in the authorize URL.
    state: String,
    /// Current status (set by callback, read by poll).
    status: LoginStatus,
    /// Creation timestamp (unix seconds) for TTL checking.
    created_at_unix: i64,
    /// Whether the callback has already processed this login. Prevents
    /// replayed states from succeeding.
    consumed: bool,
}

/// In-memory store for CLI login polling. Shared between mock and real
/// OIDC modes.
#[derive(Default)]
pub(in crate::server) struct LoginStore {
    /// login_id → PendingLogin.
    by_id: Mutex<HashMap<String, PendingLogin>>,
    /// state → login_id (reverse index for callback lookup).
    by_state: Mutex<HashMap<String, String>>,
}

impl LoginStore {
    pub(super) fn new() -> Self {
        Self::default()
    }

    /// Issue a new `login_id` for the given OIDC `state` parameter.
    /// Returns the `login_id` (an opaque random string).
    pub(super) fn issue(&self, state: &str) -> String {
        let login_id = nonce();
        let now = now_unix();
        self.by_id.lock().expect("login mutex").insert(
            login_id.clone(),
            PendingLogin {
                state: state.to_string(),
                status: LoginStatus::Pending,
                created_at_unix: now,
                consumed: false,
            },
        );
        self.by_state
            .lock()
            .expect("login mutex")
            .insert(state.to_string(), login_id.clone());
        login_id
    }

    /// Check if a state is still pending (exists and not consumed).
    pub(super) fn is_pending(&self, state: &str) -> bool {
        let login_id = match self.by_state.lock().expect("login mutex").get(state) {
            Some(id) => id.clone(),
            None => return false,
        };
        let map = self.by_id.lock().expect("login mutex");
        map.get(&login_id)
            .is_some_and(|l| !l.consumed && matches!(l.status, LoginStatus::Pending))
    }

    /// Look up a login_id by OIDC state. Used by the callback handler to
    /// find which login a callback belongs to.
    #[allow(
        dead_code,
        reason = "reserved for future use (e.g. callback state lookup)"
    )]
    pub(super) fn find_by_state(&self, state: &str) -> Option<String> {
        self.by_state
            .lock()
            .expect("login mutex")
            .get(state)
            .cloned()
    }

    /// Mark a login as **complete** (authentication succeeded). Returns
    /// `true` if the state was found and not already consumed, `false`
    /// otherwise (state unknown or replayed).
    pub(super) fn complete(
        &self,
        state: &str,
        token: &str,
        owner: &str,
        expires_at_unix: i64,
    ) -> bool {
        let login_id = match self.by_state.lock().expect("login mutex").get(state) {
            Some(id) => id.clone(),
            None => return false,
        };
        let mut map = self.by_id.lock().expect("login mutex");
        if let Some(login) = map.get_mut(&login_id) {
            if login.consumed {
                return false;
            }
            login.consumed = true;
            login.status = LoginStatus::Complete {
                token: token.to_string(),
                owner: owner.to_string(),
                expires_at_unix,
            };
            true
        } else {
            false
        }
    }

    /// Mark a login as **denied** (authentication failed). Returns `true`
    /// if the state was found and not already consumed.
    pub(super) fn deny(&self, state: &str, reason: &str) -> bool {
        let login_id = match self.by_state.lock().expect("login mutex").get(state) {
            Some(id) => id.clone(),
            None => return false,
        };
        let mut map = self.by_id.lock().expect("login mutex");
        if let Some(login) = map.get_mut(&login_id) {
            if login.consumed {
                return false;
            }
            login.consumed = true;
            login.status = LoginStatus::Denied {
                reason: reason.to_string(),
            };
            true
        } else {
            false
        }
    }

    /// Poll a login by `login_id`. Returns `None` if the id is unknown.
    ///
    /// - **Pending**: returns `Pending` (idempotent, not consumed).
    /// - **Complete / Expired / Denied**: consumes the entry (one-shot) and
    ///   returns the final status. A second poll for the same id returns
    ///   `None`.
    pub(super) fn poll(&self, login_id: &str) -> Option<LoginStatus> {
        let mut by_id = self.by_id.lock().expect("login mutex");
        let login = by_id.get_mut(login_id)?;

        // Promote stale pending logins to expired.
        if matches!(login.status, LoginStatus::Pending) {
            let now = now_unix();
            if now - login.created_at_unix > LOGIN_TTL_SECS {
                login.status = LoginStatus::Expired;
            }
        }

        // Pending is read-only (not consumed).
        if matches!(login.status, LoginStatus::Pending) {
            return Some(LoginStatus::Pending);
        }

        // Non-pending results are consumed (one-shot).
        let login = by_id.remove(login_id).unwrap();
        let state = login.state.clone();
        drop(by_id);
        self.by_state.lock().expect("login mutex").remove(&state);
        Some(login.status)
    }
}

/// Short unique-ish nonce (no uuid crate). Combines wall-clock nanos with a
/// static counter for same-nanosecond calls.
fn nonce() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{nanos:x}-{n:x}")
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
