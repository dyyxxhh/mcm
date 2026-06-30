//! SQLite metadata layer — schema, row types, low-level queries.
//!
//! One `rusqlite::Connection` per storage instance. The DB schema:
//! ```sql
//! CREATE TABLE packages (
//!   slug TEXT PRIMARY KEY, version TEXT NOT NULL, owner TEXT NOT NULL,
//!   content_path TEXT NOT NULL, created_at TEXT NOT NULL, updated_at TEXT NOT NULL
//! );
//! CREATE TABLE reservations (
//!   slug TEXT PRIMARY KEY, owner TEXT NOT NULL,
//!   reserved_until TEXT NOT NULL, reserved_until_unix INTEGER NOT NULL
//! );
//! CREATE TABLE pushes (
//!   owner TEXT NOT NULL, pushed_at TEXT NOT NULL, pushed_at_unix INTEGER NOT NULL
//! );
//! ```

use std::path::Path;

use anyhow::{Context, Result};
use rusqlite::{params, Connection};

/// A row of the `packages` table.
pub(super) struct PackageRow {
    pub slug: String,
    pub version: String,
    pub owner: String,
    pub content_path: String,
    pub created_at: String,
    pub updated_at: String,
    pub name: String,
    pub description: String,
    pub sha256: String,
    pub size_bytes: i64,
}

/// A row of the `reservations` table.
pub(super) struct ReservationRow {
    pub owner: String,
    pub reserved_until_unix: i64,
}

/// Wrapper holding a single SQLite connection + the schema.
pub(super) struct MetaDb {
    conn: Connection,
}

impl MetaDb {
    /// Open (or create) the DB at `path` and ensure the schema exists.
    pub(super) fn open(path: &Path) -> Result<Self> {
        let conn =
            Connection::open(path).with_context(|| format!("open sqlite db {}", path.display()))?;
        conn.execute_batch(SCHEMA)?;
        for stmt in MIGRATION_V2 {
            let _ = conn.execute_batch(stmt);
        }
        Ok(Self { conn })
    }

