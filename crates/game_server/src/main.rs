//! Neon Veil dedicated headless server.
//!
//! Usage: `game_server [bind address]` (default `0.0.0.0:5000`).

use game_server::{build_server_app, RenetServerTransport};
use std::net::SocketAddr;

const DEFAULT_BIND: &str = "0.0.0.0:5000";
const MAX_CLIENTS: usize = 64;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_target(false).init();
    let bind: SocketAddr = std::env::args()
        .nth(1)
        .as_deref()
        .unwrap_or(DEFAULT_BIND)
        .parse()?;
    let transport = RenetServerTransport::bind(bind, MAX_CLIENTS)?;
    tracing::info!("Neon Veil server listening on {}", transport.local_addr());
    build_server_app(transport).run();
    Ok(())
}
