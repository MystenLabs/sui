module 0x2::A {
    #[test]
    public fun if_else_1(): (u64, u64) {
        let a = 1;
        let b = 2;
        let r = if (a > b) { &mut a } else  { &mut b };
        *r = 3;
        (a, b)
    }

    #[test]
    public fun if_else_2(): (u64, u64) {
        let a = 2;
        let b = 1;
        let r = if (a > b) { &mut a } else  { &mut b };
        *r = 3;
        (a, b)
    }

}
