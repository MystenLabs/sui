use sui_types::base_types::{
    ObjectDigest, ObjectID, SequenceNumber, SuiAddress, TransactionDigest,
};
use sui_types::gas::GasCostSummary;
use sui_types::messages::{ExecutionStatus, TransactionData, TransactionEffects};
use sui_types::object::Owner;

use crate::operations::Operation;
use crate::state::extract_balance_changes_from_ops;

#[test]
fn test_transfer_sui_null_amount() {
    let sender = SuiAddress::random_for_testing_only();
    let gas = (
        ObjectID::random(),
        SequenceNumber::new(),
        ObjectDigest::random(),
    );
    let data = TransactionData::new_transfer_sui(
        SuiAddress::random_for_testing_only(),
        sender,
        None,
        gas,
        1000,
    );

    let effect = TransactionEffects {
        status: ExecutionStatus::Success,
        gas_used: GasCostSummary {
            computation_cost: 100,
            storage_cost: 100,
            storage_rebate: 50,
        },
        shared_objects: vec![],
        transaction_digest: TransactionDigest::random(),
        created: vec![],
        mutated: vec![],
        unwrapped: vec![],
        deleted: vec![],
        wrapped: vec![],
        gas_object: (gas, Owner::AddressOwner(sender)),
        events: vec![],
        dependencies: vec![],
    };
    let ops = Operation::from_data_and_effect(&data, &effect, &[]).unwrap();
    let balances = extract_balance_changes_from_ops(ops).unwrap();

    println!("{:?}", balances)
}
