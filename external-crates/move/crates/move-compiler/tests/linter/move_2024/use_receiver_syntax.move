module 0x42::m;

public struct S {}

public fun new(): S {
    S {}
}

public fun drop(s: S) {
    let S {} = s;
}

fun t0() {
    let mut v = vector[1u64, 2, 3];
    v.push_back(4);
    pad_right(&mut v, 10, 0);
    (S {}).drop();
}

fun t1() {
    let mut v = vector[1u64, 2, 3];
    vector::push_back(&mut v, 4);
    pad_right(&mut v, 10, 0);
    drop(new());
}

fun t2() {
    let _ = std::u64::max(1, 2);
    let _ = std::u16::pow(2, 3);
}

fun t3() {
    let _ = vector::length(&{
        let mut v = vector[1u64, 2, 3];
        v.push_back(4);
        v
    });
}

fun t4() {
    let o = option::some(10u64);
    assert!(option::is_some(&o));
    let _ = option::destroy_some(o);
}

fun pad_right<T: copy + drop>(v: &mut vector<T>, width: u64, pad: T) {
    while (v.length() < width) {
        v.push_back(pad);
    }
}
