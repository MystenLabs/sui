#[allow(duplicate_alias)]
module a::t1 {
    use std::vector;

    public entry fun ascii_vec_arg(v: vector<u8>) {
        assert!(vector::is_empty(&v), 0);
    }
}

module a::t2 {
    // implicit use std::vector;

    public entry fun ascii_vec_arg(v: vector<u8>) {
        assert!(vector::is_empty(&v), 0);
    }
}
