//# init --edition 2024.beta

//# publish
module 0x42::m {

    public enum Op has drop {
        LoopOpen,
        BreakIfEven(u64), // encodes jump PC
        Return,
        Push(u64),
        Add,
        LoopClose,
    }

    fun interp(ops: vector<Op>): u64 {
        let mut stack = vector[];
        let mut loop_pcs = vector[];
        let mut cur_pc = 0;

        'exit: {
            'interp: loop {
                match (&ops[cur_pc]) {
                    Op::LoopOpen => {
                        loop_pcs.push_back(cur_pc + 1);
                    },
                    Op::BreakIfEven(new_pc) => {
                        let top = stack[stack.length() - 1];
                        if (top % 2 == 0) {
                            loop_pcs.pop_back();
                            cur_pc = *new_pc;
                            continue 'interp
                        }
                    },
                    Op::Return => {
                        return 'exit
                    },
                    Op::Push(value) => {
                        stack.push_back(*value);
                    },
                    Op::Add => {
                        let n0 = stack.pop_back();
                        let n1 = stack.pop_back();
                        stack.push_back(n0 + n1);
                    },
                    Op::LoopClose => {
                        cur_pc = loop_pcs[loop_pcs.length() - 1];
                        continue 'interp
                    }
                };
                cur_pc = cur_pc + 1;
            }
        };

        stack.pop_back()
    }

    fun test() {

        let push = vector[
            Op::Push(1),
            Op::Return,
        ];

        assert!(interp(push) == 1);

        let add = vector[
            Op::Push(1),
            Op::Push(1),
            Op::Add,
            Op::Return,
        ];

        assert!(interp(add) == 2);

        let early_break = vector[
            Op::Push(1),
            Op::LoopOpen,
                Op::Push(1),
                Op::Add,
                Op::BreakIfEven(7),
                Op::Return,
            Op::LoopClose,
            Op::Push(100),
            Op::Return,
        ];

        assert!(interp(early_break) == 100);

        let exiting = vector[
            Op::Push(0),
            Op::LoopOpen,
                Op::Push(1),
                Op::Add,
                Op::BreakIfEven(7),
                Op::Return,
            Op::LoopClose,
            Op::Push(100),
            Op::Return,
        ];

        assert!(interp(exiting) == 1);

        let loop_and_break = vector[
            Op::Push(0),
            Op::LoopOpen,
                Op::Push(1),
                Op::Add,
                Op::BreakIfEven(6),
            Op::LoopClose,
            Op::Return,
        ];

        assert!(interp(loop_and_break) == 2);
    }
}

//# run 0x42::m::test
