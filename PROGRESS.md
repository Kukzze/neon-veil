# NEON VEIL: ZERO PROTOCOL // BUILD PROGRESS

- Current Active Phase: Phase 3 (Core Parkour Locomotion Engine)
- Last Verified Commit: 14b90e3 (Phase 1); Phase 2 is the commit following it, see `git log`
- Passing Test Count: 78 (game_core 37, game_world 9, game_combat 17, game_dev_tools 15; 0 failed)
- Active Workstream: Phase 2 complete. Next: game_parkour state machine, kinematics and input buffers.
- Blockers / Notes: none blocking. See the phase log below.
- Next Immediate Action: Implement `crates/game_parkour/src/state_machine.rs`, then `cargo test -p game_parkour --lib kinematics`.

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

### Phase 1: Shared Core ECS Primitives, Math Sanitation & Protocol (COMPLETE, commit `14b90e3`)
- `math.rs`: `sanitize_f32`, `sanitize_vec3` (per-component NaN/Inf -> 0.0), `sanitize_vec3_or` (whole-vector fallback, for positions), `sanitize_quat`, `safe_normalize`, `clamp_velocity`, `wrap_angle_radians` ([-PI, PI)), `hermite_interpolate` (t clamped, output sanitized).
- `time.rs`: `NETWORK_TICK_RATE_HZ = 64`, `FIXED_TIMESTEP_SECS`, `SERVER_TICK_DURATION_SECS`, exact `FIXED_TIMESTEP: Duration` (15.625 ms), `NetworkTick` with wrapping `next`/`prev`, rollover-safe signed `diff`, saturating `duration_since`.
- `identifiers.rs`: `PlayerId(Uuid)`, `NetworkEntityId(u64)`, `ItemId(u32)`, `QuestId(u32)`; reflected components `PlayerCharacterMarker` and `NetworkAuthority` (default `ServerAuthoritative`).
- `protocol.rs`: `PlayerInputPacket` with `InputActions` bit flags, `EntityStateSnapshot`, `ServerStateSnapshot`, `ClientMessage`, `ServerMessage`, `PROTOCOL_VERSION`, bitcode `encode`/`decode` returning `ProtocolError` (no panics on hostile bytes), `sanitized()` for client input and snapshots. `NetworkTick` re-exported for the `game_core::protocol::NetworkTick` path.
- `lib.rs`: `CoreGameplayPlugin` sets `Time<Fixed>` to 64 Hz, adds the `CurrentNetworkTick` resource (advanced in `FixedFirst`), chains `CoreSimulationSet::{Input, Simulate, Sanitize}` in `FixedUpdate`, registers reflection.
- game_core now enables bevy's `serialize` feature (needed for serde on `Vec2`/`Vec3`).
- API note for later phases: design docs call `sanitize_vec3(pos, Vec3::ZERO)` (two args). That form is `sanitize_vec3_or` here; `sanitize_vec3` takes one argument.
- Decisions: `Handshake` carries `protocol_version`; `HandshakeAck` carries `player_id` and `server_tick`. Wire format is bitcode (the spec's protocol codec); round trips are also verified through serde_json.
- Gates: `cargo test -p game_core` 37/37, `cargo clippy -p game_core --all-targets -- -D warnings` clean, `cargo fmt --all -- --check` clean, `cargo check --workspace --locked` clean.

### Phase 2: Procedural Greybox Physics Playground & In-Game Dev Gym (COMPLETE)
- `game_world::greybox_gym`: the gym is pure data (`greybox_gym_layout()`) spawned by `setup_greybox_gym`. Ground slab 200x0.2x200 (top at y=0); Low/Mid/High vaults (0.75/1.25/1.80 m at z=-10/-20/-30); 25 m wall-run walls at x=+-3; 35-degree slide ramp; 40 m drop tower with a +X overhang and a `LandingCushion` sensor below it; gap bed at x=40 with 3/5/8/12/16 m chasms between 3 m tall pads. `GreyboxMaterials` resource; reflected `GreyboxGymMarker`, `ParkourObstacle`, `ObstacleType` (incl. `DropTower`). `GreyboxGymPlugin` is opt-in: add it only under `--greybox`.
- `game_combat::combat_dummy`: `CombatDummy` (100k HP, 100 posture, posture break flag, `home_position`), `CombatDummyType`, `FloatingCombatText`, `DummyDamageEvent`; `apply_dummy_damage` (sanitized amounts, health floor 0, break reported once), `calculate_dps`, `tick_dummy`; parry telegraph phases (yellow at <= 300 ms, red at <= 120 ms) with material swap; sine strafing; 5 s idle reset. `CombatDummyPlugin` is opt-in and spawns the arena.
- `game_dev_tools`: `LivePhysicsTuning` with one bounds table shared by the sliders and `clamp_to_bounds` (NaN -> default); `DevToolsState` (open by default in debug builds, F1 toggle, bounded console history); `render_dev_gui` (sliders, overlay toggles, frame-time profiler plot, console); `parse_console_command` (`/teleport|tp x y z`, `/godmode`, `/set_skill <name> <1-99>`, `/heal`; rejects NaN/inf coordinates); `parse_and_execute_console_command` applies teleport/godmode and broadcasts every command as `ConsoleCommandEvent`; `DebugVisualizerConfig`, bounded `DebugShapecastQueue`, `draw_debug_gizmos` (green = clear, red = contact, dummy hurtbox capsules). `DevToolsPlugin` adds `EguiPlugin` and `FrameTimeDiagnosticsPlugin` if missing.
- Spec fixes:
  - Dummy capsule: the spec's `Capsule3d::new(0.4, 1.8)` at y=0.9 is a 2.6 m capsule sunk 0.4 m into the ground (bevy/avian take the cylinder segment length). Now radius 0.4 + segment 1.0 = 1.8 m total, standing on y=0.
  - `combat_timer`: the spec's `Timer::from_seconds(0.0, Repeating)` never accumulates elapsed time, so the auto-heal could never fire. Now a 5 s `Once` timer reset on every hit; engagement length for DPS is a separate `combat_duration: Stopwatch`. DPS uses a 1 s minimum window to avoid single-hit spikes.
  - `egui_plot` pinned to 0.29.0: 0.30 targets egui 0.30, but bevy_egui 0.31.1 uses egui 0.29, so plots could not render in bevy_egui's context.
  - `bevy_gizmos` feature enabled for game_dev_tools (missing from the workspace feature list).
- Toolchain fix: `libredox` pinned to 0.1.10. 0.1.25 pulled `redox_syscall 0.9.5`, whose manifest sets `resolver = "3"`, which cargo 1.82 rejects. `cargo check` never downloaded it, but `cargo fetch` and non-Windows builds would have failed. The lockfile compatibility scan must also check for `resolver = "3"`. cargo 1.82 `cargo fetch` (all targets) now succeeds.
- Deferred to later phases: on-screen FCT rendering (HUD), `/heal` and `/set_skill` handlers (already broadcast), god_mode/noclip effects (parkour reads the flags), shapecast producers (Phase 4 probes), LivePhysicsTuning -> parkour constants sync.
- Gates: game_world 9/9, game_combat 17/17, game_dev_tools 15/15 (incl. a headless full-plugin smoke test), game_core 37/37 regression, `cargo clippy --workspace --all-targets -- -D warnings` clean, `cargo fmt --all -- --check` clean, `cargo check --workspace --locked` clean.
