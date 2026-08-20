// An unfoldable constant used only from another module's function body: one error at the
// definition and one at the use, with no ICE

module 0x42::a {

public(package) const BAD: u64 = 1 / 0;

}

module 0x42::b {

use 0x42::a;

public fun bad(): u64 {
    a::BAD
}

}
