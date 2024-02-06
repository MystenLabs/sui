module a::m {
    macro fun do($f: || -> ()): () {
        $f()
    }

    // TODO Fix deadcode bug
    // fun t(cond: bool) {
    //     let _: u64 = 'a: loop {
    //         do!(|| {
    //             if (cond) continue 'a;
    //             break 'a 0
    //         })
    //     };
    //     'b: while (true) {
    //         do!(|| {
    //             if (cond) continue 'b;
    //             break 'b
    //         })
    //     };
    // }

}
