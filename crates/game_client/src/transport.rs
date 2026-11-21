//! Client end of the UDP transport. The in-process transports live in
//! `game_server::transport` and are re-exported here.

pub use game_server::transport::{
    ClientTransport, LoopbackClientTransport, LoopbackNetwork, NetworkConditions,
    NETCODE_PROTOCOL_ID,
};

use game_core::protocol::{decode, encode, ClientMessage, ServerMessage};
use game_server::transport::client_message_channel;
use renet::transport::{ClientAuthentication, NetcodeClientTransport};
use renet::{ConnectionConfig, DefaultChannel, RenetClient};
use std::net::{SocketAddr, UdpSocket};
use std::time::{Duration, SystemTime};

pub struct RenetClientTransport {
    client: RenetClient,
    transport: NetcodeClientTransport,
}

impl RenetClientTransport {
    /// Starts connecting to `server_addr` as `client_id` (unsecured netcode).
    pub fn connect(server_addr: SocketAddr, client_id: u64) -> std::io::Result<Self> {
        let bind: SocketAddr = if server_addr.is_ipv4() {
            "0.0.0.0:0".parse().expect("valid address")
        } else {
            "[::]:0".parse().expect("valid address")
        };
        let socket = UdpSocket::bind(bind)?;
        let current_time = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default();
        let authentication = ClientAuthentication::Unsecure {
            protocol_id: NETCODE_PROTOCOL_ID,
            client_id,
            server_addr,
            user_data: None,
        };
        let transport = NetcodeClientTransport::new(current_time, authentication, socket)
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        Ok(Self {
            client: RenetClient::new(ConnectionConfig::default()),
            transport,
        })
    }

    pub fn is_handshake_complete(&self) -> bool {
        self.client.is_connected()
    }
}

impl ClientTransport for RenetClientTransport {
    fn update(&mut self, dt: Duration) {
        self.client.update(dt);
        if let Err(error) = self.transport.update(dt, &mut self.client) {
            tracing::warn!("client transport error: {error}");
        }
    }

    fn send(&mut self, message: ClientMessage) {
        match encode(&message) {
            Ok(bytes) => self
                .client
                .send_message(client_message_channel(&message), bytes),
            Err(error) => tracing::error!("failed to encode client message: {error}"),
        }
    }

    fn receive(&mut self) -> Vec<ServerMessage> {
        let mut out = Vec::new();
        for channel in [DefaultChannel::ReliableOrdered, DefaultChannel::Unreliable] {
            let channel = u8::from(channel);
            while let Some(bytes) = self.client.receive_message(channel) {
                match decode::<ServerMessage>(&bytes) {
                    Ok(message) => out.push(message),
                    Err(error) => tracing::debug!("dropping malformed message: {error}"),
                }
            }
        }
        out
    }

    fn flush(&mut self) {
        if let Err(error) = self.transport.send_packets(&mut self.client) {
            tracing::warn!("client send error: {error}");
        }
    }

    fn is_connected(&self) -> bool {
        !self.client.is_disconnected()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_server::transport::{RenetServerTransport, ServerTransport};
    use std::time::Instant;

    #[test]
    fn udp_round_trip_on_localhost() {
        let mut server =
            RenetServerTransport::bind("127.0.0.1:0".parse().unwrap(), 4).expect("bind server");
        let mut client = RenetClientTransport::connect(server.local_addr(), 77).expect("client");
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut last = Instant::now();
        let (mut server_got_ping, mut client_got_pong) = (false, false);
        while Instant::now() < deadline && !client_got_pong {
            let dt = last.elapsed();
            last = Instant::now();
            server.update(dt);
            client.update(dt);
            if client.is_handshake_complete() && !server_got_ping {
                client.send(ClientMessage::Ping(99));
            }
            for (connection, message) in server.receive() {
                if message == ClientMessage::Ping(99) {
                    server_got_ping = true;
                    assert_eq!(connection, 77);
                    server.send(connection, ServerMessage::Pong(99));
                }
            }
            client_got_pong |= client.receive().contains(&ServerMessage::Pong(99));
            server.flush();
            client.flush();
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(
            server_got_ping && client_got_pong,
            "ping/pong over UDP within 10 s"
        );
        assert_eq!(server.connected_clients(), 1);
        assert!(client.is_connected());
    }
}
