module Macros::macros {

    public struct SomeStruct has drop {
        some_field: u64,
    }

    macro fun foo($i: u64, $body: |u64| -> u64): u64 {
        $body($i)
    }

    macro fun two_body_foo($i: u64, $body1: |u64| -> u64, $body2: |u64| -> u64): u64 {
        $body1($i) + $body2($i)
    }

    macro fun bar($i: SomeStruct, $body: |SomeStruct| -> SomeStruct): SomeStruct {
        $body($i)
    }

    macro fun for_each<$T>($v: &vector<$T>, $body: |&$T|) {
        let v = $v;
        let mut i = 0;
        let n = v.length();
        while (i < n) {
            $body(v.borrow(i));
            i = i + 1
        }
    }

    fun test() {
        use fun Macros::macros::for_each as vector.feach;

        let p = 42;
        Macros::macros::foo!(p, |x| x);

        // tests if blocks representing lambdas in the same macro have unique labels
        Macros::macros::two_body_foo!(p, |x| x, |y| y);

        Macros::macros::two_body_foo!(p, |x| x, |y| Macros::macros::foo!(y, |z| z));

        Macros::macros::bar!(SomeStruct { some_field: 42 }, |x| x);

        let es = vector[0, 1, 2, 3, 4, 5, 6, 7];
        let mut sum = 0;
        Macros::macros::for_each!<u64>(&es, |x| sum = sum + *x);
        es.feach!<u64>(|x| sum = sum + *x);

        let es = vector[
            SomeStruct { some_field: 42},
            SomeStruct { some_field: 7},
        ];
        let mut sum = 0;
        Macros::macros::for_each!<SomeStruct>(&es, |x| sum = sum + x.some_field);
    }
}
