// The grammar `let <pat> = <e> else <e>` accepts an arbitrary expression
// after `else`, not just a block. This test pins that: a bare divergent
// expression (here `return 0`) is a valid else clause.
module 0x42::m {

    public enum ABC<T> has drop {
        A(T),
        B,
        C(T)
    }

    fun non_block_else(): u64 {
        let subject = ABC::C(42u64);
        let ABC::C(x) = subject else return 0;
        x
    }

}
