//# print-bytecode --syntax=mvir
module 0x42.M {
entry foo<T, U>() {
label b0:
    return;
}
}

//# print-bytecode
module 0x43::M {
entry fun foo<X, Y>() {
}
}
