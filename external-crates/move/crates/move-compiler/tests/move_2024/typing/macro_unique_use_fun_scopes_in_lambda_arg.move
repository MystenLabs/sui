module a::m {
    public fun id(x: u64): u64 { x }

    use fun id as u64.id;

    public macro fun apply($f: |u64| -> u64, $x: u64): u64 {
        let x = $x;
        $f(x.id())
    }
}

module b::other {
    use a::m::apply;

    public fun t(): u64 {
        apply!(|x| x + 1, 1)
    }
}
