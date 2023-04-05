//# publish
module 0x42::t {

    struct Ints has key, drop {
        f8: u8,
        f16: u16,
        f32: u32,
        f64: u64,
        f128: u128,
        f256: u256,
    }

    public entry fun add(s: signer) {
        move_to<Ints>(
            &s,
            Ints {
                f8:   0x80,
                f16:  0x8000,
                f32:  0x80000000,
                f64:  0x8000000000000000,
                f128: 0x80000000000000000000000000000000,
                f256: 0x8000000000000000000000000000000000000000000000000000000000000000,
            }
        )
    }

    public entry fun remove(a: address) acquires Ints {
        let Ints { f8, f16, f32, f64, f128, f256} = move_from<Ints>(a);
        assert!(f8   == 0x80u8, 0);
        assert!(f16  == 0x8000u16, 0);
        assert!(f32  == 0x80000000u32, 0);
        assert!(f64  == 0x8000000000000000u64, 0);
        assert!(f128 == 0x80000000000000000000000000000000u128, 0);
        assert!(f256 == 0x8000000000000000000000000000000000000000000000000000000000000000u256, 0);

    }

}

//# run 0x42::t::add --args @0x42

//# view --address 0x42 --resource 0x42::t::Ints

//# run 0x42::t::remove --args @0x42
