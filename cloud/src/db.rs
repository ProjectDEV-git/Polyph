//! SQLite storage: accounts, sessions, linked devices, teams, invites and
//! subscriptions. Secrets (session tokens, sign-in codes, invite tokens) are
//! stored only as SHA-256 hashes.

use polyph_proto::ids::DeviceId;
use sha2::{Digest, Sha256};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};

use crate::plans::PlanId;

#[derive(Debug, Clone)]
pub struct Db {
    pool: SqlitePool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Account {
    pub id: String,
    pub email: Option<String>,
    pub github_login: Option<String>,
    pub lang: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Device {
    pub device: String,
    pub label: String,
    pub linked_at: i64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct TeamMember {
    pub account: String,
    pub email: Option<String>,
    pub github_login: Option<String>,
    pub role: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Team {
    pub id: String,
    pub name: String,
    pub role: String,
}

/// Who a subscription pays for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Owner {
    Account(String),
    Team(String),
}

impl Owner {
    pub fn kind(&self) -> &'static str {
        match self {
            Owner::Account(_) => "account",
            Owner::Team(_) => "team",
        }
    }
    pub fn id(&self) -> &str {
        match self {
            Owner::Account(i) | Owner::Team(i) => i,
        }
    }
    /// `account:<id>` or `team:<id>`, as carried in Stripe metadata.
    pub fn tag(&self) -> String {
        format!("{}:{}", self.kind(), self.id())
    }
    pub fn parse_tag(s: &str) -> Option<Self> {
        let (k, id) = s.split_once(':')?;
        match k {
            "account" => Some(Owner::Account(id.to_string())),
            "team" => Some(Owner::Team(id.to_string())),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Subscription {
    pub plan: String,
    pub status: String,
    pub owner: String,
}

pub fn hash(secret: &str) -> Vec<u8> {
    Sha256::digest(secret.as_bytes()).to_vec()
}

pub fn new_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

/// Subscription states that grant the plan.
pub fn active(status: &str) -> bool {
    matches!(status, "active" | "trialing" | "past_due")
}

const SCHEMA: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS accounts (
        id TEXT PRIMARY KEY,
        email TEXT UNIQUE,
        github_id INTEGER UNIQUE,
        github_login TEXT,
        lang TEXT NOT NULL,
        created_at INTEGER NOT NULL)",
    "CREATE TABLE IF NOT EXISTS sessions (
        token_hash BLOB PRIMARY KEY,
        account TEXT NOT NULL,
        expires_at INTEGER NOT NULL)",
    "CREATE TABLE IF NOT EXISTS email_codes (
        id TEXT PRIMARY KEY,
        email TEXT NOT NULL,
        code_hash BLOB NOT NULL,
        lang TEXT NOT NULL,
        created_at INTEGER NOT NULL,
        expires_at INTEGER NOT NULL,
        attempts INTEGER NOT NULL DEFAULT 0)",
    "CREATE TABLE IF NOT EXISTS github_pending (
        id TEXT PRIMARY KEY,
        device_code TEXT NOT NULL,
        lang TEXT NOT NULL,
        expires_at INTEGER NOT NULL)",
    "CREATE TABLE IF NOT EXISTS link_challenges (
        nonce TEXT PRIMARY KEY,
        account TEXT NOT NULL,
        expires_at INTEGER NOT NULL)",
    "CREATE TABLE IF NOT EXISTS devices (
        device BLOB PRIMARY KEY,
        account TEXT NOT NULL,
        label TEXT NOT NULL,
        linked_at INTEGER NOT NULL)",
    "CREATE TABLE IF NOT EXISTS teams (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        created_at INTEGER NOT NULL)",
    "CREATE TABLE IF NOT EXISTS team_members (
        team TEXT NOT NULL,
        account TEXT NOT NULL,
        role TEXT NOT NULL,
        PRIMARY KEY (team, account))",
    "CREATE TABLE IF NOT EXISTS invites (
        token_hash BLOB PRIMARY KEY,
        team TEXT NOT NULL,
        email TEXT NOT NULL,
        expires_at INTEGER NOT NULL)",
    "CREATE TABLE IF NOT EXISTS customers (
        owner TEXT PRIMARY KEY,
        customer TEXT NOT NULL)",
    "CREATE TABLE IF NOT EXISTS subscriptions (
        id TEXT PRIMARY KEY,
        owner TEXT NOT NULL,
        plan TEXT NOT NULL,
        status TEXT NOT NULL,
        updated_at INTEGER NOT NULL)",
    "CREATE TABLE IF NOT EXISTS stripe_events (
        id TEXT PRIMARY KEY,
        received_at INTEGER NOT NULL)",
];

