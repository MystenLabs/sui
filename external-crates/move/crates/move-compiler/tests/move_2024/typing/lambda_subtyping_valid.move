module a::m {
    macro fun mut_arg($f: |&mut u64| -> u64) {
        let mut x = 0;
        $f(&mut x);
    }

    macro fun imm_ret($f: || -> &u64) {
        $f();
    }

    fun t() {
        mut_arg!(|x: &u64| *x);
        imm_ret!(|| -> &mut u64 { &mut 0 });
    }
}
