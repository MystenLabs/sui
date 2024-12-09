module prover::prover;

#[spec_only]
native public fun requires(p: bool);
#[spec_only]
native public fun ensures(p: bool);
#[spec_only]
native public fun asserts(p: bool);
#[spec_only]
public macro fun invariant($invariants: ||) {
    invariant_begin();
    $invariants();
    invariant_end();
}

public fun implies(p: bool, q: bool): bool {
    !p || q
}

#[spec_only]
native public fun invariant_begin();
#[spec_only]
native public fun invariant_end();

#[spec_only]
native public fun val<T>(x: &T): T;
#[spec_only]
fun val_spec<T>(x: &T): T {
    let result = val(x);

    ensures(result == x);

    result
}

#[spec_only]
native public fun ref<T>(x: T): &T;
#[spec_only]
fun ref_spec<T>(x: T): &T {
    let old_x = val(&x);

    let result = ref(x);

    ensures(result == old_x);
    drop(old_x);

    result
}

#[spec_only]
native public fun drop<T>(x: T);
#[spec_only]
fun drop_spec<T>(x: T) {
    drop(x);
}

#[spec_only]
public macro fun old<$T>($x: &$T): &$T {
    ref(val($x))
}

#[spec_only]
native public fun fresh<T>(): T;
#[spec_only]
fun fresh_spec<T>(): T {
    fresh()
}

#[allow(unused)]
native fun type_inv<T>(x: &T): bool;
