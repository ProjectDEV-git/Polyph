//! Multi-daemon harness: one relay and several daemons in one process,
//! sharing a manual clock so elections and leases run without real waiting.

use std::sync::Arc;
use std::time::Duration;

use polyph_daemon::events::ApiEvent;
use polyph_daemon::{Clock, Daemon, DaemonOptions, Paths};
use polyph_relay::{RelayConfig, RelayHandle, ServeOptions};
use serde_json::{Value, json};
use tokio::sync::broadcast;

pub struct Node {
    pub name: String,
    pub daemon: Arc<Daemon>,
    pub events: broadcast::Receiver<ApiEvent>,
    _dir: tempfile::TempDir,
}

impl std::fmt::Debug for Node {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Node({})", self.name)
    }
}

impl Node {
    /// Records this node's trust in the `polyph.toml` found from `dir`, the way
    /// a person does with `polyph trust`. Without it the daemon drops the
    /// file's `checks`, `[sandbox]`, `[agents.*]` and `[policy] preset`.
    pub fn trust_project(&self, dir: &std::path::Path) -> anyhow::Result<()> {
        use polyph_agents::project::{ProjectFile, TrustStore};
        let file =
            ProjectFile::find(dir)?.ok_or_else(|| anyhow::anyhow!("no polyph.toml at or above {}", dir.display()))?;
        TrustStore::at(Paths::at(self._dir.path()).trusted_projects()).trust(&file)
    }

    pub async fn call(&self, method: &str, params: Value) -> anyhow::Result<Value> {
        self.daemon.call(method, params).await.map_err(|e| anyhow::anyhow!("{} {method}: {e}", self.name))
    }

    /// Waits for an event matching `pred`, discarding others.
    pub async fn wait_event<F: Fn(&ApiEvent) -> bool>(&mut self, what: &str, pred: F) -> anyhow::Result<ApiEvent> {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        loop {
            let left = deadline.saturating_duration_since(tokio::time::Instant::now());
            match tokio::time::timeout(left, self.events.recv()).await {
                Ok(Ok(e)) if pred(&e) => return Ok(e),
                Ok(Ok(e)) => {
                    if std::env::var_os("POLYPH_TEST_DEBUG").is_some() {
                        eprintln!("[{}] skipped event {}", self.name, serde_json::to_string(&e).unwrap_or_default());
                    }
                }
                Ok(Err(broadcast::error::RecvError::Lagged(_))) => continue,
                _ => anyhow::bail!("{}: timed out waiting for {what}", self.name),
            }
        }
    }
}

/// Binaries for running agents in tests (paths from `env!("CARGO_BIN_EXE_...")`).
#[derive(Debug, Clone)]
pub struct AgentBins {
    pub fake_agent: String,
    pub sandbox_helper: String,
    /// The MCP server executable (`polyph-test-mcp`).
    pub mcp: String,
}

#[derive(Debug)]
pub struct Harness {
    pub relay: RelayHandle,
    pub clock: Clock,
    pub nodes: Vec<Node>,
    pub agents: Option<AgentBins>,
    /// More agent definitions for every node (for example the local agent).
    pub extra_agents: std::collections::BTreeMap<String, polyph_agents::registry::AgentConfig>,
    /// D-Bus for the status service: "off" (default here, so tests never
    /// touch the developer's session bus) or a private bus address.
    pub dbus: String,
}

impl Harness {
    pub async fn new() -> anyhow::Result<Self> {
        Self::with_relay(RelayConfig::ephemeral()).await
    }

    pub async fn with_relay(cfg: RelayConfig) -> anyhow::Result<Self> {
        let relay = polyph_relay::serve(cfg, ServeOptions::default()).await?;
        // Start at real time: the relay checks expiries and certificate times
        // against its own clock.
        Ok(Self {
            relay,
            clock: Clock::manual(polyph_proto::now()),
            nodes: vec![],
            agents: None,
            extra_agents: Default::default(),
            dbus: "off".into(),
        })
    }

    pub async fn add(&mut self, name: &str) -> anyhow::Result<usize> {
        let dir = tempfile::tempdir()?;
        let node = self.start_in(name, dir).await?;
        self.nodes.push(node);
        Ok(self.nodes.len() - 1)
    }

