/// Async runtime functions. Supporting the compiler.
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
