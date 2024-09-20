//# publish
module 0x42::m {

    fun and(x: bool, y: bool): bool {
        x && y
    }

    fun add(a: u64, b: u64): u64 {
        a + b
    }

    fun sub(a: u64, b: u64): u64 {
        a - b
    }

    fun inc_by(x: &mut u64, y: u64): u64 {
        *x = *x + y;
        *x
    }

    fun inc(x: &mut u64): u64 {
        *x = *x + 1;
        *x
    }

    public fun test00(): u64 {
        let a = 1;
        a + {a = a + 1; a}
    }

    public fun test01(): bool {
        let x = 1;
        and({x = x - 1; x == 0}, {x = x + 3; x == 3}) && {x = x * 2; x == 6}
    }

    public fun test02(): u64 {
        let x = 10;
        let y = 20;
        {let (x, y) = (y, x + 1); x + y} + {let (x, y) = (y * 2, x - 1); y / x}
    }

    public fun test03(): u64 {
        let x = 1;
        add(add({x = x - 1; x + 8}, {x = x + 3; x - 3}), add({x = x * 2; x * 2}, {x = x + 1; x}))
    }

    public fun test04(): u64 {
        let x = 1;
        add({x = x - 1; x + 8}, {x = x + 3; x - 3}) + add({x = x * 2; x * 2}, {x = x + 1; x})
    }

    public fun test05(): u64 {
        let _x = 1;
        {let y = _x + 2; _x = _x + 5; y} +
            {let y = _x; _x = _x - 1; y} +
            {let y = _x; _x = _x * 2; y}
    }

    public fun test06(): u64 {
        let x = 1;
        inc_by(&mut x, 5) + inc_by(&mut x, 6) + inc_by(&mut x, 7)
    }

    public fun test07(): u64 {
        let x = 1;
        add({x = x - 1; x + 8}, {x = x + 3; x - 3}) + {x = x * 2; x * 2}
    }

    public fun test08(): u64 {
        let x = 158;
        {x = x - 1; x + 8} + add({x = x + 3; x - 3}, {x = x * 2; x * 2})
    }

    public fun test09(): u64 {
        let x = 146;
        add({x = sub(x, 1); x + 8}, {x = x + 3; x - 3}) + {x = x * 2; x * 2}
    }

    public fun test10(): u64 {
        let x = 122;
        {x = x - 1; x + 8} + {x = x + 3; x - 3} + {x = x * 2; x * 2}
    }

    #[allow(dead_code)]
    public fun test11(): u64 {
        let x = 1;
        {return x; x + 1} + {x = x + 1; x} + {x = x + 1; x}
    }

    #[allow(dead_code)]
    public fun test12(): u64 {
        let x = 1;
        {x = x + 1; x = x + 1; x } + {x = x + 1; return x; x } + {x = x + 1; x = x + 1; x }
    }

    public fun test13(): u64 {
        let x = 1;
        {let one = 1; x = x + one; x} +
            {{let two = 2; x = x + two; x} + {let three = 3; x = x + three; x} + x} +
            {x = x + 1; x = x + 1; x }
    }

    public fun test14(): u64 {
        let x = 1;
        {x = x + 1; x = x + 1; x } + {x = x + 1; x = x + 1; x } + {x = x + 1; x = x + 1; x}
    }

    public fun test15(): u64 {
        let x = 1;
        inc(&mut x) +
            { inc(&mut x); inc(&mut x) + inc(&mut x) } +
            { inc(&mut x); inc(&mut x); inc(&mut x) }
    }

    public fun test16(): u64 {
        let x = 1;
        inc(&mut {x = x + 1; x}) +
            { inc(&mut x); inc(&mut x) + inc(&mut x) } +
            { inc(&mut x); inc(&mut x); inc(&mut x) }
    }

    public fun test17(p: u64): u64 {
        let x = p;
        (x + 1) + {x = x + 1; x + 1} + {x = x + 1; x + 1}
    }

    public fun test18(p: bool): u64 {
        let x = 1;
        if (p) {x} else {x = x + 1; x} + if (!p) {x} else {x = x + 1; x} + if (!p) {x} else {x = x + 1; x}
    }

    public fun test19(): u64 {
        let x = 1;
        x + {x = {x + {x = x + 1; x} + {x = x + 1; x}}; x} + {x = {x + {x = x + 1; x} + {x = x + 1; x}}; x}
    }

   public fun test20(p: bool): bool {
        (!p && {p = p && false; p}) || {p = !p; !p}
    }

    public fun test21(): bool {
        let x = 1;
        {x = x << 1; x} < {x = x << 1; x}
    }

    public fun test22(p: u64): vector<u64> {
        vector[p, {p = p + 1; p}, {p = p + 1; p}]
    }

    fun add2(x: u64, y: u64): u64 {
        x + y
    }

    fun add3(x: u64, y: u64, z: u64): u64 {
        x + y + z
    }

    public fun test23(): u64 {
        let x = 1;
        add3(x, {x = add2(x, 1); x}, {x = add2(x, 1); x})
    }

    public fun test24(): u64 {
        let x = 1;
        x + add3(x, {x = inc(&mut x); add3(x, {x = x + 1; x}, {x = x + 1; x})}, {x = inc(&mut x); add3({x = x + 1; x}, x, {x = x + 1; x})}) + {x = inc(&mut x) + 1; x}
    }

    public fun test25(): u64 {
        let x = 1;
        x + add3(x, {x = inc_by(&mut x, 3); add3(x, {x = x + 1; x}, {x = x + 1; x})}, {x = inc(&mut x); add3({x = x + 1; x}, x, {x = x + 1; x})}) + {x = inc_by(&mut x, 47) + 1; x}
    }

     public fun test26(): u64 {
        let x = 1;
        x + {x = inc(&mut x) + 1; x} + {x = inc(&mut x) + 1; x}
    }

    public fun test27(): u64 {
        let a = 1;
        let x;
        let y;
        let z;
        (x, y, z) = (a, {a = a + 1; a}, {a = a + 1; a});
        x + y + z
    }

    public fun test28(): u64 {
        let x = 1;
        x + inc_by(&mut x, 7) + inc_by(&mut x, 11)
    }

    public fun test29(): u64 {
        let x = 1;
        let (a, b, c) = (x + 1, {x = x + 1; x + 7}, {x = x + 1; x - 3});
        a + b + c
    }

    public fun test30(): u64 {
        let x = 1;
        (x + {x = x + 1; x - 1}) + {x = x + 1; x * 2}
    }

    public fun test31(): u64 {
        let x = 1;
        {x + {x = x + 1; x}} + {x = x + 1; x}
    }

    public fun test32(): u64 {
        let x = 1;
        {x = x + 1; x} + x + {x = x + 1; x}
    }

    public fun test33(): u64 {
        let x = 1;
        x + {x = x + 1; x} + {x = x + 1; x}
    }

    #[allow(dead_code)]
    public fun test34(): u64 {
        let x = 1;
        {return x} + {x = x + 1; x} + {x = x + 1; x}
    }

    public fun test35(): u64 {
        let x = 1;
        add3(x, { x = x + 1; x }, { x = x + 1; x })
    }

    public fun test36(p: u64): u64 {
        1 + (p + {p = p + 1; p})
    }
}

