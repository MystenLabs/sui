//# run
module 0x42::m {
fun main() {
  assert!(true || true && false, 99); // "&&" has precedence over "||"
  assert!(true != false && false != true, 100); // "&&" has precedence over comparisons
  assert!(1 | 3 ^ 1u64 == 3, 101); // binary XOR has precedence over OR
  assert!(2u64 ^ 3 & 1 == 3, 102); // binary AND has precedence over XOR
  assert!(3 & 3 + 1u64 == 0, 103); // addition has precedence over binary AND
  assert!(1 + 2 * 3 == 7u64, 104); // multiplication has precedence over addition
}
}
