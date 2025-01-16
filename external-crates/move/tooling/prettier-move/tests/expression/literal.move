// options:
// printWidth: 35
// tabWidth: 4
// useModuleLabel: true

module prettier::literal;

fun address() {
    @1;
    @0x1;
    @a11ce;
}

fun bytestring() {
    b"hello";
    b"world \u{1F600}";
    b"🐘";
}

fun hexstring() {
    x"AF";
    x"10af";
}

fun bool() {
    true;
    false;
}

fun uint() {
    10;
    10u8;
    200u256;
    10_00u128;
}