// Run the tests separately so that we get all the results

//# run
module 0x43::test00 { public fun main() { assert!(0x42::m::test00() == 3, 0); } }

//# run
module 0x44::test01 { public fun main() { assert!(0x42::m::test01() == true, 1); } }

//# run
module 0x45::test02 { public fun main() { assert!(0x42::m::test02() == 31, 2); } }

//# run
module 0x46::test03 { public fun main() { assert!(0x42::m::test03() == 27, 3); } }

//# run
module 0x47::test04 { public fun main() { assert!(0x42::m::test04() == 27, 4); } }

//# run
module 0x48::test05 { public fun main() { assert!(0x42::m::test05() == 14, 5); } }

//# run
module 0x49::test06 { public fun main() { assert!(0x42::m::test06() == 37, 6); } }

//# run
module 0x4a::test07 { public fun main() { assert!(0x42::m::test07() == 20, 7); } }

//# run
module 0x4b::test08 { public fun main() { assert!(0x42::m::test08() == 962, 8); } }

//# run
module 0x4c::test09 { public fun main() { assert!(0x42::m::test09() == 890, 9); } }

//# run
module 0x4d::test10 { public fun main() { assert!(0x42::m::test10() == 746, 10); } }

//# run
module 0x4e::test11 { public fun main() { assert!(0x42::m::test11() == 1, 11); } }

