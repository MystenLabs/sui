#[evm_contract]
module 0x2::M {
    use Evm::U256::U256;

    #[event(sig=b"Transfer(address,address,uint256) returns (uint8)")]
    struct Transfer_Err {
        from: address,
        to: address,
        value: U256,
    }

    #[event(sig=b"Approval(addressindexed ,address,uint256)")]
    struct Approval_Err_1 {
        owner: address,
        spender: address,
        value: U256,
    }

    #[event(sig=b"Approval(,address,uint256)")]
    struct Approval_Err_2 {
        owner: address,
        spender: address,
        value: U256,
    }

}
