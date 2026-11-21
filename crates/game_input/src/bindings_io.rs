//! Saving and loading player bindings as JSON.
//!
//! Leafwing deserializes boxed inputs through a registry that
//! `InputManagerPlugin::<PlayerAction>` fills when it is built, so
//! [`load_bindings_from_file`] only succeeds after `GameInputPlugin` (or
//! `InputManagerPlugin`) has been added to an `App` in this process.

use crate::actions::PlayerAction;
use bevy::log::warn;
use leafwing_input_manager::prelude::InputMap;
use std::fs;
use std::io;
use std::path::Path;

/// Default bindings file, relative to the working directory.
pub const DEFAULT_BINDINGS_PATH: &str = "config/bindings.json";

/// Writes `map` as pretty JSON, creating parent directories. The file is
/// written to a temporary sibling and renamed so a crash never leaves a
/// truncated bindings file.
pub fn save_bindings_to_file(path: &Path, map: &InputMap<PlayerAction>) -> io::Result<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(map).map_err(io::Error::other)?;
    let temp = path.with_extension("json.tmp");
    fs::write(&temp, json)?;
    fs::rename(&temp, path)
}

/// Reads bindings from `path`. Returns `None` (and logs a warning) when the
/// file is missing or invalid, so callers can fall back to the defaults.
pub fn load_bindings_from_file(path: &Path) -> Option<InputMap<PlayerAction>> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return None,
        Err(error) => {
            warn!("could not read bindings {}: {error}", path.display());
            return None;
        }
    };
    match serde_json::from_str(&text) {
        Ok(map) => Some(map),
        Err(error) => {
            warn!("ignoring invalid bindings {}: {error}", path.display());
            None
        }
    }
}

/// Loads bindings from `path`, falling back to [`crate::actions::default_input_map`].
pub fn load_bindings_or_default(path: &Path) -> InputMap<PlayerAction> {
    load_bindings_from_file(path).unwrap_or_else(crate::actions::default_input_map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::{default_gamepad_input_map, default_input_map, default_kbm_input_map};
    use bevy::prelude::*;
    use leafwing_input_manager::prelude::InputManagerPlugin;
    use std::path::PathBuf;
    use std::sync::Once;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// Fills leafwing's deserialization registry once for this test binary.
    fn register_input_kinds() {
        static INIT: Once = Once::new();
        INIT.call_once(|| {
            App::new().add_plugins(InputManagerPlugin::<PlayerAction>::default());
        });
    }

    fn scratch_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        std::env::temp_dir().join(format!(
            "neon_veil_bindings_{name}_{}_{nanos}",
            std::process::id()
        ))
    }

    #[test]
    fn round_trip_preserves_every_binding() {
        register_input_kinds();
        let dir = scratch_dir("roundtrip");
        for (label, map) in [
            ("kbm", default_kbm_input_map()),
            ("gamepad", default_gamepad_input_map()),
            ("merged", default_input_map()),
        ] {
            let path = dir.join("config").join(format!("{label}.json"));
            save_bindings_to_file(&path, &map).expect("save");
            assert!(path.exists(), "created nested directories");
            assert!(
                !path.with_extension("json.tmp").exists(),
                "temp file renamed"
            );
            let loaded = load_bindings_from_file(&path).expect("load");
            assert_eq!(loaded, map, "{label} bindings changed in the round trip");
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_or_corrupt_files_fall_back() {
        register_input_kinds();
        let dir = scratch_dir("fallback");
        let missing = dir.join("nope.json");
        assert!(load_bindings_from_file(&missing).is_none());
        assert_eq!(load_bindings_or_default(&missing), default_input_map());

        fs::create_dir_all(&dir).expect("dir");
        let corrupt = dir.join("corrupt.json");
        fs::write(&corrupt, "{ not json").expect("write");
        assert!(load_bindings_from_file(&corrupt).is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn default_path_is_under_config() {
        assert_eq!(
            Path::new(DEFAULT_BINDINGS_PATH).parent(),
            Some(Path::new("config"))
        );
    }
}
