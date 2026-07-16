# sc2reader-rs

A learning port of [sc2reader](https://github.com/ggtracker/sc2reader) (Python) to Rust, written **from scratch** — without using existing MPQ-parsing crates — with the explicit goal of learning Rust through a real project with a well-defined scope.

## Project goal

Build a StarCraft II replay (`.SC2Replay`) parser functionally equivalent to sc2reader, validating each step against the real output of the original Python library as a correctness "oracle".

This isn't meant to outperform sc2reader or to be production-ready — it's a Rust learning vehicle: binary parsing, idiomatic error handling, simple cryptography, domain modeling with `struct`/`enum`, macros, and organizing a crate into modules.

## Current status

🚧 Actively in development. **Phase 1 (MPQ container) complete.** Phase 2 (SC2 event protocol): `replay.details` and `replay.tracker.events` both decoding end-to-end against real replays.

### Architecture change: extracting `mpq-parser`

MPQ container parsing (which isn't specific to StarCraft II — it's a generic Blizzard format) was extracted into its own independent, published library: **[mpq-parser](https://crates.io/crates/mpq-parser)** ([repo](https://github.com/aldezex/mpq-parser)).

`sc2reader-rs` depends on `mpq-parser` as a real external dependency (via crates.io), not as in-repo code. This added an unplanned extra bit of learning to the project: managing an independent crate, semantic versioning, and real publishing to the registry.

Now published itself on crates.io as [`sc2reader-rs`](https://crates.io/crates/sc2reader-rs).

### Completed (Phase 1 — MPQ container, in `mpq-parser`)

- [x] `MPQUserData` and `MpqHeader` — MPQ header parsing (V4 format).
- [x] MPQ's own cryptography: crypt table, multi-purpose hash function, stream decryption.
- [x] Hash table and block table, decrypted and typed, verified against real data.
- [x] Internal file lookup by name (`find_file`).
- [x] Extraction with automatic decompression (zlib and bzip2).
- [x] Integration tests with real local fixtures (not distributed, `tests/fixtures/` in `.gitignore`).

See the [mpq-parser README](https://github.com/aldezex/mpq-parser) for the full detail of this phase.

### Completed (Phase 2, part 1 — `replay.details`)

- [x] **`VersionedDecoder` primitives** (`protocol.rs`): `read_vint`, `read_blob`, `read_optional`, `read_array`, `read_struct`, `read_u8`/`read_tagged_int` (generic tagged-integer reading, covering `u8`/`u32`/`u64`/`vint`), `read_choice_as_int` (decodes `SVarUint32`-style `choice` values, used for tracker event gameloop deltas), and `skip_value` (recursive skip of any tagged value).
- [x] **`replay.details` decoding** (`details.rs`, `player.rs`): map name and player list (name + race).
- [x] **In-game text markup formatting** (`format.rs`): resolves SC2's name markup into plain text, using `regex`.

**Scope decision:** `SDetails` has ~18 fields; only `m_playerList` and `m_title` are decoded — the rest (map speed, timestamps, etc.) are effectively constant for this project's target use case (1v1 ladder replays) and intentionally skipped.

### Completed (Phase 2, part 2 — `replay.tracker.events`)

- [x] **Full event stream decoding** (`events.rs`): all 10 `NNet.Replay.Tracker.*Event` types — `PlayerStats`, `UnitBorn`, `UnitDied`, `UnitOwnerChange`, `UnitTypeChange`, `Upgrade`, `UnitInit`, `UnitDone`, `UnitPositions`, `PlayerSetup` — modeled as a `TrackerEvent` enum, each variant carrying the gameloop it occurred at.
- [x] **`SPlayerStatsEvent`'s 39-field economy/army snapshot** (`PlayerStats`), decoded via a purpose-built `read_int_fields!` macro (`macros.rs`) instead of 39 hand-written `match` arms.
- [x] **Gameloop-delta + event-id stream orchestration** (`decode_tracker_events`), verified end-to-end against a real replay (513 events decoded, starting with real `PlayerSetup` and `Upgrade` events matching the actual match).

**Key insight that shaped this phase:** for the `VersionedDecoder` encoding (used by both `replay.details` and `replay.tracker.events`), the bit-width/offset parameters attached to types in Blizzard's `typeinfos` tables (e.g. `_int(0,8)`) are irrelevant — only the runtime type tag byte matters. This is what makes tag-generic helpers like `read_tagged_int` and `skip_value` possible without modeling every type's exact parameters.

### In progress / next up

- [ ] `replay.game.events` — uses the *other* encoding mode (`BitPackedDecoder`, untagged/positional), not yet started. Field bit-widths *do* matter for this decoder, unlike `VersionedDecoder`.
- [ ] Higher-level analysis built on top of decoded events (build order reconstruction, resource efficiency, engagement detection) — the original motivation for this whole project.

### Pending

See [`plan-sc2reader-rust.md`](./plan-sc2reader-rust.md) for the full milestone plan (Phases 3-5: domain layer, datapacks, robustness).

## Project structure

```
sc2reader-rs/
├── src/
│   ├── lib.rs          # declares the crate's public modules
│   ├── macros.rs         # read_int_fields! and other decoding-boilerplate macros
│   ├── protocol.rs      # generic VersionedDecoder primitives (read_vint, read_struct, skip_value, ...)
│   ├── details.rs        # replay.details decoding (SDetails)
│   ├── player.rs         # Player domain type
│   ├── events.rs          # replay.tracker.events decoding (TrackerEvent, PlayerStats)
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
- **Split into two crates.** The MPQ container is a generic Blizzard format, not specific to SC2 — it was extracted into `mpq-parser` as an independent library and project, published on crates.io.
- **`Result<T, E>` instead of panics** throughout the parsing and extraction logic (inside `mpq-parser`). Panics (`.expect()`) are reserved for the debug binary (`inspect.rs`) and for genuinely unrecoverable protocol errors (e.g. `skip_value` on an unsupported tag).
- **Named constants for offsets** instead of magic numbers in slice ranges, so the code stays readable without the MPQ spec open next to it.
- **`thiserror`** to generate `Display`/`Error` for the custom error types.
- **Incrementally supported compression**: zlib and bzip2 (the two methods observed in real data), with an explicit error for any other method.
- **Integration tests with local, unversioned fixtures** (`tests/fixtures/`, in `.gitignore`).
- **Fields decoded on a need basis.** Rather than modeling every field of every `SDetails`/event struct upfront, only fields actually useful for the project's goal are decoded; everything else is explicitly skipped (`skip_value`) to keep the byte stream aligned.
- **`regex` for in-game text markup**, instead of chained `.replace()` calls.
- **A small `macro_rules!` macro for repetitive field decoding**, used specifically where hand-writing every `match` arm would add volume without adding clarity (`PlayerStats`'s 39 fields). Not used elsewhere — most structs are small enough that explicit `match` arms are more readable than a macro invocation.

## Resources used

- [sc2reader (Python)](https://github.com/ggtracker/sc2reader) — de facto specification of the behavior being replicated.
- [Blizzard/s2protocol](https://github.com/Blizzard/s2protocol) — reference for the event serialization protocol; per-build protocol definitions used to resolve field layouts for both `SDetails` and tracker events.
- Community documentation on the MPQ format (StormLib / modding wiki) for the container and its cryptography.
- [mpq-parser](https://github.com/aldezex/mpq-parser) — own library (sibling crate) for MPQ container parsing.
- [nom-mpq](https://lib.rs/crates/nom-mpq) — MPQ parser used by `s2protocol`, with a different approach (parser combinators via `nom`); interesting reference, not used as a dependency.
