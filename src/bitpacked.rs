//! Decoding primitives for SC2's "bit-packed" protocol encoding
//! (`BitPackedDecoder` in Blizzard's reference implementation), used by
//! `replay.game.events`.
//!
//! Unlike [`crate::protocol`]'s `VersionedDecoder` primitives, there are no
//! self-describing type tags: every field is read as an exact, fixed
//! number of bits determined entirely by the protocol's `typeinfos` table.
//! The `(offset, bits)` parameters below are load-bearing — get them wrong
//! and every field after the first mistake silently corrupts.
//!
//! Cursor convention matches [`crate::protocol`]: functions take
//! `bytes: &[u8]` and a cursor advanced by `&mut usize` rather than a
//! reader struct — except here the cursor counts *bits*, not bytes.
//!
//! Bit order (verified against Blizzard's reference `BitPackedBuffer`,
//! not assumed): within a byte, bits are consumed low-to-high, but across
//! a byte boundary the earlier-consumed byte occupies the *more*
//! significant part of the result. This is not the same as flattening the
//! whole buffer into one little-endian bitstream and slicing out `n` bits
//! — see [`tests::reads_across_a_byte_boundary_is_not_ambiguous`], the one
//! test in this module whose expected value actually distinguishes the two
//! models (a naive flatten model would give a different answer).
//!
//! There is no generic `_struct`/`_choice` combinator here, unlike
//! Blizzard's Python reference: this codebase has no dynamic `typeinfos`
//! table to dispatch through, so struct/choice decoding is just a
//! hand-written function per event type (see [`crate::game_events`])
//! calling these primitives in sequence — adding a generic dispatch layer
//! here would add indirection without reuse value at this scope.

/// Reads `n` bits (0-64) starting at `*bit_pos`, advancing it by `n`.
pub fn read_bits(bytes: &[u8], bit_pos: &mut usize, n: u32) -> u64 {
    let mut result: u64 = 0;
    let mut result_bits: u32 = 0;

    while result_bits != n {
        let byte_idx = *bit_pos / 8;
        let bit_offset = (*bit_pos % 8) as u32;
        let byte = bytes[byte_idx] as u64;
        let remaining_in_byte = 8 - bit_offset;
        let copy_bits = (n - result_bits).min(remaining_in_byte);
        let mask = (1u64 << copy_bits) - 1;
        let chunk = (byte >> bit_offset) & mask;

        result = (result << copy_bits) | chunk;
        result_bits += copy_bits;
        *bit_pos += copy_bits as usize;
    }

    result
}

/// Advances `*bit_pos` to the next byte boundary (round up); a no-op if
/// already aligned.
pub fn byte_align(bit_pos: &mut usize) {
    *bit_pos = bit_pos.div_ceil(8) * 8;
}

/// Reads `n` bytes at a byte-aligned position, aligning first if needed.
pub fn read_aligned_bytes<'a>(bytes: &'a [u8], bit_pos: &mut usize, n: usize) -> &'a [u8] {
    byte_align(bit_pos);
    let start = *bit_pos / 8;
    let res = &bytes[start..start + n];
    *bit_pos += n * 8;
    res
}

/// `_int(offset, bits)`: reads `bits` bits and adds `offset`.
pub fn read_int(bytes: &[u8], bit_pos: &mut usize, offset: i64, bits: u32) -> i64 {
    offset + read_bits(bytes, bit_pos, bits) as i64
}

/// `_optional(inner)`: reads one presence bit; if set, decodes the inner
/// value via `read_inner`.
pub fn read_optional<T>(
    bytes: &[u8],
    bit_pos: &mut usize,
    read_inner: impl FnOnce(&[u8], &mut usize) -> T,
) -> Option<T> {
    let present = read_bits(bytes, bit_pos, 1) != 0;

    if present {
        Some(read_inner(bytes, bit_pos))
    } else {
        None
    }
}

/// `_optional(_int(offset, bits))` — the shape used by every optional
/// integer field in `SCmdEvent`.
pub fn read_optional_int(bytes: &[u8], bit_pos: &mut usize, offset: i64, bits: u32) -> Option<i64> {
    read_optional(bytes, bit_pos, |b, p| read_int(b, p, offset, bits))
}

