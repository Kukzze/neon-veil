//! Message transports shared by the server and the client.
//!
//! - [`LoopbackNetwork`]: in-process queues passing messages by value. With
//!   [`NetworkConditions::PERFECT`] it is the offline single-player link
//!   (no serialization, zero latency, no loss); with other conditions it
//!   simulates latency and packet loss deterministically for testing.
//! - [`RenetServerTransport`]: UDP via renet/netcode for co-op and MMO play.
//!
//! The simulated clock advances only in [`ServerTransport::update`] (one
//! server per network), so delivery times follow the server's fixed tick.

use game_core::protocol::{decode, encode, ClientMessage, ServerMessage};
use renet::transport::{NetcodeServerTransport, ServerAuthentication, ServerConfig};
use renet::{ClientId, ConnectionConfig, DefaultChannel, RenetServer, ServerEvent};
use std::collections::{BTreeMap, VecDeque};
use std::net::{SocketAddr, UdpSocket};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, SystemTime};

/// Transport-level connection handle.
pub type ConnectionId = u64;

/// Netcode protocol id; bump with incompatible wire changes.
pub const NETCODE_PROTOCOL_ID: u64 = 0x4E56_5A50_0000_0003;

pub trait ServerTransport: Send + Sync + 'static {
    /// Pumps the transport by `dt` of server time.
    fn update(&mut self, dt: Duration);
    /// Messages that arrived since the last call, oldest first.
    fn receive(&mut self) -> Vec<(ConnectionId, ClientMessage)>;
    fn send(&mut self, connection: ConnectionId, message: ServerMessage);
    /// Pushes queued outgoing data onto the wire.
    fn flush(&mut self) {}
    fn disconnect(&mut self, connection: ConnectionId);
    /// Connections that closed since the last call.
    fn take_disconnections(&mut self) -> Vec<ConnectionId>;
}

pub trait ClientTransport: Send + Sync + 'static {
    fn update(&mut self, dt: Duration);
    fn send(&mut self, message: ClientMessage);
    fn receive(&mut self) -> Vec<ServerMessage>;
    fn flush(&mut self) {}
    fn is_connected(&self) -> bool;
}

/// Unreliable channel for per-tick traffic, reliable for everything else.
pub fn client_message_channel(message: &ClientMessage) -> DefaultChannel {
    match message {
        ClientMessage::Input(_) => DefaultChannel::Unreliable,
        _ => DefaultChannel::ReliableOrdered,
    }
}

pub fn server_message_channel(message: &ServerMessage) -> DefaultChannel {
    match message {
        ServerMessage::Snapshot(_) => DefaultChannel::Unreliable,
        _ => DefaultChannel::ReliableOrdered,
    }
}

// ---------------------------------------------------------------------------
// In-process network
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NetworkConditions {
    pub one_way_latency: Duration,
    /// Probability in `[0, 1]` that a message is lost.
    pub drop_rate: f32,
}

impl NetworkConditions {
    pub const PERFECT: Self = Self {
        one_way_latency: Duration::ZERO,
        drop_rate: 0.0,
    };

    /// Symmetric link with the given round-trip time and no loss.
    pub fn with_rtt_ms(rtt_ms: f32) -> Self {
        Self {
            one_way_latency: Duration::from_nanos(
                (f64::from(rtt_ms.max(0.0)) * 500_000.0).round() as u64
            ),
            drop_rate: 0.0,
        }
    }

    pub fn with_drop_rate(self, drop_rate: f32) -> Self {
        Self {
            drop_rate: drop_rate.clamp(0.0, 1.0),
            ..self
        }
    }

