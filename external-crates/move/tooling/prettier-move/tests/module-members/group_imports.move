// options:
// printWidth: 40
// useModuleLabel: true
// autoGroupImports: package

module prettier::group_imports;

use sui::{
    clock::Clock,
    coin::Coin,
    dynamic_field as df,
    dynamic_object_field as dof,
    sui::SUI,
    table::{Self, Table},
    table_vec::{Self, TableVec as TV}
};

use std::{
    string::String as UTF8,
    ascii::String as ASCII,
    vector as vec,
    vector as haha,
    option::{Self as opt, Option},
    type_name::get as type_name_get,
};

use 0x0::Account;
use 0x0::Something;
use 0x0::Account::Account;