/// `NNet.SVarUint32` (typeid 7) in bit-packed mode: a 2-bit selector
/// choosing between four fixed-width unsigned ints (6/14/22/32 bits).
/// Used for the gameloop delta prefixing every event in the stream.
///
/// Do not confuse with [`crate::protocol::read_choice_as_int`], which
/// decodes the *tagged* `VersionedDecoder` flavor of the same typeid.
pub fn read_var_uint32(bytes: &[u8], bit_pos: &mut usize) -> i64 {
    match read_int(bytes, bit_pos, 0, 2) {
        0 => read_int(bytes, bit_pos, 0, 6),
        1 => read_int(bytes, bit_pos, 0, 14),
        2 => read_int(bytes, bit_pos, 0, 22),
        3 => read_int(bytes, bit_pos, 0, 32),
        _ => unreachable!("2-bit selector is always 0..=3"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_single_byte_as_8_bits() {
        let bytes = [0b1010_1100];
        let mut bit_pos = 0;
        let value = read_bits(&bytes, &mut bit_pos, 8);
        assert_eq!(value, 0b1010_1100);
        assert_eq!(bit_pos, 8);
    }

    #[test]
    fn reads_partial_bits_from_start_of_byte() {
        // Confirmed against Blizzard's reference BitPackedBuffer: bits are
        // consumed low-to-high within a byte, so the low 4 bits of
        // 0b1010_1100 (0b1100 = 12) are read first.
        let bytes = [0b1010_1100];
        let mut bit_pos = 0;
        let value = read_bits(&bytes, &mut bit_pos, 4);
        assert_eq!(value, 12);
        assert_eq!(bit_pos, 4);
    }

    #[test]
    fn reads_across_a_byte_boundary() {
        // 12 bits spanning 2 bytes. Note: this byte pattern is bit-order
        // *ambiguous* (every involved bit is 1), so both a correct and an
        // incorrect bit-order model agree here — see
        // `reads_across_a_byte_boundary_is_not_ambiguous` below for the
        // test that actually pins down the order.
        let bytes = [0xFF, 0x0F];
        let mut bit_pos = 0;
        let value = read_bits(&bytes, &mut bit_pos, 12);
        assert_eq!(value, 0xFFF);
        assert_eq!(bit_pos, 12);
    }

    #[test]
    fn reads_across_a_byte_boundary_is_not_ambiguous() {
        // Unlike the test above, this byte pattern distinguishes the
        // correct bit-order model from a naive "flatten the whole buffer
        // into one little-endian bitstream" model: the earlier-consumed
        // byte (0x01) occupies the *more* significant part of the result,
        // giving 18 (0x01 in the high nibble, 0x02's low nibble in the
        // low nibble: 0b0001_0010), not the flattened-bitstream answer of
        // 513 (0x01 | (0x02 << 8)).
        let bytes = [0x01, 0x02];
        let mut bit_pos = 0;
        let value = read_bits(&bytes, &mut bit_pos, 12);
        assert_eq!(value, 18);
        assert_eq!(bit_pos, 12);
    }

    #[test]
    fn byte_align_advances_to_next_boundary() {
        let bytes = [0xFF, 0xFF];
        let mut bit_pos = 3;
        byte_align(&mut bit_pos);
        assert_eq!(bit_pos, 8);
        let _ = bytes;
    }

    #[test]
    fn byte_align_is_a_no_op_when_already_aligned() {
        let mut bit_pos = 8;
        byte_align(&mut bit_pos);
        assert_eq!(bit_pos, 8);
    }

    #[test]
    fn reads_int_with_offset() {
        // _int(offset=100, bits=8): raw bits 5 -> value 105.
        let bytes = [5u8];
        let mut bit_pos = 0;
        let value = read_int(&bytes, &mut bit_pos, 100, 8);
        assert_eq!(value, 105);
    }

    #[test]
    fn reads_int_with_full_range_negative_offset() {
        // Exercises the (offset=-2147483648, bits=32) shape used by
        // Point3.z / TargetPoint.z across all 4 bytes.
        let bytes = [0xFF, 0xFF, 0xFF, 0xFF];
        let mut bit_pos = 0;
        let value = read_int(&bytes, &mut bit_pos, -2147483648, 32);
        assert_eq!(value, 2147483647);
        assert_eq!(bit_pos, 32);
    }

    #[test]
    fn reads_optional_absent() {
        // First bit 0 => None, no further bits consumed for the inner value.
        let bytes = [0b0000_0000];
        let mut bit_pos = 0;
        let value = read_optional_int(&bytes, &mut bit_pos, 0, 8);
        assert_eq!(value, None);
        assert_eq!(bit_pos, 1);
    }

    #[test]
    fn reads_optional_present() {
        // First bit (LSB of 0x81) is 1 => Some, followed by an 8-bit int
        // spanning both bytes: 7 remaining bits of byte 0 (0x81 >> 1 =
        // 0x40) as the high part, 1 bit of byte 1 (0x01 & 1 = 1) as the
        // low part -> 0x40 << 1 | 1 = 129.
        let bytes = [0x81, 0x01];
        let mut bit_pos = 0;
        let value = read_optional_int(&bytes, &mut bit_pos, 0, 8);
        assert_eq!(value, Some(129));
        assert_eq!(bit_pos, 9);
    }
}
