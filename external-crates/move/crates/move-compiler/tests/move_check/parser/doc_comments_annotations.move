/// This is a doc comment above an annotation.
#[allow(unused_const)]
module 0x42::m {
    #[allow(dead_code)]
    /// This is a doc comment on a constant with an annotation. Below the annotation.
    const Error: u32 = 0;

    /// This is a doc comment on a constant with an annotation. Above the annotation.
    #[allow(dead_code)]
    const OtherError: u32 = 0;

    /// This is the top doc comment
    #[allow(dead_code)]
    /// This is the middle doc comment
    #[ext(something)]
    /// This is the bottom doc comment
    const Woah: u32 = 0;

    /// This is the top doc comment
    #[allow(dead_code)]
    /// This is the middle doc comment
    #[ext(something)]
    const Cool: u32 = 0;

    /// This is a doc comment above a function with an annotation. Above the annotation.
    #[allow(dead_code)]
    fun test() { }

    #[allow(dead_code)]
    /// This is a doc comment above a function with an annotation. Below the annotation.
    fun test1() { }
}
