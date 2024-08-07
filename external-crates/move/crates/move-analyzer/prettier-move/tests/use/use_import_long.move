// options:
// printWidth: 100
// autoGroupImports: module

module prettier::use_declaration {
    use sui::transfer_policy::{Self as policy, TransferPolicy,
        TransferPolicyCap,
        TransferRequest
    };
    use sui::transfer_policy::TransferPolicyCap as cap;
    use sui::{
        transfer_policy::{
            TransferPolicy,
            TransferPolicyCap,
            TransferRequest
        },
        transfer_policy::TransferPolicyCap as cap,
    };

    public use fun my_custom_function_with_a_long_name as TransferPolicyCap.very_long_function_name;
}