    async fn start_in(&self, name: &str, dir: tempfile::TempDir) -> anyhow::Result<Node> {
        let mut opts = DaemonOptions::new(Paths::at(dir.path()));
        opts.clock = self.clock.clone();
        opts.tick_every = Duration::from_millis(40);
        opts.lan_bind = std::net::IpAddr::from([127, 0, 0, 1]);
        opts.mdns = false;
        opts.file_keystore = true;
        opts.dbus = self.dbus.clone();
        if let Some(b) = &self.agents {
            opts.sandbox_helper = vec![b.sandbox_helper.clone()];
            opts.mcp_command = vec![b.mcp.clone()];
            opts.extra_agents.insert(
                "fake".into(),
                polyph_agents::registry::AgentConfig { command: b.fake_agent.clone(), ..Default::default() },
            );
            opts.extra_agents.extend(self.extra_agents.clone());
            // Use the best sandbox here, or none when the machine has none, so
            // policy tests still run; sandbox tests check availability themselves.
            if polyph_sandbox::best(&opts.sandbox_helper, false) == polyph_sandbox::Mechanism::None {
                opts.sandbox = Some(polyph_sandbox::Mechanism::None);
            }
        }
        let daemon = Daemon::start(opts).await?;
        let events = daemon.subscribe();
        daemon
            .call("config.set", json!({ "key": "relay", "value": self.relay.url() }))
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        daemon.call("config.set", json!({ "key": "name", "value": name })).await.map_err(|e| anyhow::anyhow!("{e}"))?;
        Ok(Node { name: name.into(), daemon, events, _dir: dir })
    }

    /// Stops a node's daemon and starts a new one on the same home directory.
    pub async fn restart(&mut self, i: usize) -> anyhow::Result<()> {
        let old = self.nodes.remove(i);
        old.daemon.stop().await;
        let name = old.name.clone();
        let dir = old._dir;
        drop(old.daemon);
        let node = self.start_in(&name, dir).await?;
        self.nodes.insert(i, node);
        Ok(())
    }

    /// Advances the shared clock in heartbeat steps, letting ticks run.
    pub async fn advance(&self, secs: i64) {
        let mut left = secs;
        while left > 0 {
            let step = left.min(10);
            self.clock.advance(step);
            left -= step;
            tokio::time::sleep(Duration::from_millis(150)).await;
        }
    }

    /// Runs the full join: `joiner` uses a code from `inviter`, `approver` approves the knock.
    pub async fn join(
        &mut self,
        room: &str,
        inviter: usize,
        joiner: usize,
        approver: usize,
        role: &str,
    ) -> anyhow::Result<String> {
        let inv = self.nodes[inviter].call("room.invite", json!({ "room": room })).await?;
        let code = inv["code"].as_str().unwrap_or_default().to_string();
        let jd = self.nodes[joiner].daemon.clone();
        let jname = self.nodes[joiner].name.clone();
        let mut handle =
            tokio::spawn(
                async move { jd.call("join", json!({ "code": code, "name": jname, "timeout_secs": 20 })).await },
            );
        let knock = tokio::select! {
            k = self.nodes[approver].wait_event("knock", |e| e.kind() == "knock") => k?,
            r = &mut handle => anyhow::bail!("join ended before the knock: {:?}", r.map(|x| x.map_err(|e| e.to_string()))),
        };
        let kid = knock.event.pointer("/knock/id").and_then(|v| v.as_str()).unwrap_or_default().to_string();
        let room_on_approver = knock.room.clone().unwrap_or_default();
        self.nodes[approver]
            .call("knock.decide", json!({ "room": room_on_approver, "knock": kid, "approve": true, "role": role }))
            .await?;
        let res = handle.await?.map_err(|e| anyhow::anyhow!("join failed: {e}"))?;
        Ok(res["id"].as_str().unwrap_or_default().to_string())
    }

    /// Trusts the project file at `dir` on every node. Call it after the
    /// file's final content is written; any later edit invalidates the record.
    pub fn trust_project(&self, dir: &std::path::Path) -> anyhow::Result<()> {
        self.nodes.iter().try_for_each(|n| n.trust_project(dir))
    }

    /// Polls `f` until it returns true or ten seconds pass.
    pub async fn until<F, Fut>(&self, what: &str, mut f: F) -> anyhow::Result<()>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = bool>,
    {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        while tokio::time::Instant::now() < deadline {
            if f().await {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        anyhow::bail!("timed out waiting until {what}")
    }

    /// Like [`Harness::until`], returning the value once `f` produces one.
    pub async fn until_value<T, F, Fut>(&self, what: &str, mut f: F) -> anyhow::Result<T>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = Option<T>>,
    {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        while tokio::time::Instant::now() < deadline {
            if let Some(v) = f().await {
                return Ok(v);
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        anyhow::bail!("timed out waiting until {what}")
    }
}
