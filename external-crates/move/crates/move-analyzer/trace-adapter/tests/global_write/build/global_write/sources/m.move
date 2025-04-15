// Test that write to a global location is handled
// correctly.
module global_write::m;

use sui::linked_table;

#[test]
fun test() {
    let ctx = &mut tx_context::dummy();
    let mut table = linked_table::new<u64, u8>(ctx);
    table.push_back(7, 42);
    // linked table contains the following line which (interestingly) uses global location
    // for the write of `next` but only if we call the `push_back` function twice (otherwise
    // it looks like it creates a temporary local variable for this assignment)
    //
    // field::borrow_mut<K, Node<K, V>>(&mut table.id, old_tail_k).next = option::some(k);
    table.push_back(42, 7);
    table.drop();
}
