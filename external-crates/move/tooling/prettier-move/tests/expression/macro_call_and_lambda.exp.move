// options:
// printWidth: 40
// useModuleLabel: true

module prettier::macro_and_lambda;

fun macro_call() {
    // regular cases of macro calls and lambdas:
    // - no arguments
    // - few arguments
    // - breaking / many arguments
    stack.some!();
    stack.some!(alice, bob, carl, dave);
    stack.some!(
        alice,
        bob,
        carl,
        dave,
        smith,
    );

    // comment inside
    stack.some!(
        /* comment */
    );
    stack.some!(
        // comment
    );

    // module access macro_call
    vector::tabulate!(|i| i);

    // for chains, see `dot_expression`
    vector
        .length()
        .min!(10000)
        .do_range!(x, |_| {});
}

fun lambda() {
    // lambda fits on one line
    vector.length().do!(|x| print(&x));

    // lambda without block does not break first
    vector
        .length()
        .do!(|x| other_function(&x));

    // call_args breaks
    vector
        .length()
        .do!(
            |argument| option(argument),
        );

    // once chain is broken, lambda
    // breaks, inner expression breaks
    vector
        .length()
        .do!(
            |argument| option::some(
                argument,
            ),
        );

    // argument list can also break if
    // this way it fits; breaks the parent
    vector
        .length()
        .do!(
            |
                argument,
                argument2,
            | option::some(argument),
        );

    // single line blocks should be allowed
    length.do!(|x| { let y = x; y });

    // and get expanded when content is
    // too long
    length.do!(|x| {
        let y = x;
        y + x
    });

    // if lamda has a block argument, it
    // does not require a newline
    vector.length().do!(|x| {
        // even this comment does not
        // break the chain!
        function_call(x + 100); // and neither does this one

        // locally broken calls should
        // NOT break the chain
        vector::tabulate!(
            x,
            |index| option::some(index),
        );

        // similarly, does not break
        vector::tabulate!(x, |index| {
            option::some(index)
        });
    });

    // if a block does not fit the line,
    // parent group will get broken, and
    // we expect correct indentation
    vector
        .length()
        .min(100)
        .max(200)
        .do!(|x| { function_call(x) });

    // same applies to multiple calls in
    // a row
    vector
        .map!(|elem| option::some(elem))
        .map!(|elem| {
            elem.destroy_some()
        })
        .map!(|elem| {
            option::some(elem)
        })
        .destroy!(|_| {});

    // trying different breaking cases
    vector.length().do!(|elem| {
        if (elem % 2 == 0) {
            function_call(100)
        } else {
            function_call(200)
        };

        return
    });

    // conditional group should be broken
    // by a non-breaking group (somehow)
    // TODO: revisit this example
    if (vector.find_index!(|el| {
            el % 2 == 0 && el % 3 == 0
        }).is_some()) {
        function_call();
    };
}

fun f() {
    animation.do_ref!(|el| {
        contents =
            contents
                .map!(|mut contents| {
                    contents.push_back(el.to_string());
                    contents
                })
                .or!(
                    vector[
                        el.to_string(),
                    ],
                );
    });

    stack.other!(argument, |x| x.do!());
    stack.another!(
        argument,
        argument,
        argument,
        argument,
        |x| {
            x.do!();
        },
    );
    stack.another!(
        argument,
        argument,
        argument,
        |x| x.destroy!(|_| {}),
    );
    stack.destroy!(
        |e| get_group_int(
            b,
            e,
            visited,
        ),
    );
    stack.destroy!(|e| {
        get_group_int(b, e, visited);
        get_group_int(b, e, visited);
        get_group_int(b, e, visited);
    });
    stack.destroy!(|e| {
        get_group_int(b, e, visited);
    });
    stack.destroy!(|x| {});
    stack.very_very_very_long!(|x| {
        x.do!();
    });
    stack.very_very_very_long_macro_name_with_a_cherry!(
        |x| {
            x.do!();
        },
    );

    stack.do!(|el| {
        if (some) {
            expressions_break;
        };

        if (el.is_some()) {
            field::borrow_mut<
                K,
                Node<K, V>,
            >(
                &mut table.id,
                *el.borrow(),
            ).next = next;
        };
    });

    if (el.is_some()) {
        field::borrow_mut<
            K,
            Node<K, V>,
        >(
            &mut table.id,
            *el.borrow(),
        ).next = next;
    };

    idx_sorted.find_index!(|idx| {
        let (el, value) = set
            .elems
            .get_entry_by_idx(
                *idx as u64,
            );
        expected > *value
    });

    aha.idx_sorted.find_index!(|idx| {
        let (el, value) = set
            .elems
            .get_entry_by_idx(
                *idx as u64,
            );
        expected > *value
    });

    aha.idx_sorted.find_index!(|idx| {
        // line comment contains a hardline
        let (el, value) = set
            .elems
            .get_entry_by_idx(
                *idx as u64,
            );
        expected > *value
    });

    let insert_idx = set
        .idx_sorted
        .find_index!(|idx| {
            let (_elem, value) = set
                .elements
                .get_entry_by_idx(
                    *idx as u64,
                );
            total_value > *value
        })
        .destroy_or!(
            set.idx_sorted.length(),
        );
}
