// Test that #[expect] with an unknown filter name produces an error.
#[expect(who_am_i)]
module 0x42::m {
}
