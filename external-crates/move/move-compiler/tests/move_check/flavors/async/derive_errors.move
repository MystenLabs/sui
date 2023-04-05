#[actor]
module 0x3::NoState {
}

#[actor]
module 0x3::DuplicateState {
    #[state] struct A {}
    #[state] struct B {}
}

#[actor]
module 0x3::WrongHandlerParam {
    #[state] struct A {}
    struct B {}

    #[message] fun f1() {}
    #[message] fun f2(_x: &B) {}
    #[message] fun f3(_x: A) {}
    #[message] fun f4(_x: &u64) {}
    #[message] fun f5(_x: &A) {}
    #[message] fun f6(_x: &mut A) {}
    #[message] fun f7(_x: &A, _a1: u64, _a2: u64, _a3: u64, _a4: u64, _a5: u64, _a6: u64,
                              _a7: u64, _a8: u64, _a9: u64) {}
    #[message] fun f8(_x: &mut Self::A) {}
}

// Simulate Async runtime module
module Async::Runtime {
    public native fun send__0(actor: address, message_hash: u64);
    public native fun send__1(actor: address, message_hash: u64, arg1: vector<u8>);
    public native fun send__2(actor: address, message_hash: u64, arg1: vector<u8>, arg2: vector<u8>);
    public native fun send__3(actor: address, message_hash: u64, arg1: vector<u8>, arg2: vector<u8>, arg3: vector<u8>);
    public native fun send__4(actor: address, message_hash: u64,
        arg1: vector<u8>, arg2: vector<u8>, arg3: vector<u8>, arg4: vector<u8>);
    public native fun send__5(actor: address, message_hash: u64,
        arg1: vector<u8>, arg2: vector<u8>, arg3: vector<u8>, arg4: vector<u8>, arg5: vector<u8>);
    public native fun send__6(actor: address, message_hash: u64,
        arg1: vector<u8>, arg2: vector<u8>, arg3: vector<u8>, arg4: vector<u8>, arg5: vector<u8>, arg6: vector<u8>);
    public native fun send__7(actor: address, message_hash: u64,
        arg1: vector<u8>, arg2: vector<u8>, arg3: vector<u8>, arg4: vector<u8>, arg5: vector<u8>, arg6: vector<u8>,
        arg7: vector<u8>);
    public native fun send__8(actor: address, message_hash: u64,
        arg1: vector<u8>, arg2: vector<u8>, arg3: vector<u8>, arg4: vector<u8>, arg5: vector<u8>, arg6: vector<u8>,
        arg7: vector<u8>, arg8: vector<u8>);
}
