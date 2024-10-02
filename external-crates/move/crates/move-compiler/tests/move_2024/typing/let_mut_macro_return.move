module a::loopy {
    public macro fun for_each($start: u64, $end: u64, $body: |u64|) {
        let mut i = $start;
        let end = $end;
        while (i < end) {
            $body(i);
            i = i + 1;
        }
    }
}

module a::m {
    use a::loopy::for_each;
    fun t() {
        // TODO this probably shouldn't give any warnings
        'a: { for_each!(0, 1, |_| return 'a) }
    }
}
