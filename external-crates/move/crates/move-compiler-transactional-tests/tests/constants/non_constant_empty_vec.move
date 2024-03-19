//# publish
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

//# run 0x42::M::empty_struct_vec

//# run 0x42::M::empty_struct_vec

//# run 0x42::M::empty_signer_vec

//# run 0x42::M::empty_generic_vec --type-args 0x42::M::S

//# run 0x42::M::empty_struct_vec_vec

//# run 0x42::M::empty_signer_vec_vec

//# run 0x42::M::empty_generic_vec_vec --type-args 0x42::M::S
