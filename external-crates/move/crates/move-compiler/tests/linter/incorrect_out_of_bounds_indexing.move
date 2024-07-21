module 0x42::M {
    use std::vector;
    fun out_of_bound_index() {
        let arr2 = vector[1, 2, 3, 4, 5];
        vector::push_back(&mut arr2, 6);
        vector::push_back(&mut arr2, 6);
        vector::push_back(&mut arr2, 6);
        vector::pop_back(&mut arr2);
        vector::pop_back(&mut arr2);

        vector::borrow(&arr2, 7);
    }
}
