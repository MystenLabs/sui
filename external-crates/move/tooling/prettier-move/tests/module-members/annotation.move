// options:
// printWidth: 40
// useModuleLabel: true

/// Covers `annotation` node in grammar
module prettier::annotation;

// if it fits, printed on one line
#[test]
#[allow(unused_imports)]
#[allow(unused_const, unused_variable)]
fun e() {}

// long annotations will get broken
#[
    expected_error(
        abort_code = ::ENotImplemented
    )
]
fun f() {}

// a list of annotations will also be
// broken however, will never break the
// assignment
#[
    test,
    expected_failure(
        abort_code = ::other_module::ENotFound
    )
]
fun g() {}

// assignments are printed as is, no breaking
#[
    expected_failure(
        arithmetic_error,
        location = pkg_addr::other_module
    )
]
#[
    allow(
        unused_const,
        unused_variable,
        unused_imports,
        unused_field
    )
]
fun h() {}

// we support literals in annotations
#[error, code = b"string"]
fun i() {}

// we support full type idents
#[
    error,
    abort_code = ::other_module::EArithmeticError
]
fun j() {}
