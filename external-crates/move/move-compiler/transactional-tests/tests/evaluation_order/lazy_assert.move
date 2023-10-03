//# run
script {
    fun main() {
        // does not abort
        assert!(true, 1 / 0);
    }
}

//# run
script {
    fun main() {
        // does abort
        assert!(false, 1 / 0);
    }
}

//# run
script {
    fun main() {
        // does abort, will be deprecated
        assert!(true, 1 / 0);
    }
}
