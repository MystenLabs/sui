module 0x8675309::M {
    struct R {}
    struct Cup<T> has copy, drop { f: T }

    fun t0() {
        (false as u8);
        (true as u128);

        (() as u64);
        ((0u64, 1u64) as u8);

        (0u64 as bool);
        (0u64 as address);
        R{} = (0u64 as R);
        (0u64 as Cup<u8>);
        (0u64 as ());
        (0u64 as (u64, u8));

        (x"1234" as u64);
    }

    fun t1() {
        false as u8;
        true as u128;

        () as u64;
        (0u64, 1u64) as u8;

        0u64 as bool;
        0u64 as address;
        R{} = 0u64 as R;
        0u64 as Cup<u8>;
        0u64 as ();
        0u64 as (u64, u8);

        x"1234" as u64;
    }
}
