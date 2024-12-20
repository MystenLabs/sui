module prover::enum_tests;

public enum E {
  A(u64, u64),
  B(u64),
  C,
}

public fun is_A(e: &E): bool {
  match (e) {
    E::A(_x, _y) => true,
    _ => false,
  }
}

public fun unwrap_A(e: E): (u64, u64) {
  match (e) {
    E::A(x, y) => {
      (x, y)
    },
    E::B(_) => {
      abort (0)
    },
    E::C => {
      abort (0)
    },
  }
}

public fun test_is_A_unwrap_A(x: u64) {
  let e = E::A(x, 0);
  assert!(is_A(&e));
  let (y, _) = unwrap_A(e);
  assert!(x == y);
}

#[spec(verify)]
public fun test_is_A_unwrap_A_spec(x: u64) {
  test_is_A_unwrap_A(x);
}
