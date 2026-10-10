use std::{env, time::Duration};

use anyhow::{Context, Result, bail};
use pool_common::config::Config;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use sqlx::PgPool;
use tracing::warn;

const HEARTBEAT: Duration = Duration::from_secs(30);
const CONFIG_CHECK: Duration = Duration::from_secs(60);

/// Where logins, shares and blocks go.
#[derive(Clone)]
pub enum Backend {
    /// Straight into the pool's database. For a stratum server run next to it.
    Direct(PgPool),
    /// Through the pool's node API. For a server that enrolled itself with a one-time
    /// code; it holds a node token and has no database access.
    Api(ApiClient),
}

#[derive(Clone)]
pub struct ApiClient {
    http: reqwest::Client,
    base: String,
    token: String,
}

impl ApiClient {
    /// `Some` when `POOL_API_URL` and `POOL_NODE_TOKEN` say this is an enrolled server.
    pub fn from_env() -> Result<Option<Self>> {
        let (Ok(base), Ok(token)) = (env::var("POOL_API_URL"), env::var("POOL_NODE_TOKEN")) else {
            return Ok(None);
        };
        let http = reqwest::Client::builder().timeout(Duration::from_secs(20)).build()?;
        Ok(Some(Self { http, base: base.trim_end_matches('/').to_owned(), token }))
    }

    /// The configuration the pool has for this server, parsed and as it was sent.
    pub async fn config(&self) -> Result<(Config, Value)> {
        let path = "/api/node/config";
        let value: Value = self.send(self.http.get(self.url(path)), path).await?;
        let config = Config::from_json(value.clone()).context("the pool sent an unusable configuration")?;
        Ok((config, value))
    }

    /// Resolves once the pool has a different configuration for this server than `current`.
    pub async fn config_changed(&self, current: Value) {
        let mut tick = tokio::time::interval(CONFIG_CHECK);
        loop {
            tick.tick().await;
            // An unreachable pool is no reason to drop the miners.
            if let Ok((_, latest)) = self.config().await
                && latest != current
            {
                return;
            }
        }
    }

    pub async fn post<T: DeserializeOwned>(&self, path: &str, body: &Value) -> Result<T> {
        self.send(self.http.post(self.url(path)).json(body), path).await
    }

    /// Keeps this server listed as running on the website.
    pub fn spawn_heartbeat(&self) {
        let api = self.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(HEARTBEAT);
            loop {
                tick.tick().await;
                if let Err(err) = api.post::<Value>("/api/node/heartbeat", &json!({})).await {
                    warn!("heartbeat failed: {err:#}");
                }
            }
        });
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base)
    }

    async fn send<T: DeserializeOwned>(&self, request: reqwest::RequestBuilder, path: &str) -> Result<T> {
        let response =
            request.bearer_auth(&self.token).send().await.with_context(|| format!("pool api {path}: no answer"))?;
        let status = response.status();
        let text = response.text().await?;
        if !status.is_success() {
            let detail: String = text.chars().take(200).collect();
            bail!("pool api {path}: HTTP {status}: {detail}");
        }
        serde_json::from_str(&text).with_context(|| format!("pool api {path}: unexpected answer"))
    }
}
