//# init --edition development

// vector<i8>: pack, len, borrow, push, pop, swap
//# run
module 1::m {
fun main() {
    let v = vector[1i8, -1i8, 0i8, 127i8, -128i8];
    assert!(v.length() == 5, 1000);
    assert!(*v.borrow(0) == 1i8, 1001);
    assert!(*v.borrow(1) == -1i8, 1002);
    assert!(*v.borrow(4) == -128i8, 1003);

    v.push_back(42i8);
    assert!(v.length() == 6, 1010);
    let last = v.pop_back();
    assert!(last == 42i8, 1011);

    v.swap(0, 1);
    assert!(*v.borrow(0) == -1i8, 1020);
    assert!(*v.borrow(1) == 1i8, 1021);
}
}

// vector<i16>
//# run
module 2::m {
fun main() {
    let v = vector[100i16, -200i16, 32767i16, -32768i16];
    assert!(v.length() == 4, 2000);
    assert!(*v.borrow(0) == 100i16, 2001);
    assert!(*v.borrow(3) == -32768i16, 2002);

    v.push_back(0i16);
    let last = v.pop_back();
    assert!(last == 0i16, 2010);

    v.swap(0, 3);
    assert!(*v.borrow(0) == -32768i16, 2020);
    assert!(*v.borrow(3) == 100i16, 2021);
}
}

// vector<i32>
//# run
module 3::m {
fun main() {
    let v = vector[1000i32, -1000i32, 2147483647i32, -2147483648i32];
    assert!(v.length() == 4, 3000);
    assert!(*v.borrow(2) == 2147483647i32, 3001);
    assert!(*v.borrow(3) == -2147483648i32, 3002);

    v.push_back(-999i32);
    assert!(v.length() == 5, 3010);
    let last = v.pop_back();
    assert!(last == -999i32, 3011);
}
}

// vector<i64>
//# run
module 4::m {
fun main() {
    let v = vector[9223372036854775807i64, -9223372036854775808i64, 0i64];
    assert!(v.length() == 3, 4000);
    assert!(*v.borrow(0) == 9223372036854775807i64, 4001);
    assert!(*v.borrow(1) == -9223372036854775808i64, 4002);

    v.swap(0, 1);
    assert!(*v.borrow(0) == -9223372036854775808i64, 4010);
    assert!(*v.borrow(1) == 9223372036854775807i64, 4011);
}
}

// vector<i128>
//# run
module 5::m {
fun main() {
    let v = vector[0i128, 1i128, -1i128];
    assert!(v.length() == 3, 5000);
    assert!(*v.borrow(2) == -1i128, 5001);

    v.push_back(170141183460469231731687303715884105727i128);
    v.push_back(-170141183460469231731687303715884105728i128);
    assert!(v.length() == 5, 5010);
    assert!(*v.borrow(3) == 170141183460469231731687303715884105727i128, 5011);
    assert!(*v.borrow(4) == -170141183460469231731687303715884105728i128, 5012);
}
}

// vector<i256>
//# run
module 6::m {
fun main() {
    let v = vector[0i256, 1i256, -1i256];
    assert!(v.length() == 3, 6000);
    assert!(*v.borrow(0) == 0i256, 6001);
    assert!(*v.borrow(2) == -1i256, 6002);

    v.push_back(42i256);
    let last = v.pop_back();
    assert!(last == 42i256, 6010);
}
}

// Empty vectors of signed types
//# run
module 7::m {
fun main() {
    let v1: vector<i8> = vector[];
    assert!(v1.length() == 0, 7000);
    let v2: vector<i16> = vector[];
    assert!(v2.length() == 0, 7001);
    let v3: vector<i32> = vector[];
    assert!(v3.length() == 0, 7002);
    let v4: vector<i64> = vector[];
    assert!(v4.length() == 0, 7003);
    let v5: vector<i128> = vector[];
    assert!(v5.length() == 0, 7004);
    let v6: vector<i256> = vector[];
    assert!(v6.length() == 0, 7005);
}
}

// Mut borrow and write through reference
//# run
module 8::m {
fun main() {
    let v = vector[10i32, 20i32, 30i32];
    let r = v.borrow_mut(1);
    *r = -999i32;
    assert!(*v.borrow(1) == -999i32, 8000);
}
}
