// dep: ../../move-stdlib/sources/vector.move

module 0x1::ReturnRefsIntoVec {
    use std::vector;

    // should not complain
    fun return_vec_index_immut(v: &vector<u64>): &u64 {
        vector::borrow(v, 0)
    }

    // should complain
    fun return_vec_index_mut(v: &mut vector<u64>): &mut u64 {
        vector::borrow_mut(v, 0)
    }

}
