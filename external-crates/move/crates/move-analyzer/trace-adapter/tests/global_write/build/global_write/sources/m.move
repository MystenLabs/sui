// Test that write to a global location is handled
// correctly.
module global_write::m;

use sui::linked_table;

#[test]
fun test() {
    let ctx = &mut tx_context::dummy();
    let mut table = linked_table::new<u64, u8>(ctx);
    table.push_back(7, 42);
    // linked table contains the following line of code which uses global location
    // for the write of `next` (we need to call `push_back` twice to actually exercise
    // this line of code):
    //
    // field::borrow_mut<K, Node<K, V>>(&mut table.id, old_tail_k).next = option::some(k);
    table.push_back(42, 7);
    table.drop();
}
