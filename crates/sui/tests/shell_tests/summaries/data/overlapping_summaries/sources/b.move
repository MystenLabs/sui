module overlapping_summaries::b;

#[allow(unused)]
public struct Y {
    x: child_pkg::a::X,
    y: other_child::a::X,
}

public fun g() { }
