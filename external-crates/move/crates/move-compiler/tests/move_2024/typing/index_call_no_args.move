module 0x42::t {

public struct X has copy, drop { i: u64 }

#[syntax(index)]
public fun f(self: &X): &u64 { &self.i }

public fun foo (x: X) {
    x[];
}

}
