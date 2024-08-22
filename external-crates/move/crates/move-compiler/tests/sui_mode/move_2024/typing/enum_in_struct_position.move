module a::m {

public enum E {
    V()
}

public enum M has drop {
    V()
}

public enum Obj has key, store {
    V()
}

fun init(_: M, _: &mut TxContext) {
}

entry fun ret(): E {
    E::V()
}

entry fun x3(_: E) {
    abort 0
}

}

module a::n {
public fun transfer(o: a::m::Obj) {
    transfer::transfer(o, @0)
}
}

module sui::transfer {
public fun transfer<T: key>(_: T, _: address) {
    abort 0
}
}

module sui::tx_context{
public struct TxContext has drop {}
}
