// Constants cannot be called as functions

module 0x42::a {

public(package) const MAX: u64 = 100;

public fun touch(): u64 { MAX }

}

module 0x42::b {

use 0x42::a;

public fun call_const(): u64 { a::MAX() }

public fun use_it(): u64 { a::MAX }

}
