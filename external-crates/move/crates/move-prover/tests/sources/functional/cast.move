module 0x42::TestCast {

    spec module {
        pragma verify = true;
    }

    // --------------
    // Type promotion
    // --------------

    fun u8_cast_incorrect(x: u8): u64 {
        (x as u64)
    }
    spec u8_cast_incorrect {
        aborts_if false;
    }

    fun u16_cast_incorrect(x: u16): u64 {
        (x as u64)
    }
    spec u16_cast_incorrect {
        aborts_if false;
    }

    fun u32_cast_incorrect(x: u32): u64 {
        (x as u64)
    }
    spec u8_cast_incorrect {
        aborts_if false;
    }

    fun u64_cast(x: u64): u128 {
        (x as u128)
    }
    spec aborting_u64_cast {
        aborts_if false;
    }

    fun u128_cast(x: u128): u256 {
        (x as u256)
    }
    spec aborting_u128_cast {
        aborts_if false;
    }

    // -------------
    // Type demotion
    // -------------

    fun aborting_u8_cast_incorrect(x: u64): u8 {
        (x as u8)
    }
    spec aborting_u8_cast_incorrect {
        aborts_if false;
    }

    fun aborting_u8_cast(x: u64): u8 {
        (x as u8)
    }
    spec aborting_u8_cast {
        aborts_if x > 255;
    }

    fun aborting_u16_cast_incorrect(x: u64): u16 {
        (x as u16)
    }
    spec aborting_u16_cast_incorrect {
        aborts_if false;
    }

    fun aborting_u16_cast(x: u64): u16 {
        (x as u16)
    }
    spec aborting_u16_cast {
        aborts_if x > 65535;
    }

    fun aborting_u32_cast_incorrect(x: u64): u32 {
        (x as u32)
    }
    spec aborting_u32_cast_incorrect {
        aborts_if false;
    }

    fun aborting_u32_cast(x: u64): u32 {
        (x as u32)
    }
    spec aborting_u16_cast {
        aborts_if x > 4294967295;
    }

    fun aborting_u64_cast_incorrect(x: u128): u64 {
        (x as u64)
    }
    spec aborting_u64_cast_incorrect {
        aborts_if false;
    }

    fun aborting_u64_cast(x: u128): u64 {
        (x as u64)
    }
    spec aborting_u64_cast {
        aborts_if x > 18446744073709551615;
    }

    fun aborting_u128_cast_incorrect(x: u256): u128 {
        (x as u128)
    }
    spec aborting_u64_cast_incorrect {
        aborts_if false;
    }

    fun aborting_u128_cast(x: u256): u128 {
        (x as u128)
    }
    spec aborting_u128_cast {
        aborts_if x > 340282366920938463463374607431768211455;
    }

}
