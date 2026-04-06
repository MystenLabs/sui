//# init --edition development

// Equality (==)
//# run
module 1::m {
fun main() {
    assert!(0i8 == 0i8, 1000);
    assert!(0i16 == 0i16, 1001);
    assert!(0i32 == 0i32, 1002);
    assert!(0i64 == 0i64, 1003);
    assert!(0i128 == 0i128, 1004);
    assert!(0i256 == 0i256, 1005);

    assert!(!(0i8 == 1i8), 1100);
    assert!(!(0i16 == 1i16), 1101);
    assert!(!(0i32 == 1i32), 1102);
    assert!(!(0i64 == 1i64), 1103);
    assert!(!(0i128 == 1i128), 1104);
    assert!(!(0i256 == 1i256), 1105);

    assert!(!(1i8 == 0i8), 1200);
    assert!(!(1i16 == 0i16), 1201);
    assert!(!(1i32 == 0i32), 1202);
    assert!(!(1i64 == 0i64), 1203);
    assert!(!(1i128 == 0i128), 1204);
    assert!(!(1i256 == 0i256), 1205);

    // Negative values
    assert!(-1i8 == -1i8, 1300);
    assert!(-1i16 == -1i16, 1301);
    assert!(-1i32 == -1i32, 1302);
    assert!(-1i64 == -1i64, 1303);
    assert!(-1i128 == -1i128, 1304);
    assert!(-1i256 == -1i256, 1305);

    assert!(!(-1i8 == 1i8), 1400);
    assert!(!(-1i16 == 1i16), 1401);
    assert!(!(-1i32 == 1i32), 1402);
    assert!(!(-1i64 == 1i64), 1403);
    assert!(!(-1i128 == 1i128), 1404);
    assert!(!(-1i256 == 1i256), 1405);
}
}

// Inequality (!=)
//# run
module 2::m {
fun main() {
    assert!(0i8 != 1i8, 2000);
    assert!(0i16 != 1i16, 2001);
    assert!(0i32 != 1i32, 2002);
    assert!(0i64 != 1i64, 2003);
    assert!(0i128 != 1i128, 2004);
    assert!(0i256 != 1i256, 2005);

    assert!(1i8 != 0i8, 2100);
    assert!(1i16 != 0i16, 2101);
    assert!(1i32 != 0i32, 2102);
    assert!(1i64 != 0i64, 2103);
    assert!(1i128 != 0i128, 2104);
    assert!(1i256 != 0i256, 2105);

    assert!(!(0i8 != 0i8), 2200);
    assert!(!(0i16 != 0i16), 2201);
    assert!(!(0i32 != 0i32), 2202);
    assert!(!(0i64 != 0i64), 2203);
    assert!(!(0i128 != 0i128), 2204);
    assert!(!(0i256 != 0i256), 2205);

    // Negative values
    assert!(-1i8 != 1i8, 2300);
    assert!(-1i16 != 1i16, 2301);
    assert!(-1i32 != 1i32, 2302);
    assert!(-1i64 != 1i64, 2303);
    assert!(-1i128 != 1i128, 2304);
    assert!(-1i256 != 1i256, 2305);

    assert!(!(-1i8 != -1i8), 2400);
    assert!(!(-1i16 != -1i16), 2401);
    assert!(!(-1i32 != -1i32), 2402);
    assert!(!(-1i64 != -1i64), 2403);
    assert!(!(-1i128 != -1i128), 2404);
    assert!(!(-1i256 != -1i256), 2405);
}
}

// Less than (<)
//# run
module 3::m {
fun main() {
    assert!(0i8 < 1i8, 3000);
    assert!(0i16 < 1i16, 3001);
    assert!(0i32 < 1i32, 3002);
    assert!(0i64 < 1i64, 3003);
    assert!(0i128 < 1i128, 3004);
    assert!(0i256 < 1i256, 3005);

    assert!(!(1i8 < 0i8), 3100);
    assert!(!(1i16 < 0i16), 3101);
    assert!(!(1i32 < 0i32), 3102);
    assert!(!(1i64 < 0i64), 3103);
    assert!(!(1i128 < 0i128), 3104);
    assert!(!(1i256 < 0i256), 3105);

    assert!(!(0i8 < 0i8), 3200);
    assert!(!(0i16 < 0i16), 3201);
    assert!(!(0i32 < 0i32), 3202);
    assert!(!(0i64 < 0i64), 3203);
    assert!(!(0i128 < 0i128), 3204);
    assert!(!(0i256 < 0i256), 3205);

    // Negative values
    assert!(-1i8 < 0i8, 3300);
    assert!(-1i16 < 0i16, 3301);
    assert!(-1i32 < 0i32, 3302);
    assert!(-1i64 < 0i64, 3303);
    assert!(-1i128 < 0i128, 3304);
    assert!(-1i256 < 0i256, 3305);

    assert!(-2i8 < -1i8, 3400);
    assert!(-2i16 < -1i16, 3401);
    assert!(-2i32 < -1i32, 3402);
    assert!(-2i64 < -1i64, 3403);
    assert!(-2i128 < -1i128, 3404);
    assert!(-2i256 < -1i256, 3405);

    // Boundary: MIN < MAX
    assert!(-128i8 < 127i8, 3500);
    assert!(-32768i16 < 32767i16, 3501);
    assert!(-2147483648i32 < 2147483647i32, 3502);
    assert!(-9223372036854775808i64 < 9223372036854775807i64, 3503);
}
}

