//# init --edition 2024.beta

//# publish
module 0x42::m {

    const A: u64 = 10;
    const B: u64 = 20;

    public fun test0(): u64 {
        match (A) {
            A => 0,
            B => 1,
            _ => 2,
        }
    }

    #[allow(implicit_const_copy)]
    public fun test1(): u64 {
        match (&A) {
            A => 0,
            B => 1,
            _ => 2,
        }
    }

    #[allow(implicit_const_copy)]
    public fun test2(): u64 {
        match (&mut A) {
            A => 0,
            B => 1,
            _ => 2,
        }
    }

    public fun test3(): u64 {
        match (B) {
            A => 0,
            B => 1,
            _ => 2,
        }
    }

    #[allow(implicit_const_copy)]
    public fun test4(): u64 {
        match (&B) {
            A => 0,
            B => 1,
            _ => 2,
        }
    }

    #[allow(implicit_const_copy)]
    public fun test5(): u64 {
        match (&mut B) {
            A => 0,
            B => 1,
            _ => 2,
        }
    }


}

//#run
module 0x43::main {
    use 0x42::m;

    fun main() {
        assert!(m::test0() == 0, 0);
        assert!(m::test1() == 0, 2);
        assert!(m::test2() == 0, 3);
        assert!(m::test3() == 1, 4);
        assert!(m::test4() == 1, 5);
        assert!(m::test5() == 1, 6);
    }
}
