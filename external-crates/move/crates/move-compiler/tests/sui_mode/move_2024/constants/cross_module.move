// Cross-module constant uses do not trip sui-mode checks

module 0x42::a {

public(package) const MAX: u64 = 100;

}

module 0x42::b {

use 0x42::a;

const DOUBLE: u64 = a::MAX * 2;

public fun max(): u64 { a::MAX }
public fun double(): u64 { DOUBLE }

}
