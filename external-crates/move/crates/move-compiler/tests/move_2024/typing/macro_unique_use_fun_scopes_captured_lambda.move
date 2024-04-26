module a::m {
    public fun into(x: u64): u64 { x }

    use fun into as u64.into;

    public macro fun apply($f: |u64| -> u64, $x: u64): u64 {
        let x = $x;
        $f(x.into())
    }
}

module b::other {
    use a::m::apply;

    public fun into(x: u8): u64 { (x as u64) }
    use fun into as u8.into;

    public macro fun myapply($f: |u8| -> u64, $x: u8): u64 {
        let x = $x;
        apply!(|u| $f(x) + x.into() + u, x.into())
    }

    public fun t(): u64 {
        myapply!(|x| x.into(), 1)
    }
}
