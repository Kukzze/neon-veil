//! Neon Veil: Zero Protocol native client.
//!
//! Usage: `game_client [--connect <address>] [--greybox]`
//!
//! Offline by default: an in-process server runs on its own thread over the
//! loopback transport. `--connect` joins a dedicated server over UDP;
//! `--greybox` loads the movement test gym and developer tools.

fn main() -> anyhow::Result<()> {
    let options =
        game_client::parse_client_args(std::env::args().skip(1)).map_err(anyhow::Error::msg)?;
    game_client::build_windowed_client_app(options)?.run();
    Ok(())
}
