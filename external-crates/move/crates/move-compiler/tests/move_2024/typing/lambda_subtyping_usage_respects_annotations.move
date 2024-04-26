module a::m {
    // test that despite "valid" usage, we respect the annotations
    macro fun pass_imm($f: |&u64|) {
        let mut x = 0;
        $f(&mut x)
    }

    macro fun pass_mut($f: |&mut u64|) {
        let mut x = 0;
        $f(&mut x)
    }


    macro fun return_imm($f: || -> &u64) {
        *$f() = 0;
    }

    fun t() {
        // this should error since it was annotated as taking a &u64
        pass_imm!(|x| *x = 0);
        pass_mut!(|x: &u64| *x = 0);
        // this should error since it was annotated as returning a &u64
        let mut x = 0;
        return_imm!(|| &mut x);
    }
}
