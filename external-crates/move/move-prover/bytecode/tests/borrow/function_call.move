// dep: ../../move-stdlib/sources/vector.move

module 0x2::MultiLayerCalling {
    use std::vector;

    struct HasVector {
        v: vector<HasAnotherVector>,
    }

    struct HasAnotherVector {
        v: vector<u8>,
    }

    fun outer(has_vector: &mut HasVector) {
        let has_another_vector = mid(has_vector);
        vector::push_back(&mut has_another_vector.v, 42)
    }

    fun mid(has_vector: &mut HasVector): &mut HasAnotherVector {
        inner(has_vector)
    }

    fun inner(has_vector: &mut HasVector): &mut HasAnotherVector {
        vector::borrow_mut(&mut has_vector.v, 7)
    }
}