//# run
module 0x4f::test12 { public fun main() { assert!(0x42::m::test12() == 4, 12); } }

//# run
module 0x50::test13 { public fun main() { assert!(0x42::m::test13() == 29, 13); } }

//# run
module 0x51::test14 { public fun main() { assert!(0x42::m::test14() == 15, 14); } }

//# run
module 0x52::test15 { public fun main() { assert!(0x42::m::test15() == 19, 15); } }

//# run
module 0x53::test16 { public fun main() { assert!(0x42::m::test16() == 20, 16); } }

//# run
module 0x54::test17 { public fun main() { assert!(0x42::m::test17(54) == 168, 17); } }

//# run
module 0x55::test18 { public fun main() { assert!(0x42::m::test18(true) == 6, 18); } }

//# run
module 0x56::test19 { public fun main() { assert!(0x42::m::test18(false) == 6, 18); } }

//# run
module 0x57::test20 { public fun main() { assert!(0x42::m::test19() == 28, 18); } }

//# run
module 0x58::test21 { public fun main() { assert!(0x42::m::test20(true) == true, 20); } }

//# run
module 0x59::test22 { public fun main() { assert!(0x42::m::test20(false) == false, 20); } }

//# run
module 0x5a::test23 { public fun main() { assert!(0x42::m::test21() == true, 21); } }

//# run
module 0x5b::test24 { public fun main() { assert!(0x42::m::test22(3) == vector[3,4,5], 22); } }

//# run
module 0x5c::test25 { public fun main() { assert!(0x42::m::test23() == 6, 23); } }

//# run
module 0x5d::test26 { public fun main() { assert!(0x42::m::test24() == 39, 24); } }

//# run
module 0x5e::test27 { public fun main() { assert!(0x42::m::test25() == 99, 25); } }

//# run
module 0x5f::test28 { public fun main() { assert!(0x42::m::test26() == 9, 26); } }

//# run
module 0x60::test29 { public fun main() { assert!(0x42::m::test27() == 6, 27); } }

//# run
module 0x61::test30 { public fun main() { assert!(0x42::m::test28() == 28, 28); } }

//# run
module 0x62::test31 { public fun main() { assert!(0x42::m::test29() == 11, 29); } }

//# run
module 0x63::test32 { public fun main() { assert!(0x42::m::test30() == 8, 30); } }

//# run
module 0x64::test33 { public fun main() { assert!(0x42::m::test31() == 6, 31); } }

//# run
module 0x65::test34 { public fun main() { assert!(0x42::m::test32() == 7, 32); } }

//# run
module 0x66::test35 { public fun main() { assert!(0x42::m::test33() == 6, 33); } }

//# run
module 0x67::test36 { public fun main() { assert!(0x42::m::test34() == 1, 34); } }

//# run
module 0x68::test37 { public fun main() { assert!(0x42::m::test35() == 6, 35); } }

//# run
module 0x69::test38 { public fun main() { assert!(0x42::m::test36(1) == 4, 36); } }

