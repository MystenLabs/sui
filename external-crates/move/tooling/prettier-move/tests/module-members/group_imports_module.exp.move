// options:
// printWidth: 40
// useModuleLabel: true
// autoGroupImports: module

module prettier::group_imports;

use a::b::{
    Self as c,
    e as f,
    g as f,
    h as i
};
use std::ascii::String as ASCII;
use std::option::{Self as opt, Option};
use std::string::String;
use std::type_name::get as type_name_get;
use std::vector as vec;
use sui::balance::{Self, Balance};
use sui::coin::{Self, Coin};
use sui::dynamic_field as df;
use sui::dynamic_object_field as dof;
use sui::event;
use sui::sui::SUI;
use sui::transfer_policy::{
    Self,
    TransferPolicy,
    TransferRequest
};

public fun do_something() {}
