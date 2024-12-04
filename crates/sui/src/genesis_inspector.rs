// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use inquire::Select;
use std::collections::BTreeMap;
use sui_config::genesis::UnsignedGenesis;
use sui_types::sui_system_state::SuiValidatorGenesis;
use sui_types::{
    base_types::ObjectID,
    coin::CoinMetadata,
    gas_coin::{GasCoin, MIST_PER_SUI, TOTAL_SUPPLY_MIST},
    governance::StakedSui,
    move_package::MovePackage,
    object::{MoveObject, Owner},
};

const STR_ALL: &str = "All";
const STR_EXIT: &str = "Exit";
const STR_SUI: &str = "Sui";
const STR_STAKED_SUI: &str = "StakedSui";
const STR_PACKAGE: &str = "Package";
const STR_COIN_METADATA: &str = "CoinMetadata";
const STR_OTHER: &str = "Other";
const STR_SUI_DISTRIBUTION: &str = "Sui Distribution";
const STR_OBJECTS: &str = "Objects";
const STR_VALIDATORS: &str = "Validators";

#[allow(clippy::or_fun_call)]
pub(crate) fn examine_genesis_checkpoint(genesis: UnsignedGenesis) {
    let system_object = genesis
        .sui_system_object()
        .into_genesis_version_for_tooling();

    // Prepare Validator info
    let validator_set = &system_object.validators.active_validators;
    let validator_map = validator_set
        .iter()
        .map(|v| (v.verified_metadata().name.as_str(), v))
        .collect::<BTreeMap<_, _>>();
    let validator_pool_id_map = validator_set
        .iter()
        .map(|v| (v.staking_pool.id, v))
        .collect::<BTreeMap<_, _>>();

    let mut validator_options: Vec<_> = validator_map.keys().copied().collect();
    validator_options.extend_from_slice(&[STR_ALL, STR_EXIT]);
    println!("Total Number of Validators: {}", validator_set.len());

    // Prepare Sui distribution info
    let mut sui_distribution = BTreeMap::new();
    let entry = sui_distribution
        .entry("Sui System".to_string())
        .or_insert(BTreeMap::new());
    entry.insert(
        "Storage Fund".to_string(),
        (
            STR_SUI,
            system_object.storage_fund.non_refundable_balance.value(),
        ),
    );
    entry.insert(
        "Stake Subsidy".to_string(),
        (STR_SUI, system_object.stake_subsidy.balance.value()),
    );

    // Prepare Object Info
    let mut owner_map = BTreeMap::new();
    let mut package_map = BTreeMap::new();
    let mut sui_map = BTreeMap::new();
    let mut staked_sui_map = BTreeMap::new();
    let mut coin_metadata_map = BTreeMap::new();
    let mut other_object_map = BTreeMap::new();

    for object in genesis.objects() {
        let object_id = object.id();
        let object_id_str = object_id.to_string();
        assert_eq!(object.storage_rebate, 0);
        owner_map.insert(object.id(), object.owner.clone());

        match &object.data {
            sui_types::object::Data::Move(move_object) => {
                if let Ok(gas) = GasCoin::try_from(object) {
                    let entry = sui_distribution
                        .entry(object.owner.to_string())
                        .or_default();
                    entry.insert(object_id_str.clone(), (STR_SUI, gas.value()));
                    sui_map.insert(object.id(), gas);
                } else if let Ok(coin_metadata) = CoinMetadata::try_from(object) {
                    coin_metadata_map.insert(object.id(), coin_metadata);
                } else if let Ok(staked_sui) = StakedSui::try_from(object) {
                    let entry = sui_distribution
                        .entry(object.owner.to_string())
                        .or_default();
                    entry.insert(object_id_str, (STR_STAKED_SUI, staked_sui.principal()));
                    // Assert pool id is associated with a knonw validator.
                    let validator = validator_pool_id_map.get(&staked_sui.pool_id()).unwrap();
                    assert_eq!(validator.staking_pool.id, staked_sui.pool_id());

                    staked_sui_map.insert(object.id(), staked_sui);
                } else {
                    other_object_map.insert(object.id(), move_object);
                }
            }
            sui_types::object::Data::Package(p) => {
                package_map.insert(object.id(), p);
            }
        }
    }
    println!(
        "Total Number of Objects/Pacakges: {}",
        genesis.objects().len()
    );

    // Always check the Total Supply
    examine_total_supply(&sui_distribution, false);

    // Main loop for inspection
    let main_options: Vec<&str> = vec![STR_SUI_DISTRIBUTION, STR_VALIDATORS, STR_OBJECTS, STR_EXIT];
    loop {
        let ans = Select::new(
            "Select one main category to examine ('Exit' to exit the program):",
            main_options.clone(),
        )
        .prompt();
        match ans {
            Ok(name) if name == STR_SUI_DISTRIBUTION => {
                examine_total_supply(&sui_distribution, true)
            }
            Ok(name) if name == STR_VALIDATORS => {
                examine_validators(&validator_options, &validator_map);
            }
            Ok(name) if name == STR_OBJECTS => {
                println!("Examine Objects (total: {})", genesis.objects().len());
                examine_object(
                    &owner_map,
                    &validator_pool_id_map,
                    &package_map,
                    &sui_map,
                    &staked_sui_map,
                    &coin_metadata_map,
                    &other_object_map,
                );
            }
            Ok(name) if name == STR_EXIT => break,
            Ok(_) => (),
            Err(err) => {
                println!("Error: {err}");
                break;
            }
        }
    }
}

