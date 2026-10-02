// SPDX-License-Identifier: GPL-3.0-only

//! Stable generator seeds (plan 03 §3.5): FNV-1a of the output name, **not**
//! Java `String.hashCode` and not upstream's time-seeded shared `rand`
//! (scorches are non-deterministic upstream; they are deterministic here).

/// FNV-1a 64-bit of `name`.
pub fn fnv1a(name: &str) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in name.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// FNV-1a as a signed 64-bit seed.
pub fn seed(name: &str) -> i64 {
    fnv1a(name) as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv1a_known_vector() {
        // FNV-1a 64 of "a" is a published constant.
        assert_eq!(fnv1a("a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv1a(""), 0xcbf2_9ce4_8422_2325);
        assert_ne!(fnv1a("scorch-0-0"), fnv1a("scorch-0-1"));
    }
}
