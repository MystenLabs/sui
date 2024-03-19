//# init --edition 2024.alpha

//# publish

module 0x42::t {

public struct X has copy, drop { count: u64 }
public struct Y has copy, drop { x: X }

public fun bump(self: &mut X) { self.count = self.count + 1 }
public fun count(self: &X): u64 { self.count }

public fun owned(mut x: X, mut y: Y) {
    assert!(x.count() == 0, 0);
    assert!(y.x.count() == 0, 0);
    x.bump();
    y.x.bump();
    assert!(x.count() == 1, 0);
    assert!(y.x.count() == 1, 0);
}

public fun mut_ref(x: &mut X, y: &mut Y) {
    assert!(x.count() == 0, 0);
    assert!(y.x.count() == 0, 0);
    x.bump();
    y.x.bump();
    assert!(x.count() == 1, 0);
    assert!(y.x.count() == 1, 0);
}

public fun test() {
    let x = X { count: 0 };
    let y = Y { x };
    owned(x, y);

    let mut x = X { count: 0 };
    let mut y = Y { x };
    mut_ref(&mut x, &mut y);
}

}

//# run 0x42::t::test
