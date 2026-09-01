module ConstDeps::defs {
    public(package) const MAX: u64 = 100;
    public(package) const LIMIT: u64 = 10;

    public macro fun clamp($x: u64): u64 {
        let x = $x;
        if (x > LIMIT) LIMIT else x
    }
}
