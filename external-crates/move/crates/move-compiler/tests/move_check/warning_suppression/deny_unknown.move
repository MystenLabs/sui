// Test that #[deny] with an unknown filter name produces an error.
#[deny(who_am_i)]
module 0x42::m {
}
