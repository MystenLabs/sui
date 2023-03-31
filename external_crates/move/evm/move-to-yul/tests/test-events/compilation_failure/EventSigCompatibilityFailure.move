#[evm_contract]
module 0x2::M {
    use Evm::U256::U256;

    #[event(sig=b"Transfer(address,uint256)")]
    struct Transfer_Err_1 {
        from: address,
        to: address,
        value: U256,
    }

    #[event(sig=b"Transfer(address, address,uint256,uint256)")]
    struct Transfer_Err_2 {
        from: address,
        to: address,
        value: U256,
    }

    #[event(sig=b"Transfer(address, address,bytes)")]
    struct Transfer_Err_3 {
        from: address,
        to: address,
        value: U256,
    }

}
