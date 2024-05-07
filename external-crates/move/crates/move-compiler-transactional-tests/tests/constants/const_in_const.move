//# init --edition 2024.alpha

//# publish

module 0x42::t {

const C_ZERO: u64 = 0;
const C_ONE: u64 = C_ZERO + 1;
const C_TWO: u64 = 2;
const C_VEC: vector<u64> = vector[C_ZERO, C_ONE, C_TWO];
const C_FIVE: u64 = 5;
const C_V2: vector<u64> = vector[C_ZERO, C_FIVE];
const C_VS: vector<vector<u64>> = vector[C_VEC, C_V2];

public fun test() {
    assert!(C_ZERO == 0, 0);
    assert!(C_ONE == 1, 1);
    assert!(C_TWO == 2, 2);
    assert!(C_VEC == vector[0, 1, 2], 3);
    assert!(C_FIVE == 5, 5);
    assert!(C_V2 == vector[0, 5], 6);
    assert!(C_VS == vector[vector[0, 1, 2], vector[0, 5]], 6);
}

}

//# run 0x42::t::test
