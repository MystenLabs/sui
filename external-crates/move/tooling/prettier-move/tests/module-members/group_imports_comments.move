// options:
// printWidth: 80
// useModuleLabel: true
// autoGroupImports: package

module prettier::group_imports_comments;

// leading comment keeps this import out of the group
use sui::coin::Coin;
/// doc comments are also preserved
use sui::balance::Balance;
use std::string::String; // trailing comment
use sui::{
    // comment inside the group
    table::Table,
};
/* block comments count too */
use sui::event;
use sui::{vec_map::VecMap, vec_set::VecSet}; // trailing comment keeps braces flat
// prettier-ignore
use sui::dynamic_field::{   Field  ,  Wrapper };
#[test_only]
use sui::{
    // annotated imports keep comments too
    test_utils::destroy,
};
use std::ascii::String as ASCII;
use sui::clock::Clock;

fun f(_: Coin<u64>, _: Balance<u64>, _: String, _: ASCII, _: Table<u8, u8>, _: &Clock) {
    abort 0
}
