module basic::vec;

#[allow(unused_function)]
fun t() {
    let _ = 0u64;
    let _ = vector<u64>[2];
    let mut v = vector[1u8, 2, 3, 4];
    let _ = v.pop_back();
    v.push_back(10);
    assert!(v.length() == 4, 0);
    v.swap(0, 1);
    assert!(v.borrow(0) == 2, 1);
}
