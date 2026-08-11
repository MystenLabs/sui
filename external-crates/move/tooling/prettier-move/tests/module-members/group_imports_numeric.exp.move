// options:
// printWidth: 80
// useModuleLabel: true
// autoGroupImports: package

module prettier::group_imports_numeric;

use 0x0::{Account::{Self, Account}, Something};
use 0x2::{coin, sui::SUI, transfer as t};
use pkg::m::{ab as c, a as bc};

fun f(_: Account, _: Something, _: SUI, _: c, _: bc) {
    abort 0
}
