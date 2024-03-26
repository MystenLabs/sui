module 0x42::m {

    const ZERO: u64 = 0;

    fun add1(n: u64): u64 {
        n + 1
    }

    use fun add1 as u64.add1;

    fun one(): u64 {
       ZERO.add1()
    }

}