    /// List all packages ordered by slug.
    pub(super) fn list_packages(&self) -> Result<Vec<PackageRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT slug, version, owner, content_path, created_at, updated_at, \
             name, description, sha256, size_bytes \
             FROM packages ORDER BY slug",
        )?;
        let rows = stmt.query_map([], row_to_package)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// List packages owned by `owner`, ordered by slug.
    pub(super) fn list_packages_by_owner(&self, owner: &str) -> Result<Vec<PackageRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT slug, version, owner, content_path, created_at, updated_at, \
             name, description, sha256, size_bytes \
             FROM packages WHERE owner = ?1 ORDER BY slug",
        )?;
        let rows = stmt.query_map(params![owner], row_to_package)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// Get a single package by slug. `None` if absent.
    pub(super) fn get_package(&self, slug: &str) -> Result<Option<PackageRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT slug, version, owner, content_path, created_at, updated_at, \
             name, description, sha256, size_bytes \
             FROM packages WHERE slug = ?1",
        )?;
        let mut rows = stmt.query_map(params![slug], row_to_package)?;
        match rows.next() {
            Some(r) => Ok(Some(r?)),
            None => Ok(None),
        }
    }

    /// Insert a new package row.
    pub(super) fn insert_package(&self, row: &PackageRow) -> Result<()> {
        self.conn.execute(
            "INSERT INTO packages (slug, version, owner, content_path, created_at, updated_at, \
             name, description, sha256, size_bytes) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                row.slug,
                row.version,
                row.owner,
                row.content_path,
                row.created_at,
                row.updated_at,
                row.name,
                row.description,
                row.sha256,
                row.size_bytes,
            ],
        )?;
        Ok(())
    }

    /// Update version/content_path/updated_at for an existing package.
    pub(super) fn update_package(&self, row: &PackageRow) -> Result<()> {
        self.conn.execute(
            "UPDATE packages SET version = ?2, content_path = ?3, updated_at = ?4, \
             name = ?5, description = ?6, sha256 = ?7, size_bytes = ?8 \
             WHERE slug = ?1",
            params![
                row.slug,
                row.version,
                row.content_path,
                row.updated_at,
                row.name,
                row.description,
                row.sha256,
                row.size_bytes,
            ],
        )?;
        Ok(())
    }

    /// Get a reservation by slug. `None` if absent.
    pub(super) fn get_reservation(&self, slug: &str) -> Result<Option<ReservationRow>> {
        let mut stmt = self
            .conn
            .prepare("SELECT owner, reserved_until_unix FROM reservations WHERE slug = ?1")?;
        let mut rows = stmt.query_map(params![slug], |r| {
            Ok(ReservationRow {
                owner: r.get::<_, String>(0)?,
                reserved_until_unix: r.get::<_, i64>(1)?,
            })
        })?;
        match rows.next() {
            Some(r) => Ok(Some(r?)),
            None => Ok(None),
        }
    }

    /// Delete a reservation by slug.
    pub(super) fn delete_reservation(&self, slug: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM reservations WHERE slug = ?1", params![slug])?;
        Ok(())
    }

    /// Atomically delete a package and insert a reservation for its slug.
    pub(super) fn delete_package_and_reserve(
        &self,
        slug: &str,
        owner: &str,
        reserved_until_rfc: &str,
        reserved_until_unix: i64,
    ) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM packages WHERE slug = ?1", params![slug])?;
        tx.execute(
            "INSERT OR REPLACE INTO reservations (slug, owner, reserved_until, reserved_until_unix) \
             VALUES (?1, ?2, ?3, ?4)",
            params![slug, owner, reserved_until_rfc, reserved_until_unix],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Count packages owned by `owner`. Used by the publish policy (max 5).
    pub(super) fn count_packages_by_owner(&self, owner: &str) -> Result<i64> {
        let mut stmt = self
            .conn
            .prepare("SELECT COUNT(*) FROM packages WHERE owner = ?1")?;
        let count: i64 = stmt.query_row(params![owner], |r| r.get(0))?;
        Ok(count)
    }

    /// Last push unix timestamp for `owner`, or `None` if never pushed.
    /// Used by the publish policy (1 push/day).
    pub(super) fn last_push_unix(&self, owner: &str) -> Result<Option<i64>> {
        let mut stmt = self
            .conn
            .prepare("SELECT MAX(pushed_at_unix) FROM pushes WHERE owner = ?1")?;
        let v: Option<i64> = stmt.query_row(params![owner], |r| r.get(0))?;
        Ok(v)
    }

    /// Record a push (publish or update) for `owner` at `now_unix`.
    /// Delete does NOT insert here — delete is not a push.
    pub(super) fn insert_push(
        &self,
        owner: &str,
        pushed_at_rfc: &str,
        pushed_at_unix: i64,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO pushes (owner, pushed_at, pushed_at_unix) VALUES (?1, ?2, ?3)",
            params![owner, pushed_at_rfc, pushed_at_unix],
        )?;
        Ok(())
    }
}

/// Map a `rusqlite::Row` to a `PackageRow`. Column order must match the
/// SELECTs above.
fn row_to_package(r: &rusqlite::Row<'_>) -> rusqlite::Result<PackageRow> {
    Ok(PackageRow {
        slug: r.get(0)?,
        version: r.get(1)?,
        owner: r.get(2)?,
        content_path: r.get(3)?,
        created_at: r.get(4)?,
        updated_at: r.get(5)?,
        name: r.get(6)?,
        description: r.get(7)?,
        sha256: r.get(8)?,
        size_bytes: r.get(9)?,
    })
}

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS packages (
    slug TEXT PRIMARY KEY,
    version TEXT NOT NULL,
    owner TEXT NOT NULL,
    content_path TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    name TEXT NOT NULL DEFAULT '',
    description TEXT NOT NULL DEFAULT '',
    sha256 TEXT NOT NULL DEFAULT '',
    size_bytes INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS reservations (
    slug TEXT PRIMARY KEY,
    owner TEXT NOT NULL,
    reserved_until TEXT NOT NULL,
    reserved_until_unix INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS pushes (
    owner TEXT NOT NULL,
    pushed_at TEXT NOT NULL,
    pushed_at_unix INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_pushes_owner ON pushes(owner);
"#;

const MIGRATION_V2: &[&str] = &[
    "ALTER TABLE packages ADD COLUMN name TEXT NOT NULL DEFAULT ''",
    "ALTER TABLE packages ADD COLUMN description TEXT NOT NULL DEFAULT ''",
    "ALTER TABLE packages ADD COLUMN sha256 TEXT NOT NULL DEFAULT ''",
    "ALTER TABLE packages ADD COLUMN size_bytes INTEGER NOT NULL DEFAULT 0",
];
