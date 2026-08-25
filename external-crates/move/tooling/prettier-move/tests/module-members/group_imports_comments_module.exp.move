// options:
// printWidth: 80
// useModuleLabel: true
// autoGroupImports: module

module prettier::group_imports_comments_module;

use std::ascii::String as ASCII;
use std::string::String;

// stays in place in module mode too
use sui::coin::Coin;
use sui::{balance::Balance, sui::SUI}; // kept with its comment

fun f(_: Coin<u64>, _: Balance<u64>, _: SUI, _: String, _: ASCII) {
    abort 0
}
