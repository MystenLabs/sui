//# init --edition 2024.alpha

//# publish

module 0x42::a;

public struct SingleFieldStruct {
    f: u32
}

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
    let mut value_f;
    SingleFieldStruct{ f: value_f } = SingleFieldStruct { f: 0 };
    SingleFieldStruct{ f: value_f } = s;
    value_f
}

public struct Pos(u8, u8)

#[allow(unused_assignment)]
public fun pos_struct(value: u8): u8 {
    let p = Pos(value, 0);
    let mut value_1 = 0;
    let mut value_2 = 0;
    Pos(value_1, value_2) = p;
    value_1
}

#[allow(unused_assignment)]
public fun pos_struct_2(value: u8): u8 {
    let p = Pos(value, 0);
    let mut value_1;
    let mut value_2;
    Pos(value_1, value_2) = Pos(0, 0);
    Pos(value_1, value_2) = p;
    value_1
}

public struct Box { top_left: Pos, bottom_right: Pos }

#[allow(unused_assignment)]
public fun box(value: u8): u8 {
    let b = Box { top_left: Pos(value, 0), bottom_right: Pos(0, 0) };
    let mut top_left_x = 0;
    let mut top_left_y = 0;
    let mut bottom_right_x = 0;
    let mut bottom_right_y = 0;
    Box {
        top_left: Pos(top_left_x, top_left_y),
        bottom_right: Pos(bottom_right_x, bottom_right_y),
    } = b;
    top_left_x
}

#[allow(unused_assignment)]
public fun box_2(value: u8): u8 {
    let b = Box { top_left: Pos(value, 0), bottom_right: Pos(0, 0) };
    let mut top_left_x;
    let mut top_left_y;
    let mut bottom_right_x;
    let mut bottom_right_y;
    Box {
        top_left: Pos(top_left_x, top_left_y),
        bottom_right: Pos(bottom_right_x, bottom_right_y),
    } = Box { top_left: Pos(0, 0), bottom_right: Pos(0, 0) };
    Box {
        top_left: Pos(top_left_x, top_left_y),
        bottom_right: Pos(bottom_right_x, bottom_right_y),
    } = b;
    top_left_x
}

public fun assert_42() {
    assert!(single_field_struct(42) == 42);
    assert!(single_field_struct_2(42) == 42);
    assert!(pos_struct(42) == 42);
    assert!(pos_struct_2(42) == 42);
    assert!(box(42) == 42);
    assert!(box_2(42) == 42);
}

//# run 0x42::a::assert_42
