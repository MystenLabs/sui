// options:
// printWidth: 80

module prettier::binary_expression {
    fun main() {
        if (
            bytes ==
            &b"bool" ||
            bytes == &b"u8" ||
            bytes == &b"u16" ||
            bytes == &b"u32" ||
            bytes == &b"u64" ||
            bytes == &b"u128" ||
            bytes == &b"u256" ||
            bytes == &b"address" || (
                bytes.length() >= 6 &&
                bytes[0] == ASCII_V &&
                bytes[1] == ASCII_E &&
                bytes[2] == ASCII_C &&
                bytes[3] == ASCII_T &&
                bytes[4] == ASCII_O &&
                bytes[5] == ASCII_R,
            )
        ) {};

        bytes ==
        &b"bool" ||
        bytes == &b"u8" ||
        bytes == &b"u16" ||
        bytes == &b"u32" ||
        bytes == &b"u64" ||
        bytes == &b"u128" ||
        bytes == &b"u256" ||
        bytes == &b"address" || (
            bytes.length() >= 6 &&
            bytes[0] == ASCII_V &&
            bytes[1] == ASCII_E &&
            bytes[2] == ASCII_C &&
            bytes[3] == ASCII_T &&
            bytes[4] == ASCII_O &&
            bytes[5] == ASCII_R,
        )
    }
}
