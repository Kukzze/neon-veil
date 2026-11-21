//! Platform service abstraction and the in-memory implementation used for
//! DRM-free builds, tests, and whenever Steam is not running.

use crate::achievements::find_achievement;
use std::collections::{HashMap, HashSet};
use std::sync::{PoisonError, RwLock};

/// Largest cloud save accepted (Steam's per-file quota is 100 MiB).
pub const MAX_CLOUD_SAVE_BYTES: usize = 100 * 1024 * 1024;

pub trait PlatformService: Send + Sync {
    /// Returns `true` if the achievement exists and is now unlocked.
    fn unlock_achievement(&self, achievement_id: &str) -> bool;
    fn is_achievement_unlocked(&self, achievement_id: &str) -> bool;
    fn update_rich_presence(&self, status: &str);
    /// Returns `true` once the data is stored.
    fn sync_cloud_save(&self, filename: &str, data: &[u8]) -> bool;
    fn load_cloud_save(&self, filename: &str) -> Option<Vec<u8>>;
    /// Short label for logs and diagnostics.
    fn backend_name(&self) -> &'static str;
}

/// Cloud save names are plain file names: no paths, no control characters.
pub fn is_valid_cloud_filename(filename: &str) -> bool {
    !filename.is_empty()
        && filename.len() <= 255
        && !filename.contains(['/', '\\', ':'])
        && filename != "."
        && filename != ".."
        && !filename.chars().any(char::is_control)
}

#[derive(Debug, Default)]
pub struct MockPlatformService {
    unlocked: RwLock<HashSet<String>>,
    cloud: RwLock<HashMap<String, Vec<u8>>>,
    presence: RwLock<String>,
}

impl MockPlatformService {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn unlocked_achievements(&self) -> Vec<String> {
        let mut ids: Vec<String> = self
            .unlocked
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .cloned()
            .collect();
        ids.sort();
        ids
    }

    pub fn rich_presence(&self) -> String {
        self.presence
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl PlatformService for MockPlatformService {
    fn unlock_achievement(&self, achievement_id: &str) -> bool {
        if find_achievement(achievement_id).is_none() {
            return false;
        }
        self.unlocked
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(achievement_id.to_owned());
        true
    }

    fn is_achievement_unlocked(&self, achievement_id: &str) -> bool {
        self.unlocked
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .contains(achievement_id)
    }

    fn update_rich_presence(&self, status: &str) {
        let mut presence = self
            .presence
            .write()
            .unwrap_or_else(PoisonError::into_inner);
        presence.clear();
        presence.push_str(status);
    }

    fn sync_cloud_save(&self, filename: &str, data: &[u8]) -> bool {
        if !is_valid_cloud_filename(filename) || data.len() > MAX_CLOUD_SAVE_BYTES {
            return false;
        }
        self.cloud
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(filename.to_owned(), data.to_vec());
        true
    }

    fn load_cloud_save(&self, filename: &str) -> Option<Vec<u8>> {
        self.cloud
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .get(filename)
            .cloned()
    }

    fn backend_name(&self) -> &'static str {
        "offline"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::achievements::{ACH_LEVEL_99_AGILITY, ACH_PROTOCOL_ZERO};
    use std::sync::Arc;

    #[test]
    fn achievements_unlock_once_and_only_if_known() {
        let mock = MockPlatformService::new();
        assert!(!mock.is_achievement_unlocked(ACH_PROTOCOL_ZERO));
        assert!(mock.unlock_achievement(ACH_PROTOCOL_ZERO));
        assert!(
            mock.unlock_achievement(ACH_PROTOCOL_ZERO),
            "repeat unlocks are harmless"
        );
        assert!(mock.is_achievement_unlocked(ACH_PROTOCOL_ZERO));
        assert!(!mock.unlock_achievement("ACH_TYPO"));
        assert!(!mock.is_achievement_unlocked("ACH_TYPO"));
        assert_eq!(
            mock.unlocked_achievements(),
            vec![ACH_PROTOCOL_ZERO.to_string()]
        );
    }

    #[test]
    fn cloud_saves_roundtrip_and_reject_bad_names() {
        let mock = MockPlatformService::new();
        assert!(mock.sync_cloud_save("slot1.sav", b"neon"));
        assert_eq!(
            mock.load_cloud_save("slot1.sav").as_deref(),
            Some(&b"neon"[..])
        );
        assert!(mock.sync_cloud_save("slot1.sav", b"veil"), "overwrite");
        assert_eq!(
            mock.load_cloud_save("slot1.sav").as_deref(),
            Some(&b"veil"[..])
        );
        assert_eq!(mock.load_cloud_save("missing.sav"), None);
        for bad in [
            "",
            "../escape.sav",
            "dir/slot.sav",
            "c:slot",
            "..",
            "bad\nname",
        ] {
            assert!(!mock.sync_cloud_save(bad, b"x"), "{bad:?}");
        }
    }

    #[test]
    fn rich_presence_is_replaced() {
        let mock = MockPlatformService::new();
        mock.update_rich_presence("Level 8 // Undercity Slums // Idle");
        mock.update_rich_presence("Level 9 // Undercity Slums // Sprinting");
        assert_eq!(
            mock.rich_presence(),
            "Level 9 // Undercity Slums // Sprinting"
        );
        assert_eq!(mock.backend_name(), "offline");
    }

    #[test]
    fn shared_across_threads() {
        let mock = Arc::new(MockPlatformService::new());
        let handles: Vec<_> = (0..8)
            .map(|i| {
                let mock = Arc::clone(&mock);
                std::thread::spawn(move || {
                    mock.unlock_achievement(ACH_LEVEL_99_AGILITY);
                    mock.sync_cloud_save(&format!("slot{i}.sav"), &[i as u8; 16])
                })
            })
            .collect();
        assert!(handles.into_iter().all(|h| h.join().unwrap()));
        assert!(mock.is_achievement_unlocked(ACH_LEVEL_99_AGILITY));
        assert_eq!(mock.load_cloud_save("slot5.sav"), Some(vec![5; 16]));
    }
}
