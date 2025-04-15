// emit a warning during code generation for equal operands in binary operations that result
// in a constant value
module a::m;

fun test_equal_operands_comparison() {
    1 == 1;
    {1} == 1;
    1 == {1};
    {1} == {1};
    {{{1}}} == {{{{1}}}};
}

fun test_equal_operands_values() {
    false == false;
    0u8 == 0u8;
    1u16 == 1u16;
    2u32 == 2u32;
    3u64 == 3u64;
    4u128 == 4u128;
    5u256 == 5u256;
    @a == @a;
    vector<vector<u8>>[] == vector[];
    vector[1] == vector[1];
}
