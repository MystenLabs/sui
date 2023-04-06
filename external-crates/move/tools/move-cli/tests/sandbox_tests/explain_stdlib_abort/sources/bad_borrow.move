script {
    use std::vector;
    fun bad_borrow() {
        let v = vector::empty<bool>();
        let _ref = vector::borrow(&v, 0);
    }
}
