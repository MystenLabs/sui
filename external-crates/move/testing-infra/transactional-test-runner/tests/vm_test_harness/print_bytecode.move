//# print-bytecode
main<T, U>() {
label b0:
    return;
}

//# print-bytecode --input=module
module 0x42.M {
    f<X, Y>() {
    label b0:
        return;
    }
}
