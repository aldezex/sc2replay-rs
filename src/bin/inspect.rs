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

    println!("{:?}", block_table_result);
}
