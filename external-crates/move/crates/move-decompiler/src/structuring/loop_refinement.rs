use crate::structuring::ast::{Code, Structured};

pub fn refine_loop(loop_head: Structured) -> Structured {
    let loop_head = refine_while(loop_head);
    // TODO
    // let structured = refine_do_while(structured);
    loop_head
}

fn refine_while(loop_head: Structured) -> Structured {
    use Structured as S;

    println!("Loop head: {loop_head:#?}");
    let S::Loop(loop_) = loop_head else {
        return loop_head;
    };
    let S::Seq(mut loop_seq) = *loop_ else {
        return S::Loop(loop_);
    };
    // FIXME fix what to do when loop guard has more than one condition?
    // which means that the inner sequence has more than 1 element: seq[seq[..], seq[..]]
    if loop_seq.len() > 1 {
        return S::Loop(Box::new(S::Seq(loop_seq)));
    }
    
    let S::Seq(mut loop_seq) = loop_seq.pop().unwrap() else {
        return S::Loop(Box::new(S::Seq(loop_seq)));
    };
    let Some(S::IfElse(cond, conseq, alt_opt)) = loop_seq.first().cloned() else {
        return S::Loop(Box::new(S::Seq(loop_seq)));
    };
    match (*conseq, *alt_opt) {
        (S::Break, _) => {
            let S::IfElse(_, _, alt) = loop_seq.remove(0) else {
                unreachable!("Expected an IfElse condition");
            };
            let loop_seq = if let Some(alt) = *alt {
                loop_seq.insert(0, alt);
                loop_seq
            } else {
                loop_seq
            };
            S::While(cond, Box::new(S::Seq(loop_seq)))
        }
        (_, Some(S::Break)) => {
            /* alt case */
            let S::IfElse(_, conseq, _) = loop_seq.remove(0) else {
                unreachable!("Expected an IfElse condition");
            };
            loop_seq.insert(0, *conseq);
            Structured::While(cond, Box::new(Structured::Seq(loop_seq)))
        }
        _ => S::Loop(Box::new(S::Seq(loop_seq))),
    }
}

fn refine_do_while(structured: Structured) -> Structured {
    use Structured as S;

    let S::Loop(body) = structured else {
        return structured;
    };
    let S::Seq(seq) = *body else {
        return S::Loop(body);
    };
    let Some(S::IfElse(_, conseq, alt_opt)) = seq.last().cloned() else {
        return S::Loop(Box::new(S::Seq(seq)));
    };
    match (*conseq, *alt_opt) {
        (S::Break, _) | (_, Some(S::Break)) => {
            println!("This could be a Do-while loop");
        }
        _ => {}
    }
    S::Loop(Box::new(S::Seq(seq)))
}

fn refine_non_loop(structured: Structured) -> Structured {
    todo!()
}

