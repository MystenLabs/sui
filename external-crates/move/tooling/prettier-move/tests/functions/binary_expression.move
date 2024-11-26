// options:
// printWidth: 40

module prettier::binary_expression {
    fun main() {
        alice == bob && bob == carol && (dave > eve && eve > frank) && grace <= heidi && heidi <= ian && jack >= kate && kate >= larry && mary == nancy && nancy == olivia && peter != quincy && quincy != robert;

        a + 10 / (100 as u64) * (b - c) % d;

        let slivers_size = (
            source_symbols_primary(n_shards) as u64 + (source_symbols_secondary(n_shards) as u64),
        ) * (symbol_size(unencoded_length, n_shards) as u64);


        if (
            bytes == &b"bool" ||
            bytes == &b"u8" ||
            bytes == &b"u16" ||
            bytes == &b"u32" ||
            bytes == &b"u64" ||
            bytes == &b"u128" ||
            bytes == &b"u256" ||
            bytes == &b"address" ||
            (bytes.length() >= 6 &&
            bytes[0] == ASCII_V &&
            bytes[1] == ASCII_E &&
            bytes[2] == ASCII_C &&
            bytes[3] == ASCII_T &&
            bytes[4] == ASCII_O &&
            bytes[5] == ASCII_R)) {
            // do something
        };

        return a < b && b < c && (d > e && e > f) && g <= h && h <= i && j >= k && k >= l && m == n && n == o && p != q && q != r;

        bytes == &b"bool" ||
        bytes == &b"u8" ||
        bytes == &b"u16" ||
        bytes == &b"u32" ||
        bytes == &b"u64" ||
        bytes == &b"u128" ||
        bytes == &b"u256" ||
        bytes == &b"address" ||
        (bytes.length() >= 6 &&
        bytes[0] == ASCII_V &&
        bytes[1] == ASCII_E &&
        bytes[2] == ASCII_C &&
        bytes[3] == ASCII_T &&
        bytes[4] == ASCII_O &&
        bytes[5] == ASCII_R)
    }
}
