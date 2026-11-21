//! Player persistence on SQLite (offline play, tests) or PostgreSQL (hosted
//! servers). Queries are built at runtime, so compiling never needs a
//! database. A save writes all three tables in one transaction.

use game_core::identifiers::PlayerId;
use game_economy::PlayerInventory;
use game_rpg::{CharacterSkills, SkillType};
use sqlx::postgres::{PgPool, PgPoolOptions};
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use sqlx::Row;
use uuid::Uuid;

const SQLITE_SCHEMA: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS players (
        player_id TEXT PRIMARY KEY,
        username TEXT NOT NULL,
        credits INTEGER NOT NULL DEFAULT 1000,
        created_at TEXT DEFAULT CURRENT_TIMESTAMP
    )",
    "CREATE TABLE IF NOT EXISTS player_skills (
        player_id TEXT PRIMARY KEY REFERENCES players(player_id) ON DELETE CASCADE,
        xp_data TEXT NOT NULL,
        updated_at TEXT DEFAULT CURRENT_TIMESTAMP
    )",
    "CREATE TABLE IF NOT EXISTS player_inventory (
        player_id TEXT PRIMARY KEY REFERENCES players(player_id) ON DELETE CASCADE,
        inventory_data TEXT NOT NULL,
        updated_at TEXT DEFAULT CURRENT_TIMESTAMP
    )",
];

const POSTGRES_SCHEMA: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS players (
        player_id TEXT PRIMARY KEY,
        username TEXT NOT NULL,
        credits BIGINT NOT NULL DEFAULT 1000,
        created_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP
    )",
    "CREATE TABLE IF NOT EXISTS player_skills (
        player_id TEXT PRIMARY KEY REFERENCES players(player_id) ON DELETE CASCADE,
        xp_data TEXT NOT NULL,
        updated_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP
    )",
    "CREATE TABLE IF NOT EXISTS player_inventory (
        player_id TEXT PRIMARY KEY REFERENCES players(player_id) ON DELETE CASCADE,
        inventory_data TEXT NOT NULL,
        updated_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP
    )",
];

// Placeholders are `$N`, which both drivers accept.
const UPSERT_PLAYER: &str = "INSERT INTO players (player_id, username, credits) VALUES ($1, $2, $3)
    ON CONFLICT (player_id) DO UPDATE SET username = excluded.username, credits = excluded.credits";
const UPSERT_SKILLS: &str = "INSERT INTO player_skills (player_id, xp_data, updated_at) VALUES ($1, $2, CURRENT_TIMESTAMP)
    ON CONFLICT (player_id) DO UPDATE SET xp_data = excluded.xp_data, updated_at = CURRENT_TIMESTAMP";
const UPSERT_INVENTORY: &str = "INSERT INTO player_inventory (player_id, inventory_data, updated_at) VALUES ($1, $2, CURRENT_TIMESTAMP)
    ON CONFLICT (player_id) DO UPDATE SET inventory_data = excluded.inventory_data, updated_at = CURRENT_TIMESTAMP";
const SELECT_PLAYER: &str = "SELECT p.username, p.credits, s.xp_data, i.inventory_data
    FROM players p
    LEFT JOIN player_skills s ON s.player_id = p.player_id
    LEFT JOIN player_inventory i ON i.player_id = p.player_id
    WHERE p.player_id = $1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatabaseBackend {
    Sqlite,
    Postgres,
}

#[derive(Debug, Clone)]
pub enum DatabasePool {
    Sqlite(SqlitePool),
    Postgres(PgPool),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerSaveRecord {
    pub player_id: PlayerId,
    pub username: String,
    pub credits: i64,
    pub skills: CharacterSkills,
    pub inventory: PlayerInventory,
}

fn decode_error(error: serde_json::Error) -> sqlx::Error {
    sqlx::Error::Decode(Box::new(error))
}

fn skills_from_xp(xp: [u32; game_rpg::skills::SKILL_COUNT]) -> CharacterSkills {
    let mut skills = CharacterSkills::default();
    for skill in SkillType::ALL {
        skills.add_xp(skill, xp[skill.index()]);
    }
    skills
}

/// Runs the same body against whichever pool is configured.
macro_rules! with_pool {
    ($pool:expr, $p:ident => $body:expr) => {
        match $pool {
            DatabasePool::Sqlite($p) => $body,
            DatabasePool::Postgres($p) => $body,
        }
    };
}

#[derive(Debug, Clone)]
pub struct PersistenceEngine {
    pool: DatabasePool,
}

impl PersistenceEngine {
    pub fn from_pool(pool: DatabasePool) -> Self {
        Self { pool }
    }

