module sui::internal;

public struct InternalWitness<phantom T> has drop {}

public fun new_witness<T /* internal */>(): InternalWitness<T> {
    InternalWitness {}
}
