//# init --edition 2024.alpha

//# publish

module 0x42::a;

public struct SingleFieldStruct {
    f: u32
}

public struct Pos(u32, u32)

#[allow(unused_assignment)]
public fun single_field_struct(value: u32): u32 {
    let s = SingleFieldStruct{ f: value };
    let mut value_f = 0;
    SingleFieldStruct{ f: value_f } = s;
    value_f
}

#[allow(unused_assignment)]
public fun single_field_struct_2(value: u32): u32 {
    let s = SingleFieldStruct{ f: value };
    let mut value_f = 0;
    SingleFieldStruct{ f: value_f } = SingleFieldStruct { f: 0 };
    SingleFieldStruct{ f: value_f } = s;
    value_f
}

#[allow(unused_assignment)]
public fun pos_struct(value: u32): u32 {
    let p = Pos(value, 0);
    let mut value_1 = 0;
    let mut value_2 = 0;
    Pos(value_1, value_2) = p;
    value_1
}

#[allow(unused_assignment)]
public fun pos_struct_2(value: u32): u32 {
    let p = Pos(value, 0);
    let mut value_1 = 0;
    let mut value_2 = 0;
    Pos(value_1, value_2) = Pos(0, 0);
    Pos(value_1, value_2) = p;
    value_1
}


public fun assert_42() {
    assert!(single_field_struct(42) == 42);
    assert!(single_field_struct_2(42) == 42);
    assert!(pos_struct(42) == 42);
    assert!(pos_struct_2(42) == 42);
}

//# run 0x42::a::assert_42
