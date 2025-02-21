// Test that write to a local variable `option_key` with a value
// coming from a global reference is handled correctly
module global_write_ref::m;

use sui::linked_table;

fun foo(table: &linked_table::LinkedTable<u64, u8>): u64 {
    let mut res = 0;
    let mut option_key = table.front();
    while (option_key.is_some()) {
        let key = *option_key.borrow();
        res = res + key;
        option_key = table.next(key);
    };
    res
}

#[test]
fun test() {
    let ctx = &mut tx_context::dummy();
    let mut table = linked_table::new<u64, u8>(ctx);
    table.push_back(7, 42);
    foo(&table);
    table.drop();
}