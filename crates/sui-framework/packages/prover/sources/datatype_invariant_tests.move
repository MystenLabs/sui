module prover::datatype_invariant_tests;

#[spec_only]
use prover::prover::requires;

public struct Point<phantom T> has copy, drop {
    x: u64,
}

public fun point_average<T>(a: Point<T>, b: Point<T>): Point<T> {
    Point { x: (((a.x as u128) + (b.x as u128)) / 2) as u64 }
}

public struct Range<phantom T> {
    begin: Point<T>,
    end: Point<T>,
}

public fun range_new<T>(begin: Point<T>, end: Point<T>): Range<T> {
    assert!(begin.x <= end.x);
    Range { begin, end } 
}

public fun range_length<T>(self: &Range<T>): u64 {
    self.end.x - self.begin.x
}

public fun range_contains<T>(self: &Range<T>, p: Point<T>): bool {
    !(p.x < self.begin.x && p.x < self.end.x) && !(self.begin.x < p.x && self.end.x < p.x)
}

public fun range_split<T>(self: &mut Range<T>, p: Point<T>): Range<T> {
    let r = range_new(p, self.end);
    self.end = p;
    r
}

#[spec(verify)]
public fun range_split_spec<T>(self: &mut Range<T>, p: Point<T>): Range<T> {
    requires(self.range_contains(p));
    let result = range_split(self, p);
    result
}

public fun range_join<T>(self: &mut Range<T>, r: Range<T>) {
    let Range { begin, end } = r;
    if (begin.x < self.begin.x) {
        self.begin = begin;
    };
    if (end.x > self.end.x) {
        self.end = end;
    };
}

#[spec(verify)]
public fun range_join_spec<T>(self: &mut Range<T>, r: Range<T>) {
    requires(self.range_contains(r.begin) || self.range_contains(r.end));
    range_join(self, r);
}

public fun test<T>(a: Point<T>, b: Point<T>, c: Point<T>): (Range<T>, Range<T>) {
    let mut r1 = range_new(a, b);
    let r2 = range_new(b, c);
    range_join(&mut r1, r2);
    let d = point_average(r1.begin, r1.end);
    let r3 = range_split(&mut r1, d);
    (r1, r3)
}

#[spec(verify)]
public fun test_spec<T>(a: Point<T>, b: Point<T>, c: Point<T>): (Range<T>, Range<T>) {
    requires(a.x <= b.x);
    requires(b.x <= c.x);
    test(a, b, c)
}

#[spec_only]
public fun Range_inv<T>(self: &Range<T>): bool {
    self.begin.x <= self.end.x
}
