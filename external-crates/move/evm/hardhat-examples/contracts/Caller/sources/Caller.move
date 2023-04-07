#[evm_contract]
module Evm::Caller {
    use Evm::U256::{Self, U256};
    use Evm::ExternalResult::{Self, ExternalResult};

    #[external(sig=b"success_uint() returns (uint)")]
    public native fun success(contract: address): ExternalResult<U256>;

    #[external(sig=b"panic() returns (uint)")]
    public native fun panic(contract: address): ExternalResult<U256>;

    #[external(sig=b"ret_revert() returns (uint)")]
    public native fun ret_revert(contract: address): ExternalResult<U256>;

    #[callable(sig=b"call_success(address) returns (uint)"), pure]
    public fun call_success(addr: address): U256 {
        let v = success(addr);
        if (ExternalResult::is_ok(&v)) {
            return ExternalResult::unwrap<U256>(v)
        };
        return U256::zero()
    }

    #[callable(sig=b"call_revert(address) returns (string)"), pure]
    public fun call_revert(addr: address): vector<u8> {
        let v = ret_revert(addr);
        if (ExternalResult::is_ok(&v)) {
            return b"success"
        } else if (ExternalResult::is_err_reason(&v)) {
            return ExternalResult::unwrap_err_reason(v)
        } else if (ExternalResult::is_panic(&v)) {
            return b"panic"
        };
        return b"data"
    }

    #[callable(sig=b"call_panic(address) returns (uint)"), pure]
    public fun call_panic(addr: address): U256 {
        let v = panic(addr);
        if (ExternalResult::is_ok(&v)) {
            return U256::zero();
        } else if (ExternalResult::is_err_reason(&v)) {
            return U256::one();
        } else if (ExternalResult::is_panic(&v)) {
            return ExternalResult::unwrap_panic(v)
        };
        return  U256::max()
    }

}
