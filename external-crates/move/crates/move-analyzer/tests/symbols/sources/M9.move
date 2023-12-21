module Symbols::M10 {
   use Symbols::M9;
}

/// A module doc comment
module Symbols::M9 {

    use Symbols::M1;
    use Symbols::M1 as ALIAS_M1;
    use Symbols::{M1 as BLAH, M2 as BLEH};
    use Symbols::M2::{SomeOtherStruct as S, some_other_struct};
}
