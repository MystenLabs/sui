module a::m {
    macro fun imm_arg($f: |&u64| -> u64) {
        let mut x = 0;
        $f(&mut x);
    }

    macro fun mut_ret($f: || -> &mut u64) {
        $f();
    }

    fun t() {
        imm_arg!(|x: &mut u64| *x = 1);
        mut_ret!(|| -> &u64 { &0 });
    }
}
