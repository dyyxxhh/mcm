//! Durable share storage — SQLite metadata + filesystem blobs.
//!
//! Module layout:
//! - `mod.rs` (this file) — `Storage` facade, `Clock` trait, public API,
//!   init.
//! - `meta.rs` — SQLite schema, row types, low-level query helpers.
//! - `blob.rs` — atomic blob writes/reads on disk.
//! - `helpers.rs` — slug normalization, payload validation, `/x` refusal,
//!   time formatting.
//!
//! # Design
//! - Single `rusqlite::Connection` behind `std::sync::Mutex`. Rusqlite is
//!   sync; for a low-volume personal share service the lock is short-lived.
//! - Blobs live on disk as `data_dir/blobs/<slug>.mcm`; DB stores the relative
//!   path. Writes are atomic (tmp + rename) before the DB txn commits.
//! - `Clock` trait makes time injectable so tests can fast-forward the 2-day
//!   slug reservation without `sleep`.
//! - `publish`/`update`/`delete` enforce: case-insensitive slug uniqueness,
//!   2-day post-delete slug reservation for the deleting owner, owner-match
//!   on update. Overwrites do NOT retain backups.

pub(crate) mod blob;
mod helpers;
pub(crate) mod meta;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};

use helpers::{
    format_rfc3339, format_rfc3339_from_unix, is_expired, normalize_slug, refuse_under_x,
    validate_payload,
};
use meta::{MetaDb, PackageRow};

struct WriteParams<'a> {
    slug: &'a str,
    version: &'a str,
    owner: &'a str,
    content: &'a [u8],
    now_rfc: &'a str,
    is_new: bool,
    name: &'a str,
    description: &'a str,
}

/// Reservation window after delete: the slug is reserved for the deleting
/// owner for this many seconds. Tests inject a `Clock` to fast-forward.
pub(crate) const RESERVATION_SECS: i64 = 2 * 24 * 60 * 60;

#[derive(Clone, Debug, serde::Serialize)]
pub struct PackageMeta {
    pub slug: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub owner: String,
    pub updated_at: String,
    pub created_at: String,
    pub sha256: String,
    pub size_bytes: i64,
    pub install_command: String,
}

/// Outcome of a publish attempt. `Created` carries the new slug; `Conflict`
/// carries a human-readable reason and implies HTTP 409.
#[derive(Debug)]
pub enum PublishOutcome {
    Created { slug: String },
    Conflict { reason: String },
}

/// Outcome of an update attempt.
#[derive(Debug)]
pub enum UpdateOutcome {
    Ok { slug: String },
    NotFound,
    Forbidden,
}

/// Outcome of a delete attempt.
#[derive(Debug)]
pub enum DeleteOutcome {
    Ok,
    NotFound,
    Forbidden,
}

/// Clock abstraction so tests can fast-forward time without `sleep`.
pub trait Clock: Send + Sync {
    /// Current time as RFC3339 (UTC).
    fn now_rfc3339(&self) -> String;
    /// Current time as unix epoch seconds.
    fn now_unix(&self) -> i64;
}

/// Default clock reading `SystemTime::now`.
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_rfc3339(&self) -> String {
        format_rfc3339(time::OffsetDateTime::now_utc())
    }
    fn now_unix(&self) -> i64 {
        time::OffsetDateTime::now_utc().unix_timestamp()
    }
}

/// Durable share storage. Cheap to clone (Arc inner).
#[derive(Clone)]
pub struct Storage {
    inner: Arc<StorageInner>,
}

struct StorageInner {
    data_dir: PathBuf,
    db: Mutex<MetaDb>,
    clock: Box<dyn Clock>,
}

impl Storage {
    /// Initialize storage at `data_dir`. Creates the directory, `blobs/`,
    /// and the SQLite DB file if missing. Re-checks that `data_dir` is NOT
    /// under `/x` (defense-in-depth on top of `ServerConfig` validation).
    pub fn open(data_dir: PathBuf) -> Result<Self> {
        Self::open_with_clock(data_dir, Box::new(SystemClock))
    }

