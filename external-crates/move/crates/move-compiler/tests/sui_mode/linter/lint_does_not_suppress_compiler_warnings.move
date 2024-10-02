module a::m {
    // a linter suppression should not work for regular compiler warnings
    #[allow(lint(all))]
    struct S { f: u64 }
}
