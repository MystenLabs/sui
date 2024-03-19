module a::m {
    // macros have their own unique scopes
    macro fun `for`($start: u64, $stop: u64, $body: |u64|) {
        let mut i = $start;
        let stop = $stop;
        while (i < stop) {
            $body(i);
            i = i + 1
        }
    }

    macro fun for_each<$T>($v: &vector<$T>, $body: |&$T|) {
        let v = $v;
        let mut i = 0;
        let n = v.length();
        while (i < n) {
            $body(v.borrow(i));
            i = i + 1
        }
    }

    macro fun new<$T>($len: u64, $f: |u64| -> $T): vector<$T> {
        let len = $len;
        let mut v = vector[];
        `for`!(0, len, |i| v.push_back($f(i)));
        v
    }

    macro fun sum($v: &vector<u64>): u64 {
        let mut s = 0;
        for_each!($v, |i| s = s + *i);
        s
    }

    entry fun main() {
        let v = new!(10, |i| i);
        assert!(sum!(&v) == 45, 0);
    }

}
