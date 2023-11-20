address 0x2 {

module M {
    // restricted names are invalid due to not starting with A-Z
    const address: u64 = 0;
    const signer: u64 = 0;
    const u8: u64 = 0;
    const u64: u64 = 0;
    const u128: u64 = 0;
    const vector: u64 = 0;
    const freeze: u64 = 0;
    const assert: u64 = 0;
    // restricted
    const Self: u64 = 0;
}

}
