#[actor]
module 0x3::M {
    #[state] struct A {}

    #[init] fun init(): A { A{} }

    #[message] fun f(_x: &A, _y: u64) {}

    fun expect_send_f_resolves() {
        send_f(@10, 22)
    }
}

// Simulate Async runtime module
module Async::Runtime {
    public native fun send__1(a: address, message_hash: u64, arg: vector<u8>);
}
