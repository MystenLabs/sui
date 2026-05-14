// 'let ... else' is gated to Move 2024+; using it under the legacy edition
// must produce a feature-edition diagnostic at the 'else' keyword.
module 0x42::m {
    fun f(): u64 {
        let x = 1u64 else { abort 0 };
        x
    }
}