    pub fn rtt(&self) -> Duration {
        self.one_way_latency * 2
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NetworkStats {
    pub sent: u64,
    pub dropped: u64,
    pub delivered: u64,
}

struct InFlight<T> {
    deliver_at: Duration,
    message: T,
}

struct Link {
    to_server: VecDeque<InFlight<ClientMessage>>,
    to_client: VecDeque<InFlight<ServerMessage>>,
    client_open: bool,
    server_open: bool,
}

struct NetworkState {
    now: Duration,
    conditions: NetworkConditions,
    rng: u64,
    links: BTreeMap<ConnectionId, Link>,
    next_id: ConnectionId,
    stats: NetworkStats,
}

impl NetworkState {
    /// Xorshift; decides losses deterministically from the seed.
    fn roll_drop(&mut self) -> bool {
        if self.conditions.drop_rate <= 0.0 {
            return false;
        }
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        ((self.rng >> 40) as f32 / (1u64 << 24) as f32) < self.conditions.drop_rate
    }

    /// Returns the delivery time, or `None` if the message is lost.
    fn schedule(&mut self) -> Option<Duration> {
        self.stats.sent += 1;
        if self.roll_drop() {
            self.stats.dropped += 1;
            None
        } else {
            Some(self.now + self.conditions.one_way_latency)
        }
    }
}

fn lock(state: &Mutex<NetworkState>) -> MutexGuard<'_, NetworkState> {
    state.lock().unwrap_or_else(PoisonError::into_inner)
}

/// In-process network hub. Clones share the same network.
#[derive(Clone)]
pub struct LoopbackNetwork {
    state: Arc<Mutex<NetworkState>>,
}

impl Default for LoopbackNetwork {
    fn default() -> Self {
        Self::perfect()
    }
}

impl LoopbackNetwork {
    /// Offline link: zero latency, no loss.
    pub fn perfect() -> Self {
        Self::simulated(NetworkConditions::PERFECT, 1)
    }

    /// Simulated link; `seed` fixes which messages are lost.
    pub fn simulated(conditions: NetworkConditions, seed: u64) -> Self {
        Self {
            state: Arc::new(Mutex::new(NetworkState {
                now: Duration::ZERO,
                conditions,
                rng: seed.max(1),
                links: BTreeMap::new(),
                next_id: 1,
                stats: NetworkStats::default(),
            })),
        }
    }

    pub fn conditions(&self) -> NetworkConditions {
        lock(&self.state).conditions
    }

    pub fn set_conditions(&self, conditions: NetworkConditions) {
        lock(&self.state).conditions = conditions;
    }

    pub fn stats(&self) -> NetworkStats {
        lock(&self.state).stats
    }

    pub fn now(&self) -> Duration {
        lock(&self.state).now
    }

    /// The server end. Create one per network.
    pub fn server_transport(&self) -> LoopbackServerTransport {
        LoopbackServerTransport {
            state: Arc::clone(&self.state),
        }
    }

    /// Opens a new client connection.
    pub fn connect(&self) -> LoopbackClientTransport {
        let mut state = lock(&self.state);
        let id = state.next_id;
        state.next_id += 1;
        state.links.insert(
            id,
            Link {
                to_server: VecDeque::new(),
                to_client: VecDeque::new(),
                client_open: true,
                server_open: true,
            },
        );
        LoopbackClientTransport {
            state: Arc::clone(&self.state),
            id,
        }
    }
}

pub struct LoopbackServerTransport {
    state: Arc<Mutex<NetworkState>>,
}

impl ServerTransport for LoopbackServerTransport {
    fn update(&mut self, dt: Duration) {
        lock(&self.state).now += dt;
    }

    fn receive(&mut self) -> Vec<(ConnectionId, ClientMessage)> {
        let mut state = lock(&self.state);
        let now = state.now;
        let mut out = Vec::new();
        for (&id, link) in state.links.iter_mut() {
            if !link.server_open {
                link.to_server.clear();
                continue;
            }
            while link.to_server.front().is_some_and(|m| m.deliver_at <= now) {
                let message = link.to_server.pop_front().expect("front exists").message;
                out.push((id, message));
            }
        }
        state.stats.delivered += out.len() as u64;
        out
    }

    fn send(&mut self, connection: ConnectionId, message: ServerMessage) {
        let mut state = lock(&self.state);
        let open = state
            .links
            .get(&connection)
            .is_some_and(|l| l.client_open && l.server_open);
        if !open {
            return;
        }
        if let Some(deliver_at) = state.schedule() {
            if let Some(link) = state.links.get_mut(&connection) {
                link.to_client.push_back(InFlight {
                    deliver_at,
                    message,
                });
            }
        }
    }

    fn disconnect(&mut self, connection: ConnectionId) {
        if let Some(link) = lock(&self.state).links.get_mut(&connection) {
            link.server_open = false;
        }
    }

