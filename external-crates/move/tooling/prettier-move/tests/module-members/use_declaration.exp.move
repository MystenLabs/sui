// options:
// printWidth: 50
// useModuleLabel: true
// autoGroupImports: module

module prettier::use_declaration;

use beep::staked_sui::StakedSui;
use sui::coin::{
    Self as c,
    Coin,
    Coin as C,
    very_long_function_name_very_long_function_name as short_name
};
use sui::transfer_policy::{
    Self as policy,
    TransferPolicy,
    TransferPolicyCap,
    TransferRequest,
    TransferPolicyCap as cap,
    Kek as KEK
};

public use fun my_custom_function_with_a_long_name as
    TransferPolicyCap.very_long_function_name;

friend has_been::here;

// will break before `as`
public use fun my_custom_function_with_a_long_name as
    TransferPolicyCap.very_long_function_name;
