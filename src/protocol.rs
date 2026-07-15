pub fn read_vint(bytes: &[u8], pos: &mut usize) -> i64 {
    let mut b0 = next_byte(bytes, pos) as u64;
    let is_negative = (b0 & 1) == 1;
    let mut res: u64 = (b0 >> 1) & 0x3F;
    let mut bits_read = 6;

    while (b0 & 0x80) != 0 {
        b0 = next_byte(bytes, pos) as u64;
        res |= (b0 & 0x7F) << bits_read;
        bits_read += 7;
    }

    if is_negative {
        -(res as i64)
    } else {
        res as i64
    }
}

fn next_byte(bytes: &[u8], pos: &mut usize) -> u8 {
    let byte = bytes[*pos];
    *pos += 1;
    byte
}

fn read_blob<'a>(bytes: &'a [u8], pos: &mut usize) -> &'a [u8] {
    let long = read_vint(bytes, pos) as usize;
    let start = *pos;
    let res = &bytes[start..start + long];
    *pos += long;
    res
}

pub fn read_optional<T>(
    bytes: &[u8],
    pos: &mut usize,
    read_inner: impl FnOnce(&[u8], &mut usize) -> T,
) -> Option<T> {
    let present = next_byte(bytes, pos);

    if present != 0 {
        Some(read_inner(bytes, pos))
    } else {
        None
    }
}

fn read_array<T>(
    bytes: &[u8],
    pos: &mut usize,
    mut read_inner: impl FnMut(&[u8], &mut usize) -> T,
) -> Vec<T> {
    let long = read_vint(bytes, pos);
    let mut res = Vec::new();

    for _ in 0..long {
        res.push(read_inner(bytes, pos));
    }

    res
}

fn read_struct(bytes: &[u8], pos: &mut usize, mut on_field: impl FnMut(&[u8], &mut usize, i64)) {
    let _tag = next_byte(bytes, pos);
    let num_fields = read_vint(bytes, pos);

    for _ in 0..num_fields {
        let field_index = read_vint(bytes, pos);
        on_field(bytes, pos, field_index);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_zero() {
        let bytes = [0x00];
        let mut pos = 0;
        assert_eq!(read_vint(&bytes, &mut pos), 0);
        assert_eq!(pos, 1);
    }

    #[test]
    fn reads_blob() {
        let bytes = [0x04, 0x41, 0x42];
        let mut pos = 0;

        let result = read_blob(&bytes, &mut pos);

        assert_eq!(result, [0x41, 0x42]);
        assert_eq!(pos, 3);
    }

    #[test]
    fn reads_optional_present() {
        let bytes = [0x01, 0x2A];
        let mut pos = 0;

        let value = read_optional(&bytes, &mut pos, |b, p| next_byte(b, p));

        assert_eq!(value, Some(0x2A));
        assert_eq!(pos, 2);
    }

    #[test]
    fn reads_optional_absent() {
        let bytes = [0x00];
        let mut pos = 0;

        let value: Option<u8> = read_optional(&bytes, &mut pos, |b, p| next_byte(b, p));

        assert_eq!(value, None);
        assert_eq!(pos, 1);
    }

    #[test]
    fn reads_struct_with_two_fields() {
        let bytes = [0x05, 0x04, 0x00, 0x41, 0x02, 0x42];
        let mut pos = 0;

        let mut field_0 = 0u8;
        let mut field_1 = 0u8;

        read_struct(&bytes, &mut pos, |b, p, field_index| match field_index {
            0 => field_0 = next_byte(b, p),
            1 => field_1 = next_byte(b, p),
            _ => {}
        });

        assert_eq!(field_0, 0x41);
        assert_eq!(field_1, 0x42);
        assert_eq!(pos, 6);
    }
}
