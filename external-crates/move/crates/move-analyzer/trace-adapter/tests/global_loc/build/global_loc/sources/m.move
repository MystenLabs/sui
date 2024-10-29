// Test handling of global locations in the trace.
module global_loc::m;

public struct SomeObject has key {
    id: UID,
    num: u8,
}

fun foo(o: SomeObject, p: u8): u64 {
    let n = object::id(&o).to_bytes()[0];
    let SomeObject { id, num } = o;
    object::delete(id);
    (n as u64) + (num as u64) + (p as u64)
}

#[test]
fun test() {
    let mut ctx = tx_context::dummy();
    let mut _res = foo(SomeObject { id: object::new(&mut ctx), num: 42 }, 42);
    // line below is to force another unoptimized read to keep `res` visible
    _res = _res + foo(SomeObject { id: object::new(&mut ctx), num: 42 }, 42);
}