    /// Private in-memory SQLite database. One connection, so every query
    /// sees the same database for the engine's lifetime.
    pub async fn new_in_memory_sqlite() -> Result<Self, sqlx::Error> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .idle_timeout(None)
            .max_lifetime(None)
            .connect("sqlite::memory:")
            .await?;
        Ok(Self::from_pool(DatabasePool::Sqlite(pool)))
    }

    /// File-backed SQLite for offline saves, e.g. `sqlite://saves/slot1.db?mode=rwc`.
    pub async fn connect_sqlite(url: &str) -> Result<Self, sqlx::Error> {
        let pool = SqlitePoolOptions::new().connect(url).await?;
        Ok(Self::from_pool(DatabasePool::Sqlite(pool)))
    }

    pub async fn connect_postgres(url: &str) -> Result<Self, sqlx::Error> {
        let pool = PgPoolOptions::new()
            .max_connections(16)
            .connect(url)
            .await?;
        Ok(Self::from_pool(DatabasePool::Postgres(pool)))
    }

    pub fn backend(&self) -> DatabaseBackend {
        match self.pool {
            DatabasePool::Sqlite(_) => DatabaseBackend::Sqlite,
            DatabasePool::Postgres(_) => DatabaseBackend::Postgres,
        }
    }

    pub async fn init_schema(&self) -> Result<(), sqlx::Error> {
        let statements = match self.backend() {
            DatabaseBackend::Sqlite => SQLITE_SCHEMA,
            DatabaseBackend::Postgres => POSTGRES_SCHEMA,
        };
        with_pool!(&self.pool, pool => {
            for statement in statements {
                sqlx::query(statement).execute(pool).await?;
            }
            Ok(())
        })
    }

    /// Creates or overwrites the player's row, skills and inventory
    /// atomically.
    pub async fn save_player_state(
        &self,
        player_id: PlayerId,
        username: &str,
        credits: i64,
        skills: &CharacterSkills,
        inventory: &PlayerInventory,
    ) -> Result<(), sqlx::Error> {
        let id = player_id.0.to_string();
        let xp_json = serde_json::to_string(&skills.xp).map_err(decode_error)?;
        let inventory_json = serde_json::to_string(inventory).map_err(decode_error)?;
        with_pool!(&self.pool, pool => {
            let mut tx = pool.begin().await?;
            sqlx::query(UPSERT_PLAYER)
                .bind(&id)
                .bind(username)
                .bind(credits)
                .execute(&mut *tx)
                .await?;
            sqlx::query(UPSERT_SKILLS)
                .bind(&id)
                .bind(&xp_json)
                .execute(&mut *tx)
                .await?;
            sqlx::query(UPSERT_INVENTORY)
                .bind(&id)
                .bind(&inventory_json)
                .execute(&mut *tx)
                .await?;
            tx.commit().await
        })
    }

    pub async fn load_player_state(
        &self,
        player_id: PlayerId,
    ) -> Result<Option<PlayerSaveRecord>, sqlx::Error> {
        let id = player_id.0.to_string();
        let row: Option<(String, i64, Option<String>, Option<String>)> = with_pool!(&self.pool, pool => {
            sqlx::query(SELECT_PLAYER)
                .bind(&id)
                .fetch_optional(pool)
                .await?
                .map(|row| -> Result<_, sqlx::Error> {
                    Ok((
                        row.try_get("username")?,
                        row.try_get("credits")?,
                        row.try_get("xp_data")?,
                        row.try_get("inventory_data")?,
                    ))
                })
                .transpose()?
        });
        let Some((username, credits, xp_json, inventory_json)) = row else {
            return Ok(None);
        };
        let skills = match xp_json {
            Some(json) => skills_from_xp(serde_json::from_str(&json).map_err(decode_error)?),
            None => CharacterSkills::default(),
        };
        let inventory = match inventory_json {
            Some(json) => serde_json::from_str(&json).map_err(decode_error)?,
            None => PlayerInventory::default(),
        };
        Ok(Some(PlayerSaveRecord {
            player_id,
            username,
            credits,
            skills,
            inventory,
        }))
    }

    pub async fn delete_player(&self, player_id: PlayerId) -> Result<bool, sqlx::Error> {
        let id = player_id.0.to_string();
        with_pool!(&self.pool, pool => {
            let mut tx = pool.begin().await?;
            for table in ["player_skills", "player_inventory"] {
                sqlx::query(&format!("DELETE FROM {table} WHERE player_id = $1"))
                    .bind(&id)
                    .execute(&mut *tx)
                    .await?;
            }
            let removed = sqlx::query("DELETE FROM players WHERE player_id = $1")
                .bind(&id)
                .execute(&mut *tx)
                .await?
                .rows_affected();
            tx.commit().await?;
            Ok(removed > 0)
        })
    }

