// dep: ../move-stdlib/sources/macros.move
// dep: ../move-stdlib/sources/u64.move
// dep: ../move-stdlib/sources/option.move
// dep: ../move-stdlib/sources/ascii.move
// dep: ../move-stdlib/sources/string.move
// dep: ../move-stdlib/sources/vector.move

module 0x6::ReturnRefsIntoVec {

    // should not complain
    fun return_vec_index_immut(v: &vector<u64>): &u64 {
        vector::borrow(v, 0)
    }

    // should complain
    fun return_vec_index_mut(v: &mut vector<u64>): &mut u64 {
        vector::borrow_mut(v, 0)
    }

}
