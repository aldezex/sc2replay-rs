# sc2reader-rs

A learning port of [sc2reader](https://github.com/ggtracker/sc2reader) (Python) to Rust, written **from scratch** — without using existing MPQ-parsing crates — with the explicit goal of learning Rust through a real project with a well-defined scope.

## Project goal

Build a StarCraft II replay (`.SC2Replay`) parser functionally equivalent to sc2reader, validating each step against the real output of the original Python library as a correctness "oracle".

This isn't meant to outperform sc2reader or to be production-ready — it's a Rust learning vehicle: binary parsing, idiomatic error handling, simple cryptography, domain modeling with `struct`/`enum`, macros, and organizing a crate into modules.

## Current status

🚧 Actively in development. **Phase 1 (MPQ container) complete.** Phase 2 (SC2 event protocol): `replay.details`, `replay.tracker.events`, and `replay.game.events` (`SCmdEvent`) all decoding end-to-end against real replays.

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

### Completed (Phase 2, part 3 — `replay.game.events`, `BitPackedDecoder` + `SCmdEvent`)

- [x] **`BitPackedDecoder` bit reader primitives** (`bitpacked.rs`): `read_bits`, `byte_align`, `read_aligned_bytes`, `read_int`, `read_optional`, `read_optional_int`, `read_var_uint32`. Unlike `VersionedDecoder`, there are no type tags here — the `(offset, bits)` parameters are load-bearing, since every field's exact bit width has to be known and hardcoded ahead of time.
- [x] **Bit order verified against Blizzard's actual reference implementation** (`decoders.py`'s `BitPackedBuffer.read_bits`, not assumed): within a byte, bits are consumed low-to-high, but across a byte boundary the *earlier*-consumed byte occupies the *more* significant part of the result (`endian='big'` is `BitPackedDecoder`'s default) — this is **not** the same as flattening the buffer into one little-endian bitstream and slicing. Getting this wrong silently corrupts every field after the first mistake; see `bitpacked.rs`'s unit tests, in particular `reads_across_a_byte_boundary_is_not_ambiguous`, the one test whose expected value actually distinguishes the two models.
- [x] **`SCmdEvent` decoding** (`game_events.rs`, typeid 100, event id 27): `m_cmdFlags`, `m_abil` (`abil_link`/`abil_cmd_index`/`abil_cmd_data`), `m_data`'s 4-way choice (`None`/`TargetPoint`/`TargetUnit`/`Data`, modeled as `CmdData`), `m_sequence`, `m_otherUnit`, `m_unitGroup`. Field layout cross-checked directly against `protocol97425.py`'s `typeinfos`.
- [x] **Generic bit-level skip for unmodeled event types** (`typeinfos.rs` + `game_events.rs::skip_bitpacked_value`): a Rust transcription of `protocol97425.py`'s full `typeinfos` table (209 entries, mechanically generated from the fetched reference file, not hand-derived) plus a recursive interpreter that computes exactly how many bits *any* typeid occupies — including the ~99 other `NNet.Game.*Event` types (camera updates, hotkeys, selections, sync markers, etc.) that aren't individually modeled. This is the untagged-format equivalent of `protocol.rs`'s `skip_value`, except it can't be a blind byte-count skip: `_optional`/`_choice`/`_array`/`_blob`/`_bitarray` all require actually decoding a presence bit, selector, or count to know how much more to skip.
- [x] **Gameloop-delta + userid + event-id stream orchestration** (`decode_game_events`), verified end-to-end against a full, real 1v1 ladder replay: the entire `replay.game.events` stream decodes successfully (hundreds of `SCmdEvent`s extracted, gameloops strictly increasing, alternating between both players, `TargetPoint`/`TargetUnit` data consistent with real move/attack commands), landing exactly on `bytes.len()` with no leftover bits and no panics.
- [x] **`SSelectionDeltaEvent`/`SControlGroupUpdateEvent` decoding** (`game_events.rs`, typeids 109/110, event ids 28/29) — added specifically to unblock a downstream consumer (`sc2trainer`'s idle-production-time analysis) that needs to know *which unit tags a player had selected* at the moment a command was issued, which `SCmdEvent` alone doesn't carry. `SelectionDeltaEvent::add_unit_tags` is the field most consumers want: the raw unit tags newly added to a player's selection (same tag encoding as `TargetUnit::tag`). `SControlGroupUpdateEvent::control_group_update`'s enum values (`0`=Set, `1`=Add, `2`=Get/Recall, `3`=an unconfirmed rarer variant) were cross-checked against the `sc2reader` Python project's `create_control_group_event`, since `protocol97425.py` itself doesn't name them. **Deliberately not fully modeled:** `SelectionMask`'s `Mask`/`OneIndices`/`ZeroIndices` variants (which units are being *removed* from a selection) are exposed as raw index/length data, not resolved back to specific unit tags — that requires tracking each unit's position within the selection's flattened subgroup order across events, out of scope for what this crate's consumers currently need (they care about *additions*, i.e. what got newly selected, not precise removal tracking). **Real bug found and fixed via real-fixture verification:** an initial version hardcoded the `Mask` variant's bit-array length at 9 bits instead of reading its actual 9-bit length *prefix* first (the payload itself is variable-length, up to 511 bits) — this silently desynced the entire rest of the bit stream on any replay containing a `Mask`-encoded removal, caught by a real-fixture test (`decodes_game_events_stream_without_panicking` started failing with a bogus event id) rather than any unit test in isolation.

**Also out of scope:** ability-ID → human-readable name mapping (`abil_link`/`abil_cmd_index` → "Train SCV") requires a `CommandCard` data table not present in `protocol97425.py`; callers get raw numeric ids.

### Completed (API — in-memory loading, 0.4.0)

- [x] **`load_replay_from_bytes(&[u8]) -> Result<Replay, ReplayError>`** (`replay.rs`): decodes a replay already held in memory (HTTP upload, object-storage download) without a backing file. The whole pipeline always operated on byte slices internally; `load_replay(path)` is now a thin `std::fs::read` wrapper over this entry point. Verified against the real fixture (byte-based and path-based loads decode to identical structures) plus malformed-input tests (garbage/empty bytes error instead of panicking).

- [x] **Replay build/version decoding** (`header.rs`): `decode_replay_version(&[u8]) -> ReplayVersion` reads `SHeader.m_version` (`major`/`minor`/`revision`/`build`/`base_build`) from the MPQ user-data block that `load_replay` otherwise skips, and `Replay` now carries a `version` field. Lets a downstream consumer branch on the exact game build a replay was recorded on — added to support `sc2trainer`'s patch-5.0.16 balance re-verification (some hardcoded balance constants change between builds). Verified with a synthetic-header unit test and a real-fixture exact-value assertion (the fixture is build `97425`).
  - **`protocol97563` (patch 5.0.16) compatibility confirmed.** Blizzard's `protocol97563.py` was compared field-by-field against `protocol97425.py` (this crate's reference build) for every structure decoding depends on — `SDetails`/`SPlayerListEntry`, all tracker event typeids incl. the 39-field `PlayerStats`, `SCmdEvent`/selection typeids, `SVarUint32`, and the key id constants — and they are identical, so **no decoder changes are needed for build-97563 replays**. Confirmed empirically as well: a homogeneous 25-replay build-97563 batch decodes end-to-end with zero errors. (Caveat: ability IDs — `abil_link`/`abil_cmd_index` — are balance data, not part of `typeinfos`; a build *could* change them independently. They were separately re-verified as unchanged on build 97563 by `sc2trainer`.)

### Completed (camera events, 0.7.0)

- [x] **`SCameraUpdateEvent` decoding** (`game_events.rs`, typeid 149, event id 49): `GameEvent::CameraUpdate` exposes `target` (`CameraTarget`, in **1/256 of a map tile** — use `x_tiles()`/`y_tiles()`), `distance`, `pitch`, `yaw`, `reason` and `follow`. Added to unblock `sc2trainer`'s scouting-fidelity work, which needs *where each player was looking* to confirm observations that no other signal proves. Field layout confirmed verbatim against `protocol97425.py`'s `typeinfos[149]` (`146`=`_optional(84)`, `84`=`_struct(x:83, y:83)` with `83`=`_int(0,16)`; `147`=`_optional(83)`; `148`=`_optional(116)` with `116`=`_int(0,8)`).
  - **Empirically verified before anything was built on it**, per the consumer project's "verify, don't assume" rule — a camera decode that silently produces plausible-*looking* garbage would be worse than none. Across four distinct real replays (PvZ/ZvT/PvT/PvP) **and** the full 175-replay HomeStory Cup batch (2,022,571 camera events): zero decode failures, **zero** targets outside the map extent implied by independently-decoded `UnitBorn` positions, both players emit events in 175/175 replays, and the early-game camera sits nearer the player's *own* starting base than the opponent's in **344/345** player-replays. The decisive check is step size: a random/misaligned decode would put the median consecutive-target jump at roughly a third of the map diagonal, whereas the measured median across 748 real player-replays is **0.5%** of it (p90 3.1%, worst case 16.7%). Camera targets also land within 0.4–2.4 tiles of the centroid of large death clusters, i.e. players are looking at their fights.
  - **`replay.smartcam.events` was evaluated as the alternative source and rejected.** It is a real, populated stream (~4 KB/replay), but `s2protocol` ships no decoder or type definition for it, so it would need reverse-engineering from zero with no reference to check against — against a route whose structure is fully specified, already proven correct by the existing generic skip, and ~2,000-3,600 events per replay. `replay.game.events` was the clear pick.
  - **Honest gap:** `target` is absent on ~13% of camera events (1,758,534 of 2,022,571 carried one); consumers must treat those as "no information", not interpolate. `distance`/`pitch`/`yaw`/`reason` were absent and `follow` uniformly `false` in *every* event of all verification fixtures — they are modeled because the protocol defines them, not because any observed replay populates them.

### In progress / next up

- [ ] Ability-ID → unit/building-name mapping (`CommandCard` data), needed to turn raw `abil_link`/`abil_cmd_index` pairs into readable build-order entries.
- [ ] Higher-level analysis built on top of decoded events (build order reconstruction, resource efficiency, engagement detection) — the original motivation for this whole project.

### Pending

See [`plan-sc2reader-rust.md`](./plan-sc2reader-rust.md) for the full milestone plan (Phases 3-5: domain layer, datapacks, robustness).

## Project structure

```
sc2reader-rs/
├── src/
│   ├── lib.rs          # declares the crate's public modules
│   ├── macros.rs         # read_int_fields! and other decoding-boilerplate macros
│   ├── bitpacked.rs      # generic BitPackedDecoder primitives (read_bits, read_int, ...)
│   ├── protocol.rs      # generic VersionedDecoder primitives (read_vint, read_struct, skip_value, ...)
│   ├── details.rs        # replay.details decoding (SDetails)
│   ├── player.rs         # Player domain type
│   ├── events.rs          # replay.tracker.events decoding (TrackerEvent, PlayerStats)
│   ├── game_events.rs    # replay.game.events decoding (SCmdEvent + generic skip)
│   ├── typeinfos.rs      # generated protocol97425 typeinfos table, used by the generic skip
│   ├── format.rs         # SC2 in-game text markup formatting
│   └── bin/
│       └── inspect.rs   # debug binary: loads a replay and explores its structure,
│                          using mpq-parser (external dependency) for the MPQ container
├── fixtures/             # real .SC2Replay files used for manual testing
├── tests/
│   ├── game_events.rs    # integration tests against a real fixture replay
│   └── fixtures/         # real .SC2Replay files for integration tests (gitignored)
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
- **Fields decoded on a need basis.** Rather than modeling every field of every `SDetails`/event struct upfront, only fields actually useful for the project's goal are decoded; everything else is explicitly skipped (`skip_value`, or `skip_bitpacked_value` for `replay.game.events`) to keep the byte stream aligned.
- **`typeinfos.rs` is mechanically generated, not hand-transcribed.** For a 209-entry table where a single wrong bit width silently corrupts everything downstream, a small script parsing `protocol97425.py`'s `typeinfos` literal directly (Python's own `ast.literal_eval`) is far less error-prone than retyping 209 entries by hand — the same reasoning already applied at smaller scale to `SCmdEvent`'s own field layout.
- **`regex` for in-game text markup**, instead of chained `.replace()` calls.
- **A small `macro_rules!` macro for repetitive field decoding**, used specifically where hand-writing every `match` arm would add volume without adding clarity (`PlayerStats`'s 39 fields). Not used elsewhere — most structs are small enough that explicit `match` arms are more readable than a macro invocation.

## Resources used

- [sc2reader (Python)](https://github.com/ggtracker/sc2reader) — de facto specification of the behavior being replicated.
- [Blizzard/s2protocol](https://github.com/Blizzard/s2protocol) — reference for the event serialization protocol; per-build protocol definitions used to resolve field layouts for both `SDetails` and tracker events.
- Community documentation on the MPQ format (StormLib / modding wiki) for the container and its cryptography.
- [mpq-parser](https://github.com/aldezex/mpq-parser) — own library (sibling crate) for MPQ container parsing.
- [nom-mpq](https://lib.rs/crates/nom-mpq) — MPQ parser used by `s2protocol`, with a different approach (parser combinators via `nom`); interesting reference, not used as a dependency.
