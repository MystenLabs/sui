// Copyright (c) Facebook, Inc. and its affiliates.
// SPDX-License-Identifier: Apache-2.0

use super::*;

// handle_funding_transaction
#[test]
fn test_handle_funding_transaction_zero_amount() {
    let (mut contract_state, _name, _secret) = init_contract();
    let mut funding_transaction = init_funding_transaction();
    funding_transaction.primary_coins = Amount::zero();

    assert!(contract_state
        .handle_funding_transaction(funding_transaction)
        .is_err());
    assert_eq!(contract_state.total_balance, Amount::zero());
    assert_eq!(contract_state.last_transaction_index, VersionNumber::new());
    assert!(contract_state.blockchain.is_empty());
    assert!(contract_state.accounts.is_empty());
}

#[test]
fn test_handle_funding_transaction_ok() {
    let (mut contract_state, _name, _secret) = init_contract();
    let funding_transaction = init_funding_transaction();

    assert!(contract_state
        .handle_funding_transaction(funding_transaction.clone())
        .is_ok());
    assert_eq!(
        contract_state.total_balance,
        funding_transaction.primary_coins
    );
    let mut updated_last_transaction_index = VersionNumber::new();
    updated_last_transaction_index = updated_last_transaction_index.increment().unwrap();
    assert_eq!(
        contract_state.last_transaction_index,
        updated_last_transaction_index
    );
    assert_eq!(contract_state.blockchain.len(), 1);
    assert_eq!(contract_state.blockchain[0], funding_transaction);
    assert!(contract_state.accounts.is_empty());
}

// handle_redeem_transaction

#[test]
fn test_handle_redeem_transaction_ok() {
    let (mut contract_state, name, secret) = init_contract();
    let redeem_transaction =
        init_redeem_transaction(contract_state.committee.clone(), name, secret);
    let funding_transaction = init_funding_transaction();
    assert!(contract_state
        .handle_funding_transaction(funding_transaction)
        .is_ok());
    let mut old_total_balance = contract_state.total_balance;

    assert!(contract_state
        .handle_redeem_transaction(redeem_transaction.clone())
        .is_ok());
    let sender = redeem_transaction
        .transfer_certificate
        .value
        .transfer
        .sender;
    let amount = redeem_transaction
        .transfer_certificate
        .value
        .transfer
        .amount;
    let account = contract_state.accounts.get(&sender).unwrap();
    let sequence_number = redeem_transaction
        .transfer_certificate
        .value
        .transfer
        .sequence_number;
    assert_eq!(account.last_redeemed, Some(sequence_number));
    old_total_balance = old_total_balance.try_sub(amount).unwrap();
    assert_eq!(contract_state.total_balance, old_total_balance);
}

#[test]
fn test_handle_redeem_transaction_negative_balance() {
    let (mut contract_state, name, secret) = init_contract();
    let mut redeem_transaction =
        init_redeem_transaction(contract_state.committee.clone(), name, secret);
    let funding_transaction = init_funding_transaction();
    let too_much_money = Amount::from(1000);
    assert!(contract_state
        .handle_funding_transaction(funding_transaction)
        .is_ok());
    let old_balance = contract_state.total_balance;

    redeem_transaction
        .transfer_certificate
        .value
        .transfer
        .amount = redeem_transaction
        .transfer_certificate
        .value
        .transfer
        .amount
        .try_add(too_much_money)
        .unwrap();
    assert!(contract_state
        .handle_redeem_transaction(redeem_transaction)
        .is_err());
    assert_eq!(old_balance, contract_state.total_balance);
    assert!(contract_state.accounts.is_empty());
}

#[test]
fn test_handle_redeem_transaction_double_spend() {
    let (mut contract_state, name, secret) = init_contract();
    let redeem_transaction =
        init_redeem_transaction(contract_state.committee.clone(), name, secret);
    let funding_transaction = init_funding_transaction();
    assert!(contract_state
        .handle_funding_transaction(funding_transaction)
        .is_ok());
    assert!(contract_state
        .handle_redeem_transaction(redeem_transaction.clone())
        .is_ok());
    let old_balance = contract_state.total_balance;

    assert!(contract_state
        .handle_redeem_transaction(redeem_transaction)
        .is_err());
    assert_eq!(old_balance, contract_state.total_balance);
}

// helpers
#[cfg(test)]
fn init_contract() -> (FastPaySmartContractState, AuthorityName, KeyPair) {
    let (authority_address, authority_key) = get_key_pair();
    let mut authorities = BTreeMap::new();
    authorities.insert(
        /* address */ authority_address,
        /* voting right */ 1,
    );
    let committee = Committee::new(authorities);
    (
        FastPaySmartContractState::new(committee),
        authority_address,
        authority_key,
    )
}

fn init_funding_transaction() -> FundingTransaction {
    FundingTransaction {
        recipient: dbg_addr(1),
        primary_coins: Amount::from(5),
    }
}

#[cfg(test)]
fn init_redeem_transaction(
    committee: Committee,
    name: AuthorityName,
    secret: KeyPair,
) -> RedeemTransaction {
    let (sender_address, sender_key) = get_key_pair();
    let primary_transfer = Transfer {
        sender: sender_address,
        recipient: Address::Primary(dbg_addr(2)),
        amount: Amount::from(3),
        sequence_number: SequenceNumber::new(),
        user_data: UserData::default(),
    };
    let order = TransferOrder::new(primary_transfer, &sender_key);
    let vote = SignedTransferOrder::new(order.clone(), name, &secret);
    let mut builder = SignatureAggregator::try_new(order, &committee).unwrap();
    let certificate = builder
        .append(vote.authority, vote.signature)
        .unwrap()
        .unwrap();
    RedeemTransaction {
        transfer_certificate: certificate,
    }
}
