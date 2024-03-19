module 0x42::M {

  fun add_some(x: &mut u64): u64 { *x = *x + 1; *x }

  fun with_emits<T: drop>(_guid: vector<u8>, _msg: T, x: u64): u64 { x }

}
