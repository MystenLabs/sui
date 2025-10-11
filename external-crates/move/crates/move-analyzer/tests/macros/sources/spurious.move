module Macros::spurious {

    public fun test() {
        let es = vector[0, 1, 2, 3, 4, 5, 6, 7];
        let mut sum = 0;
        Macros::macros::for_each!<u64>(&es, |x| sum = sum + *x);
    }














//                  spurious (incorrect) use from macro

}
