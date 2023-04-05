#[evm_contract]
module 0x1::FortyTwo {
    use Evm::U256::{u256_from_u128, U256};

    #[callable, pure]
    public fun forty_two(): u64 {
        42
    }

    #[callable, pure]
    public fun forty_two_as_u256(): U256 {
        u256_from_u128(42)
    }

    // TODO: move-to-yul does not support literal string.
    #[callable(sig=b"forty_two_as_string() returns (string)"), pure]
    public fun forty_two_as_string(): vector<u8> {
        b"forty two"
    }

    #[callable, pure]
    public fun forty_two_plus_alpha(alpha: u64): u64 {
        42 + alpha
    }
}
