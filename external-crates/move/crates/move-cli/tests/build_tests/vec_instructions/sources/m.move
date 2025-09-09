module 0x42::m;

public fun vec_ops_post() {
    let mut v = vector::empty();
    v.push_back(0);
    v.push_back(1);
    // assert!(v.length() == 2);
    // assert!(v[0] == 0);
    // assert!(v[1] == 1);
    let y = &mut v[1];
    *y = 2;
    assert!(v[1] == 2);
    v.swap(0,1);
    v.pop_back();
    v.pop_back();
    v.destroy_empty();
}
