# sc2reader-rs

A learning port of [sc2reader](https://github.com/ggtracker/sc2reader) (Python) to Rust, written **from scratch** — without using existing MPQ-parsing crates — with the explicit goal of learning Rust through a real project with a well-defined scope.

## Project goal

Build a StarCraft II replay (`.SC2Replay`) parser functionally equivalent to sc2reader, validating each step against the real output of the original Python library as a correctness "oracle".

This isn't meant to outperform sc2reader or to be production-ready — it's a Rust learning vehicle: binary parsing, idiomatic error handling, simple cryptography, domain modeling with `struct`/`enum`, and organizing a crate into modules.

## Current status

🚧 Actively in development. **Phase 1 (MPQ container) complete** — starting Phase 2 (SC2 event protocol).

### Architecture change: extracting `mpq-parser`

MPQ container parsing (which isn't specific to StarCraft II — it's a generic Blizzard format) was extracted into its own independent, published library: **[mpq-parser](https://crates.io/crates/mpq-parser)** ([repo](https://github.com/aldezex/mpq-parser)).

`sc2reader-rs` depends on `mpq-parser` as a real external dependency (via crates.io), not as in-repo code. This added an unplanned extra bit of learning to the project: managing an independent crate, semantic versioning, and real publishing to the registry.

### Completed (Phase 1 — MPQ container, in `mpq-parser`)

- [x] `MPQUserData` and `MpqHeader` — MPQ header parsing (V4 format).
- [x] MPQ's own cryptography: crypt table, multi-purpose hash function, stream decryption.
- [x] Hash table and block table, decrypted and typed, verified against real data.
- [x] Internal file lookup by name (`find_file`).
- [x] Extraction with automatic decompression (zlib and bzip2).
- [x] Integration tests with real local fixtures (not distributed, `tests/fixtures/` in `.gitignore`).

See the [mpq-parser README](https://github.com/aldezex/mpq-parser) for the full detail of this phase.

### In progress

- [ ] **Phase 2 — Protocol deserialization**: interpreting the already-extracted contents of `replay.details`, `replay.tracker.events`, and `replay.game.events` as meaningful data (map, players, build order, game events). Unlike the MPQ container, the protocol is versioned by game build — scope decision: support recent versions only, not the game's entire history.

### Pending

See [`plan-sc2reader-rust.md`](./plan-sc2reader-rust.md) for the full milestone plan (Phases 3-5: domain layer, datapacks, robustness).

## Project structure

```
sc2reader-rs/
├── src/
│   ├── lib.rs          # declares the crate's public modules
│   └── bin/
│       └── inspect.rs   # debug binary: loads a replay and explores its structure,
│                          using mpq-parser (external dependency) for the MPQ container
├── fixtures/             # real .SC2Replay files used for manual testing
└── plan-sc2reader-rust.md
```

MPQ container parsing itself lives in the separate [mpq-parser](https://github.com/aldezex/mpq-parser) crate, not in this repo.

## Design decisions

- **No third-party MPQ-parsing crates.** The MPQ container is implemented by hand in `mpq-parser` (unlike `s2protocol-rs`, which does use existing libraries) because the goal is to learn, not to move fast.
- **Split into two crates.** The MPQ container is a generic Blizzard format, not specific to SC2 — it was extracted into `mpq-parser` as an independent library and project, published on crates.io, to avoid unnecessarily coupling two distinct concerns (container format vs. a specific game's replay protocol).
- **`Result<T, E>` instead of panics** throughout the parsing and extraction logic (inside `mpq-parser`). Panics (`.expect()`) are reserved for the debug binary (`inspect.rs`), where failing loudly is acceptable.
- **Named constants for offsets** instead of magic numbers in slice ranges, so the code stays readable without the MPQ spec open next to it.
- **`thiserror`** to generate `Display`/`Error` for the custom error types, after implementing both by hand once to understand what they do.
- **Incrementally supported compression**: zlib and bzip2 (the two methods observed in real data), with an explicit error for any other method instead of trying to cover the full spec upfront.
- **Integration tests with local, unversioned fixtures** (`tests/fixtures/`, in `.gitignore`) to verify against real replays without publishing a new version on every iteration.

## Resources used

- [sc2reader (Python)](https://github.com/ggtracker/sc2reader) — de facto specification of the behavior being replicated.
- [Blizzard/s2protocol](https://github.com/Blizzard/s2protocol) — reference for the event serialization protocol.
- Community documentation on the MPQ format (StormLib / modding wiki) for the container and its cryptography.
- [mpq-parser](https://github.com/aldezex/mpq-parser) — own library (sibling crate) for MPQ container parsing.
- [nom-mpq](https://lib.rs/crates/nom-mpq) — MPQ parser used by `s2protocol`, with a different approach (parser combinators via `nom`); interesting reference, not used as a dependency.
