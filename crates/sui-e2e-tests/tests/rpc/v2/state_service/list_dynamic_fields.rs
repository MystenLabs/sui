// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prost_types::FieldMask;
use sui_macros::sim_test;
use sui_rpc::Client;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2::changed_object::IdOperation;
use sui_rpc::proto::sui::rpc::v2::dynamic_field::DynamicFieldKind;
use sui_rpc::proto::sui::rpc::v2::{
    DynamicFieldFilter, DynamicFieldLiteral, DynamicFieldNameTypeFilter, DynamicFieldTerm,
    ListDynamicFieldsRequest, dynamic_field_literal,
};
use sui_types::SUI_FRAMEWORK_PACKAGE_ID;
use sui_types::gas_coin::GasCoin;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::{Command, TransactionData};
use test_cluster::TestClusterBuilder;

/// A kiosk holding one listed item has a dynamic object field for the item
/// (`0x2::kiosk::Item`) and a dynamic field for its listing
/// (`0x2::kiosk::Listing`). Filters select each by name type or kind.
#[sim_test]
async fn test_filter_kiosk_fields() {
    let cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .build()
        .await;
    let mut client = Client::new(cluster.rpc_url().to_owned()).unwrap();
    let address = cluster.get_address_0();

    let mut builder = ProgrammableTransactionBuilder::new();
    let call = |builder: &mut ProgrammableTransactionBuilder,
                module: &str,
                function: &str,
                type_args,
                args| {
        builder.programmable_move_call(
            SUI_FRAMEWORK_PACKAGE_ID,
            module.parse().unwrap(),
            function.parse().unwrap(),
            type_args,
            args,
        )
    };
    let amount = builder.pure(1_000u64).unwrap();
    let coin = builder.command(Command::SplitCoins(
        sui_types::transaction::Argument::GasCoin,
        vec![amount],
    ));
    let kiosk_and_cap = call(&mut builder, "kiosk", "new", vec![], vec![]);
    let sui_types::transaction::Argument::Result(kiosk_and_cap) = kiosk_and_cap else {
        unreachable!("a move call returns a result");
    };
    let kiosk = sui_types::transaction::Argument::NestedResult(kiosk_and_cap, 0);
    let cap = sui_types::transaction::Argument::NestedResult(kiosk_and_cap, 1);
    let coin_type = vec![GasCoin::type_().into()];
    let item_id = call(&mut builder, "object", "id", coin_type.clone(), vec![coin]);
    call(
        &mut builder,
        "kiosk",
        "place",
        coin_type.clone(),
        vec![kiosk, cap, coin],
    );
    let price = builder.pure(5u64).unwrap();
    call(
        &mut builder,
        "kiosk",
        "list",
        coin_type,
        vec![kiosk, cap, item_id, price],
    );
    call(
        &mut builder,
        "transfer",
        "public_share_object",
        vec![sui_types::parse_sui_type_tag("0x2::kiosk::Kiosk").unwrap()],
        vec![kiosk],
    );
    builder.transfer_arg(address, cap);

    let gas = cluster
        .wallet
        .get_one_gas_object_owned_by_address(address)
        .await
        .unwrap()
        .unwrap();
    let gas_price = cluster.get_reference_gas_price().await;
    let tx_data = TransactionData::new_programmable(
        address,
        vec![gas],
        builder.finish(),
        50_000_000,
        gas_price,
    );
    let txn = cluster.wallet.sign_transaction(&tx_data).await;
    let transaction = super::super::execute_transaction(&mut client, &txn).await;
    let kiosk_id = transaction
        .effects
        .unwrap()
        .changed_objects
        .iter()
        .find(|o| {
            matches!(o.id_operation(), IdOperation::Created)
                && o.object_type().ends_with("::kiosk::Kiosk")
        })
        .unwrap()
        .object_id()
        .to_owned();

    let mut list_kinds = async |filter: Option<DynamicFieldFilter>| {
        let mut kinds = Vec::new();
        let mut page_token = None;
        loop {
            let response = client
                .state_client()
                .list_dynamic_fields({
                    let mut message = ListDynamicFieldsRequest::default();
                    message.parent = Some(kiosk_id.clone());
                    message.filter = filter.clone();
                    message.page_size = Some(1);
                    message.page_token = page_token.take();
                    message.read_mask = Some(FieldMask::from_str("kind"));
                    message
                })
                .await
                .unwrap()
                .into_inner();
            kinds.extend(response.dynamic_fields.iter().map(|field| field.kind()));
            match response.next_page_token {
                Some(token) => page_token = Some(token),
                None => break,
            }
        }
        kinds.sort();
        kinds
    };

    let both = vec![DynamicFieldKind::Field, DynamicFieldKind::Object];
    assert_eq!(list_kinds(None).await, both);
    assert_eq!(
        list_kinds(Some(name_type("0x2::kiosk::Item"))).await,
        vec![DynamicFieldKind::Object]
    );
    assert_eq!(
        list_kinds(Some(name_type("0x2::kiosk::Listing"))).await,
        vec![DynamicFieldKind::Field]
    );
    assert_eq!(list_kinds(Some(name_type("0x2::kiosk"))).await, both);
    assert_eq!(list_kinds(Some(name_type("0x2::coin::Coin"))).await, vec![]);

    let mut not_object_field = DynamicFieldLiteral::default();
    not_object_field.negated = true;
    not_object_field.predicate = Some(dynamic_field_literal::Predicate::DynamicObjectField(
        Default::default(),
    ));
    assert_eq!(
        list_kinds(Some(filter_of(vec![not_object_field]))).await,
        vec![DynamicFieldKind::Field]
    );
}

fn name_type(name_type: &str) -> DynamicFieldFilter {
    let mut name_type_filter = DynamicFieldNameTypeFilter::default();
    name_type_filter.name_type = Some(name_type.to_owned());
    let mut literal = DynamicFieldLiteral::default();
    literal.predicate = Some(dynamic_field_literal::Predicate::NameType(name_type_filter));
    filter_of(vec![literal])
}

fn filter_of(literals: Vec<DynamicFieldLiteral>) -> DynamicFieldFilter {
    let mut term = DynamicFieldTerm::default();
    term.literals = literals;
    let mut filter = DynamicFieldFilter::default();
    filter.terms = vec![term];
    filter
}
