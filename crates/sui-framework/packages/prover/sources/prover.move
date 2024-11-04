module prover::prover;

#[verify_only]
native public fun requires(p: bool);
#[verify_only]
native public fun ensures(p: bool);
#[verify_only]
native public fun asserts(p: bool);
#[verify_only]
public macro fun invariant($invariants: ||) {
    invariant_begin();
    $invariants();
    invariant_end();
}

public fun implies(p: bool, q: bool): bool {
    !p || q
}

#[verify_only]
native public fun invariant_begin();
#[verify_only]
native public fun invariant_end();

#[verify_only]
native public fun val<T>(x: &T): T;
#[verify_only]
fun val_spec<T>(x: &T): T {
    let result = val(x);

    ensures(result == x);

    result
}

#[verify_only]
native public fun ref<T>(x: T): &T;
#[verify_only]
fun ref_spec<T>(x: T): &T {
    let old_x = val(&x);

    let result = ref(x);

    ensures(result == old_x);
    drop(old_x);

    result
}

#[verify_only]
native public fun drop<T>(x: T);
#[verify_only]
fun drop_spec<T>(x: T) {
    drop(x);
}

#[verify_only]
public macro fun old<$T>($x: &$T): &$T {
    ref(val($x))
}

#[verify_only]
native public fun fresh<T>(): T;
#[verify_only]
fun fresh_spec<T>(): T {
    fresh()
}

#[allow(unused)]
native fun type_inv<T>(x: &T): bool;
