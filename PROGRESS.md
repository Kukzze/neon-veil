# NEON VEIL: ZERO PROTOCOL // BUILD PROGRESS

- Current Active Phase: Phase 2 (Procedural Greybox Physics Playground & In-Game Dev Gym)
- Last Verified Commit: be88eaa (Phase 0); Phase 1 is the commit following it, see `git log`
- Passing Test Count: 37 (game_core: 37 passed, 0 failed)
- Active Workstream: Phase 1 complete. Next: game_world greybox gym, game_combat dummies, game_dev_tools editor and visualizers.
- Blockers / Notes: none blocking. See the phase log below.
- Next Immediate Action: Implement `crates/game_world/src/greybox_gym.rs`, then `cargo test -p game_world && cargo test -p game_dev_tools`.

## Phase Log

### Phase 0: Workspace Bootstrap & Cargo Configuration (COMPLETE, commit `be88eaa`)
- 15-crate workspace, Rust 1.82.0 toolchain, `cargo check --workspace --locked` and clippy pass with zero warnings.
- Spec deviations:
  - `renet_netcode = "0.0.16"` does not exist on crates.io (first release is 1.0.0). Netcode for renet 0.0.16 is enabled via `renet = { features = ["transport"] }`.
  - `steamworks` cannot be `optional` inside `[workspace.dependencies]`; optionality is declared per crate (game_steam).
  - `rust-toolchain.toml` profile `complete` is unsatisfiable on stable (miri/cranelift are nightly-only); changed to `default`.
  - Lint flags live in each `[target.<triple>]` rustflags table because Cargo ignores `build.rustflags` when target rustflags are set.
  - `rust-version.workspace = true` added to every crate.
- Cargo.lock pinned for rustc 1.82: blake3 1.5.5, constant_time_eq 0.3.1, avian_derive 0.2.0, coreaudio-sys 0.2.16, stackfuture 0.3.0, idna_adapter 1.1.0 (drops the ICU 2.x stack). Do NOT run a bare `cargo update`; use `CARGO_RESOLVER_INCOMPATIBLE_RUST_VERSIONS=fallback cargo +stable update` and re-verify.
- Watch items: `renet_visualizer 0.0.3` (bevy feature) targets an older bevy_ecs and is unused; `target-cpu=native` must be overridden for Steam/itch release builds.

### Phase 1: Shared Core ECS Primitives, Math Sanitation & Protocol (COMPLETE)
- `math.rs`: `sanitize_f32`, `sanitize_vec3` (per-component NaN/Inf -> 0.0), `sanitize_vec3_or` (whole-vector fallback, for positions), `sanitize_quat`, `safe_normalize`, `clamp_velocity`, `wrap_angle_radians` ([-PI, PI)), `hermite_interpolate` (t clamped, output sanitized).
- `time.rs`: `NETWORK_TICK_RATE_HZ = 64`, `FIXED_TIMESTEP_SECS`, `SERVER_TICK_DURATION_SECS`, exact `FIXED_TIMESTEP: Duration` (15.625 ms), `NetworkTick` with wrapping `next`/`prev`, rollover-safe signed `diff`, saturating `duration_since`.
- `identifiers.rs`: `PlayerId(Uuid)`, `NetworkEntityId(u64)`, `ItemId(u32)`, `QuestId(u32)`; reflected components `PlayerCharacterMarker` and `NetworkAuthority` (default `ServerAuthoritative`).
- `protocol.rs`: `PlayerInputPacket` with `InputActions` bit flags, `EntityStateSnapshot`, `ServerStateSnapshot`, `ClientMessage`, `ServerMessage`, `PROTOCOL_VERSION`, bitcode `encode`/`decode` returning `ProtocolError` (no panics on hostile bytes), `sanitized()` for client input and snapshots. `NetworkTick` re-exported for the `game_core::protocol::NetworkTick` path.
- `lib.rs`: `CoreGameplayPlugin` sets `Time<Fixed>` to 64 Hz, adds the `CurrentNetworkTick` resource (advanced in `FixedFirst`), chains `CoreSimulationSet::{Input, Simulate, Sanitize}` in `FixedUpdate`, registers reflection.
- game_core now enables bevy's `serialize` feature (needed for serde on `Vec2`/`Vec3`).
- API note for later phases: design docs call `sanitize_vec3(pos, Vec3::ZERO)` (two args). That form is `sanitize_vec3_or` here; `sanitize_vec3` takes one argument.
- Decisions: `Handshake` carries `protocol_version`; `HandshakeAck` carries `player_id` and `server_tick`. Wire format is bitcode (the spec's protocol codec); round trips are also verified through serde_json.
- Gates: `cargo test -p game_core` 37/37, `cargo clippy -p game_core --all-targets -- -D warnings` clean, `cargo fmt --all -- --check` clean, `cargo check --workspace --locked` clean.
