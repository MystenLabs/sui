//# run
module 1::m {
    fun main() {
        // does not abort
        assert!(true, 1 / 0);
    }
}

//# run
module 2::m {
    fun main() {
        // does abort
        assert!(false, 1 / 0);
    }
}

//# run
module 3::m {
    fun main() {
        // does abort, will be deprecated
        assert(true, 1 / 0);
    }
}
