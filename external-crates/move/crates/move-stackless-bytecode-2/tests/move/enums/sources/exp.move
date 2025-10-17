module enums::exp;

public enum Exp has copy, drop {
    Push(u64),
    PushTwo(u64, u64),
    Add,
    Sub,
    Mul,
}

fun pop_back(_stack: &mut vector<u64>): u64 {
    abort 0
}

fun push_back(_stack: &mut vector<u64>, _value: u64) {
    abort 0
}

public fun step(stack: &mut vector<u64>, instr: Exp) {
    match (instr) {
        Exp::Push(value) => push_back(stack, value),
        Exp::PushTwo(v0, v1) => {
            push_back(stack, v0);
            push_back(stack, v1);
        },
        Exp::Add => {
            let a = pop_back(stack);
            let b = pop_back(stack);
            push_back(stack, a + b);
        },
        Exp::Sub => {
            let a = pop_back(stack);
            let b = pop_back(stack);
            push_back(stack, a - b);
        },
        Exp::Mul => {
            let a = pop_back(stack);
            let b = pop_back(stack);
            push_back(stack, a * b);
        },
    }
}
