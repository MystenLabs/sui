//# print-bytecode --syntax=mvir
mvir 0x42::M {
entry fun foo<T, U>() {
label b0:
    return;
}
}

//# print-bytecode
module 0x43::M {
entry fun foo<X, Y>() {
}
}
