// Attribute references like '#[expected_failure(abort_code = ...)]' do not go through
// constant visibility checks: a cross-module reference to a constant without
// 'public(package)' is accepted there, as it was before cross-module constants existed

module 0x42::a {

const ENotFound: u64 = 5;

public fun fail() { abort ENotFound }

}

module 0x42::b {

#[test]
#[expected_failure(abort_code = 0x42::a::ENotFound)]
fun expect_cross_module_code() {
    0x42::a::fail()
}

}
