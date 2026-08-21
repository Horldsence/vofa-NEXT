//! 位域读取 — 与 FrameParser 的 read_bitfield 语义一致
//!
//! - `bytes`: 起始字节切片 (至少包含 bit_offset + bit_length 位)
//! - `bit_offset`: 起始位偏移 (0-7, MSB first)
//! - `bit_length`: 位长度 (1-32)
//! - `is_signed`: 是否带符号 (true=最高位为符号位, 二补码)
#[allow(clippy::cast_precision_loss)]
pub fn read_bitfield(bytes: &[u8], bit_offset: u8, bit_length: u8, is_signed: bool) -> f32 {
    if bit_length == 0 || bytes.is_empty() {
        return 0.0;
    }
    let mut value: u32 = 0;
    for i in 0..bit_length as usize {
        let abs_bit = bit_offset as usize + i;
        let byte_idx = abs_bit / 8;
        let bit_in_byte = 7 - (abs_bit % 8); // MSB first: bit 7 是最高位
        if byte_idx >= bytes.len() {
            break;
        }
        let bit = (bytes[byte_idx] >> bit_in_byte) & 1;
        value = (value << 1) | u32::from(bit);
    }
    if is_signed && bit_length < 32 {
        let sign_bit = 1u32 << (bit_length - 1);
        if value & sign_bit != 0 {
            value |= u32::MAX << bit_length;
        }
    }
    if is_signed {
        value.cast_signed() as f32
    } else {
        value as f32
    }
}