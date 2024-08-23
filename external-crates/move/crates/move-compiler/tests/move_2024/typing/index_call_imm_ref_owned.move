module 0x42::t {

public struct X has copy, drop { i: u64 }
public struct Y has copy, drop { x: X }

#[syntax(index)]
public fun f(self: &X, _i: u64): &u64 { &self.i }

public fun foo (x: &X, y1: &Y, y2: &Y) {
    let i = 0;
    x[i];
    y1.x[i];
    y2.x[i];
}

}
