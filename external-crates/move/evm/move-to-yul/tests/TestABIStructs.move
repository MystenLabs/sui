#[evm_contract]
module 0x2::M {
    use std::vector;
    use std::ascii::{Self, String};
    use Evm::Evm::{emit};

    #[decode(sig=b"decode_S(bytes) returns (S)")]
    public native fun decode_S(input: vector<u8>) :S;

    #[encode(sig=b"encode_S(S) returns (bytes)")]
    public native fun encode_S(s: S) : vector<u8>;

    #[decode(sig=b"decode_String(bytes) returns (String)")]
    public native fun decode_String(input: vector<u8>) :String;

    #[encode(sig=b"encode_String(String) returns (bytes)")]
    public native fun encode_String(s: String) : vector<u8>;

    #[external(sig=b"safeTransferFrom(S) returns (S2)")]
    public native fun safe_transfer_form(contract: address, s: S): S2;

    #[abi_struct(sig=b"S(uint64, bool, S2[])")]
    struct S has drop, copy {
        a: u64,
        b: bool,
        c: vector<S2>
    }

    #[abi_struct(sig=b"S2(uint128[])")]
    struct S2 has drop, copy {
        x: vector<u128>
    }

    #[event(sig=b"Event_S(S)")]
    struct Event_S {
        s: S
    }

    #[callable]
    fun do_transfer(){
        let s = pack_S(42, true);
        emit(Event_S{s});
    }

    fun pack_S2(x: u128): S2 {
        let v = vector::empty<u128>();
        vector::push_back(&mut v, x);
        S2{x:v}
    }

    fun pack_S(a: u64, b: bool): S {
        let v = vector::empty<S2>();
        let s2 = pack_S2((a as u128));
        vector::push_back(&mut v, s2);
        S{a, b, c: v}
    }

    #[evm_test]
    fun test_abi_S() {
        let s = pack_S(42, true);
        let v = encode_S(s);
        let _s = decode_S(v);
        assert!(s.a == _s.a, 100);
        assert!(_s.a == 42, 101);
        assert!(s.b == _s.b, 102);
        assert!(_s.b == true, 103);
        let s2 = s.c;
        let _s2 = _s.c;
        assert!(vector::length(&s2) == 1, 104);
        assert!(vector::length(&_s2) == 1, 105);
        let _s2x = vector::borrow(&_s2, 0);
        assert!(*vector::borrow(&_s2x.x, 0) == 42, 106);
    }

    #[evm_test]
    fun test_abi_String() {
        let i = 0;
        let end = 128;
        let vec = vector::empty();

        while (i < end) {
            assert!(ascii::is_valid_char(i), 0);
            vector::push_back(&mut vec, i);
            i = i + 1;
        };

        let str = ascii::string(vec);
        let v = encode_String(str);
        let _str = decode_String(v);

        assert!(vector::length(ascii::as_bytes(&_str)) == 128, 100);
        // This call to all_characters_printable will lead to solc compiler error:
        // Error: Cannot use builtin function name "byte" as identifier name.
        // let byte, i, len...
        // assert!(!ASCII::all_characters_printable(&_str), 1);
        let bytes = ascii::into_bytes(_str);
        assert!(vector::length(&bytes) == 128, 99);

        i = 0;
        while (i < end) {
            assert!(*vector::borrow(&bytes, (i as u64)) == i, (i as u64));
            i = i + 1;
        };

    }

    #[callable(sig=b"test() returns (S)")]
    fun test_pack_S(): S {
        pack_S(42, true)
    }

    #[callable(sig=b"test_2(S) returns (S)")]
    fun test_2_S(s: S): S {
        s
    }

    #[callable(sig=b"test_array(S[][2]) returns (S[])")]
    fun test_array(v: vector<vector<S>>): vector<S> {
        *vector::borrow(&v, 0)
    }

    #[callable(sig=b"test_s_struct(String) returns (String)")]
    fun test_String_struct(s: String) : String {
        s
    }

    #[callable]
    fun test_safe_transfer_from(): S2 {
        let contract_addr = @3;
        let s = pack_S(42, true);
        safe_transfer_form(contract_addr, s)
    }

}