    /// Like [`Storage::open`] but with an injectable clock (tests).
    pub fn open_with_clock(data_dir: PathBuf, clock: Box<dyn Clock>) -> Result<Self> {
        refuse_under_x(&data_dir)?;
        std::fs::create_dir_all(&data_dir)
            .with_context(|| format!("create data dir {}", data_dir.display()))?;
        let blobs_dir = data_dir.join("blobs");
        std::fs::create_dir_all(&blobs_dir)
            .with_context(|| format!("create blobs dir {}", blobs_dir.display()))?;
        let db_path = data_dir.join("mcm-share.db");
        let meta_db = MetaDb::open(&db_path)?;
        Ok(Self {
            inner: Arc::new(StorageInner {
                data_dir,
                db: Mutex::new(meta_db),
                clock,
            }),
        })
    }

    pub fn list(&self) -> Result<Vec<PackageMeta>> {
        let db = self.inner.db.lock().expect("meta db mutex poisoned");
        let rows = db.list_packages()?;
        Ok(rows.into_iter().map(PackageMeta::from).collect())
    }

    pub fn list_by_owner(&self, owner: &str) -> Result<Vec<PackageMeta>> {
        let db = self.inner.db.lock().expect("meta db mutex poisoned");
        let rows = db.list_packages_by_owner(owner)?;
        Ok(rows.into_iter().map(PackageMeta::from).collect())
    }

    /// Get a package's stored `.mcm` content bytes by slug. `None` if the
    /// slug does not exist.
    pub fn get_content(&self, slug: &str) -> Result<Option<Vec<u8>>> {
        let normalized = normalize_slug(slug)?;
        let db = self.inner.db.lock().expect("meta db mutex poisoned");
        let Some(row) = db.get_package(&normalized)? else {
            return Ok(None);
        };
        let blob_path = self.inner.data_dir.join(&row.content_path);
        if !blob_path.exists() {
            return Ok(None);
        }
        let bytes = std::fs::read(&blob_path)
            .with_context(|| format!("read blob {}", blob_path.display()))?;
        Ok(Some(bytes))
    }

    /// Publish a new package. Enforces: slug validation, secret scan,
    /// reservation check (different owner cannot claim a reserved slug),
    /// uniqueness (case-insensitive). On conflict returns
    /// [`PublishOutcome::Conflict`].
    pub fn publish(
        &self,
        slug: &str,
        version: &str,
        content: &[u8],
        owner: &str,
    ) -> Result<PublishOutcome> {
        let normalized = normalize_slug(slug)?;
        let meta = validate_payload(version, content)?;

        let db = self.inner.db.lock().expect("meta db mutex poisoned");
        if let Some(res) = db.get_reservation(&normalized)? {
            if res.owner != owner && !is_expired(&res, self.inner.clock.now_unix()) {
                return Ok(PublishOutcome::Conflict {
                    reason: format!(
                        "slug {normalized} is reserved by another owner for {} more seconds",
                        res.reserved_until_unix - self.inner.clock.now_unix()
                    ),
                });
            }
            if is_expired(&res, self.inner.clock.now_unix()) {
                db.delete_reservation(&normalized)?;
            }
        }
        let is_new = match db.get_package(&normalized)? {
            None => true,
            Some(existing) if existing.owner != owner => {
                return Ok(PublishOutcome::Conflict {
                    reason: format!("slug {normalized} already owned by another user"),
                });
            }
            Some(_) => false,
        };
        let now = self.inner.clock.now_rfc3339();
        self.write_blob_and_commit(
            &db,
            WriteParams {
                slug: &normalized,
                version,
                owner,
                content,
                now_rfc: &now,
                is_new,
                name: &meta.name,
                description: &meta.description,
            },
        )?;
        Ok(PublishOutcome::Created { slug: normalized })
    }

    /// Update an existing package. Owner must match. Overwrites the blob
    /// (no backup). Returns [`UpdateOutcome::NotFound`] if the slug does not
    /// exist, [`UpdateOutcome::Forbidden`] if the owner mismatches.
    pub fn update(
        &self,
        slug: &str,
        version: &str,
        content: &[u8],
        owner: &str,
    ) -> Result<UpdateOutcome> {
        let normalized = normalize_slug(slug)?;
        let meta = validate_payload(version, content)?;
        let db = self.inner.db.lock().expect("meta db mutex poisoned");
        let Some(existing) = db.get_package(&normalized)? else {
            return Ok(UpdateOutcome::NotFound);
        };
        if existing.owner != owner {
            return Ok(UpdateOutcome::Forbidden);
        }
        let now = self.inner.clock.now_rfc3339();
        self.write_blob_and_commit(
            &db,
            WriteParams {
                slug: &normalized,
                version,
                owner,
                content,
                now_rfc: &now,
                is_new: false,
                name: &meta.name,
                description: &meta.description,
            },
        )?;
        Ok(UpdateOutcome::Ok { slug: normalized })
    }

