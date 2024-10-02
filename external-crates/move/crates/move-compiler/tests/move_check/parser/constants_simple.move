address 0x42 {
module M {
    fun u(): u64 { 0 }

    const C1: u64 = u();
    const C2: u64 = 0 + 1 * 2 % 3 / 4 - 5 >> 6 << 7;
    const C3: bool = loop ();
    const C4: u8 = if (cond) 0 else 1;
    const C5: vector<vector<bool>> = abort 0;
    const C6: u128 = 0;
    const C7: u256 = 4 / 3 + 4 - 1 << 143;
    const C8: u16 = 123;
    const C9: u32 = (453 as u32);

}
}
