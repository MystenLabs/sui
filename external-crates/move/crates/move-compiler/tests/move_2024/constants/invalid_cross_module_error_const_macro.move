// An internal '#[error]' constant used in an abort position inside a macro is rejected when the
// macro expands in another module: visibility is resolved in the scope of the caller, which
// cannot access the internal constant

module 0x42::a {

#[error]
const ENotValid: vector<u8> = b"invalid";

public macro fun check_valid($x: u64) {
    assert!($x < 10, ENotValid);
}

}

module 0x42::b {

use 0x42::a;

public fun check(x: u64) {
    a::check_valid!(x)
}

}
