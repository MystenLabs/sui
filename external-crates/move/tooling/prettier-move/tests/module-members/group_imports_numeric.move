// options:
// printWidth: 80
// useModuleLabel: true
// autoGroupImports: package

module prettier::group_imports_numeric;

use 0x2::{coin, transfer as t};
use 0x0::{Account::{Self, Account}, Something};
use 0x2::sui::SUI;
use 0x2::sui::SUI;
use pkg::m::{ab as c, a as bc};

fun f(_: Account, _: Something, _: SUI, _: c, _: bc) {
    abort 0
}