#[allow(clippy::ptr_arg)]
fn examine_validators(
    validator_options: &Vec<&str>,
    validator_map: &BTreeMap<&str, &SuiValidatorGenesis>,
) {
    loop {
        let ans = Select::new("Select one validator to examine ('All' to display all Validators, 'Exit' to return to Main):", validator_options.clone()).prompt();
        match ans {
            Ok(name) if name == STR_ALL => {
                for validator in validator_map.values() {
                    display_validator(validator);
                }
            }
            Ok(name) if name == STR_EXIT => break,
            Ok(name) => {
                let validator = validator_map.get(name).unwrap();
                display_validator(validator);
            }
            Err(err) => {
                println!("Error: {err}");
                break;
            }
        }
    }
    print_divider("Validator");
}

fn examine_object(
    owner_map: &BTreeMap<ObjectID, Owner>,
    validator_pool_id_map: &BTreeMap<ObjectID, &SuiValidatorGenesis>,
    package_map: &BTreeMap<ObjectID, &MovePackage>,
    sui_map: &BTreeMap<ObjectID, GasCoin>,
    staked_sui_map: &BTreeMap<ObjectID, StakedSui>,
    coin_metadata_map: &BTreeMap<ObjectID, CoinMetadata>,
    other_object_map: &BTreeMap<ObjectID, &MoveObject>,
) {
    let object_options: Vec<&str> = vec![
        STR_SUI,
        STR_STAKED_SUI,
        STR_COIN_METADATA,
        STR_PACKAGE,
        STR_OTHER,
        STR_EXIT,
    ];
    loop {
        let ans = Select::new(
            "Select one object category to examine ('Exit' to return to Main):",
            object_options.clone(),
        )
        .prompt();
        match ans {
            Ok(name) if name == STR_EXIT => break,
            Ok(name) if name == STR_SUI => {
                for gas_coin in sui_map.values() {
                    display_sui(gas_coin, owner_map);
                }
                print_divider("Sui");
            }
            Ok(name) if name == STR_STAKED_SUI => {
                for staked_sui_coin in staked_sui_map.values() {
                    display_staked_sui(staked_sui_coin, validator_pool_id_map, owner_map);
                }
                print_divider(STR_STAKED_SUI);
            }
            Ok(name) if name == STR_PACKAGE => {
                for package in package_map.values() {
                    println!("Package ID: {}", package.id());
                    println!("Version: {}", package.version());
                    println!("Modules: {:?}\n", package.serialized_module_map().keys());
                }
                print_divider("Package");
            }
            Ok(name) if name == STR_OTHER => {
                for other_obj in other_object_map.values() {
                    println!("{:#?}", other_obj.type_());
                    println!("{:?}", other_obj.version());
                    println!("Has Public Transfer: {}\n", other_obj.has_public_transfer());
                }
                print_divider("Other");
            }
            Ok(name) if name == STR_COIN_METADATA => {
                for coin_metadata in coin_metadata_map.values() {
                    println!("{:#?}\n", coin_metadata);
                }
                print_divider("CoinMetadata");
            }
            Ok(_) => (),
            Err(err) => {
                println!("Error: {err}");
                break;
            }
        }
    }
    print_divider("Object");
}

