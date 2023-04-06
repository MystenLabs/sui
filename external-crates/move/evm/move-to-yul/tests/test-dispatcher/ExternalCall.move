#[evm_contract]
module 0x2::M {
    use Evm::U256::{Self, U256, u256_from_words};
    use Evm::ExternalResult::{Self, ExternalResult};
    use Evm::Evm::Unit;
    use std::vector;

    #[external]
    public native fun no_para(contract: address);

    #[external(sig=b"safeTransferFrom(address,address,uint256,bytes)")]
    public native fun safe_transfer_form(contract: address, from: address, to: address, tokenId: U256, data: vector<u8>);

    #[external(sig=b"isApprovedForAll(address,address)returns(bool)"), view]
    public native fun is_approved_for_all(contract: address, account: address, operator: address): bool;

    #[external, view]
    public native fun multi_ret(contract: address, v: U256, vec: vector<U256>): (vector<U256>, U256);

    #[external(sig=b"testExternalReturn(uint) returns (uint)")]
    public native fun test_try_call(contract: address, v: U256): ExternalResult<U256>;

    #[external(sig=b"success_uint() returns (uint)")]
    public native fun success(contract: address): ExternalResult<U256>;

    #[external(sig=b"test_unit()")]
    public native fun test_unit(contract: address): ExternalResult<Unit>;

    #[callable(sig=b"call_unit(address)"), pure]
    fun call_unit(addr: address) {
        let v = test_unit(addr);
        if (ExternalResult::is_ok(&v)) {
            let _u = ExternalResult::unwrap<Unit>(v);
        }
    }

    #[callable(sig=b"call_success(address) returns (uint)"), pure]
    fun call_success(addr: address): U256 {
        let v = success(addr);
        if (ExternalResult::is_ok(&v)) {
            return ExternalResult::unwrap<U256>(v)
        };
        return U256::one()
    }

    #[callable]
    fun test_try(): u8 {
        let contract_addr = @3;
        let v = u256_from_words(1, 2);
        let value = test_try_call(contract_addr, v);
        if (ExternalResult::is_ok(&value)) {
            return 0
        } else if (ExternalResult::is_err_reason(&value)) {
            return 1
        } else if (ExternalResult::is_panic(&value)) {
            return 2
        };
        return 3
    }

    #[callable]
    fun test_no_para() {
        let contract_addr = @3;
        no_para(contract_addr);
    }

    #[callable]
    fun test_safe_transfer_from(x: u128, y: u128) {
        let contract_addr = @3;
        let from_addr = @4;
        let to_addr = @5;
        let token_id = u256_from_words(x, y);
        let data = vector::empty<u8>();
        safe_transfer_form(contract_addr, from_addr, to_addr, token_id, data)
    }

    #[callable]
    fun test_is_approved_for_all(): bool {
        let contract_addr = @3;
        let account = @4;
        let operator = @5;
        is_approved_for_all(contract_addr, account, operator)
    }

    #[callable]
    fun test_multi_ret(): (vector<U256>, U256) {
        let contract_addr = @3;
        let v = u256_from_words(0, 0);
        let data = vector::empty<U256>();
        multi_ret(contract_addr, v, data)
    }

}
