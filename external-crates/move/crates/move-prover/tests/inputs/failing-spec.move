module 0x42::foo;

public fun foo() {
  assert!(true);
}

#[spec(prove)]
fun foo_spec() {
  foo();
  this_is_fine_for_some_reason();
}