// Greater than (>)
//# run
module 4::m {
fun main() {
    assert!(1i8 > 0i8, 4000);
    assert!(1i16 > 0i16, 4001);
    assert!(1i32 > 0i32, 4002);
    assert!(1i64 > 0i64, 4003);
    assert!(1i128 > 0i128, 4004);
    assert!(1i256 > 0i256, 4005);

    assert!(!(0i8 > 1i8), 4100);
    assert!(!(0i16 > 1i16), 4101);
    assert!(!(0i32 > 1i32), 4102);
    assert!(!(0i64 > 1i64), 4103);
    assert!(!(0i128 > 1i128), 4104);
    assert!(!(0i256 > 1i256), 4105);

    assert!(!(0i8 > 0i8), 4200);
    assert!(!(0i16 > 0i16), 4201);
    assert!(!(0i32 > 0i32), 4202);
    assert!(!(0i64 > 0i64), 4203);
    assert!(!(0i128 > 0i128), 4204);
    assert!(!(0i256 > 0i256), 4205);

    // Negative values
    assert!(0i8 > -1i8, 4300);
    assert!(0i16 > -1i16, 4301);
    assert!(0i32 > -1i32, 4302);
    assert!(0i64 > -1i64, 4303);
    assert!(0i128 > -1i128, 4304);
    assert!(0i256 > -1i256, 4305);

    assert!(-1i8 > -2i8, 4400);
    assert!(-1i16 > -2i16, 4401);
    assert!(-1i32 > -2i32, 4402);
    assert!(-1i64 > -2i64, 4403);
    assert!(-1i128 > -2i128, 4404);
    assert!(-1i256 > -2i256, 4405);

    // Boundary: MAX > MIN
    assert!(127i8 > -128i8, 4500);
    assert!(32767i16 > -32768i16, 4501);
    assert!(2147483647i32 > -2147483648i32, 4502);
    assert!(9223372036854775807i64 > -9223372036854775808i64, 4503);
}
}

// Less than or equal (<=)
//# run
module 5::m {
fun main() {
    assert!(0i8 <= 1i8, 5000);
    assert!(0i16 <= 1i16, 5001);
    assert!(0i32 <= 1i32, 5002);
    assert!(0i64 <= 1i64, 5003);
    assert!(0i128 <= 1i128, 5004);
    assert!(0i256 <= 1i256, 5005);

    assert!(!(1i8 <= 0i8), 5100);
    assert!(!(1i16 <= 0i16), 5101);
    assert!(!(1i32 <= 0i32), 5102);
    assert!(!(1i64 <= 0i64), 5103);
    assert!(!(1i128 <= 0i128), 5104);
    assert!(!(1i256 <= 0i256), 5105);

    assert!(0i8 <= 0i8, 5200);
    assert!(0i16 <= 0i16, 5201);
    assert!(0i32 <= 0i32, 5202);
    assert!(0i64 <= 0i64, 5203);
    assert!(0i128 <= 0i128, 5204);
    assert!(0i256 <= 0i256, 5205);

    // Negative values
    assert!(-1i8 <= 0i8, 5300);
    assert!(-1i8 <= -1i8, 5301);
    assert!(-128i8 <= -128i8, 5302);
    assert!(-128i8 <= 127i8, 5303);
}
}

// Greater than or equal (>=)
//# run
module 6::m {
fun main() {
    assert!(1i8 >= 0i8, 6000);
    assert!(1i16 >= 0i16, 6001);
    assert!(1i32 >= 0i32, 6002);
    assert!(1i64 >= 0i64, 6003);
    assert!(1i128 >= 0i128, 6004);
    assert!(1i256 >= 0i256, 6005);

    assert!(!(0i8 >= 1i8), 6100);
    assert!(!(0i16 >= 1i16), 6101);
    assert!(!(0i32 >= 1i32), 6102);
    assert!(!(0i64 >= 1i64), 6103);
    assert!(!(0i128 >= 1i128), 6104);
    assert!(!(0i256 >= 1i256), 6105);

    assert!(0i8 >= 0i8, 6200);
    assert!(0i16 >= 0i16, 6201);
    assert!(0i32 >= 0i32, 6202);
    assert!(0i64 >= 0i64, 6203);
    assert!(0i128 >= 0i128, 6204);
    assert!(0i256 >= 0i256, 6205);

    // Negative values
    assert!(0i8 >= -1i8, 6300);
    assert!(-1i8 >= -1i8, 6301);
    assert!(127i8 >= -128i8, 6302);
    assert!(-128i8 >= -128i8, 6303);
}
}
