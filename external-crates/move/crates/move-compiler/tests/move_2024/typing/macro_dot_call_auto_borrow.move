module 0x42::t {

public struct X has drop {}
public struct Y has drop { x: X }

macro fun val($_self: X) {}
macro fun imm($_self: &X) {}
macro fun mut_($_self: &mut X) {}

public fun foo(mut x1: X, x2: &X, x3: &mut X) {
    x1.mut_!();
    x3.mut_!();

    x1.imm!();
    x2.imm!();
    x3.imm!();

    x1.val!();
}

public fun bar(mut y1: Y, y2: &Y, y3: &mut Y) {
    y1.x.mut_!();
    y3.x.mut_!();

    y1.x.imm!();
    y2.x.imm!();
    y3.x.imm!();
}

}
