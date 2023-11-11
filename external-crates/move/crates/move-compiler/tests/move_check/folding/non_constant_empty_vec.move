// These vectors should not be made constants
// If they are made constants, the IR will error

module 0x42::M {
    struct S {}

    public fun empty_struct_vec(): vector<S> {
        vector[]
    }

    public fun empty_signer_vec(): vector<signer> {
        vector[]
    }

    public fun empty_generic_vec<T>(): vector<T> {
        vector[]
    }

    public fun empty_struct_vec_vec(): vector<vector<S>> {
        vector[]
    }

    public fun empty_signer_vec_vec(): vector<vector<signer>> {
        vector[]
    }

    public fun empty_generic_vec_vec<T>(): vector<vector<T>> {
        vector[]
    }
}
