//# init --edition 2024.alpha

//# publish

module 0x42::m {

    macro fun `for`($start: u64, $stop: u64, $body: |u64|) {
        let mut i = $start;
        let stop = $stop;
        while (i < stop) {
            $body(i);
            i = i + 1
        }
    }

    macro fun new<$T>($len: u64, $f: |u64| -> $T): vector<$T> {
        let mut v = vector[];
        `for`!(0, $len, |i| v.push_back($f(i)));
        v
    }

    macro fun for_each<$T>($v: &vector<$T>, $body: |&$T|) {
        let v = $v;
        `for`!(0, v.length(), |i| $body(v.borrow(i)))
    }

    macro fun fold<$T, $U>(
        $xs: &vector<$T>,
        $init: $U,
        $body: |$U, &$T| -> $U,
    ): $U {
        let xs = $xs;
        let mut acc = $init;
        for_each!(xs, |x| acc = $body(acc, x));
        acc
    }

    macro fun sum($v: &vector<u64>): u64 {
        fold!($v, 0, |acc, x| acc + *x)
    }

    entry fun main() {
        let v = new!(10, |i| i);
        assert!(sum!(&v) == 45, 0);

        let vs = new!(10, |i| new!(10, |j| i * j));
        let total = fold!(&vs, 0, |acc, v| acc + sum!(v));
        assert!(total == 2025, 0);
    }

}

//# run 0x42::m::main
