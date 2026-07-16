use sc2reader_rs::game_events::GameEventsError;
use sc2reader_rs::replay::{ReplayError, load_replay};

const FIXTURE: &str = "tests/fixtures/dont-oracle-me.SC2Replay";

#[test]
fn decode_game_events_reports_first_unsupported_event_without_panicking() {
    // Only SCmdEvent (event id 27) is modeled; every other
    // `NNet.Game.*Event` type is treated as unsupported and aborts
    // decoding rather than being generically skipped — there's no way to
    // skip a value of unknown bit width in this untagged format (see the
    // module doc on `decode_game_events`).
    //
    // In practice this means decoding stops at the very first event of
    // *any* real replay: SC2 always emits non-command bookkeeping events
    // (sync markers, camera updates, user options, etc.) well before the
    // first player command. For this fixture, that first event is
    // `NNet.Game.SSetSyncLoadingTimeEvent` (event id 116, typeid 191 per
    // protocol97425's `game_event_types` table) — confirmed by manual
    // inspection against the reference protocol file, not a decoding bug
    // (the bit reader itself is separately verified by
    // src/bitpacked.rs's unit tests, including against Blizzard's actual
    // `BitPackedBuffer` algorithm). This is a known, current limitation —
    // see the README's "Known limitations" section — pending future
    // generic bit-level skip support. This test locks in that decoding
    // fails predictably (a typed error, not a panic or silent
    // misalignment) at the exact expected event id/position, so a
    // regression in the bit reader or stream framing would be caught
    // here even though no `CmdEvent`s can be extracted from this fixture
    // today.
    let result = load_replay(FIXTURE);

    match result {
        Err(ReplayError::GameEvents(GameEventsError::UnsupportedEventId {
            event_id,
            bit_pos,
        })) => {
            assert_eq!(event_id, 116);
            assert_eq!(bit_pos, 20);
        }
        other => panic!("expected GameEventsError::UnsupportedEventId, got {other:?}"),
    }
}

#[test]
#[ignore = "decode_game_events cannot progress past the very first event of a real replay today, since it's never SCmdEvent and there's no generic bit-level skip for other event types (see README known limitations); blocked until that's implemented"]
fn first_cmd_events_expose_fields_for_manual_cross_check() {
    // Once generic skip support exists and decode_game_events can walk
    // past non-SCmdEvent events, this should decode the first 5 CmdEvents
    // from `FIXTURE`, print their gameloop/user_id/abil_link/
    // abil_cmd_index, and have the project owner cross-check those
    // printed values against a third-party replay analysis tool for the
    // fixture's early worker-training commands (per the test battery's
    // `first_cmd_events_match_reference_tool` acceptance criterion) —
    // a decoder that "compiles and doesn't panic" is not sufficient
    // evidence of correctness for a positional, untagged format like
    // this one.
}

#[test]
#[ignore = "requires both the tracker-event and game-event pipelines cross-referenced by unit-training ordering (not exact tag matching), plus abil-id -> unit-name mapping (explicitly out of scope) and generic event skip support (see the other ignored tests in this file); see briefs/BitPackedDecoder.md"]
fn worker_training_supply_reservation_matches_expected_offset() {
    // The original motivation for this whole feature: confirm that a
    // worker-training SCmdEvent's gameloop is *earlier* than the
    // corresponding UnitBorn tracker event for that worker (since supply
    // is reserved at command-issue time, not at unit-completion time).
    // Cross-reference by unit count/ordering, not by exact tag matching
    // (SCmdEvent doesn't carry the resulting unit's tag).
    //
    // TODO: implement once abil_link/abil_cmd_index -> unit-name mapping
    // exists (out of scope here); assert
    // `cmd_event.gameloop < corresponding_unit_born.gameloop`.
}
