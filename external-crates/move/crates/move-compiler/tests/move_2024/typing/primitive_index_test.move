
#[defines_primitive(vector)]
module std::vector {
    #[syntax(index)]
    native public fun vborrow<Element>(v: &vector<Element>, i: u64): &Element;
    #[syntax(index)]
    native public fun vborrow_mut<Element>(v: &mut vector<Element>, i: u64): &mut Element;
}

module a::m {
    fun indexing_works<T: copy + drop>(vec: &mut vector<T>, vss: &mut vector<vector<T>>, i: u64, j: u64) {
        &vec[i];
        &mut vec[i];
        vec[i];
        &vss[i][j];
        &mut vss[i][j];
        vss[i][j];
    }
}