    fn take_disconnections(&mut self) -> Vec<ConnectionId> {
        let mut state = lock(&self.state);
        let closed: Vec<ConnectionId> = state
            .links
            .iter()
            .filter(|(_, l)| !l.client_open)
            .map(|(&id, _)| id)
            .collect();
        for id in &closed {
            state.links.remove(id);
        }
        closed
    }
}

pub struct LoopbackClientTransport {
    state: Arc<Mutex<NetworkState>>,
    id: ConnectionId,
}

impl LoopbackClientTransport {
    pub fn connection_id(&self) -> ConnectionId {
        self.id
    }
}

impl ClientTransport for LoopbackClientTransport {
    fn update(&mut self, _dt: Duration) {}

    fn send(&mut self, message: ClientMessage) {
        let mut state = lock(&self.state);
        let open = state
            .links
            .get(&self.id)
            .is_some_and(|l| l.client_open && l.server_open);
        if !open {
            return;
        }
        if let Some(deliver_at) = state.schedule() {
            if let Some(link) = state.links.get_mut(&self.id) {
                link.to_server.push_back(InFlight {
                    deliver_at,
                    message,
                });
            }
        }
    }

    fn receive(&mut self) -> Vec<ServerMessage> {
        let mut state = lock(&self.state);
        let now = state.now;
        let Some(link) = state.links.get_mut(&self.id) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        while link.to_client.front().is_some_and(|m| m.deliver_at <= now) {
            out.push(link.to_client.pop_front().expect("front exists").message);
        }
        state.stats.delivered += out.len() as u64;
        out
    }

    fn is_connected(&self) -> bool {
        lock(&self.state)
            .links
            .get(&self.id)
            .is_some_and(|l| l.client_open && l.server_open)
    }
}

impl Drop for LoopbackClientTransport {
    fn drop(&mut self) {
        if let Some(link) = lock(&self.state).links.get_mut(&self.id) {
            link.client_open = false;
        }
    }
}

// ---------------------------------------------------------------------------
// UDP (renet / netcode)
// ---------------------------------------------------------------------------

pub struct RenetServerTransport {
    server: RenetServer,
    transport: NetcodeServerTransport,
    local_addr: SocketAddr,
    disconnected: Vec<ConnectionId>,
}

impl RenetServerTransport {
    /// Binds `bind_addr` and accepts unsecured netcode connections.
    pub fn bind(bind_addr: SocketAddr, max_clients: usize) -> std::io::Result<Self> {
        let socket = UdpSocket::bind(bind_addr)?;
        let public_addr = socket.local_addr()?;
        let current_time = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default();
        let config = ServerConfig {
            current_time,
            max_clients,
            protocol_id: NETCODE_PROTOCOL_ID,
            public_addresses: vec![public_addr],
            authentication: ServerAuthentication::Unsecure,
        };
        Ok(Self {
            server: RenetServer::new(ConnectionConfig::default()),
            transport: NetcodeServerTransport::new(config, socket)?,
            local_addr: public_addr,
            disconnected: Vec::new(),
        })
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    pub fn connected_clients(&self) -> usize {
        self.server.connected_clients()
    }
}

impl ServerTransport for RenetServerTransport {
    fn update(&mut self, dt: Duration) {
        self.server.update(dt);
        if let Err(error) = self.transport.update(dt, &mut self.server) {
            tracing::warn!("server transport error: {error}");
        }
        while let Some(event) = self.server.get_event() {
            if let ServerEvent::ClientDisconnected { client_id, .. } = event {
                self.disconnected.push(client_id.raw());
            }
        }
    }

    fn receive(&mut self) -> Vec<(ConnectionId, ClientMessage)> {
        let mut out = Vec::new();
        for client_id in self.server.clients_id() {
            for channel in [DefaultChannel::ReliableOrdered, DefaultChannel::Unreliable] {
                let channel = u8::from(channel);
                while let Some(bytes) = self.server.receive_message(client_id, channel) {
                    match decode::<ClientMessage>(&bytes) {
                        Ok(message) => out.push((client_id.raw(), message)),
                        Err(error) => tracing::debug!("dropping malformed message: {error}"),
                    }
                }
            }
        }
        out
    }

    fn send(&mut self, connection: ConnectionId, message: ServerMessage) {
        match encode(&message) {
            Ok(bytes) => self.server.send_message(
                ClientId::from_raw(connection),
                server_message_channel(&message),
                bytes,
            ),
            Err(error) => tracing::error!("failed to encode server message: {error}"),
        }
    }

    fn flush(&mut self) {
        self.transport.send_packets(&mut self.server);
    }

