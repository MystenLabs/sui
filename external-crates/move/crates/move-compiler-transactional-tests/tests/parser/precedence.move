//# run
module 0x42::m {

fun main() {
    assert!(true || true && false, 99); // "&&" has precedence over "||"
    assert!(true != false && false != true, 100u64); // "&&" has precedence over comparisons
    assert!(1 | 3 ^ 1 == 3u64, 101); // binary XOR has precedence over OR
    assert!(2 ^ 3 & 1 == 3u64, 102); // binary AND has precedence over XOR
    assert!(3u64 & 3 + 1 == 0, 103); // addition has precedence over binary AND
    assert!(1 + 2 * 3 == 7u64, 104); // multiplication has precedence over addition
}
}
