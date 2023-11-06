//# run
script {
fun main() {
    assert!(0u8 == 0u8, 1000);
    assert!(0u64 == 0u64, 1001);
    assert!(0u128 == 0u128, 1002);
    assert!(0u16 == 0u16, 1003);
    assert!(0u32 == 0u32, 1004);
    assert!(0u256 == 0u256, 1005);

    assert!(!(0u8 == 1u8), 1100);
    assert!(!(0u64 == 1u64), 1101);
    assert!(!(0u128 == 1u128), 1102);
    assert!(!(0u16 == 1u16), 1103);
    assert!(!(0u32 == 1u32), 1104);
    assert!(!(0u256 == 1u256), 1105);


    assert!(!(1u8 == 0u8), 1200);
    assert!(!(1u64 == 0u64), 1201);
    assert!(!(1u128 == 0u128), 1202);
    assert!(!(1u16 == 0u16), 1203);
    assert!(!(1u32 == 0u32), 1204);
    assert!(!(1u256 == 0u256), 1205);
}
}

//# run
script {
fun main() {
    assert!(0u8 != 1u8, 2000);
    assert!(0u64 != 1u64, 2001);
    assert!(0u128 != 1u128, 2001);

    assert!(1u8 != 0u8, 2100);
    assert!(1u64 != 0u64, 2101);
    assert!(1u128 != 0u128, 2101);

    assert!(!(0u8 != 0u8), 2200);
    assert!(!(0u64 != 0u64), 2201);
    assert!(!(0u128 != 0u128), 2201);

    assert!(0u16 != 1u16, 2000);
    assert!(0u32 != 1u32, 2001);
    assert!(0u256 != 1u256, 2001);

    assert!(1u16 != 0u16, 2100);
    assert!(1u32 != 0u32, 2101);
    assert!(1u256 != 0u256, 2101);

    assert!(!(0u16 != 0u16), 2200);
    assert!(!(0u32 != 0u32), 2201);
    assert!(!(0u256 != 0u256), 2201);
}
}

//# run
script {
fun main() {
    assert!(0u8 < 1u8, 3000);
    assert!(0u64 < 1u64, 3001);
    assert!(0u128 < 1u128, 3002);

    assert!(!(1u8 < 0u8), 3100);
    assert!(!(1u64 < 0u64), 3101);
    assert!(!(1u128 < 0u128), 3102);

    assert!(!(0u8 < 0u8), 3200);
    assert!(!(0u64 < 0u64), 3201);
    assert!(!(0u128 < 0u128), 3202);

    assert!(0u16 < 1u16, 3000);
    assert!(0u32 < 1u32, 3001);
    assert!(0u256 < 1u256, 3002);

    assert!(!(1u16 < 0u16), 3100);
    assert!(!(1u32 < 0u32), 3101);
    assert!(!(1u256 < 0u256), 3102);

    assert!(!(0u16 < 0u16), 3200);
    assert!(!(0u32 < 0u32), 3201);
    assert!(!(0u256 < 0u256), 3202);
}
}

//# run
script {
fun main() {
    assert!(1u8 > 0u8, 4000);
    assert!(1u64 > 0u64, 4001);
    assert!(1u128 > 0u128, 4002);

    assert!(!(0u8 > 1u8), 4100);
    assert!(!(0u64 > 1u64), 4101);
    assert!(!(0u128 > 1u128), 4102);

    assert!(!(0u8 > 0u8), 4200);
    assert!(!(0u64 > 0u64), 4201);
    assert!(!(0u128 > 0u128), 4202);

    assert!(1u16 > 0u16, 4000);
    assert!(1u32 > 0u32, 4001);
    assert!(1u256 > 0u256, 4002);

    assert!(!(0u16 > 1u16), 4100);
    assert!(!(0u32 > 1u32), 4101);
    assert!(!(0u256 > 1u256), 4102);

    assert!(!(0u16 > 0u16), 4200);
    assert!(!(0u32 > 0u32), 4201);
    assert!(!(0u256 > 0u256), 4202);
}
}

//# run
script {
fun main() {
    assert!(0u8 <= 1u8, 5000);
    assert!(0u64 <= 1u64, 5001);
    assert!(0u128 <= 1u128, 5002);

    assert!(!(1u8 <= 0u8), 5100);
    assert!(!(1u64 <= 0u64), 5101);
    assert!(!(1u128 <= 0u128), 5102);

    assert!(0u8 <= 0u8, 5200);
    assert!(0u64 <= 0u64, 5201);
    assert!(0u128 <= 0u128, 5202);

    assert!(0u16 <= 1u16, 5000);
    assert!(0u32 <= 1u32, 5001);
    assert!(0u256 <= 1u256, 5002);

    assert!(!(1u16 <= 0u16), 5100);
    assert!(!(1u32 <= 0u32), 5101);
    assert!(!(1u256 <= 0u256), 5102);

    assert!(0u16 <= 0u16, 5200);
    assert!(0u32 <= 0u32, 5201);
    assert!(0u256 <= 0u256, 5202);
}
}

//# run
script {
fun main() {
    assert!(1u8 >= 0u8, 6000);
    assert!(1u64 >= 0u64, 6001);
    assert!(1u128 >= 0u128, 6002);

    assert!(!(0u8 >= 1u8), 6100);
    assert!(!(0u64 >= 1u64), 6101);
    assert!(!(0u128 >= 1u128), 6102);

    assert!(0u8 >= 0u8, 6200);
    assert!(0u64 >= 0u64, 6201);
    assert!(0u128 >= 0u128, 6202);

    assert!(1u16 >= 0u16, 6000);
    assert!(1u32 >= 0u32, 6001);
    assert!(1u256 >= 0u256, 6002);

    assert!(!(0u16 >= 1u16), 6100);
    assert!(!(0u32 >= 1u32), 6101);
    assert!(!(0u256 >= 1u256), 6102);

    assert!(0u16 >= 0u16, 6200);
    assert!(0u32 >= 0u32, 6201);
    assert!(0u256 >= 0u256, 6202);
}
}
