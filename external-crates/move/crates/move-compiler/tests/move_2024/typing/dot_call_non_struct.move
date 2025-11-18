module 0x42::t {

public struct Cup<T: drop> has drop { value: T }

fun call<T: drop>(t: T) {
    let x;
    let y = &x;
    let cup = Cup { value: x };
    0u64.f();
    0u64.f();
    ().f();
    (0u64, 1u64).f();
    ().f.f();
    (0u64, 1u64).f.f();
    x.f();
    y.f();
    cup.value.f();
    t.f();
}

}
