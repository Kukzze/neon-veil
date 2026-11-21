//! Strongly typed identifiers and network ownership components.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

/// Globally unique, persistent player account identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PlayerId(pub Uuid);

impl PlayerId {
    /// Generates a new random (v4) player identifier.
    pub fn new_random() -> Self {
        Self(Uuid::new_v4())
    }
}

impl fmt::Display for PlayerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Server-assigned identifier for a replicated entity. Stable across clients,
/// unlike Bevy's local [`Entity`].
#[derive(
    Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct NetworkEntityId(pub u64);

/// Static item definition identifier.
#[derive(
    Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct ItemId(pub u32);

/// Static quest definition identifier.
#[derive(
    Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct QuestId(pub u32);

/// Marks an entity as a player character, local or remote.
#[derive(Component, Reflect, Debug, Default, Clone, Copy, PartialEq, Eq)]
#[reflect(Component, Default, Debug)]
pub struct PlayerCharacterMarker;

/// Which peer owns the simulation of an entity.
#[derive(
    Component, Reflect, Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize,
)]
#[reflect(Component, Default, Debug)]
pub enum NetworkAuthority {
    /// Predicted locally by the owning client and reconciled against the server.
    ClientOwned,
    /// Simulated only by the server; clients render replicated snapshots.
    #[default]
    ServerAuthoritative,
    /// A remote entity interpolated from snapshots on this client.
    SimulatedProxy,
}

impl NetworkAuthority {
    /// Whether this peer should run gameplay simulation for the entity.
    #[inline]
    pub const fn is_locally_simulated(self, is_server: bool) -> bool {
        match self {
            Self::ClientOwned => true,
            Self::ServerAuthoritative => is_server,
            Self::SimulatedProxy => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::de::DeserializeOwned;
    use std::collections::HashSet;
    use std::fmt::Debug;

    fn roundtrip<T: Serialize + DeserializeOwned + PartialEq + Debug>(value: &T) {
        let bytes = bitcode::serialize(value).expect("bitcode encode");
        assert_eq!(
            &bitcode::deserialize::<T>(&bytes).expect("bitcode decode"),
            value
        );
        let json = serde_json::to_string(value).expect("json encode");
        assert_eq!(
            &serde_json::from_str::<T>(&json).expect("json decode"),
            value
        );
    }

    #[test]
    fn identifiers_roundtrip_through_serde() {
        roundtrip(&PlayerId::new_random());
        roundtrip(&PlayerId(Uuid::nil()));
        roundtrip(&NetworkEntityId(u64::MAX));
        roundtrip(&ItemId(4_201));
        roundtrip(&QuestId(30));
        roundtrip(&NetworkAuthority::ClientOwned);
        roundtrip(&NetworkAuthority::ServerAuthoritative);
        roundtrip(&NetworkAuthority::SimulatedProxy);
    }

    #[test]
    fn identifiers_hash_by_value() {
        let player = PlayerId::new_random();
        let set: HashSet<PlayerId> = [player, player, PlayerId::new_random()].into();
        assert_eq!(set.len(), 2);
        assert!(set.contains(&player));

        let entities: HashSet<NetworkEntityId> =
            [NetworkEntityId(1), NetworkEntityId(1), NetworkEntityId(2)].into();
        assert_eq!(entities.len(), 2);

        let items: HashSet<ItemId> = [ItemId(7), ItemId(7)].into();
        assert_eq!(items.len(), 1);

        let quests: HashSet<QuestId> = [QuestId(1), QuestId(2), QuestId(1)].into();
        assert_eq!(quests.len(), 2);
    }

    #[test]
    fn random_player_ids_are_unique() {
        assert_ne!(PlayerId::new_random(), PlayerId::new_random());
    }

    #[test]
    fn player_id_displays_as_uuid() {
        let id = PlayerId(Uuid::nil());
        assert_eq!(id.to_string(), "00000000-0000-0000-0000-000000000000");
    }

    #[test]
    fn authority_decides_local_simulation() {
        assert!(NetworkAuthority::ClientOwned.is_locally_simulated(false));
        assert!(!NetworkAuthority::ServerAuthoritative.is_locally_simulated(false));
        assert!(NetworkAuthority::ServerAuthoritative.is_locally_simulated(true));
        assert!(!NetworkAuthority::SimulatedProxy.is_locally_simulated(false));
        assert_eq!(
            NetworkAuthority::default(),
            NetworkAuthority::ServerAuthoritative
        );
    }
}
