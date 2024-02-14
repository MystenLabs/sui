module a::m {
    public struct S { f: u64 } has copy, drop;

    fun check_unique(s1: &mut S, s2: &mut S) {
        s1.f = 1;
        s2.f = 2;
    }

    macro fun foo($s: &mut S) {
        let s1 = $s;
        let s2 = $s;
        // if we do not give unique locals, this will fail since we would not have unique ownership
        check_unique(s1, s2)
    }

    fun valid() {
        // no errors!
        // a new s is made for each usage of the arg
        foo!({ let mut s = S { f: 0 }; &mut s });
    }

    fun invalid() {
        // to double check, we will make an invalid call
        let mut s = S { f: 0 };
        foo!(&mut s);
    }


}
