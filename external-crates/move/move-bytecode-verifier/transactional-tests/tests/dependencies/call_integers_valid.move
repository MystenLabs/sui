//# publish
module 0x42::m {
public fun plus8(a: u8): u8 { a + 1 }
public fun plus16(a: u16): u16 { a + 1 }
public fun plus32(a: u32): u32 { a + 1 }
public fun plus64(a: u64): u64 { a + 1 }
public fun plus128(a: u128): u128 { a + 1 }
public fun plus256(a: u256): u256 { a + 1 }
}


//# publish
module 0x42::m_test {
use 0x42::m;
public fun test8() { m::plus8(1u8); }
public fun test16() { m::plus16(1u16); }
public fun test32() { m::plus32(1u32); }
public fun test64() { m::plus64(1u64); }
public fun test128() { m::plus128(1u128); }
public fun test256() { m::plus256(1u256); }

}
