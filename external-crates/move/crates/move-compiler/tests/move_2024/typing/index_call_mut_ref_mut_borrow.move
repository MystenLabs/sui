module 0x42::t {

public struct X has copy, drop { i: u64 }
public struct Y has copy, drop { x: X }

#[syntax(index)]
public fun f(self: &mut X, _i: u64): &mut u64 { &mut self.i }

public fun foo (x: &mut X, y1: &mut Y, y2: &mut Y) {
    let i = 0;
    let _x = &mut x[i];
    let _y1 = &mut y1.x[i];
    let _z1 = &mut y2.x[i];
}

}
