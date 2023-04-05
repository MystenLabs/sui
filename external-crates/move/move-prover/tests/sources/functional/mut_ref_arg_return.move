module 0x42::Test {
    // test case 1
    fun one_of_two(cond: bool, r1: &mut u64, r2: &mut u64): &mut u64 {
        if (cond) {r1} else {r2}
    }

    fun test1(cond: bool, v1: u64, v2: u64) {
        let r = one_of_two(cond, &mut v1, &mut v2);
        *r = 0;

        spec {
            assert cond ==> v1 == 0;
            assert !cond ==> v2 == 0;
        }
    }

    // test case 2
    fun max_mut(ma: &mut u64, mb: &mut u64): &mut u64 {
        if (*ma >= *mb) {
            ma
        } else {
            mb
        }
    }

    fun test2(a: u64, b: u64) {
        let mc = max_mut(&mut a, &mut b);
        *mc = *mc + 7;

        spec {
            assert a != b;
            assert (a > b) ==> (a - b >= 7);
            assert (a < b) ==> (b - a >= 7);
        }
    }
}