    /// Delete a package. Owner must match. Reserves the slug for the owner
    /// for [`RESERVATION_SECS`] seconds. Returns [`DeleteOutcome::NotFound`]
    /// if absent, [`DeleteOutcome::Forbidden`] on owner mismatch.
    pub fn delete(&self, slug: &str, owner: &str) -> Result<DeleteOutcome> {
        let normalized = normalize_slug(slug)?;
        let db = self.inner.db.lock().expect("meta db mutex poisoned");
        let Some(existing) = db.get_package(&normalized)? else {
            return Ok(DeleteOutcome::NotFound);
        };
        if existing.owner != owner {
            return Ok(DeleteOutcome::Forbidden);
        }
        let until_unix = self.inner.clock.now_unix() + RESERVATION_SECS;
        let until_rfc = format_rfc3339_from_unix(until_unix)?;
        db.delete_package_and_reserve(&normalized, owner, &until_rfc, until_unix)?;
        // Best-effort blob removal; DB is the source of truth.
        let blob_path = self.inner.data_dir.join(&existing.content_path);
        let _ = std::fs::remove_file(&blob_path);
        Ok(DeleteOutcome::Ok)
    }

    /// Current time as unix epoch seconds (delegates to the injected `Clock`).
    pub fn now_unix(&self) -> i64 {
        self.inner.clock.now_unix()
    }

    /// Current time as RFC3339 (delegates to the injected `Clock`).
    pub fn now_rfc3339(&self) -> String {
        self.inner.clock.now_rfc3339()
    }

    /// Packages currently owned by `owner` (publish policy: max 5).
    pub fn count_packages_by_owner(&self, owner: &str) -> Result<i64> {
        let db = self.inner.db.lock().expect("meta db mutex poisoned");
        db.count_packages_by_owner(owner)
    }

    /// Owner's most recent push unix timestamp, or `None` if never.
    pub fn last_push_unix(&self, owner: &str) -> Result<Option<i64>> {
        let db = self.inner.db.lock().expect("meta db mutex poisoned");
        db.last_push_unix(owner)
    }

    /// Record a push for `owner` at now. Delete does NOT call this.
    pub fn record_push(&self, owner: &str) -> Result<()> {
        let db = self.inner.db.lock().expect("meta db mutex poisoned");
        db.insert_push(
            owner,
            &self.inner.clock.now_rfc3339(),
            self.inner.clock.now_unix(),
        )
    }

    // ---- internals ----

    /// Write the blob atomically, then commit the DB txn. `is_new` controls
    /// INSERT vs UPDATE. The blob is written first so a DB rollback leaves no
    /// dangling row pointing at a missing file (the file is just orphaned,
    /// which is harmless and can be GC'd later).
    fn write_blob_and_commit(&self, db: &MetaDb, p: WriteParams<'_>) -> Result<()> {
        let rel = format!("blobs/{}.mcm", p.slug);
        let abs = self.inner.data_dir.join(&rel);
        blob::atomic_write(&abs, p.content)?;
        let sha256 = crate::util::sha256_hex(p.content);
        let size_bytes = p.content.len() as i64;
        let row = PackageRow {
            slug: p.slug.to_string(),
            version: p.version.to_string(),
            owner: p.owner.to_string(),
            content_path: rel,
            created_at: p.now_rfc.to_string(),
            updated_at: p.now_rfc.to_string(),
            name: p.name.to_string(),
            description: p.description.to_string(),
            sha256,
            size_bytes,
        };
        if p.is_new {
            db.insert_package(&row)?;
        } else {
            db.update_package(&row)?;
        }
        Ok(())
    }
}

impl From<PackageRow> for PackageMeta {
    fn from(row: PackageRow) -> Self {
        let install_command = format!(
            "curl -fsSL https://mc.dyyapp.com/install/pkg/{} | bash",
            row.slug
        );
        Self {
            slug: row.slug,
            name: row.name,
            version: row.version,
            description: row.description,
            owner: row.owner,
            updated_at: row.updated_at,
            created_at: row.created_at,
            sha256: row.sha256,
            size_bytes: row.size_bytes,
            install_command,
        }
    }
}
