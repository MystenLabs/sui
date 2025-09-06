use crate::structuring::ast::Structured;

pub fn refine_loop(loop_head: Structured) -> Structured {
    refine_while(loop_head)
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

// TODO remove unnecessary continue
// TODO loop to seq
