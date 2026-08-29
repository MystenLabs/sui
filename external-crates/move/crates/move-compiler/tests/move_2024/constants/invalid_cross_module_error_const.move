// '#[error]' constants may not be declared 'public(package)'; rejected at the definition

module 0x42::a {

#[error]
public(package) const ENotFound: vector<u8> = b"not found";

}

module 0x42::b {

use 0x42::a;

public fun fail() {
    abort a::ENotFound
}

public fun check(cond: bool) {
    assert!(cond, a::ENotFound);
}
}
