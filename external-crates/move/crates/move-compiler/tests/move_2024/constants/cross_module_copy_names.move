// Constants whose module and constant names concatenate ambiguously (`x::A_B` and `x_A::B`)
// stay distinct when both are used from one module

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