pub fn loop_type(loop_head: Structured) -> Structured {
    match loop_head {
        Structured::Loop(body) => {
            println!("Loop body:\n{:#?}", body);
            match *body {
                Structured::Seq(mut seq) => {
                    if seq.is_empty() {
                        unreachable!("Empty loop body should not be structured as a loop");
                    }
                    // this is a while loop
                    if is_break_condition(seq.first().unwrap()) {
                        println!("While loop detected");
                        let condition = seq.remove(0);
                        match condition {
                            Structured::IfElse(cond, conseq, alt) => {
                                if matches!(*conseq, Structured::Break) {
                                    Structured::While(cond, Box::new(alt.unwrap()))
                                } else {
                                    Structured::While(cond, Box::new(*conseq))
                                }
                            }
                            _ => unreachable!(),
                        }
                    } else if is_break_condition(seq.last().unwrap()) {
                        // this is a do-while loop
                        println!("This could be a Do-while loop");
                        Structured::Loop(Box::new(Structured::Seq(seq)))
                    } else if matches!(seq.last().unwrap(), Structured::Break)
                        && !seq.iter().any(contains_continue)
                    {
                        // this is the LoopToSeq
                        println!("Loop to Seq detected");
                        let brk = seq.pop().unwrap();
                        assert!(matches!(brk, Structured::Break));
                        Structured::Seq(seq)
                    } else if matches!(seq.last().unwrap(), Structured::Continue)
                        && !seq.iter().any(is_break_condition)
                    {
                        // removing unnecessary continue
                        let cntn = seq.pop().unwrap();
                        assert!(matches!(cntn, Structured::Continue));
                        Structured::Loop(Box::new(Structured::Seq(seq)))
                    } else if contains_invertable_condition(&seq) {
                        // inverting condition
                        let brk = seq.pop().unwrap();
                        let mut condition = seq.pop().unwrap();
                        assert!(matches!(brk, Structured::Break));
                        assert!(matches!(condition, Structured::IfElse(_, _, _)));
                        let Structured::IfElse(cond, conseq, alt) = condition else {
                            unreachable!("Expected an IfElse condition");
                        };
                        // TODO invert condition here
                        let inverted_cond = invert_condition(cond);
                        condition = Structured::IfElse(
                            inverted_cond,
                            Box::new(Structured::Break),
                            Box::new(None),
                        );
                        seq.push(condition);
                        Structured::Loop(Box::new(Structured::Seq(seq)))
                    } else {
                        Structured::Loop(Box::new(Structured::Seq(seq)))
                    }
                }
                Structured::IfElse(cond, conseq, alt) => {
                    let alt = alt.unwrap();
                    if !contains_break(&conseq) && contains_break(&alt) {
                        println!("CondToSeq detected");
                        println!("If-Else with break in alt branch detected");
                        let loop_head = Structured::Loop(Box::new(Structured::Seq(vec![
                            Structured::While(cond, conseq),
                            alt,
                        ])));
                        loop_type(loop_head)
                    } else if contains_break(&conseq) && !contains_break(&alt) {
                        println!("CondToSeqNeg detected");
                        println!("If-Else with break in conseq branch detected");
                        let loop_head = Structured::Loop(Box::new(Structured::Seq(vec![
                            Structured::While(cond, Box::new(alt)),
                            *conseq,
                        ])));
                        loop_type(loop_head)
                    } else {
                        unreachable!()
                    }
                }
                Structured::JumpIf(_cond, _conseq, _alt) => {
                    Structured::Loop(Box::new(Structured::Continue))
                }
                Structured::Continue => Structured::Loop(Box::new(Structured::Continue)),
                _ => todo!(),
            }
        }
        Structured::Break
        | Structured::Continue
        | Structured::Block(_)
        | Structured::Seq(_)
        | Structured::While(_, _)
        | Structured::IfElse(_, _, _)
        | Structured::Switch(_, _)
        | Structured::Jump(_)
        | Structured::JumpIf(_, _, _) => unreachable!("Expected a loop head"),
    }
}

fn is_break_condition(structured: &Structured) -> bool {
    match structured {
        Structured::IfElse(_, conseq, alt) => match (&**conseq, &**alt) {
            (Structured::Break, _) => true,
            (_, Some(Structured::Break)) => true,
            _ => false,
        },
        Structured::Break
        | Structured::Continue
        | Structured::Block(_)
        | Structured::Loop(_)
        | Structured::Seq(_)
        | Structured::While(_, _)
        | Structured::Switch(_, _)
        | Structured::Jump(_)
        | Structured::JumpIf(_, _, _) => false,
    }
}

fn contains_break(structured: &Structured) -> bool {
    match structured {
        Structured::Break => true,
        Structured::Continue => false,
        Structured::Block(_) => false,
        Structured::Loop(_) => false,
        Structured::Seq(seq) => seq.iter().any(contains_break),
        Structured::While(_, body) => contains_break(body),
        Structured::IfElse(_, conseq, alt) => {
            contains_break(conseq) || alt.as_ref().as_ref().map(contains_break).unwrap_or(false)
        }
        Structured::Switch(_, cases) => cases.iter().any(contains_break),
        Structured::Jump(_) => false,
        Structured::JumpIf(_, _, _) => false,
    }
}

fn contains_continue(structured: &Structured) -> bool {
    match structured {
        Structured::Continue => true,
        Structured::Break => false,
        Structured::Block(_) => false,
        Structured::Loop(_) => false,
        Structured::Seq(seq) => seq.iter().any(contains_continue),
        Structured::While(_, body) => contains_continue(body),
        Structured::IfElse(_, conseq, alt) => {
            contains_continue(conseq)
                || alt
                    .as_ref()
                    .as_ref()
                    .map(contains_continue)
                    .unwrap_or(false)
        }
        Structured::Switch(_, cases) => cases.iter().any(contains_continue),
        Structured::Jump(_) => false,
        Structured::JumpIf(_, _, _) => false,
    }
}

fn contains_invertable_condition(seq: &[Structured]) -> bool {
    let [ref prev, ref last] = seq[..2] else {
        return false;
    };
    matches!((prev, last), (Structured::IfElse(_, conseq, _), Structured::Break) if matches!(**conseq, Structured::Continue))
}

fn invert_condition(cond: Code) -> Code {
    // Placeholder for actual condition inversion logic
    (cond.0, !cond.1)
}
