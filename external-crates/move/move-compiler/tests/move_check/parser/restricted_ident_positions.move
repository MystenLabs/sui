module r#a::r#acquries {
    struct r#As<r#break> { r#const: r#break, r#move: u64 }
    const r#False: bool = false;
    fun r#invariant<r#break>(r#as: r#As<r#break>): r#break {
        let r#As { r#const, r#move: r#copy } = r#as;
        assert!(r#copy > 1, 0);
        r#copy;
        r#const
    }
}