    fn disconnect(&mut self, connection: ConnectionId) {
        self.server.disconnect(ClientId::from_raw(connection));
    }

    fn take_disconnections(&mut self) -> Vec<ConnectionId> {
        std::mem::take(&mut self.disconnected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_core::protocol::PlayerInputPacket;
    use game_core::time::NetworkTick;

    fn input(tick: u64) -> ClientMessage {
        ClientMessage::Input(PlayerInputPacket {
            tick: NetworkTick(tick),
            ..Default::default()
        })
    }

    #[test]
    fn perfect_loopback_delivers_immediately_in_order() {
        let net = LoopbackNetwork::perfect();
        let mut server = net.server_transport();
        let mut client = net.connect();
        assert!(client.is_connected());
        for t in 0..5 {
            client.send(input(t));
        }
        let got = server.receive();
        assert_eq!(got.len(), 5);
        assert!(got.iter().all(|(id, _)| *id == client.connection_id()));
        assert_eq!(
            got.into_iter().map(|(_, m)| m).collect::<Vec<_>>(),
            (0..5).map(input).collect::<Vec<_>>()
        );
        server.send(client.connection_id(), ServerMessage::Pong(7));
        assert_eq!(client.receive(), vec![ServerMessage::Pong(7)]);
        assert_eq!(
            net.stats(),
            NetworkStats {
                sent: 6,
                dropped: 0,
                delivered: 6
            }
        );
    }

    #[test]
    fn simulated_latency_holds_messages_until_due() {
        let net = LoopbackNetwork::simulated(NetworkConditions::with_rtt_ms(100.0), 9);
        let mut server = net.server_transport();
        let mut client = net.connect();
        client.send(ClientMessage::Ping(1));
        server.update(Duration::from_millis(49));
        assert!(server.receive().is_empty(), "one-way latency is 50 ms");
        server.update(Duration::from_millis(1));
        assert_eq!(server.receive().len(), 1);
        server.send(client.connection_id(), ServerMessage::Pong(1));
        server.update(Duration::from_millis(30));
        assert!(client.receive().is_empty());
        server.update(Duration::from_millis(20));
        assert_eq!(client.receive(), vec![ServerMessage::Pong(1)]);
        assert_eq!(net.conditions().rtt(), Duration::from_millis(100));
    }

    #[test]
    fn simulated_loss_matches_rate_and_is_seeded() {
        let run = |seed| {
            let net =
                LoopbackNetwork::simulated(NetworkConditions::PERFECT.with_drop_rate(0.02), seed);
            let mut server = net.server_transport();
            let mut client = net.connect();
            for t in 0..10_000 {
                client.send(input(t));
            }
            let received = server.receive().len();
            (received, net.stats())
        };
        let (received, stats) = run(42);
        assert_eq!(stats.sent, 10_000);
        assert_eq!(received as u64 + stats.dropped, 10_000);
        let rate = stats.dropped as f64 / 10_000.0;
        assert!((0.015..=0.025).contains(&rate), "{rate}");
        assert_eq!(run(42), (received, stats), "deterministic for a seed");
    }

    #[test]
    fn disconnects_are_reported_both_ways() {
        let net = LoopbackNetwork::perfect();
        let mut server = net.server_transport();
        let a = net.connect();
        let b = net.connect();
        let (a_id, b_id) = (a.connection_id(), b.connection_id());
        drop(a);
        assert_eq!(server.take_disconnections(), vec![a_id]);
        assert!(server.take_disconnections().is_empty());
        server.disconnect(b_id);
        assert!(!b.is_connected());
        server.send(b_id, ServerMessage::Pong(0));
        assert_eq!(net.stats().sent, 0, "nothing is sent on a closed link");
    }

    #[test]
    fn channels_split_per_tick_traffic_from_control() {
        let id = u8::from;
        let unreliable = id(DefaultChannel::Unreliable);
        let reliable = id(DefaultChannel::ReliableOrdered);
        assert_eq!(id(client_message_channel(&input(1))), unreliable);
        assert_eq!(
            id(client_message_channel(&ClientMessage::Ping(1))),
            reliable
        );
        let snapshot = ServerMessage::Snapshot(Default::default());
        assert_eq!(id(server_message_channel(&snapshot)), unreliable);
        assert_eq!(
            id(server_message_channel(&ServerMessage::Pong(1))),
            reliable
        );
    }
}
