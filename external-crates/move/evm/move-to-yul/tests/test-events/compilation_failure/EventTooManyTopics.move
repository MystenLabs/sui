#[evm_contract]
module 0x2::M {

#[event(sig=b"Transfer(address indexed,address indexed, uint128 indexed, uint128 indexed)")]
    struct Transfer_Err_1 {
        from: address,
        to: address,
        v1: u128,
        v2: u128
    }

}
