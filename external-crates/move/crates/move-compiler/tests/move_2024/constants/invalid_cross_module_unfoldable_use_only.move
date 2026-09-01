// An unfoldable constant used only from another module's function body reports one error, at
// the definition

module 0x42::a {

public(package) const BAD: u64 = 1 / 0;

}

module 0x42::b {

use 0x42::a;

public fun bad(): u64 {
    a::BAD
}

}
