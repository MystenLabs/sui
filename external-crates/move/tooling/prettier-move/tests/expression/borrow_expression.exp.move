// options:
// printWidth: 35
// useModuleLabel: true

module prettier::borrow_expression;

// borrow expression is printed
// correctly and supports comments
fun borrow(c: &mut u64, b: &u8) {
    &a;
    &mut b;
    *&a;

    // borrow
    & /* kill me */ a; // borrow
    // borrow
    /* what */ &mut  /* again */ b;
}
