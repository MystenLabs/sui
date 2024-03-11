module A::mod_let {

    public struct S has drop { n: u64 }

    fun t0(`type`: u64, `enum`: S, `mut`: bool, `match`: u64): u64 {
        let `type` = 0;
        let `enum` = 1;
        let `mut` = 2;
        let `match` = 3;
        `type` + `enum` + `mut` + `match`
    }

}
