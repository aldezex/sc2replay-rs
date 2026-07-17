# Agent context — sc2reader-rs

StarCraft II replay protocol decoder. **Public repo, published on crates.io** — currently 0.3.0. Part of a three-repo project; the full project context and history live in the (private) `sc2trainer-workspace` repo: `CLAUDE.md` + `docs/HISTORY.md` there. Owner: `aldezex` (Spanish speaker; code/docs in English).

## What matters most here

- **This crate stays a faithful, opinion-free port of the replay format.** All gameplay interpretation lives downstream in `sc2trainer`. Don't add analysis logic here.
- Most of this crate was **hand-written by the owner as a Rust learning exercise**; only `bitpacked.rs`/`game_events.rs`/`typeinfos.rs` were delegated to agents via the briefs in `briefs/`. Respect that split — big refactors of the hand-written parts are the owner's call.
- **Two decoders, fundamentally different**: `protocol.rs` (`VersionedDecoder`, self-describing tags — `replay.details`, `replay.tracker.events`) vs `bitpacked.rs` (`BitPackedDecoder`, untagged, every bit width load-bearing — `replay.game.events`). Field layouts for the latter are cross-checked against Blizzard's `s2protocol` `protocol97425.py`; **fetch that file, never trust transcriptions** (two transcription bugs were caught that way).
- **Bit order gotcha** (verified, non-obvious): bits are consumed low-to-high within a byte, but the earlier-consumed byte is *more significant* across boundaries. See `bitpacked.rs::tests::reads_across_a_byte_boundary_is_not_ambiguous` — the one test that distinguishes correct from plausible-but-wrong.
- `game_events.rs` fully models only `SCmdEvent` (27), `SSelectionDeltaEvent` (28), `SControlGroupUpdateEvent` (29); everything else is bit-skipped via `typeinfos.rs` (mechanically generated 209-entry table — regenerate from the protocol file, never hand-edit). `GameEvent` grows variants over time: **never write irrefutable `let GameEvent::Cmd(c) = e;` patterns downstream.**
- **Real-fixture tests are the safety net** for untagged decoding: a single wrong bit width silently desyncs the whole stream and only `tests/game_events.rs` (against `tests/fixtures/dont-oracle-me.SC2Replay`, gitignored) catches it — a variable-length `_bitarray` decoded as fixed-width once slipped past all unit tests this way.
- `unit_tag_index` alone is never a unit identity — always `(unit_tag_index, unit_tag_recycle)`.
- Perf: crypt table is a process-wide `OnceLock`; ~75% of load time is bzip2 decompression (inherent). Downstream loads replays in parallel — keep `load_replay` thread-safe.
- Publishing a new version requires the owner's explicit go-ahead (irreversible).
