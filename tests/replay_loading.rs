//! Verifies the in-memory (`&[u8]`) replay-loading entry point against
//! the path-based one on a real replay fixture — both must decode to
//! identical structures, since `load_replay` is just a convenience
//! wrapper that reads the file first.

const FIXTURE: &str = "tests/fixtures/dont-oracle-me.SC2Replay";

#[test]
fn load_replay_from_bytes_matches_load_replay() {
    let bytes = std::fs::read(FIXTURE).expect("fixture missing");
    let from_bytes =
        sc2reader_rs::replay::load_replay_from_bytes(&bytes).expect("failed to load from bytes");
    let from_path = sc2reader_rs::replay::load_replay(FIXTURE).expect("failed to load from path");

    assert_eq!(from_bytes.map_name, from_path.map_name);
    assert_eq!(from_bytes.players.len(), from_path.players.len());
    assert_eq!(
        from_bytes.tracker_events.len(),
        from_path.tracker_events.len()
    );
    assert_eq!(from_bytes.game_events.len(), from_path.game_events.len());
}

#[test]
fn decodes_the_game_build_from_the_real_fixture() {
    // The fixture was recorded on build 97425 (5.0.x) — this crate's
    // reference protocol build. Exact-value assertion so a regression in
    // header decoding (or an accidental offset change) is caught, not
    // just "some non-zero number".
    let replay = sc2reader_rs::replay::load_replay(FIXTURE).expect("failed to load");
    assert_eq!(replay.version.base_build, 97425);
    assert_eq!(replay.version.build, 97425);
    assert_eq!(replay.version.major, 5);
    assert_eq!(replay.version.minor, 0);
}

#[test]
fn load_replay_from_bytes_rejects_garbage_without_panicking() {
    // Malformed input must surface as an error, never a panic — the
    // serverless upload path feeds arbitrary user bytes into this.
    assert!(sc2reader_rs::replay::load_replay_from_bytes(&[0u8; 64]).is_err());
    assert!(sc2reader_rs::replay::load_replay_from_bytes(&[]).is_err());
}
