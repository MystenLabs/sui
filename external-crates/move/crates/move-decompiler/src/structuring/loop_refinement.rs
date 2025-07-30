use crate::structuring::ast::Structured;

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
                    } else {
                        // this is the LoopToSeq
                        println!("Loop to Seq detected");
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
            contains_break(conseq)
                || alt
                    .as_ref()
                    .as_ref()
                    .map(contains_break)
                    .unwrap_or(false)
        }
        Structured::Switch(_, cases) => cases.iter().any(contains_break),
        Structured::Jump(_) => false,
        Structured::JumpIf(_, _, _) => false,
    }
}