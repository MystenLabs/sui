#[evm_contract]
module 0x2::M {

    #[external(sig=b"noPara()")]
    public native fun failure_1();

    #[external(sig=b"para1(uint)")]
    public native fun failure_2(add: u128, i:u8);

    #[external(sig=b"para2(uint)")]
    public native fun failure_3(add: address, i:u8);


    #[callable]
    fun test_failure_1() {
        failure_1();
    }

    #[callable]
    fun test_failure_2() {
        failure_2(1, 0);
    }

    #[callable]
    fun test_failure_3() {
        let addr = @3;
        failure_3(addr, 0);
    }

}
