module 0x42::t {

const C_ZERO: u64 = 0;
const C_ONE: u64 = C_ZERO + 1;
const C_TWO: u64 = 2;
const C_VEC: vector<u64> = vector[C_ZERO, C_ONE, C_TWO];
const C_FIVE: u64 = 5;
const C_V2: vector<u64> = vector[C_ZERO, C_FIVE];

public fun foo(): u64 {
    C_ZERO 
}

public fun bar(): u64 {
    C_ONE 
}

public fun baz(): vector<u64> {
    C_VEC 
}

}