impl Db {
    pub async fn open(path: &str) -> anyhow::Result<Self> {
        let opts = if path == ":memory:" {
            SqliteConnectOptions::new().in_memory(true)
        } else {
            SqliteConnectOptions::new().filename(path).create_if_missing(true)
        }
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal);
        let max = if path == ":memory:" { 1 } else { 4 };
        let pool = SqlitePoolOptions::new().max_connections(max).connect_with(opts).await?;
        for s in SCHEMA {
            sqlx::query(*s).execute(&pool).await?;
        }
        Ok(Self { pool })
    }

    // --- accounts and sessions ---

    fn account_row(r: &sqlx::sqlite::SqliteRow) -> anyhow::Result<Account> {
        Ok(Account {
            id: r.try_get("id")?,
            email: r.try_get("email")?,
            github_login: r.try_get("github_login")?,
            lang: r.try_get("lang")?,
        })
    }

    pub async fn account(&self, id: &str) -> anyhow::Result<Option<Account>> {
        let r = sqlx::query("SELECT id, email, github_login, lang FROM accounts WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        r.as_ref().map(Self::account_row).transpose()
    }

    /// The account for this email, created on first sign-in.
    pub async fn account_by_email(&self, email: &str, lang: &str, now: i64) -> anyhow::Result<Account> {
        sqlx::query("INSERT OR IGNORE INTO accounts (id, email, lang, created_at) VALUES (?, ?, ?, ?)")
            .bind(new_id())
            .bind(email)
            .bind(lang)
            .bind(now)
            .execute(&self.pool)
            .await?;
        let r = sqlx::query("SELECT id, email, github_login, lang FROM accounts WHERE email = ?")
            .bind(email)
            .fetch_one(&self.pool)
            .await?;
        Self::account_row(&r)
    }

    /// The account for this GitHub user, created on first sign-in; the login
    /// name is refreshed every time.
    pub async fn account_by_github(&self, gh_id: i64, login: &str, lang: &str, now: i64) -> anyhow::Result<Account> {
        sqlx::query(
            "INSERT OR IGNORE INTO accounts (id, github_id, github_login, lang, created_at) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(new_id())
        .bind(gh_id)
        .bind(login)
        .bind(lang)
        .bind(now)
        .execute(&self.pool)
        .await?;
        sqlx::query("UPDATE accounts SET github_login = ? WHERE github_id = ?")
            .bind(login)
            .bind(gh_id)
            .execute(&self.pool)
            .await?;
        let r = sqlx::query("SELECT id, email, github_login, lang FROM accounts WHERE github_id = ?")
            .bind(gh_id)
            .fetch_one(&self.pool)
            .await?;
        Self::account_row(&r)
    }

    pub async fn delete_account(&self, id: &str) -> anyhow::Result<()> {
        let mut tx = self.pool.begin().await?;
        for q in [
            "DELETE FROM sessions WHERE account = ?",
            "DELETE FROM devices WHERE account = ?",
            "DELETE FROM link_challenges WHERE account = ?",
            "DELETE FROM team_members WHERE account = ?",
            "DELETE FROM accounts WHERE id = ?",
        ] {
            sqlx::query(q).bind(id).execute(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn create_session(&self, token: &str, account: &str, expires_at: i64) -> anyhow::Result<()> {
        sqlx::query("INSERT INTO sessions (token_hash, account, expires_at) VALUES (?, ?, ?)")
            .bind(hash(token))
            .bind(account)
            .bind(expires_at)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn session_account(&self, token: &str, now: i64) -> anyhow::Result<Option<String>> {
        let r = sqlx::query("SELECT account FROM sessions WHERE token_hash = ? AND expires_at > ?")
            .bind(hash(token))
            .bind(now)
            .fetch_optional(&self.pool)
            .await?;
        Ok(r.map(|r| r.try_get("account")).transpose()?)
    }

    pub async fn delete_session(&self, token: &str) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM sessions WHERE token_hash = ?").bind(hash(token)).execute(&self.pool).await?;
        Ok(())
    }

    // --- email codes ---

    pub async fn recent_codes(&self, email: &str, since: i64) -> anyhow::Result<i64> {
        let r = sqlx::query("SELECT COUNT(*) AS n FROM email_codes WHERE email = ? AND created_at > ?")
            .bind(email)
            .bind(since)
            .fetch_one(&self.pool)
            .await?;
        Ok(r.try_get("n")?)
    }

    pub async fn put_code(
        &self,
        id: &str,
        email: &str,
        code: &str,
        lang: &str,
        now: i64,
        ttl: i64,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO email_codes (id, email, code_hash, lang, created_at, expires_at) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(email)
        .bind(hash(code))
        .bind(lang)
        .bind(now)
        .bind(now + ttl)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Checks a code. Returns the email and language on success; a wrong code
    /// counts an attempt and the code dies after `max_attempts`.
    pub async fn take_code(
        &self,
        id: &str,
        code: &str,
        now: i64,
        max_attempts: i64,
    ) -> anyhow::Result<Result<(String, String), CodeError>> {
        let mut tx = self.pool.begin().await?;
        let Some(r) = sqlx::query("SELECT email, code_hash, lang, expires_at, attempts FROM email_codes WHERE id = ?")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?
        else {
            return Ok(Err(CodeError::Unknown));
        };
        let expires_at: i64 = r.try_get("expires_at")?;
        let attempts: i64 = r.try_get("attempts")?;
        if expires_at <= now || attempts >= max_attempts {
            return Ok(Err(CodeError::Expired));
        }
        let stored: Vec<u8> = r.try_get("code_hash")?;
        if !bool::from(subtle::ConstantTimeEq::ct_eq(stored.as_slice(), hash(code).as_slice())) {
            sqlx::query("UPDATE email_codes SET attempts = attempts + 1 WHERE id = ?")
                .bind(id)
                .execute(&mut *tx)
                .await?;
            tx.commit().await?;
            return Ok(Err(CodeError::Wrong));
        }
        // Used codes are kept (expired) until the sweep, so they still count
        // toward the per-address hourly limit.
        sqlx::query("UPDATE email_codes SET expires_at = 0 WHERE id = ?").bind(id).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(Ok((r.try_get("email")?, r.try_get("lang")?)))
    }

    // --- GitHub device flow ---

    pub async fn put_github(&self, id: &str, device_code: &str, lang: &str, expires_at: i64) -> anyhow::Result<()> {
        sqlx::query("INSERT INTO github_pending (id, device_code, lang, expires_at) VALUES (?, ?, ?, ?)")
            .bind(id)
            .bind(device_code)
            .bind(lang)
            .bind(expires_at)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn github_pending(&self, id: &str, now: i64) -> anyhow::Result<Option<(String, String)>> {
        let r = sqlx::query("SELECT device_code, lang FROM github_pending WHERE id = ? AND expires_at > ?")
            .bind(id)
            .bind(now)
            .fetch_optional(&self.pool)
            .await?;
        r.map(|r| Ok((r.try_get("device_code")?, r.try_get("lang")?))).transpose()
    }

    pub async fn drop_github(&self, id: &str) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM github_pending WHERE id = ?").bind(id).execute(&self.pool).await?;
        Ok(())
    }

    // --- devices ---

    pub async fn put_challenge(&self, nonce: &str, account: &str, expires_at: i64) -> anyhow::Result<()> {
        sqlx::query("INSERT INTO link_challenges (nonce, account, expires_at) VALUES (?, ?, ?)")
            .bind(nonce)
            .bind(account)
            .bind(expires_at)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Consumes a challenge issued to `account`; true when it was valid.
    pub async fn take_challenge(&self, nonce: &str, account: &str, now: i64) -> anyhow::Result<bool> {
        let r = sqlx::query("DELETE FROM link_challenges WHERE nonce = ? AND account = ? AND expires_at > ?")
            .bind(nonce)
            .bind(account)
            .bind(now)
            .execute(&self.pool)
            .await?;
        Ok(r.rows_affected() == 1)
    }

    /// Links a device to an account, moving it if another account had it.
    pub async fn link_device(&self, device: &DeviceId, account: &str, label: &str, now: i64) -> anyhow::Result<()> {
        sqlx::query("INSERT OR REPLACE INTO devices (device, account, label, linked_at) VALUES (?, ?, ?, ?)")
            .bind(device.0.to_vec())
            .bind(account)
            .bind(label)
            .bind(now)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn unlink_device(&self, device: &DeviceId, account: &str) -> anyhow::Result<bool> {
        let r = sqlx::query("DELETE FROM devices WHERE device = ? AND account = ?")
            .bind(device.0.to_vec())
            .bind(account)
            .execute(&self.pool)
            .await?;
        Ok(r.rows_affected() == 1)
    }

    pub async fn devices(&self, account: &str) -> anyhow::Result<Vec<Device>> {
        let rows = sqlx::query("SELECT device, label, linked_at FROM devices WHERE account = ? ORDER BY linked_at")
            .bind(account)
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter()
            .map(|r| {
                let d: Vec<u8> = r.try_get("device")?;
                Ok(Device {
                    device: DeviceId::from_slice(&d)?.to_string(),
                    label: r.try_get("label")?,
                    linked_at: r.try_get("linked_at")?,
                })
            })
            .collect()
    }

    /// Every linked device with its account (for the relay policy).
    pub async fn all_devices(&self) -> anyhow::Result<Vec<(DeviceId, String)>> {
        let rows = sqlx::query("SELECT device, account FROM devices").fetch_all(&self.pool).await?;
        rows.into_iter()
            .map(|r| {
                let d: Vec<u8> = r.try_get("device")?;
                Ok((DeviceId::from_slice(&d)?, r.try_get("account")?))
            })
            .collect()
    }

    // --- teams ---

    pub async fn create_team(&self, name: &str, owner: &str, now: i64) -> anyhow::Result<String> {
        let id = new_id();
        let mut tx = self.pool.begin().await?;
        sqlx::query("INSERT INTO teams (id, name, created_at) VALUES (?, ?, ?)")
            .bind(&id)
            .bind(name)
            .bind(now)
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO team_members (team, account, role) VALUES (?, ?, 'owner')")
            .bind(&id)
            .bind(owner)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(id)
    }

    pub async fn team_role(&self, team: &str, account: &str) -> anyhow::Result<Option<String>> {
        let r = sqlx::query("SELECT role FROM team_members WHERE team = ? AND account = ?")
            .bind(team)
            .bind(account)
            .fetch_optional(&self.pool)
            .await?;
        Ok(r.map(|r| r.try_get("role")).transpose()?)
    }

    pub async fn teams_of(&self, account: &str) -> anyhow::Result<Vec<Team>> {
        let rows = sqlx::query(
            "SELECT t.id AS id, t.name AS name, m.role AS role FROM teams t
             JOIN team_members m ON m.team = t.id WHERE m.account = ? ORDER BY t.created_at",
        )
        .bind(account)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|r| Ok(Team { id: r.try_get("id")?, name: r.try_get("name")?, role: r.try_get("role")? }))
            .collect()
    }

    pub async fn team_name(&self, team: &str) -> anyhow::Result<Option<String>> {
        let r = sqlx::query("SELECT name FROM teams WHERE id = ?").bind(team).fetch_optional(&self.pool).await?;
        Ok(r.map(|r| r.try_get("name")).transpose()?)
    }

    pub async fn team_members(&self, team: &str) -> anyhow::Result<Vec<TeamMember>> {
        let rows = sqlx::query(
            "SELECT a.id AS id, a.email AS email, a.github_login AS github_login, m.role AS role
             FROM team_members m JOIN accounts a ON a.id = m.account WHERE m.team = ? ORDER BY m.role DESC, a.id",
        )
        .bind(team)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|r| {
                Ok(TeamMember {
                    account: r.try_get("id")?,
                    email: r.try_get("email")?,
                    github_login: r.try_get("github_login")?,
                    role: r.try_get("role")?,
                })
            })
            .collect()
    }

    pub async fn remove_member(&self, team: &str, account: &str) -> anyhow::Result<bool> {
        let r = sqlx::query("DELETE FROM team_members WHERE team = ? AND account = ? AND role != 'owner'")
            .bind(team)
            .bind(account)
            .execute(&self.pool)
            .await?;
        Ok(r.rows_affected() == 1)
    }

    pub async fn delete_team(&self, team: &str) -> anyhow::Result<()> {
        let mut tx = self.pool.begin().await?;
        for q in [
            "DELETE FROM invites WHERE team = ?",
            "DELETE FROM team_members WHERE team = ?",
            "DELETE FROM teams WHERE id = ?",
        ] {
            sqlx::query(q).bind(team).execute(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn put_invite(&self, token: &str, team: &str, email: &str, expires_at: i64) -> anyhow::Result<()> {
        sqlx::query("INSERT INTO invites (token_hash, team, email, expires_at) VALUES (?, ?, ?, ?)")
            .bind(hash(token))
            .bind(team)
            .bind(email)
            .bind(expires_at)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Accepts an invite: the account joins the team. The invite works once.
    pub async fn accept_invite(&self, token: &str, account: &str, now: i64) -> anyhow::Result<Option<String>> {
        let mut tx = self.pool.begin().await?;
        let Some(r) = sqlx::query("SELECT team FROM invites WHERE token_hash = ? AND expires_at > ?")
            .bind(hash(token))
            .bind(now)
            .fetch_optional(&mut *tx)
            .await?
        else {
            return Ok(None);
        };
        let team: String = r.try_get("team")?;
        sqlx::query("DELETE FROM invites WHERE token_hash = ?").bind(hash(token)).execute(&mut *tx).await?;
        sqlx::query("INSERT OR IGNORE INTO team_members (team, account, role) VALUES (?, ?, 'member')")
            .bind(&team)
            .bind(account)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(Some(team))
    }

    // --- billing ---

    pub async fn customer(&self, owner: &Owner) -> anyhow::Result<Option<String>> {
        let r = sqlx::query("SELECT customer FROM customers WHERE owner = ?")
            .bind(owner.tag())
            .fetch_optional(&self.pool)
            .await?;
        Ok(r.map(|r| r.try_get("customer")).transpose()?)
    }

    pub async fn set_customer(&self, owner: &Owner, customer: &str) -> anyhow::Result<()> {
        sqlx::query("INSERT OR REPLACE INTO customers (owner, customer) VALUES (?, ?)")
            .bind(owner.tag())
            .bind(customer)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn put_subscription(
        &self,
        id: &str,
        owner: &Owner,
        plan: PlanId,
        status: &str,
        now: i64,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT OR REPLACE INTO subscriptions (id, owner, plan, status, updated_at) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(owner.tag())
        .bind(plan.as_str())
        .bind(status)
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn subscriptions(&self, owner: &Owner) -> anyhow::Result<Vec<Subscription>> {
        let rows =
            sqlx::query("SELECT plan, status, owner FROM subscriptions WHERE owner = ? ORDER BY updated_at DESC")
                .bind(owner.tag())
                .fetch_all(&self.pool)
                .await?;
        rows.into_iter()
            .map(|r| {
                Ok(Subscription { plan: r.try_get("plan")?, status: r.try_get("status")?, owner: r.try_get("owner")? })
            })
            .collect()
    }

    /// Records a webhook event id; false when it was already seen.
    pub async fn first_event(&self, id: &str, now: i64) -> anyhow::Result<bool> {
        let r = sqlx::query("INSERT OR IGNORE INTO stripe_events (id, received_at) VALUES (?, ?)")
            .bind(id)
            .bind(now)
            .execute(&self.pool)
            .await?;
        Ok(r.rows_affected() == 1)
    }

    /// The best plan an account has: its own active subscriptions and those
    /// of every team it belongs to.
    pub async fn effective_plan(&self, account: &str) -> anyhow::Result<PlanId> {
        let rows = sqlx::query(
            "SELECT plan, status FROM subscriptions WHERE owner = ?1
             OR owner IN (SELECT 'team:' || team FROM team_members WHERE account = ?2)",
        )
        .bind(Owner::Account(account.to_string()).tag())
        .bind(account)
        .fetch_all(&self.pool)
        .await?;
        let mut best = PlanId::Free;
        for r in rows {
            let plan: String = r.try_get("plan")?;
            let status: String = r.try_get("status")?;
            if let Some(p) = PlanId::parse(&plan).filter(|_| active(&status)) {
                best = best.max(p);
            }
        }
        Ok(best)
    }

    /// Deletes expired codes, challenges, sessions and invites.
    pub async fn sweep(&self, now: i64) -> anyhow::Result<()> {
        for q in [
            "DELETE FROM email_codes WHERE created_at <= ?1 - 3600 AND expires_at <= ?1",
            "DELETE FROM github_pending WHERE expires_at <= ?",
            "DELETE FROM link_challenges WHERE expires_at <= ?",
            "DELETE FROM sessions WHERE expires_at <= ?",
            "DELETE FROM invites WHERE expires_at <= ?",
        ] {
            sqlx::query(q).bind(now).execute(&self.pool).await?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodeError {
    Unknown,
    Expired,
    Wrong,
}
