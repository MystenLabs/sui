module a::m {

    // chain of calls
    macro fun apply1($f: || -> u64): u64 {
        $f()
    }

    macro fun apply2($f: || -> u64): u64 {
        apply1!($f)
    }

    macro fun apply3($f: || -> u64): u64 {
        apply2!(|| apply2!($f))
    }

    macro fun apply_all($f: || -> u64): u64 {
       apply3!(|| apply2!(|| apply1!($f)))
    }

    // double lambdas
    macro fun dub1($f: || -> u64, $g: || -> u64): u64 {
        $f() + $g()
    }

    macro fun dub2($f: || -> u64, $g: || -> u64): u64 {
        dub1!($f, || $g()) +
        dub1!(|| $f(), $g) +
        dub1!(|| dub1!($f, $g), || dub1!($f, $g)) +
        dub1!(|| dub1!($f, || $g()), || dub1!(|| $f(), $g)) +
        dub1!(|| dub1!(|| $f(), || $g()), || dub1!(|| $f(), || $g()))
    }

    fun t() {
        apply1!(|| 0);
        apply2!(|| 0);
        apply3!(|| 0);
        apply_all!(|| 0);
        dub1!(|| 0, || 0);
        dub2!(|| 0, || 0);
    }
}
