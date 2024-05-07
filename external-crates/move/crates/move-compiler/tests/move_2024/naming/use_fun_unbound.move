module a::m {
    use fun id as u64.id;

    fun t(x: u64): u64 {
        use fun x as u64.x;
        x
    }

    macro fun apply($f: |u64| -> ()) {
        use fun $f as u64.f;
        $f(0);
    }
}
