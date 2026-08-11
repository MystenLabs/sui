/// Regression for method-call macro receiver metadata.
///
/// In this example, each `X {}.id!()` call is a method-style macro call. The
/// compiler represents the receiver as a synthetic by-value bind, approximately
/// `let $x = X {}`. That synthetic bind uses the macro parameter's definition
/// location.
///
/// Bad behavior: the analyzer recorded the synthetic bind as a real local
/// definition, so hovering `$x` in the macro body showed `let $x: ...`.
///
/// Good behavior: the analyzer visits the call-site receiver expression, but
/// does not let the synthetic receiver bind overwrite macro parameter metadata.
/// Hovering `$x` below should show `$x: Macros::method_macro_receiver::X`.
module Macros::method_macro_receiver {
    public struct X has drop {}

    public macro fun id($x: X): X {
        let y = $x;
        y
    }

    fun z_call() {
        let _ = X {}.id!();
        let _ = X {}.id!();
    }
}
