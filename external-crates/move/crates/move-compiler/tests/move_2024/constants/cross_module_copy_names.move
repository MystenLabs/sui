// Constants from modules whose names would collide under identifier-only mangling
// (`x::A_B` and `x_A::B` would both render as `_x_A_B`) get distinct copy names, since the
// copy names use the `#` generated-name delimiter (`const#x_A#B`)

module 0x42::x {

public(package) const A_B: u64 = 1;

}

module 0x42::x_A {

public(package) const B: u64 = 2;

}

module 0x42::user {

use 0x42::x;
use 0x42::x_A;

public fun both(): u64 {
    x::A_B + x_A::B
}

}
