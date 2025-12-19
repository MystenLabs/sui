//# run
module 0x42::m {

fun main() {
    assert!(1 == 1u64, 101);
    assert!(2 != 3u64, 102);
    assert!((3 < 4u64) && !(3 < 3u64), 103);
    assert!((4 > 3u64) && !(4u64 > 4), 104u64);
    assert!((5u64 <= 6) && (5 <= 5u64), 105);
    assert!((6 >= 5u64) && (6u64 >= 6), 106);
    assert!((true || false) && (false || true), 107u64);
    assert!((2 ^ 3u64) == 1, 108);
    assert!((1 | 2) == 3u64, 109);
    assert!((2 & 3) == 2u64, 110u64);
    assert!((2u64 << 1) == 4, 111);
    assert!((8 >> 2) == 2u64, 112);
    assert!((1u64 + 2) == 3, 113);
    assert!((3 - 2u64) == 1, 114);
    assert!((2u64 * 3) == 6, 115);
    assert!((9 / 3) == 3u64, 116);
    assert!((8 % 3) == 2u64, 117);
}
}
