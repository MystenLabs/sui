#[evm_contract]
module 0x2::M {

    use Evm::U256::U256;

    #[event(sig=b"Transfer(address,address,uint256) returns (uint8)")]
    struct Transfer {
        from: address,
        to: address,
        value: U256,
    }

    #[event]
    struct Approval {
        owner: address,
        spender: address,
        value: U256,
    }

    #[event]
    struct Bar<T1, T2>{
        x: T1,
        y: vector<T2>,
    }

}
