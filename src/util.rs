use bytes::Buf;

/// Translates boolean button values into bytes expected by the roboRIO.
/// Encoding: LSB 0 (first bool = bit 0 of the byte).
/// Byte Order: First chunk of 8 booleans corresponds to the *last* byte in the output.
pub(crate) fn to_u8_vec(vec_in: &[bool]) -> Vec<u8> {
    // Calculate needed bytes, rounding up
    let num_bytes = vec_in.len().div_ceil(8);
    let mut result = Vec::with_capacity(num_bytes);

    // Iterate over input in chunks of 8
    for chunk_start in (0..vec_in.len()).step_by(8) {
        let mut byte: u8 = 0;
        // Build one byte, processing up to 8 bits
        for bit_pos in 0..8 {
            let bool_idx = chunk_start + bit_pos;

            // Safely handle slices not perfectly divisible by 8
            if let Some(&value) = vec_in.get(bool_idx) {
                if value {
                    // Set bit 'bit_pos' if true (LSB-first)
                    byte |= 1 << bit_pos;
                }
            } else {
                break; // Reached end of input slice
            }
        }
        result.push(byte);
    }

    // Reverse byte order (first chunk processed becomes last byte)
    result.reverse();

    result
}

/// Converts the given team number into a String containing the IP of the roboRIO
/// Assumes the roboRIO will exist at 10.TE.AM.2
/// Optimized version using integer arithmetic.
pub(crate) fn ip_from_team_number(team: u32) -> Option<String> {
    if team >= 100000 {
        // Team number is to large. Mayber in the distant future of 3000 this might break.
        return None;
    }

    if team < 100 {
        // Format as 10.0.AM.2 where AM is the team number
        Some(format!("10.0.{}.2", team))
    } else {
        // Covers 3, 4, and 5 digit numbers (team >= 100 and team < 100000)
        let te = team / 100; // Integer division gives the "TE" part
        let am = team % 100; // Modulo gives the "AM" part
        Some(format!("10.{}.{}.2", te, am))
    }
}

pub(crate) trait InboundTag {
    fn chomp(buf: &mut impl Buf) -> crate::Result<Self>
    where
        Self: Sized;
}
