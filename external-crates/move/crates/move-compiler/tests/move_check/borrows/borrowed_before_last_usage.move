module 0x42::m {
fun main() {
    let x = 0;
    let r = &x;
    let y = x;
    assert!(*r == y, 0);
}
}
