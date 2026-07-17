// options:
// printWidth: 80
// useModuleLabel: true
// autoGroupImports: module

module prettier::group_imports_numeric_module;

use 0x0::Account::{Self, Account};
use 0x0::Something;
use 0x2::coin;
use 0x2::sui::SUI;
use 0x2::transfer as t;

fun f(_: Account, _: Something, _: SUI) {
    abort 0
}
