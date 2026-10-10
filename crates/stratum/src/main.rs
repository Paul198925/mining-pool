mod session;

use std::{net::SocketAddr, time::Duration};

use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use pool_common::config::Config;
use tokio::net::{TcpListener, TcpStream};
use tokio_util::codec::{Framed, LinesCodec};
use tracing::{debug, info, warn};

use crate::session::Session;

const MAX_LINE_BYTES: usize = 16 * 1024;
const IDLE_TIMEOUT: Duration = Duration::from_secs(600);

#[tokio::main]
async fn main() -> Result<()> {
    pool_common::init_logging();
    let cfg = Config::load()?;

    let listener = TcpListener::bind(&cfg.stratum.listen)
        .await
        .with_context(|| format!("binding {}", cfg.stratum.listen))?;
    info!(listen = %cfg.stratum.listen, "stratum listening");

    loop {
        tokio::select! {
            accepted = listener.accept() => {
                let (stream, peer) = accepted?;
                tokio::spawn(async move {
                    if let Err(err) = serve(stream, peer).await {
                        debug!(%peer, "connection closed: {err:#}");
                    }
                });
            }
            _ = tokio::signal::ctrl_c() => break,
        }
    }
    info!("shutting down");
    Ok(())
}

async fn serve(stream: TcpStream, peer: SocketAddr) -> Result<()> {
    stream.set_nodelay(true)?;
    let mut lines = Framed::new(stream, LinesCodec::new_with_max_length(MAX_LINE_BYTES));
    let mut session = Session::new();
    debug!(%peer, "connected");

    loop {
        let line = match tokio::time::timeout(IDLE_TIMEOUT, lines.next()).await {
            Err(_) => {
                warn!(%peer, "idle timeout");
                return Ok(());
            }
            Ok(None) => return Ok(()),
            Ok(Some(line)) => line?,
        };
        if let Some(reply) = session.handle_line(&line) {
            lines.send(reply).await?;
        }
    }
}
