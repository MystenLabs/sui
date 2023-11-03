module 0x8675309::M {
    spec schema InvalidGenericEnsures {
        ensures<T> exists<T>(0x1) <==> exists<T>(0x1);
    }
}
