module 0x42::excessive_nesting_false_positive {
    public fun nested_match_expression(x: u64) {
        match x {
            0 => {
                {
                    {
                        {
                            // This is technically 4 levels deep, but the outer level is a match expression
                            // which might be necessary and not truly contributing to excessive nesting
                        }
                    }
                }
            },
            _ => {}
        }
    }

    public fun nested_let_bindings() {
        let a = {
            let b = {
                let c = {
                    let d = {
                        // This looks like 4 levels of nesting, but it's actually a series of let bindings
                        // which might be more readable in this format than flattened
                        10
                    };
                    d + 1
                };
                c * 2
            };
            b - 5
        };
    }
}
