module 0x42::m {

struct Txn { sender: address }
struct X {}

fun begin(sender: address): Txn {
    Txn { sender }
}

fun new(_: &mut Txn): X {
    X {}
}

fun add<K: copy + drop + store, V: store>(_x: &mut X, _k: K, _v: V) {
    abort 0
}


fun borrow<K: copy + drop + store, V: store>(_x: &X, _k: K): &V {
    abort 0
}

fun borrow_wrong_type() {
    let sender = @0x0;
    let scenario = begin(sender);
    let x = new(&mut scenario);
    add(&mut x, 0, 0);
    borrow<u64, u64>(&mut x, 0, 0);
    abort 42
}

}
