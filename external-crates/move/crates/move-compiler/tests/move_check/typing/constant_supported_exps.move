address 0x42 {
module M {
    const C: u64 = {
        ();
        0u64;
        { (); () };
        !false;
        false && false;
        true && true;
        2u64 + 1;
        2 - 1u64;
        2 * 1u64;
        2 / 1u64;
        2u64 % 1;
        2u64 >> 1;
        2u64 << 1;
        2 ^ 1u64;
        2u64 & 1;
        2u64 | 1;
        0x0 == 0x1u64;
        b"ab" != x"01";
        (0u64 as u8);
        (0u64 as u64);
        (0u64 as u128);
        (0: u8);
        (0u64, 1u64);
        (0u64, 1u64, false, @112);
        0
    };
}
}
