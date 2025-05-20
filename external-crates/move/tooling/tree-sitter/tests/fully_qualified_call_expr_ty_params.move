module a::b;

fun f() {
    let x = sui::dynamic_field::borrow<vector<u8>, u64>(&parent, b"");
    let x = ::sui::dynamic_field::borrow<vector<u8>, u64>(&parent, b"");
}
