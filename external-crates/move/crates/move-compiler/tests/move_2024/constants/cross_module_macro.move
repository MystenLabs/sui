// A constant reference in a macro body is a cross-module use when the macro expands in
// another module, even though no cross-module reference appears in the defining module's
// source

module 0x42::a {

public(package) const LIMIT: u64 = 10;

public macro fun clamp($x: u64): u64 {
    let x = $x;
    if (x > LIMIT) LIMIT else x
}

}

module 0x42::b {

use 0x42::a;

public fun clamped(x: u64): u64 {
    a::clamp!(x)
}

}
