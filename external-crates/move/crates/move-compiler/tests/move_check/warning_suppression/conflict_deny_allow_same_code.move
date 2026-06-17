// Error: same diagnostic set as both 'allow' and 'deny'.
#[allow(dead_code)]
#[deny(dead_code)]
module 0x42::m {
    fun foo() {}
}
