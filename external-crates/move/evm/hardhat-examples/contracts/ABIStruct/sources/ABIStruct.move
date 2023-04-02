#[evm_contract]
module Evm::ABIStruct {
    use std::vector;
    use std::ascii::{Self, String};
    use Evm::Evm::{emit};


    #[external(sig=b"safeTransferFrom(S) returns (S2)")]
    public native fun safe_transfer_form(contract: address, s: S): S2;


    #[abi_struct(sig=b"S(uint64, bool, S2)")]
    struct S has drop, copy {
        a: u64,
        b: bool,
        c: S2
    }

    #[abi_struct(sig=b"S2(uint64)")]
    struct S2 has drop, copy {
        x: u64
    }

    #[event(sig=b"Event_u64(uint64)")]
    struct Event_u64 {
        s: u64
    }

    #[event(sig=b"Event_String(String)")]
    struct Event_String {
        s: String
    }

    #[callable(sig=b"test(S) returns (uint64)")]
    fun test_2_S(s: S): u64 {
        emit(Event_u64{s: s.a});
        s.a
    }

    #[callable(sig=b"safe_transfer_form(address)")]
    fun test_external_safe_transfer_from(addr: address) {
        let s = pack_S(100, true);
        let s2 = safe_transfer_form(addr, s);
        emit(Event_u64{s: s2.x});
    }

    fun pack_S2(x: u64): S2 {
        S2{x}
    }

    fun pack_S(a: u64, b: bool): S {
        let s2 = pack_S2(a);
        S{a, b, c: s2}
    }

    #[event(sig=b"Event_S(S)")]
    struct Event_S {
        s: S
    }

    #[event(sig=b"Event_S2(S2)")]
    struct Event_S2 {
        s: S2
    }

    #[callable]
    fun do_transfer(){
        let s = pack_S(42, true);
        emit(Event_S{s});
    }

    #[callable(sig=b"test_string(String)")]
    fun test_String_struct(s: String) {
        emit(Event_String{s});
    }

}
