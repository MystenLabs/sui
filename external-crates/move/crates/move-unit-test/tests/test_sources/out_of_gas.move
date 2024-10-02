module 0x42::m {

#[test]
#[expected_failure(out_of_gas, location=Self)]
fun t0() {}

#[test]
#[expected_failure(arithmetic_error, location=Self)]
fun t1() {
    loop {}
}

#[test]
#[expected_failure(out_of_gas, location=Self)]
fun t2() {
    0 - 1;
}

}
