module a::m;

public struct S { x: u64 }

#[mode(test)]
extend module a::m {
    fun get_x(s: &S): u64 { s.x }
}
