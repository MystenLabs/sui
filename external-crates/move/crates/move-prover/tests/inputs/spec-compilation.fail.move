module 0x42::foo;

public fun foo() {
  assert!(true);
}

#[spec(prove)]
public fun foo_spec() {
  foo();
  and_this_function_does_not_exist();
}
