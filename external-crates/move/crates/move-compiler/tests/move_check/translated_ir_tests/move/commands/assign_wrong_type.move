// check: STLOC_TYPE_MISMATCH_ERROR
module 0x42::m {

fun main() {
    let x: u64;
    x = false
}
}
