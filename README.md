# sc2reader-rs

A learning port of [sc2reader](https://github.com/ggtracker/sc2reader) (Python) to Rust, written **from scratch** — without using existing MPQ-parsing crates — with the explicit goal of learning Rust through a real project with a well-defined scope.

## Project goal

Build a StarCraft II replay (`.SC2Replay`) parser functionally equivalent to sc2reader, validating each step against the real output of the original Python library as a correctness "oracle".

This isn't meant to outperform sc2reader or to be production-ready — it's a Rust learning vehicle: binary parsing, idiomatic error handling, simple cryptography, domain modeling with `struct`/`enum`, and organizing a crate into modules.

## Current status

🚧 Actively in development. **Phase 1 (MPQ container) complete** — Phase 2 (SC2 event protocol) underway, with `replay.details` decoding working end-to-end against real replays.

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

### In progress (Phase 2 — SC2 protocol deserialization)

Unlike the MPQ container, this protocol is versioned by game build — scope decision: support recent versions only, not the game's entire history. Field layouts are cross-checked against [Blizzard/s2protocol](https://github.com/Blizzard/s2protocol)'s per-build protocol definitions (currently targeting a build close to `protocol97425`).

- [x] **`VersionedDecoder` primitives** (`protocol.rs`): `read_vint` (variable-length signed integers), `read_blob`, `read_optional`, `read_array`, `read_struct`, and `skip_value` (recursive skip of any tagged value, used to correctly bypass fields not being decoded).
- [x] **`replay.details` decoding** (`details.rs`, `player.rs`): map name and player list (name + race), verified against a real replay — including correctly handling `m_playerList` being an `optional<array<...>>` rather than a bare array, a mismatch first caught by cross-referencing an outdated protocol version against a current one.
- [x] **In-game text markup formatting** (`format.rs`): resolves SC2's name markup (`<sp/>`, escaped `&lt;`/`&gt;`/`&amp;`, embedded color tags) into plain text, using `regex`.
- [ ] Remaining `SDetails` fields (game result, timestamps, etc.).
- [ ] `replay.tracker.events` and `replay.game.events` decoding.

### Pending

See [`plan-sc2reader-rust.md`](./plan-sc2reader-rust.md) for the full milestone plan (Phases 3-5: domain layer, datapacks, robustness).

## Project structure

```
sc2reader-rs/
├── src/
│   ├── lib.rs          # declares the crate's public modules
│   ├── protocol.rs      # generic VersionedDecoder primitives (read_vint, read_struct, skip_value, ...)
│   ├── details.rs        # replay.details decoding (SDetails)
│   ├── player.rs         # Player domain type
│   ├── format.rs         # SC2 in-game text markup formatting
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
- **Fields decoded on a need basis.** Rather than modeling every field of every `SDetails`/event struct upfront, only the fields actually needed are decoded (`match` on field index); everything else is explicitly skipped (`skip_value`) to keep the byte stream aligned without requiring full knowledge of every nested type.
- **`regex` for in-game text markup**, instead of chained `.replace()` calls, since SC2's color tags carry variable hex values that literal string replacement can't match.

## Resources used

- [sc2reader (Python)](https://github.com/ggtracker/sc2reader) — de facto specification of the behavior being replicated.
- [Blizzard/s2protocol](https://github.com/Blizzard/s2protocol) — reference for the event serialization protocol; per-build protocol definitions used to resolve `SDetails` field layout.
- Community documentation on the MPQ format (StormLib / modding wiki) for the container and its cryptography.
- [mpq-parser](https://github.com/aldezex/mpq-parser) — own library (sibling crate) for MPQ container parsing.
- [nom-mpq](https://lib.rs/crates/nom-mpq) — MPQ parser used by `s2protocol`, with a different approach (parser combinators via `nom`); interesting reference, not used as a dependency.
