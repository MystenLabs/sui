//# init --edition 2024.alpha

//# publish

module 0x42::a;

public struct SingleFieldStruct {
    f: u32
}

#[allow(unused_assignment)]
public fun single_field_struct(value: u32): u32 {
    let s = SingleFieldStruct{ f: value };
    let mut value_f = 0_u32;
    SingleFieldStruct{ f: value_f } = s;
    value_f
}


public fun is_that_42() {
    assert!(single_field_struct(42) == 42);
}

public fun is_that_zero() {
    assert!(single_field_struct(0) == 0);
}

//# run 0x42::a::is_that_42

//# run 0x42::a::is_that_zero