fn examine_total_supply(
    sui_distribution: &BTreeMap<String, BTreeMap<String, (&str, u64)>>,
    print: bool,
) {
    let mut total_sui = 0;
    let mut total_staked_sui = 0;
    for (owner, coins) in sui_distribution {
        let mut amount_sum = 0;
        for (owner, value) in coins.values() {
            amount_sum += value;
            if *owner == STR_STAKED_SUI {
                total_staked_sui += value;
            }
        }
        total_sui += amount_sum;
        if print {
            println!("Owner {:?}", owner);
            println!(
                "Total Amount of Sui/StakedSui Owned: {amount_sum} MIST or {} SUI:",
                amount_sum / MIST_PER_SUI
            );
            println!("{:#?}\n", coins);
        }
    }
    assert_eq!(total_sui, TOTAL_SUPPLY_MIST);
    // Always print this.
    println!(
        "Total Supply of Sui: {total_sui} MIST or {} SUI",
        total_sui / MIST_PER_SUI
    );
    println!(
        "Total Amount of StakedSui: {total_staked_sui} MIST or {} SUI\n",
        total_staked_sui / MIST_PER_SUI
    );
    if print {
        print_divider("Sui Distribution");
    }
}

fn display_validator(validator: &SuiValidatorGenesis) {
    let metadata = validator.verified_metadata();
    println!("Validator name: {}", metadata.name);
    println!("{:#?}", metadata);
    println!("Voting Power: {}", validator.voting_power);
    println!("Gas Price: {}", validator.gas_price);
    println!("Next Epoch Gas Price: {}", validator.next_epoch_gas_price);
    println!("Commission Rate: {}", validator.commission_rate);
    println!(
        "Next Epoch Commission Rate: {}",
        validator.next_epoch_commission_rate
    );
    println!("Next Epoch Stake: {}", validator.next_epoch_stake);
    println!("Staking Pool ID: {}", validator.staking_pool.id);
    println!(
        "Staking Pool Activation Epoch: {:?}",
        validator.staking_pool.activation_epoch
    );
    println!(
        "Staking Pool Deactivation Epoch: {:?}",
        validator.staking_pool.deactivation_epoch
    );
    println!(
        "Staking Pool Sui Balance: {:?}",
        validator.staking_pool.sui_balance
    );
    println!(
        "Rewards Pool: {}",
        validator.staking_pool.rewards_pool.value()
    );
    println!(
        "Pool Token Balance: {}",
        validator.staking_pool.pool_token_balance
    );
    println!(
        "Pending Delegation: {}",
        validator.staking_pool.pending_stake
    );
    println!(
        "Pending Total Sui Withdraw: {}",
        validator.staking_pool.pending_total_sui_withdraw
    );
    println!(
        "Pendign Pool Token Withdraw: {}",
        validator.staking_pool.pending_pool_token_withdraw
    );
    println!(
        "Exchange Rates ID: {}",
        validator.staking_pool.exchange_rates.id
    );
    println!(
        "Exchange Rates Size: {}",
        validator.staking_pool.exchange_rates.size
    );
    print_divider(&metadata.name);
}

fn display_sui(gas_coin: &GasCoin, owner_map: &BTreeMap<ObjectID, Owner>) {
    println!("ID: {}", gas_coin.id());
    println!("Balance: {}", gas_coin.value());
    println!("Owner: {}\n", owner_map.get(gas_coin.id()).unwrap());
}

fn display_staked_sui(
    staked_sui: &StakedSui,
    validator_pool_id_map: &BTreeMap<ObjectID, &SuiValidatorGenesis>,
    owner_map: &BTreeMap<ObjectID, Owner>,
) {
    let validator = validator_pool_id_map.get(&staked_sui.pool_id()).unwrap();
    println!("{:#?}", staked_sui);
    println!(
        "Staked to Validator: {}",
        validator.verified_metadata().name
    );
    println!("Owner: {}\n", owner_map.get(&staked_sui.id()).unwrap());
}

fn print_divider(title: &str) {
    let title = format!("End of {title}");
    let divider_length = 80;
    let left_divider_length = 10;
    assert!(title.len() <= divider_length - left_divider_length * 2);
    let divider_op = "-";
    let divider = divider_op.repeat(divider_length);
    let left_divider = divider_op.repeat(left_divider_length);
    let margin_length = (divider_length - left_divider_length * 2 - title.len()) / 2;
    let margin = " ".repeat(margin_length);
    println!();
    println!("{divider}");
    println!("{left_divider}{margin}{title}{margin}{left_divider}");
    println!("{divider}");
    println!();
}
