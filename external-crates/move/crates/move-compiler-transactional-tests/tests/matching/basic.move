//# init --edition 2024.alpha

//# publish

module 0x42::t {

    use std::vector;
    use fun std::vector::pop_back as vector.pop_back;
    use fun std::vector::push_back as vector.push_back;
    use fun std::vector::is_empty as vector.is_empty;

    public enum Exp has drop {
       Done,
       Add,
       Mul,
       Num(u64),
    }

    const EINVALIDEXP: u64 = 0;

    public fun evaluate(mut expressions: vector<Exp>): u64 {
        let mut stack = vector[];
        while (!expressions.is_empty()) {
            match (expressions.pop_back()) {
                Exp::Done => break,
                Exp::Add => {
                    let e1 = stack.pop_back();
                    let e2 = stack.pop_back();
                    stack.push_back(e1 + e2);
                },
                Exp::Mul => {
                    let e1 = stack.pop_back();
                    let e2 = stack.pop_back();
                    stack.push_back(e1 * e2);
                },
                Exp::Num(number) => {
                    stack.push_back(number);
                }
            }
        };
        let result = vector::pop_back(&mut stack);
        assert!(expressions.is_empty(), EINVALIDEXP);
        assert!(stack.is_empty(), EINVALIDEXP);
        result
    }

    use fun evaluate as vector.evaluate;

    public fun test() {
        let input = vector[Exp::Done, Exp::Add, Exp::Num(5), Exp::Num(5)];
        assert!(input.evaluate() == 10, 0);
    }

}

//# run 0x42::t::test
