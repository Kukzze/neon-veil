# NEON VEIL: ZERO PROTOCOL // BUILD PROGRESS

- Current Active Phase: Phase 1 (Shared Core ECS Primitives, Math Sanitation & Protocol)
- Last Verified Commit: none (repository initialized, Phase 0 not yet committed)
- Passing Test Count: 0 (no tests exist yet)
- Active Workstream: Phase 0 complete — 15-crate Cargo workspace, toolchain 1.82.0, `cargo check --workspace --locked` and `cargo clippy --workspace -- -D warnings` pass with zero warnings.
- Blockers / Notes:
  - Spec deviation: `renet_netcode = "0.0.16"` does not exist on crates.io (first release is 1.0.0). Netcode for renet 0.0.16 is enabled via `renet = { features = ["transport"] }`; `renet_netcode` removed from game_client/game_server.
  - Spec deviation: `steamworks` cannot be `optional` inside `[workspace.dependencies]`; optionality kept at crate level (game_steam).
  - Spec deviation: `rust-toolchain.toml` profile `complete` is unsatisfiable on stable (miri/cranelift are nightly-only); changed to `default`. rustfmt, clippy, rust-analyzer and both targets are installed.
  - Spec deviation: lint flags moved into each `[target.<triple>]` rustflags table in `.cargo/config.toml`, because Cargo ignores `build.rustflags` whenever target rustflags are set.
  - Added `rust-version.workspace = true` to every crate so the MSRV is enforced.
  - Cargo.lock is pinned for rustc 1.82 compatibility. Manually downgraded: blake3 1.5.5, constant_time_eq 0.3.1, avian_derive 0.2.0, coreaudio-sys 0.2.16, stackfuture 0.3.0, idna_adapter 1.1.0 (removes the ICU 2.x stack, which needs 1.83+). Do NOT run a bare `cargo update`; regenerate with `CARGO_RESOLVER_INCOMPATIBLE_RUST_VERSIONS=fallback cargo +stable update` and re-verify.
  - Watch item: `renet_visualizer 0.0.3` (bevy feature) targets an older bevy_ecs; it is declared but unused. Revisit before any crate depends on it.
  - Watch item: `target-cpu=native` produces binaries that may not run on older CPUs; override for Steam/itch release builds.
- Next Immediate Action: Implement `crates/game_core/src/math.rs` (sanitize_vec3, wrap_angle_radians, hermite interpolation), then `cargo test -p game_core`.
