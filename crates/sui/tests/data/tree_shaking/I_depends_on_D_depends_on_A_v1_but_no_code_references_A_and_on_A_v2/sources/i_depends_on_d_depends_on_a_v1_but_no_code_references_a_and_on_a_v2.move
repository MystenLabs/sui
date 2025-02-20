module i_depends_on_d_depends_on_a_v1_but_no_code_references_a_and_on_a_v2::i_depends_on_d_depends_on_a_v1_but_no_code_references_a_and_on_a_v2 {

    public fun test() {
        let a = 1;
        a::a::a2();
    }
}

