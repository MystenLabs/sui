//# init --edition 2024.alpha

//#run
module 0x42::main;

fun main() {
    let x = 0;  // live: { x }
    let r = &x; // live: { r, x }
    let y = copy x + 1; // live { r, x, y }
    assert!(*r == 0); // live: {}
    assert!(y == 1); // live: { r }
}