    pub async fn player_count(&self) -> Result<i64, sqlx::Error> {
        with_pool!(&self.pool, pool => {
            let row = sqlx::query("SELECT COUNT(*) AS n FROM players").fetch_one(pool).await?;
            row.try_get::<i64, _>("n")
        })
    }
}

/// Parses a stored id.
pub fn parse_player_id(text: &str) -> Option<PlayerId> {
    Uuid::parse_str(text).ok().map(PlayerId)
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_rpg::xp_table::level_to_xp;

    async fn engine() -> PersistenceEngine {
        let engine = PersistenceEngine::new_in_memory_sqlite()
            .await
            .expect("sqlite");
        engine.init_schema().await.expect("schema");
        engine
    }

    fn veteran() -> (CharacterSkills, PlayerInventory) {
        let mut skills = CharacterSkills::default();
        skills.add_xp(SkillType::Agility, level_to_xp(45));
        skills.add_xp(SkillType::Netrunning, 123_456);
        let mut inventory = PlayerInventory::new();
        inventory.add_item(1001, 250).unwrap();
        inventory.add_item(2033, 1).unwrap();
        inventory.add_item(1021, 12).unwrap();
        (skills, inventory)
    }

    #[tokio::test]
    async fn save_and_load_roundtrip() {
        let db = engine().await;
        assert_eq!(db.backend(), DatabaseBackend::Sqlite);
        let player = PlayerId::new_random();
        let (skills, inventory) = veteran();
        db.save_player_state(player, "neon_runner", 50_000, &skills, &inventory)
            .await
            .expect("save");
        let loaded = db
            .load_player_state(player)
            .await
            .expect("load")
            .expect("exists");
        assert_eq!(loaded.username, "neon_runner");
        assert_eq!(loaded.credits, 50_000);
        assert_eq!(loaded.skills.xp, skills.xp);
        assert_eq!(loaded.skills.get_level(SkillType::Agility), 45);
        assert_eq!(loaded.skills, skills, "boosted levels rebuilt from xp");
        assert_eq!(loaded.inventory, inventory);
        assert_eq!(loaded.inventory.item_count(1001), 250);
    }

    #[tokio::test]
    async fn saving_again_overwrites() {
        let db = engine().await;
        let player = PlayerId::new_random();
        let (mut skills, mut inventory) = veteran();
        db.save_player_state(player, "neon_runner", 50_000, &skills, &inventory)
            .await
            .unwrap();
        skills.add_xp(SkillType::Agility, 1_000_000);
        inventory.remove_item(1001, 250).unwrap();
        db.save_player_state(player, "neon_runner_v2", 42, &skills, &inventory)
            .await
            .unwrap();
        let loaded = db.load_player_state(player).await.unwrap().unwrap();
        assert_eq!(
            (loaded.username.as_str(), loaded.credits),
            ("neon_runner_v2", 42)
        );
        assert_eq!(loaded.skills.xp, skills.xp);
        assert_eq!(loaded.inventory.item_count(1001), 0);
        assert_eq!(db.player_count().await.unwrap(), 1);
    }

    #[tokio::test]
    async fn unknown_and_deleted_players() {
        let db = engine().await;
        assert!(db
            .load_player_state(PlayerId::new_random())
            .await
            .unwrap()
            .is_none());
        let player = PlayerId::new_random();
        let (skills, inventory) = veteran();
        db.save_player_state(player, "a", 1, &skills, &inventory)
            .await
            .unwrap();
        assert!(db.delete_player(player).await.unwrap());
        assert!(!db.delete_player(player).await.unwrap());
        assert!(db.load_player_state(player).await.unwrap().is_none());
        assert_eq!(db.player_count().await.unwrap(), 0);
    }

    #[tokio::test]
    async fn schema_init_is_idempotent_and_players_are_isolated() {
        let db = engine().await;
        db.init_schema().await.expect("second init");
        let (a, b) = (PlayerId::new_random(), PlayerId::new_random());
        let (skills, inventory) = veteran();
        db.save_player_state(a, "a", 10, &skills, &inventory)
            .await
            .unwrap();
        db.save_player_state(
            b,
            "b",
            20,
            &CharacterSkills::default(),
            &PlayerInventory::new(),
        )
        .await
        .unwrap();
        let loaded_b = db.load_player_state(b).await.unwrap().unwrap();
        assert_eq!(loaded_b.credits, 20);
        assert_eq!(loaded_b.skills, CharacterSkills::default());
        assert!(loaded_b.inventory.items().is_empty());
        assert_eq!(parse_player_id(&a.0.to_string()), Some(a));
        assert_eq!(parse_player_id("not-a-uuid"), None);
    }
}
