use mpq_parser::{MpqHeader, MpqUserDataHeader};
use std::fs::read;

fn main() {
    let mut args = std::env::args();
    let first_arg = args.nth(1).expect("You need to pass a replay to check");

    let replay = read(first_arg).expect("The replay is empty?");

    let user_header = MpqUserDataHeader::parse(&replay).expect("Couldnt parse user header");
    let offset = user_header.header_offset as usize;
    let mpq_header = MpqHeader::parse(&replay[offset..]).expect("Couldnt parse mpq header");

    println!("{:?}", user_header);
    println!("{:?}", mpq_header);
}
