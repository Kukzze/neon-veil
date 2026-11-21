//! Native Steamworks service (feature `steam`).

use crate::achievements::find_achievement;
use crate::mock::{is_valid_cloud_filename, PlatformService, MAX_CLOUD_SAVE_BYTES};
use bevy::prelude::*;
use std::io::{Read, Write};
use std::sync::{Mutex, PoisonError};
use steamworks::{AppId, Client, SingleClient};

pub struct SteamPlatformService {
    client: Client,
}

/// Steam's callback pump; must run every frame on one thread.
#[derive(Resource)]
pub struct SteamCallbacks(Mutex<SingleClient>);

impl SteamPlatformService {
    /// Connects to the running Steam client. Fails when Steam is not
    /// running or the user does not own `app_id`.
    pub fn init(app_id: u32) -> Result<(Self, SteamCallbacks), steamworks::SteamAPIInitError> {
        let (client, single) = Client::init_app(AppId(app_id))?;
        client.user_stats().request_current_stats();
        Ok((Self { client }, SteamCallbacks(Mutex::new(single))))
    }
}

impl PlatformService for SteamPlatformService {
    fn unlock_achievement(&self, achievement_id: &str) -> bool {
        if find_achievement(achievement_id).is_none() {
            return false;
        }
        let stats = self.client.user_stats();
        stats.achievement(achievement_id).set().is_ok() && stats.store_stats().is_ok()
    }

    fn is_achievement_unlocked(&self, achievement_id: &str) -> bool {
        self.client
            .user_stats()
            .achievement(achievement_id)
            .get()
            .unwrap_or(false)
    }

    fn update_rich_presence(&self, status: &str) {
        self.client
            .friends()
            .set_rich_presence("status", Some(status));
    }

    fn sync_cloud_save(&self, filename: &str, data: &[u8]) -> bool {
        if !is_valid_cloud_filename(filename) || data.len() > MAX_CLOUD_SAVE_BYTES {
            return false;
        }
        let mut writer = self.client.remote_storage().file(filename).write();
        writer.write_all(data).is_ok() && writer.flush().is_ok()
    }

    fn load_cloud_save(&self, filename: &str) -> Option<Vec<u8>> {
        let file = self.client.remote_storage().file(filename);
        if !file.exists() {
            return None;
        }
        let mut data = Vec::new();
        file.read().read_to_end(&mut data).ok()?;
        Some(data)
    }

    fn backend_name(&self) -> &'static str {
        "steam"
    }
}

pub fn run_steam_callbacks(callbacks: Res<SteamCallbacks>) {
    callbacks
        .0
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .run_callbacks();
}
