module 0x42::t {

public struct Z has copy, drop { i: u64 }

public struct X has copy, drop { z: Z }

public struct Y has copy, drop { x: X }

#[syntax(index)]
public fun f(_self: &X, z: &Z): &Z { z }

public fun foo (y: Y, i: &u64) {
    y.x[i].i;
}

}
