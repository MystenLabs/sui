// exclude_for: simplify
// exclude_for: cvc5
// flag: --timeout=160
module 0x42::TestNonlinearArithmetic {

    spec module {
        pragma verify = true;
    }

    // -----------------------------------------
    // Overflow by multiplication of 3 variables
    // -----------------------------------------

    // fails.
    fun overflow_u8_mul_3_incorrect(a: u8, b: u8, c: u8): u8 {
        a * b * c
    }
    spec overflow_u8_mul_3_incorrect {
        aborts_if false;
    }

    // succeeds.
    fun overflow_u8_mul_3(a: u8, b: u8, c: u8): u8 {
        a * b * c
    }
    spec overflow_u8_mul_3 {
        aborts_if a * b > max_u8();
        aborts_if a * b * c > max_u8();
    }

    // fails.
    fun overflow_u64_mul_3_incorrect(a: u64, b: u64, c: u64): u64 {
        a * b * c
    }
    spec overflow_u64_mul_3_incorrect {
        aborts_if false;
    }

    fun overflow_u64_mul_3(a: u64, b: u64, c: u64): u64 {
        a * b * c
    }
    spec overflow_u64_mul_3 {
        aborts_if a * b > max_u64();
        aborts_if a * b * c > max_u64();
    }

    // fails.
    fun overflow_u128_mul_3_incorrect(a: u128, b: u128, c: u128): u128 {
        a * b * c
    }
    spec overflow_u128_mul_3_incorrect {
        aborts_if false;
    }

    fun overflow_u128_mul_3(a: u128, b: u128, c: u128): u128 {
        a * b * c
    }
    spec overflow_u128_mul_3 {
        aborts_if a * b > max_u128();
        aborts_if a * b * c > max_u128();
    }


    // -----------------------------------------
    // Overflow by multiplication of 4 variables
    // -----------------------------------------

    // fails.
    fun overflow_u8_mul_4_incorrect(a: u8, b: u8, c: u8, d: u8): u8 {
        a * b * c * d
    }
    spec overflow_u8_mul_4_incorrect {
        aborts_if false;
    }

    // succeeds.
    fun overflow_u8_mul_4(a: u8, b: u8, c: u8, d: u8): u8 {
        a * b * c * d
    }
    spec overflow_u8_mul_4 {
        aborts_if a * b > max_u8();
        aborts_if a * b * c > max_u8();
        aborts_if a * b * c * d > max_u8();
    }

    // fails.
    fun overflow_u64_mul_4_incorrect(a: u64, b: u64, c: u64, d: u64): u64 {
        a * b * c * d
    }
    spec overflow_u64_mul_4_incorrect {
        aborts_if false;
    }

    fun overflow_u64_mul_4(a: u64, b: u64, c: u64, d: u64): u64 {
        a * b * c * d
    }
    spec overflow_u64_mul_4 {
        aborts_if a * b > max_u64();
        aborts_if a * b * c > max_u64();
        aborts_if a * b * c * d > max_u64();
    }

    // fails.
    fun overflow_u128_mul_4_incorrect(a: u128, b: u128, c: u128, d: u128): u128 {
        a * b * c * d
    }
    spec overflow_u128_mul_4_incorrect {
        pragma verify = false; // times out on smaller machines
        aborts_if false;
    }

    fun overflow_u128_mul_4(a: u128, b: u128, c: u128, d: u128): u128 {
        a * b * c * d
    }
    spec overflow_u128_mul_4 {
        pragma verify = false; // times out on smaller machines
        aborts_if a * b > max_u128();
        aborts_if a * b * c > max_u128();
        aborts_if a * b * c * d > max_u128();
    }


    // -----------------------------------------
    // Overflow by multiplication of 5 variables
    // -----------------------------------------

    // fails.
    fun overflow_u8_mul_5_incorrect(a: u8, b: u8, c: u8, d: u8, e: u8): u8 {
        a * b * c * d * e
    }
    spec overflow_u8_mul_5_incorrect {
        aborts_if false;
    }

    // succeeds.
    fun overflow_u8_mul_5(a: u8, b: u8, c: u8, d: u8, e: u8): u8 {
        a * b * c * d * e
    }
    spec overflow_u8_mul_5 {
        aborts_if a * b > max_u8();
        aborts_if a * b * c > max_u8();
        aborts_if a * b * c * d > max_u8();
        aborts_if a * b * c * d * e > max_u8();
    }

    // fails.
    fun overflow_u64_mul_5_incorrect(a: u64, b: u64, c: u64, d: u64, e: u64): u64 {
        a * b * c * d * e
    }
    spec overflow_u64_mul_5_incorrect {
        aborts_if false;
    }

    fun overflow_u64_mul_5(a: u64, b: u64, c: u64, d: u64, e: u64): u64 {
        a * b * c * d * e
    }
    spec overflow_u64_mul_5 {
        aborts_if a * b > max_u64();
        aborts_if a * b * c > max_u64();
        aborts_if a * b * c * d > max_u64();
        aborts_if a * b * c * d * e > max_u64();
    }

    // fails.
    fun overflow_u128_mul_5_incorrect(a: u128, b: u128, c: u128, d: u128, e: u128): u128 {
        a * b * c * d * e
    }
    spec overflow_u128_mul_5_incorrect {
        pragma verify = false; // times out on smaller machines
        aborts_if false;
    }

    fun overflow_u128_mul_5(a: u128, b: u128, c: u128, d: u128, e: u128): u128 {
        a * b * c * d * e
    }
    spec overflow_u128_mul_5 {
        pragma verify = false; // times out on smaller machines
        aborts_if a * b > max_u128();
        aborts_if a * b * c > max_u128();
        aborts_if a * b * c * d > max_u128();
        aborts_if a * b * c * d * e > max_u128();
    }


    // -------------
    // miscellaneous
    // -------------

    fun mul5(a: u64, b: u64, c: u64, d: u64, e: u64): u64 {
        spec {
            assume a < b;
            assume b < c;
            assume c < d;
            assume d < e;
        };
        a * b * c * d * e
    }
    spec mul5 {
        // a, b, c, d and e do not exist such that a<b<c<d<e and a*b*c*d*e==72.
        ensures result != 72;
    }

    fun mul5_incorrect(a: u64, b: u64, c: u64, d: u64, e: u64): u64 {
        spec {
            assume a < b;
            assume b < c;
            assume c < d;
            assume d < e;
        };
        a * b * c * d * e
    }
    spec mul5_incorrect {
        // a=1, b=2, c=3, d=4, e=30, a*b*c*d*e==720
        ensures result != 720;
    }

    fun distribution_law(a: u64, b: u64, c: u64 , d: u64): u64 {
        a * b * (c + d)
    }
    spec distribution_law {
        ensures result == a*b*c + a*b*d;
    }

    fun distribution_law_incorrect(a: u64, b: u64, c: u64 , d: u64): u64 {
        a * b * (c + d)
    }
    spec distribution_law_incorrect {
        ensures result == a*b*c + a*b*d + a*b;
    }
}
