module 0x42::t {

const C_ZERO: u64 = 0;
const C_ONE: u64 = if (C_ZERO == 0) { 1 } else { 2 };
const C_FIVE: u64 = 5;

}

module 0x42::d {

use 0x42::t::C_ZERO;

const C_ONE: u64 = C_ZERO + 1;
const C_V2: vector<u64> = vector[C_ZERO, C_FIVE];

}
