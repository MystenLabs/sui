module a::m {

    public macro fun for_each<$T>($v: vector<$T>, $f: |$T|) {
        let mut v = $v;
        v.reverse();
        let mut i = 0;
        while (!v.is_empty()) {
            $f(v.pop_back());
            i = i + 1;
        };
        v.destroy_empty();
    }

    public fun sum(v: vector<u64>): u64 {
        let mut sum = 0;
        for_each!(v, |x| { sum = sum + x; });
        sum
    }
}
