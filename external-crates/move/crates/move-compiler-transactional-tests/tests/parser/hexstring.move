//# run
script {
use std::vector;

fun main() {
    let v = x"01020304";
    let sum: u64 = 0;
    while (!vector::is_empty(&v)) {
        sum = sum + (vector::pop_back(&mut v) as u64);
    };
    assert!(sum == 10, sum);
}
}
