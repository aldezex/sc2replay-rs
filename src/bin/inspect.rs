use mpq_parser::{MpqHeader, MpqUserDataHeader};
use std::fs::read;

fn main() {
    let mut args = std::env::args();
    let first_arg = args.nth(1).expect("You need to pass a replay to check");

    let replay = read(first_arg).expect("The replay is empty?");

    let user_header = MpqUserDataHeader::parse(&replay).expect("Couldnt parse user header");
    let offset = user_header.header_offset as usize;
    let mpq_header = MpqHeader::parse(&replay[offset..]).expect("Couldnt parse mpq header");

    let hash_table_offset = offset + mpq_header.hash_table_position as usize;
    let hash_table_size = mpq_header.hash_table_size as usize * 16;
    let replay_raw_bytes = &replay[hash_table_offset..hash_table_offset + hash_table_size];

    let crypt_table = mpq_parser::crypto::build_crypt_table();
    let hash_key = mpq_parser::crypto::hash_string(
        "(hash table)",
        mpq_parser::crypto::MPQ_HASH_FILE_KEY,
        &crypt_table,
    );
    let decrypt_out = mpq_parser::crypto::decrypt(replay_raw_bytes, hash_key, &crypt_table);

    let entries = mpq_parser::hash::parse_hash_table_entries(&decrypt_out);

    let block_table_offset = offset + mpq_header.block_table_position as usize;
    let block_table_size = mpq_header.block_table_size as usize * 16;
    let block_table_key = mpq_parser::crypto::hash_string(
        "(block table)",
        mpq_parser::crypto::MPQ_HASH_FILE_KEY,
        &crypt_table,
    );

    let block_table_raw_bytes = &replay[block_table_offset..block_table_offset + block_table_size];
    let block_table_decrypt =
        mpq_parser::crypto::decrypt(block_table_raw_bytes, block_table_key, &crypt_table);
    let block_table_result = mpq_parser::block::parse_block_table_entries(&block_table_decrypt);

    let details_block = mpq_parser::archive::find_file(
        "replay.details",
        &entries,
        &block_table_result,
        &crypt_table,
    )
    .expect("replay.details not found");

    let file_contents = mpq_parser::archive::extract_file(&replay, offset as u32, *details_block)
        .expect("couldn't extract file");

    println!("{} bytes extracted", file_contents.len());
    println!("{:?}", &file_contents[..50.min(file_contents.len())]);

    let tracker_block = mpq_parser::archive::find_file(
        "replay.tracker.events",
        &entries,
        &block_table_result,
        &crypt_table,
    )
    .expect("replay.tracker.events not found");

    let tracker_contents =
        mpq_parser::archive::extract_file(&replay, offset as u32, *tracker_block)
            .expect("couldn't extract file");

    println!("{} bytes extracted (tracker)", tracker_contents.len());

    let details_bytes = mpq_parser::archive::extract_file(&replay, offset as u32, *details_block)
        .expect("couldn't extract file");

    let details = sc2reader_rs::details::decode_replay_details(&details_bytes);
    println!("Map: {}", details.map_name);
    for player in &details.players {
        println!("  {} ({})", player.name, player.race);
    }

    let events = sc2reader_rs::events::decode_tracker_events(&tracker_contents);
    println!("Decoded {} events", events.len());
    println!("{:#?}", &events[..5.min(events.len())]);
}
