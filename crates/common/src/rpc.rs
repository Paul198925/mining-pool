use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::{Value, json};

use crate::config::Coin;

/// JSON-RPC client for a Bitcoin-style coin daemon.
#[derive(Debug, Clone)]
pub struct RpcClient {
    http: reqwest::Client,
    url: String,
    user: String,
    password: String,
}

#[derive(Deserialize)]
struct RpcResponse {
    result: Option<Value>,
    error: Option<RpcError>,
}

#[derive(Deserialize)]
struct RpcError {
    code: i64,
    message: String,
}

#[derive(Debug, Deserialize)]
pub struct BlockchainInfo {
    pub chain: String,
    pub blocks: u64,
    pub bestblockhash: String,
}

impl RpcClient {
    pub fn new(coin: &Coin) -> Result<Self> {
        let http = reqwest::Client::builder().timeout(Duration::from_secs(30)).build()?;
        Ok(Self {
            http,
            url: coin.rpc_url.clone(),
            user: coin.rpc_user.clone(),
            password: coin.rpc_password.clone(),
        })
    }

    pub async fn call<T: DeserializeOwned>(&self, method: &str, params: Value) -> Result<T> {
        let body = json!({ "jsonrpc": "1.0", "id": 0, "method": method, "params": params });
        let resp = self
            .http
            .post(&self.url)
            .basic_auth(&self.user, Some(&self.password))
            .json(&body)
            .send()
            .await
            .with_context(|| format!("rpc {method}: request failed"))?;
        let status = resp.status();
        // Daemons answer RPC errors with HTTP 4xx/5xx and a JSON body, so parse before judging the status.
        let text = resp.text().await?;
        let parsed: RpcResponse = serde_json::from_str(&text)
            .map_err(|_| anyhow!("rpc {method}: HTTP {status}, non-JSON body"))?;
        if let Some(err) = parsed.error {
            return Err(anyhow!("rpc {method}: {} (code {})", err.message, err.code));
        }
        serde_json::from_value(parsed.result.unwrap_or(Value::Null))
            .with_context(|| format!("rpc {method}: unexpected result shape"))
    }

    pub async fn blockchain_info(&self) -> Result<BlockchainInfo> {
        self.call("getblockchaininfo", json!([])).await
    }
}
