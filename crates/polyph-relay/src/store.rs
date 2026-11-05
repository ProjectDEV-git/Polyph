//! Persistent relay state in SQLite: room registrations and encrypted
//! snapshots. Every blob is opaque ciphertext.

use polyph_proto::ids::{DeviceId, MemberId, RoomId};
use polyph_proto::relay_wire::SnapshotItem;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};

#[derive(Debug, Clone)]
pub struct Store {
    pool: SqlitePool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoomRecord {
    pub owner: DeviceId,
    pub quick: bool,
}

impl Store {
    pub async fn open(path: &str) -> anyhow::Result<Self> {
        let opts = if path == ":memory:" {
            SqliteConnectOptions::new().in_memory(true)
        } else {
            SqliteConnectOptions::new().filename(path).create_if_missing(true)
        }
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal);
        // One connection for in-memory databases: each connection would
        // otherwise get its own empty database.
        let max = if path == ":memory:" { 1 } else { 4 };
        let pool = SqlitePoolOptions::new().max_connections(max).connect_with(opts).await?;
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS rooms (
                room BLOB PRIMARY KEY,
                owner BLOB NOT NULL,
                quick INTEGER NOT NULL,
                created_at INTEGER NOT NULL
            )",
        )
        .execute(&pool)
        .await?;
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS snapshots (
                room BLOB NOT NULL,
                device BLOB NOT NULL,
                epoch INTEGER NOT NULL,
                stored_at INTEGER NOT NULL,
                blob BLOB NOT NULL,
                PRIMARY KEY (room, device)
            )",
        )
        .execute(&pool)
        .await?;
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS members (
                room BLOB NOT NULL,
                member BLOB NOT NULL,
                PRIMARY KEY (room, member)
            )",
        )
        .execute(&pool)
        .await?;
        Ok(Self { pool })
    }

    /// Records `member` as part of `room` unless that would exceed `max`
    /// distinct members. Returns whether the member is admitted.
    pub async fn admit_member(&self, room: &RoomId, member: &MemberId, max: usize) -> anyhow::Result<bool> {
        let mut tx = self.pool.begin().await?;
        let known = sqlx::query("SELECT 1 FROM members WHERE room = ? AND member = ?")
            .bind(room.0.to_vec())
            .bind(member.0.to_vec())
            .fetch_optional(&mut *tx)
            .await?
            .is_some();
        if known {
            return Ok(true);
        }
        let n: i64 = sqlx::query("SELECT COUNT(*) AS n FROM members WHERE room = ?")
            .bind(room.0.to_vec())
            .fetch_one(&mut *tx)
            .await?
            .try_get("n")?;
        if n as usize >= max {
            return Ok(false);
        }
        sqlx::query("INSERT INTO members (room, member) VALUES (?, ?)")
            .bind(room.0.to_vec())
            .bind(member.0.to_vec())
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(true)
    }

    pub async fn register_room(&self, room: &RoomId, rec: &RoomRecord, now: i64) -> anyhow::Result<()> {
        sqlx::query("INSERT OR IGNORE INTO rooms (room, owner, quick, created_at) VALUES (?, ?, ?, ?)")
            .bind(room.0.to_vec())
            .bind(rec.owner.0.to_vec())
            .bind(rec.quick as i64)
            .bind(now)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn room(&self, room: &RoomId) -> anyhow::Result<Option<RoomRecord>> {
        let row = sqlx::query("SELECT owner, quick FROM rooms WHERE room = ?")
            .bind(room.0.to_vec())
            .fetch_optional(&self.pool)
            .await?;
        Ok(match row {
            Some(r) => {
                let owner: Vec<u8> = r.try_get("owner")?;
                let quick: i64 = r.try_get("quick")?;
                Some(RoomRecord { owner: DeviceId::from_slice(&owner)?, quick: quick != 0 })
            }
            None => None,
        })
    }

    /// Stores a device's snapshot and keeps only the newest `keep` per room.
    pub async fn put_snapshot(
        &self,
        room: &RoomId,
        from: &DeviceId,
        epoch: u64,
        blob: &[u8],
        now: i64,
        keep: usize,
    ) -> anyhow::Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("INSERT OR REPLACE INTO snapshots (room, device, epoch, stored_at, blob) VALUES (?, ?, ?, ?, ?)")
            .bind(room.0.to_vec())
            .bind(from.0.to_vec())
            .bind(epoch as i64)
            .bind(now)
            .bind(blob.to_vec())
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "DELETE FROM snapshots WHERE room = ?1 AND device NOT IN (
                SELECT device FROM snapshots WHERE room = ?1 ORDER BY stored_at DESC LIMIT ?2)",
        )
        .bind(room.0.to_vec())
        .bind(keep as i64)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn snapshots(&self, room: &RoomId) -> anyhow::Result<Vec<SnapshotItem>> {
        let rows =
            sqlx::query("SELECT device, epoch, stored_at, blob FROM snapshots WHERE room = ? ORDER BY stored_at DESC")
                .bind(room.0.to_vec())
                .fetch_all(&self.pool)
                .await?;
        rows.into_iter()
            .map(|r| {
                let dev: Vec<u8> = r.try_get("device")?;
                let epoch: i64 = r.try_get("epoch")?;
                Ok(SnapshotItem {
                    from: DeviceId::from_slice(&dev)?,
                    epoch: epoch as u64,
                    stored_at: r.try_get("stored_at")?,
                    blob: r.try_get("blob")?,
                })
            })
            .collect()
    }

    pub async fn delete_room(&self, room: &RoomId) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM snapshots WHERE room = ?").bind(room.0.to_vec()).execute(&self.pool).await?;
        sqlx::query("DELETE FROM rooms WHERE room = ?").bind(room.0.to_vec()).execute(&self.pool).await?;
        sqlx::query("DELETE FROM members WHERE room = ?").bind(room.0.to_vec()).execute(&self.pool).await?;
        Ok(())
    }

    /// Deletes snapshots older than `before` (Unix seconds) and returns how many.
    pub async fn expire_snapshots(&self, before: i64) -> anyhow::Result<u64> {
        let r = sqlx::query("DELETE FROM snapshots WHERE stored_at < ?").bind(before).execute(&self.pool).await?;
        Ok(r.rows_affected())
    }

    /// Rooms that hold snapshots, with their registered owner when known.
    pub async fn snapshot_rooms(&self) -> anyhow::Result<Vec<(RoomId, Option<DeviceId>)>> {
        let rows = sqlx::query(
            "SELECT DISTINCT s.room AS room, r.owner AS owner FROM snapshots s LEFT JOIN rooms r ON r.room = s.room",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|r| {
                let room: Vec<u8> = r.try_get("room")?;
                let owner: Option<Vec<u8>> = r.try_get("owner")?;
                Ok((RoomId::from_slice(&room)?, owner.map(|o| DeviceId::from_slice(&o)).transpose()?))
            })
            .collect()
    }

    /// Deletes one room's snapshots older than `before` and returns how many.
    pub async fn expire_room_snapshots(&self, room: &RoomId, before: i64) -> anyhow::Result<u64> {
        let r = sqlx::query("DELETE FROM snapshots WHERE room = ? AND stored_at < ?")
            .bind(room.0.to_vec())
            .bind(before)
            .execute(&self.pool)
            .await?;
        Ok(r.rows_affected())
    }

    pub async fn snapshot_bytes(&self) -> anyhow::Result<i64> {
        let r = sqlx::query("SELECT COALESCE(SUM(LENGTH(blob)), 0) AS n FROM snapshots").fetch_one(&self.pool).await?;
        Ok(r.try_get("n")?)
    }
}
