---
title: Module `sui_system::sui_system_state_inner`
---



-  [Struct `SystemParameters`](#sui_system_sui_system_state_inner_SystemParameters)
-  [Struct `SystemParametersV2`](#sui_system_sui_system_state_inner_SystemParametersV2)
-  [Struct `SuiSystemStateInner`](#sui_system_sui_system_state_inner_SuiSystemStateInner)
-  [Struct `SuiSystemStateInnerV2`](#sui_system_sui_system_state_inner_SuiSystemStateInnerV2)
-  [Struct `SystemEpochInfoEvent`](#sui_system_sui_system_state_inner_SystemEpochInfoEvent)
-  [Constants](#@Constants_0)
-  [Function `create`](#sui_system_sui_system_state_inner_create)
-  [Function `create_system_parameters`](#sui_system_sui_system_state_inner_create_system_parameters)
-  [Function `v1_to_v2`](#sui_system_sui_system_state_inner_v1_to_v2)
-  [Function `request_add_validator_candidate`](#sui_system_sui_system_state_inner_request_add_validator_candidate)
-  [Function `request_remove_validator_candidate`](#sui_system_sui_system_state_inner_request_remove_validator_candidate)
-  [Function `request_add_validator`](#sui_system_sui_system_state_inner_request_add_validator)
-  [Function `request_remove_validator`](#sui_system_sui_system_state_inner_request_remove_validator)
-  [Function `request_set_gas_price`](#sui_system_sui_system_state_inner_request_set_gas_price)
-  [Function `set_candidate_validator_gas_price`](#sui_system_sui_system_state_inner_set_candidate_validator_gas_price)
-  [Function `request_set_commission_rate`](#sui_system_sui_system_state_inner_request_set_commission_rate)
-  [Function `set_candidate_validator_commission_rate`](#sui_system_sui_system_state_inner_set_candidate_validator_commission_rate)
-  [Function `request_add_stake`](#sui_system_sui_system_state_inner_request_add_stake)
-  [Function `request_add_stake_mul_coin`](#sui_system_sui_system_state_inner_request_add_stake_mul_coin)
-  [Function `request_withdraw_stake`](#sui_system_sui_system_state_inner_request_withdraw_stake)
-  [Function `convert_to_fungible_staked_sui`](#sui_system_sui_system_state_inner_convert_to_fungible_staked_sui)
-  [Function `redeem_fungible_staked_sui`](#sui_system_sui_system_state_inner_redeem_fungible_staked_sui)
-  [Function `report_validator`](#sui_system_sui_system_state_inner_report_validator)
-  [Function `undo_report_validator`](#sui_system_sui_system_state_inner_undo_report_validator)
-  [Function `report_validator_impl`](#sui_system_sui_system_state_inner_report_validator_impl)
-  [Function `undo_report_validator_impl`](#sui_system_sui_system_state_inner_undo_report_validator_impl)
-  [Function `rotate_operation_cap`](#sui_system_sui_system_state_inner_rotate_operation_cap)
-  [Function `update_validator_name`](#sui_system_sui_system_state_inner_update_validator_name)
-  [Function `update_validator_description`](#sui_system_sui_system_state_inner_update_validator_description)
-  [Function `update_validator_image_url`](#sui_system_sui_system_state_inner_update_validator_image_url)
-  [Function `update_validator_project_url`](#sui_system_sui_system_state_inner_update_validator_project_url)
-  [Function `update_validator_next_epoch_network_address`](#sui_system_sui_system_state_inner_update_validator_next_epoch_network_address)
-  [Function `update_candidate_validator_network_address`](#sui_system_sui_system_state_inner_update_candidate_validator_network_address)
-  [Function `update_validator_next_epoch_p2p_address`](#sui_system_sui_system_state_inner_update_validator_next_epoch_p2p_address)
-  [Function `update_candidate_validator_p2p_address`](#sui_system_sui_system_state_inner_update_candidate_validator_p2p_address)
-  [Function `update_validator_next_epoch_primary_address`](#sui_system_sui_system_state_inner_update_validator_next_epoch_primary_address)
-  [Function `update_candidate_validator_primary_address`](#sui_system_sui_system_state_inner_update_candidate_validator_primary_address)
-  [Function `update_validator_next_epoch_worker_address`](#sui_system_sui_system_state_inner_update_validator_next_epoch_worker_address)
-  [Function `update_candidate_validator_worker_address`](#sui_system_sui_system_state_inner_update_candidate_validator_worker_address)
-  [Function `update_validator_next_epoch_protocol_pubkey`](#sui_system_sui_system_state_inner_update_validator_next_epoch_protocol_pubkey)
-  [Function `update_candidate_validator_protocol_pubkey`](#sui_system_sui_system_state_inner_update_candidate_validator_protocol_pubkey)
-  [Function `update_validator_next_epoch_worker_pubkey`](#sui_system_sui_system_state_inner_update_validator_next_epoch_worker_pubkey)
-  [Function `update_candidate_validator_worker_pubkey`](#sui_system_sui_system_state_inner_update_candidate_validator_worker_pubkey)
-  [Function `update_validator_next_epoch_network_pubkey`](#sui_system_sui_system_state_inner_update_validator_next_epoch_network_pubkey)
-  [Function `update_candidate_validator_network_pubkey`](#sui_system_sui_system_state_inner_update_candidate_validator_network_pubkey)
-  [Function `advance_epoch`](#sui_system_sui_system_state_inner_advance_epoch)
-  [Function `epoch`](#sui_system_sui_system_state_inner_epoch)
-  [Function `protocol_version`](#sui_system_sui_system_state_inner_protocol_version)
-  [Function `system_state_version`](#sui_system_sui_system_state_inner_system_state_version)
-  [Function `genesis_system_state_version`](#sui_system_sui_system_state_inner_genesis_system_state_version)
-  [Function `epoch_start_timestamp_ms`](#sui_system_sui_system_state_inner_epoch_start_timestamp_ms)
-  [Function `validator_stake_amount`](#sui_system_sui_system_state_inner_validator_stake_amount)
-  [Function `active_validator_voting_powers`](#sui_system_sui_system_state_inner_active_validator_voting_powers)
-  [Function `validator_staking_pool_id`](#sui_system_sui_system_state_inner_validator_staking_pool_id)
-  [Function `validator_staking_pool_mappings`](#sui_system_sui_system_state_inner_validator_staking_pool_mappings)
-  [Function `get_reporters_of`](#sui_system_sui_system_state_inner_get_reporters_of)
-  [Function `get_storage_fund_total_balance`](#sui_system_sui_system_state_inner_get_storage_fund_total_balance)
-  [Function `get_storage_fund_object_rebates`](#sui_system_sui_system_state_inner_get_storage_fund_object_rebates)
-  [Function `validator_address_by_pool_id`](#sui_system_sui_system_state_inner_validator_address_by_pool_id)
-  [Function `pool_exchange_rates`](#sui_system_sui_system_state_inner_pool_exchange_rates)
-  [Function `active_validator_addresses`](#sui_system_sui_system_state_inner_active_validator_addresses)
-  [Function `extract_coin_balance`](#sui_system_sui_system_state_inner_extract_coin_balance)


<pre><code><b>use</b> <a href="../std/address.md#std_address">std::address</a>;
<b>use</b> <a href="../std/ascii.md#std_ascii">std::ascii</a>;
<b>use</b> <a href="../std/bcs.md#std_bcs">std::bcs</a>;
<b>use</b> <a href="../std/option.md#std_option">std::option</a>;
<b>use</b> <a href="../std/string.md#std_string">std::string</a>;
<b>use</b> <a href="../std/type_name.md#std_type_name">std::type_name</a>;
<b>use</b> <a href="../std/u64.md#std_u64">std::u64</a>;
<b>use</b> <a href="../std/vector.md#std_vector">std::vector</a>;
<b>use</b> <a href="../sui/address.md#sui_address">sui::address</a>;
<b>use</b> <a href="../sui/bag.md#sui_bag">sui::bag</a>;
<b>use</b> <a href="../sui/balance.md#sui_balance">sui::balance</a>;
<b>use</b> <a href="../sui/coin.md#sui_coin">sui::coin</a>;
<b>use</b> <a href="../sui/config.md#sui_config">sui::config</a>;
<b>use</b> <a href="../sui/deny_list.md#sui_deny_list">sui::deny_list</a>;
<b>use</b> <a href="../sui/dynamic_field.md#sui_dynamic_field">sui::dynamic_field</a>;
<b>use</b> <a href="../sui/dynamic_object_field.md#sui_dynamic_object_field">sui::dynamic_object_field</a>;
<b>use</b> <a href="../sui/event.md#sui_event">sui::event</a>;
<b>use</b> <a href="../sui/hex.md#sui_hex">sui::hex</a>;
<b>use</b> <a href="../sui/object.md#sui_object">sui::object</a>;
<b>use</b> <a href="../sui/pay.md#sui_pay">sui::pay</a>;
<b>use</b> <a href="../sui/priority_queue.md#sui_priority_queue">sui::priority_queue</a>;
<b>use</b> <a href="../sui/sui.md#sui_sui">sui::sui</a>;
<b>use</b> <a href="../sui/table.md#sui_table">sui::table</a>;
<b>use</b> <a href="../sui/table_vec.md#sui_table_vec">sui::table_vec</a>;
<b>use</b> <a href="../sui/transfer.md#sui_transfer">sui::transfer</a>;
<b>use</b> <a href="../sui/tx_context.md#sui_tx_context">sui::tx_context</a>;
<b>use</b> <a href="../sui/types.md#sui_types">sui::types</a>;
<b>use</b> <a href="../sui/url.md#sui_url">sui::url</a>;
<b>use</b> <a href="../sui/vec_map.md#sui_vec_map">sui::vec_map</a>;
<b>use</b> <a href="../sui/vec_set.md#sui_vec_set">sui::vec_set</a>;
<b>use</b> <a href="../sui/versioned.md#sui_versioned">sui::versioned</a>;
<b>use</b> <a href="../sui_system/stake_subsidy.md#sui_system_stake_subsidy">sui_system::stake_subsidy</a>;
<b>use</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool">sui_system::staking_pool</a>;
<b>use</b> <a href="../sui_system/storage_fund.md#sui_system_storage_fund">sui_system::storage_fund</a>;
<b>use</b> <a href="../sui_system/validator.md#sui_system_validator">sui_system::validator</a>;
<b>use</b> <a href="../sui_system/validator_cap.md#sui_system_validator_cap">sui_system::validator_cap</a>;
<b>use</b> <a href="../sui_system/validator_set.md#sui_system_validator_set">sui_system::validator_set</a>;
<b>use</b> <a href="../sui_system/validator_wrapper.md#sui_system_validator_wrapper">sui_system::validator_wrapper</a>;
<b>use</b> <a href="../sui_system/voting_power.md#sui_system_voting_power">sui_system::voting_power</a>;
</code></pre>



<a name="sui_system_sui_system_state_inner_SystemParameters"></a>

## Struct `SystemParameters`

A list of system config parameters.


<pre><code><b>public</b> <b>struct</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SystemParameters">SystemParameters</a> <b>has</b> store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>epoch_duration_ms: u64</code>
</dt>
<dd>
 The duration of an epoch, in milliseconds.
</dd>
<dt>
<code>stake_subsidy_start_epoch: u64</code>
</dt>
<dd>
 The starting epoch in which stake subsidies start being paid out
</dd>
<dt>
<code>max_validator_count: u64</code>
</dt>
<dd>
 Maximum number of active validators at any moment.
 We do not allow the number of validators in any epoch to go above this.
</dd>
<dt>
<code>min_validator_joining_stake: u64</code>
</dt>
<dd>
 Lower-bound on the amount of stake required to become a validator.
</dd>
<dt>
<code>validator_low_stake_threshold: u64</code>
</dt>
<dd>
 Validators with stake amount below <code>validator_low_stake_threshold</code> are considered to
 have low stake and will be escorted out of the validator set after being below this
 threshold for more than <code>validator_low_stake_grace_period</code> number of epochs.
</dd>
<dt>
<code>validator_very_low_stake_threshold: u64</code>
</dt>
<dd>
 Validators with stake below <code>validator_very_low_stake_threshold</code> will be removed
 immediately at epoch change, no grace period.
</dd>
<dt>
<code>validator_low_stake_grace_period: u64</code>
</dt>
<dd>
 A validator can have stake below <code>validator_low_stake_threshold</code>
 for this many epochs before being kicked out.
</dd>
<dt>
<code>extra_fields: <a href="../sui/bag.md#sui_bag_Bag">sui::bag::Bag</a></code>
</dt>
<dd>
 Any extra fields that's not defined statically.
</dd>
</dl>


</details>

<a name="sui_system_sui_system_state_inner_SystemParametersV2"></a>

## Struct `SystemParametersV2`

Added min_validator_count.


<pre><code><b>public</b> <b>struct</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SystemParametersV2">SystemParametersV2</a> <b>has</b> store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>epoch_duration_ms: u64</code>
</dt>
<dd>
 The duration of an epoch, in milliseconds.
</dd>
<dt>
<code>stake_subsidy_start_epoch: u64</code>
</dt>
<dd>
 The starting epoch in which stake subsidies start being paid out
</dd>
<dt>
<code>min_validator_count: u64</code>
</dt>
<dd>
 Minimum number of active validators at any moment.
</dd>
<dt>
<code>max_validator_count: u64</code>
</dt>
<dd>
 Maximum number of active validators at any moment.
 We do not allow the number of validators in any epoch to go above this.
</dd>
<dt>
<code>min_validator_joining_stake: u64</code>
</dt>
<dd>
 Lower-bound on the amount of stake required to become a validator.
</dd>
<dt>
<code>validator_low_stake_threshold: u64</code>
</dt>
<dd>
 Validators with stake amount below <code>validator_low_stake_threshold</code> are considered to
 have low stake and will be escorted out of the validator set after being below this
 threshold for more than <code>validator_low_stake_grace_period</code> number of epochs.
</dd>
<dt>
<code>validator_very_low_stake_threshold: u64</code>
</dt>
<dd>
 Validators with stake below <code>validator_very_low_stake_threshold</code> will be removed
 immediately at epoch change, no grace period.
</dd>
<dt>
<code>validator_low_stake_grace_period: u64</code>
</dt>
<dd>
 A validator can have stake below <code>validator_low_stake_threshold</code>
 for this many epochs before being kicked out.
</dd>
<dt>
<code>extra_fields: <a href="../sui/bag.md#sui_bag_Bag">sui::bag::Bag</a></code>
</dt>
<dd>
 Any extra fields that's not defined statically.
</dd>
</dl>


</details>

<a name="sui_system_sui_system_state_inner_SuiSystemStateInner"></a>

## Struct `SuiSystemStateInner`

The top-level object containing all information of the Sui system.


<pre><code><b>public</b> <b>struct</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInner">SuiSystemStateInner</a> <b>has</b> store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code><a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_epoch">epoch</a>: u64</code>
</dt>
<dd>
 The current epoch ID, starting from 0.
</dd>
<dt>
<code><a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_protocol_version">protocol_version</a>: u64</code>
</dt>
<dd>
 The current protocol version, starting from 1.
</dd>
<dt>
<code><a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_system_state_version">system_state_version</a>: u64</code>
</dt>
<dd>
 The current version of the system state data structure type.
 This is always the same as SuiSystemState.version. Keeping a copy here so that
 we know what version it is by inspecting SuiSystemStateInner as well.
</dd>
<dt>
<code>validators: <a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">sui_system::validator_set::ValidatorSet</a></code>
</dt>
<dd>
 Contains all information about the validators.
</dd>
<dt>
<code><a href="../sui_system/storage_fund.md#sui_system_storage_fund">storage_fund</a>: <a href="../sui_system/storage_fund.md#sui_system_storage_fund_StorageFund">sui_system::storage_fund::StorageFund</a></code>
</dt>
<dd>
 The storage fund.
</dd>
<dt>
<code>parameters: <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SystemParameters">sui_system::sui_system_state_inner::SystemParameters</a></code>
</dt>
<dd>
 A list of system config parameters.
</dd>
<dt>
<code>reference_gas_price: u64</code>
</dt>
<dd>
 The reference gas price for the current epoch.
</dd>
<dt>
<code>validator_report_records: <a href="../sui/vec_map.md#sui_vec_map_VecMap">sui::vec_map::VecMap</a>&lt;<b>address</b>, <a href="../sui/vec_set.md#sui_vec_set_VecSet">sui::vec_set::VecSet</a>&lt;<b>address</b>&gt;&gt;</code>
</dt>
<dd>
 A map storing the records of validator reporting each other.
 There is an entry in the map for each validator that has been reported
 at least once. The entry VecSet contains all the validators that reported
 them. If a validator has never been reported they don't have an entry in this map.
 This map persists across epoch: a peer continues being in a reported state until the
 reporter doesn't explicitly remove their report.
 Note that in case we want to support validator address change in future,
 the reports should be based on validator ids
</dd>
<dt>
<code><a href="../sui_system/stake_subsidy.md#sui_system_stake_subsidy">stake_subsidy</a>: <a href="../sui_system/stake_subsidy.md#sui_system_stake_subsidy_StakeSubsidy">sui_system::stake_subsidy::StakeSubsidy</a></code>
</dt>
<dd>
 Schedule of stake subsidies given out each epoch.
</dd>
<dt>
<code>safe_mode: bool</code>
</dt>
<dd>
 Whether the system is running in a downgraded safe mode due to a non-recoverable bug.
 This is set whenever we failed to execute advance_epoch, and ended up executing advance_epoch_safe_mode.
 It can be reset once we are able to successfully execute advance_epoch.
 The rest of the fields starting with <code>safe_mode_</code> are accumulated during safe mode
 when advance_epoch_safe_mode is executed. They will eventually be processed once we
 are out of safe mode.
</dd>
<dt>
<code>safe_mode_storage_rewards: <a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;<a href="../sui/sui.md#sui_sui_SUI">sui::sui::SUI</a>&gt;</code>
</dt>
<dd>
</dd>
<dt>
<code>safe_mode_computation_rewards: <a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;<a href="../sui/sui.md#sui_sui_SUI">sui::sui::SUI</a>&gt;</code>
</dt>
<dd>
</dd>
<dt>
<code>safe_mode_storage_rebates: u64</code>
</dt>
<dd>
</dd>
<dt>
<code>safe_mode_non_refundable_storage_fee: u64</code>
</dt>
<dd>
</dd>
<dt>
<code><a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_epoch_start_timestamp_ms">epoch_start_timestamp_ms</a>: u64</code>
</dt>
<dd>
 Unix timestamp of the current epoch start
</dd>
<dt>
<code>extra_fields: <a href="../sui/bag.md#sui_bag_Bag">sui::bag::Bag</a></code>
</dt>
<dd>
 Any extra fields that's not defined statically.
</dd>
</dl>


</details>

<a name="sui_system_sui_system_state_inner_SuiSystemStateInnerV2"></a>

## Struct `SuiSystemStateInnerV2`

Uses SystemParametersV2 as the parameters.


<pre><code><b>public</b> <b>struct</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a> <b>has</b> store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code><a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_epoch">epoch</a>: u64</code>
</dt>
<dd>
 The current epoch ID, starting from 0.
</dd>
<dt>
<code><a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_protocol_version">protocol_version</a>: u64</code>
</dt>
<dd>
 The current protocol version, starting from 1.
</dd>
<dt>
<code><a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_system_state_version">system_state_version</a>: u64</code>
</dt>
<dd>
 The current version of the system state data structure type.
 This is always the same as SuiSystemState.version. Keeping a copy here so that
 we know what version it is by inspecting SuiSystemStateInner as well.
</dd>
<dt>
<code>validators: <a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">sui_system::validator_set::ValidatorSet</a></code>
</dt>
<dd>
 Contains all information about the validators.
</dd>
<dt>
<code><a href="../sui_system/storage_fund.md#sui_system_storage_fund">storage_fund</a>: <a href="../sui_system/storage_fund.md#sui_system_storage_fund_StorageFund">sui_system::storage_fund::StorageFund</a></code>
</dt>
<dd>
 The storage fund.
</dd>
<dt>
<code>parameters: <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SystemParametersV2">sui_system::sui_system_state_inner::SystemParametersV2</a></code>
</dt>
<dd>
 A list of system config parameters.
</dd>
<dt>
<code>reference_gas_price: u64</code>
</dt>
<dd>
 The reference gas price for the current epoch.
</dd>
<dt>
<code>validator_report_records: <a href="../sui/vec_map.md#sui_vec_map_VecMap">sui::vec_map::VecMap</a>&lt;<b>address</b>, <a href="../sui/vec_set.md#sui_vec_set_VecSet">sui::vec_set::VecSet</a>&lt;<b>address</b>&gt;&gt;</code>
</dt>
<dd>
 A map storing the records of validator reporting each other.
 There is an entry in the map for each validator that has been reported
 at least once. The entry VecSet contains all the validators that reported
 them. If a validator has never been reported they don't have an entry in this map.
 This map persists across epoch: a peer continues being in a reported state until the
 reporter doesn't explicitly remove their report.
 Note that in case we want to support validator address change in future,
 the reports should be based on validator ids
</dd>
<dt>
<code><a href="../sui_system/stake_subsidy.md#sui_system_stake_subsidy">stake_subsidy</a>: <a href="../sui_system/stake_subsidy.md#sui_system_stake_subsidy_StakeSubsidy">sui_system::stake_subsidy::StakeSubsidy</a></code>
</dt>
<dd>
 Schedule of stake subsidies given out each epoch.
</dd>
<dt>
<code>safe_mode: bool</code>
</dt>
<dd>
 Whether the system is running in a downgraded safe mode due to a non-recoverable bug.
 This is set whenever we failed to execute advance_epoch, and ended up executing advance_epoch_safe_mode.
 It can be reset once we are able to successfully execute advance_epoch.
 The rest of the fields starting with <code>safe_mode_</code> are accumulated during safe mode
 when advance_epoch_safe_mode is executed. They will eventually be processed once we
 are out of safe mode.
</dd>
<dt>
<code>safe_mode_storage_rewards: <a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;<a href="../sui/sui.md#sui_sui_SUI">sui::sui::SUI</a>&gt;</code>
</dt>
<dd>
</dd>
<dt>
<code>safe_mode_computation_rewards: <a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;<a href="../sui/sui.md#sui_sui_SUI">sui::sui::SUI</a>&gt;</code>
</dt>
<dd>
</dd>
<dt>
<code>safe_mode_storage_rebates: u64</code>
</dt>
<dd>
</dd>
<dt>
<code>safe_mode_non_refundable_storage_fee: u64</code>
</dt>
<dd>
</dd>
<dt>
<code><a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_epoch_start_timestamp_ms">epoch_start_timestamp_ms</a>: u64</code>
</dt>
<dd>
 Unix timestamp of the current epoch start
</dd>
<dt>
<code>extra_fields: <a href="../sui/bag.md#sui_bag_Bag">sui::bag::Bag</a></code>
</dt>
<dd>
 Any extra fields that's not defined statically.
</dd>
</dl>


</details>

<a name="sui_system_sui_system_state_inner_SystemEpochInfoEvent"></a>

## Struct `SystemEpochInfoEvent`

Event containing system-level epoch information, emitted during
the epoch advancement transaction.


<pre><code><b>public</b> <b>struct</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SystemEpochInfoEvent">SystemEpochInfoEvent</a> <b>has</b> <b>copy</b>, drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code><a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_epoch">epoch</a>: u64</code>
</dt>
<dd>
</dd>
<dt>
<code><a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_protocol_version">protocol_version</a>: u64</code>
</dt>
<dd>
</dd>
<dt>
<code>reference_gas_price: u64</code>
</dt>
<dd>
</dd>
<dt>
<code>total_stake: u64</code>
</dt>
<dd>
</dd>
<dt>
<code>storage_fund_reinvestment: u64</code>
</dt>
<dd>
</dd>
<dt>
<code>storage_charge: u64</code>
</dt>
<dd>
</dd>
<dt>
<code>storage_rebate: u64</code>
</dt>
<dd>
</dd>
<dt>
<code>storage_fund_balance: u64</code>
</dt>
<dd>
</dd>
<dt>
<code>stake_subsidy_amount: u64</code>
</dt>
<dd>
</dd>
<dt>
<code>total_gas_fees: u64</code>
</dt>
<dd>
</dd>
<dt>
<code>total_stake_rewards_distributed: u64</code>
</dt>
<dd>
</dd>
<dt>
<code>leftover_storage_fund_inflow: u64</code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="sui_system_sui_system_state_inner_ACTIVE_OR_PENDING_VALIDATOR"></a>



<pre><code><b>const</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_ACTIVE_OR_PENDING_VALIDATOR">ACTIVE_OR_PENDING_VALIDATOR</a>: u8 = 2;
</code></pre>



<a name="sui_system_sui_system_state_inner_ACTIVE_VALIDATOR_ONLY"></a>



<pre><code><b>const</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_ACTIVE_VALIDATOR_ONLY">ACTIVE_VALIDATOR_ONLY</a>: u8 = 1;
</code></pre>



<a name="sui_system_sui_system_state_inner_ANY_VALIDATOR"></a>



<pre><code><b>const</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_ANY_VALIDATOR">ANY_VALIDATOR</a>: u8 = 3;
</code></pre>



<a name="sui_system_sui_system_state_inner_BASIS_POINT_DENOMINATOR"></a>



<pre><code><b>const</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_BASIS_POINT_DENOMINATOR">BASIS_POINT_DENOMINATOR</a>: u128 = 10000;
</code></pre>



<a name="sui_system_sui_system_state_inner_EAdvancedToWrongEpoch"></a>



<pre><code><b>const</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_EAdvancedToWrongEpoch">EAdvancedToWrongEpoch</a>: u64 = 8;
</code></pre>



<a name="sui_system_sui_system_state_inner_EBpsTooLarge"></a>



<pre><code><b>const</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_EBpsTooLarge">EBpsTooLarge</a>: u64 = 5;
</code></pre>



<a name="sui_system_sui_system_state_inner_ECannotReportOneself"></a>



<pre><code><b>const</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_ECannotReportOneself">ECannotReportOneself</a>: u64 = 3;
</code></pre>



<a name="sui_system_sui_system_state_inner_ELimitExceeded"></a>



<pre><code><b>const</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_ELimitExceeded">ELimitExceeded</a>: u64 = 1;
</code></pre>



<a name="sui_system_sui_system_state_inner_ENotSystemAddress"></a>



<pre><code><b>const</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_ENotSystemAddress">ENotSystemAddress</a>: u64 = 2;
</code></pre>



<a name="sui_system_sui_system_state_inner_ENotValidator"></a>



<pre><code><b>const</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_ENotValidator">ENotValidator</a>: u64 = 0;
</code></pre>



<a name="sui_system_sui_system_state_inner_EReportRecordNotFound"></a>



<pre><code><b>const</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_EReportRecordNotFound">EReportRecordNotFound</a>: u64 = 4;
</code></pre>



<a name="sui_system_sui_system_state_inner_ESafeModeGasNotProcessed"></a>



<pre><code><b>const</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_ESafeModeGasNotProcessed">ESafeModeGasNotProcessed</a>: u64 = 7;
</code></pre>



<a name="sui_system_sui_system_state_inner_SYSTEM_STATE_VERSION_V1"></a>



<pre><code><b>const</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SYSTEM_STATE_VERSION_V1">SYSTEM_STATE_VERSION_V1</a>: u64 = 1;
</code></pre>



<a name="sui_system_sui_system_state_inner_create"></a>

## Function `create`

Create a new SuiSystemState object and make it shared.
This function will be called only once in genesis.


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_create">create</a>(validators: vector&lt;<a href="../sui_system/validator.md#sui_system_validator_Validator">sui_system::validator::Validator</a>&gt;, initial_storage_fund: <a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;<a href="../sui/sui.md#sui_sui_SUI">sui::sui::SUI</a>&gt;, <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_protocol_version">protocol_version</a>: u64, <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_epoch_start_timestamp_ms">epoch_start_timestamp_ms</a>: u64, parameters: <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SystemParameters">sui_system::sui_system_state_inner::SystemParameters</a>, <a href="../sui_system/stake_subsidy.md#sui_system_stake_subsidy">stake_subsidy</a>: <a href="../sui_system/stake_subsidy.md#sui_system_stake_subsidy_StakeSubsidy">sui_system::stake_subsidy::StakeSubsidy</a>, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInner">sui_system::sui_system_state_inner::SuiSystemStateInner</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_create">create</a>(
    validators: vector&lt;Validator&gt;,
    initial_storage_fund: Balance&lt;SUI&gt;,
    <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_protocol_version">protocol_version</a>: u64,
    <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_epoch_start_timestamp_ms">epoch_start_timestamp_ms</a>: u64,
    parameters: <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SystemParameters">SystemParameters</a>,
    <a href="../sui_system/stake_subsidy.md#sui_system_stake_subsidy">stake_subsidy</a>: StakeSubsidy,
    ctx: &<b>mut</b> TxContext,
): <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInner">SuiSystemStateInner</a> {
    <b>let</b> validators = <a href="../sui_system/validator_set.md#sui_system_validator_set_new">validator_set::new</a>(validators, ctx);
    <b>let</b> reference_gas_price = validators.derive_reference_gas_price();
    // This type is fixed <b>as</b> it's created at <a href="../sui_system/genesis.md#sui_system_genesis">genesis</a>. It should not be updated during type upgrade.
    <b>let</b> system_state = <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInner">SuiSystemStateInner</a> {
        <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_epoch">epoch</a>: 0,
        <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_protocol_version">protocol_version</a>,
        <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_system_state_version">system_state_version</a>: <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_genesis_system_state_version">genesis_system_state_version</a>(),
        validators,
        <a href="../sui_system/storage_fund.md#sui_system_storage_fund">storage_fund</a>: <a href="../sui_system/storage_fund.md#sui_system_storage_fund_new">storage_fund::new</a>(initial_storage_fund),
        parameters,
        reference_gas_price,
        validator_report_records: vec_map::empty(),
        <a href="../sui_system/stake_subsidy.md#sui_system_stake_subsidy">stake_subsidy</a>,
        safe_mode: <b>false</b>,
        safe_mode_storage_rewards: balance::zero(),
        safe_mode_computation_rewards: balance::zero(),
        safe_mode_storage_rebates: 0,
        safe_mode_non_refundable_storage_fee: 0,
        <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_epoch_start_timestamp_ms">epoch_start_timestamp_ms</a>,
        extra_fields: bag::new(ctx),
    };
    system_state
}
</code></pre>



</details>

<a name="sui_system_sui_system_state_inner_create_system_parameters"></a>

## Function `create_system_parameters`



<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_create_system_parameters">create_system_parameters</a>(epoch_duration_ms: u64, stake_subsidy_start_epoch: u64, max_validator_count: u64, min_validator_joining_stake: u64, validator_low_stake_threshold: u64, validator_very_low_stake_threshold: u64, validator_low_stake_grace_period: u64, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SystemParameters">sui_system::sui_system_state_inner::SystemParameters</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_create_system_parameters">create_system_parameters</a>(
    epoch_duration_ms: u64,
    stake_subsidy_start_epoch: u64,
    // Validator committee parameters
    max_validator_count: u64,
    min_validator_joining_stake: u64,
    validator_low_stake_threshold: u64,
    validator_very_low_stake_threshold: u64,
    validator_low_stake_grace_period: u64,
    ctx: &<b>mut</b> TxContext,
): <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SystemParameters">SystemParameters</a> {
    <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SystemParameters">SystemParameters</a> {
        epoch_duration_ms,
        stake_subsidy_start_epoch,
        max_validator_count,
        min_validator_joining_stake,
        validator_low_stake_threshold,
        validator_very_low_stake_threshold,
        validator_low_stake_grace_period,
        extra_fields: bag::new(ctx),
    }
}
</code></pre>



</details>

<a name="sui_system_sui_system_state_inner_v1_to_v2"></a>

## Function `v1_to_v2`



<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_v1_to_v2">v1_to_v2</a>(self: <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInner">sui_system::sui_system_state_inner::SuiSystemStateInner</a>): <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">sui_system::sui_system_state_inner::SuiSystemStateInnerV2</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_v1_to_v2">v1_to_v2</a>(self: <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInner">SuiSystemStateInner</a>): <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a> {
    <b>let</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInner">SuiSystemStateInner</a> {
        <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_epoch">epoch</a>,
        <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_protocol_version">protocol_version</a>,
        <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_system_state_version">system_state_version</a>: _,
        validators,
        <a href="../sui_system/storage_fund.md#sui_system_storage_fund">storage_fund</a>,
        parameters,
        reference_gas_price,
        validator_report_records,
        <a href="../sui_system/stake_subsidy.md#sui_system_stake_subsidy">stake_subsidy</a>,
        safe_mode,
        safe_mode_storage_rewards,
        safe_mode_computation_rewards,
        safe_mode_storage_rebates,
        safe_mode_non_refundable_storage_fee,
        <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_epoch_start_timestamp_ms">epoch_start_timestamp_ms</a>,
        extra_fields: state_extra_fields,
    } = self;
    <b>let</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SystemParameters">SystemParameters</a> {
        epoch_duration_ms,
        stake_subsidy_start_epoch,
        max_validator_count,
        min_validator_joining_stake,
        validator_low_stake_threshold,
        validator_very_low_stake_threshold,
        validator_low_stake_grace_period,
        extra_fields: param_extra_fields,
    } = parameters;
    <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a> {
        <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_epoch">epoch</a>,
        <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_protocol_version">protocol_version</a>,
        <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_system_state_version">system_state_version</a>: 2,
        validators,
        <a href="../sui_system/storage_fund.md#sui_system_storage_fund">storage_fund</a>,
        parameters: <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SystemParametersV2">SystemParametersV2</a> {
            epoch_duration_ms,
            stake_subsidy_start_epoch,
            min_validator_count: 4,
            max_validator_count,
            min_validator_joining_stake,
            validator_low_stake_threshold,
            validator_very_low_stake_threshold,
            validator_low_stake_grace_period,
            extra_fields: param_extra_fields,
        },
        reference_gas_price,
        validator_report_records,
        <a href="../sui_system/stake_subsidy.md#sui_system_stake_subsidy">stake_subsidy</a>,
        safe_mode,
        safe_mode_storage_rewards,
        safe_mode_computation_rewards,
        safe_mode_storage_rebates,
        safe_mode_non_refundable_storage_fee,
        <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_epoch_start_timestamp_ms">epoch_start_timestamp_ms</a>,
        extra_fields: state_extra_fields
    }
}
</code></pre>



</details>

<a name="sui_system_sui_system_state_inner_request_add_validator_candidate"></a>

## Function `request_add_validator_candidate`

Can be called by anyone who wishes to become a validator candidate and starts accruing delegated
stakes in their staking pool. Once they have at least <code>MIN_VALIDATOR_JOINING_STAKE</code> amount of stake they
can call <code><a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_request_add_validator">request_add_validator</a></code> to officially become an active validator at the next epoch.
Aborts if the caller is already a pending or active validator, or a validator candidate.
Note: <code>proof_of_possession</code> MUST be a valid signature using sui_address and protocol_pubkey_bytes.
To produce a valid PoP, run [fn test_proof_of_possession].


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_request_add_validator_candidate">request_add_validator_candidate</a>(self: &<b>mut</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">sui_system::sui_system_state_inner::SuiSystemStateInnerV2</a>, pubkey_bytes: vector&lt;u8&gt;, network_pubkey_bytes: vector&lt;u8&gt;, worker_pubkey_bytes: vector&lt;u8&gt;, proof_of_possession: vector&lt;u8&gt;, name: vector&lt;u8&gt;, description: vector&lt;u8&gt;, image_url: vector&lt;u8&gt;, project_url: vector&lt;u8&gt;, net_address: vector&lt;u8&gt;, p2p_address: vector&lt;u8&gt;, primary_address: vector&lt;u8&gt;, worker_address: vector&lt;u8&gt;, gas_price: u64, commission_rate: u64, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_request_add_validator_candidate">request_add_validator_candidate</a>(
    self: &<b>mut</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>,
    pubkey_bytes: vector&lt;u8&gt;,
    network_pubkey_bytes: vector&lt;u8&gt;,
    worker_pubkey_bytes: vector&lt;u8&gt;,
    proof_of_possession: vector&lt;u8&gt;,
    name: vector&lt;u8&gt;,
    description: vector&lt;u8&gt;,
    image_url: vector&lt;u8&gt;,
    project_url: vector&lt;u8&gt;,
    net_address: vector&lt;u8&gt;,
    p2p_address: vector&lt;u8&gt;,
    primary_address: vector&lt;u8&gt;,
    worker_address: vector&lt;u8&gt;,
    gas_price: u64,
    commission_rate: u64,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> <a href="../sui_system/validator.md#sui_system_validator">validator</a> = <a href="../sui_system/validator.md#sui_system_validator_new">validator::new</a>(
        ctx.sender(),
        pubkey_bytes,
        network_pubkey_bytes,
        worker_pubkey_bytes,
        proof_of_possession,
        name,
        description,
        image_url,
        project_url,
        net_address,
        p2p_address,
        primary_address,
        worker_address,
        gas_price,
        commission_rate,
        ctx
    );
    self.validators.<a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_request_add_validator_candidate">request_add_validator_candidate</a>(<a href="../sui_system/validator.md#sui_system_validator">validator</a>, ctx);
}
</code></pre>



</details>

<a name="sui_system_sui_system_state_inner_request_remove_validator_candidate"></a>

## Function `request_remove_validator_candidate`

Called by a validator candidate to remove themselves from the candidacy. After this call
their staking pool becomes deactivate.


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_request_remove_validator_candidate">request_remove_validator_candidate</a>(self: &<b>mut</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">sui_system::sui_system_state_inner::SuiSystemStateInnerV2</a>, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_request_remove_validator_candidate">request_remove_validator_candidate</a>(
    self: &<b>mut</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>,
    ctx: &<b>mut</b> TxContext,
) {
    self.validators.<a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_request_remove_validator_candidate">request_remove_validator_candidate</a>(ctx);
}
</code></pre>



</details>

<a name="sui_system_sui_system_state_inner_request_add_validator"></a>

## Function `request_add_validator`

Called by a validator candidate to add themselves to the active validator set beginning next epoch.
Aborts if the validator is a duplicate with one of the pending or active validators, or if the amount of
stake the validator has doesn't meet the min threshold, or if the number of new validators for the next
epoch has already reached the maximum.


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_request_add_validator">request_add_validator</a>(self: &<b>mut</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">sui_system::sui_system_state_inner::SuiSystemStateInnerV2</a>, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_request_add_validator">request_add_validator</a>(
    self: &<b>mut</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>,
    ctx: &TxContext,
) {
    <b>assert</b>!(
        self.validators.next_epoch_validator_count() &lt; self.parameters.max_validator_count,
        <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_ELimitExceeded">ELimitExceeded</a>,
    );
    self.validators.<a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_request_add_validator">request_add_validator</a>(self.parameters.min_validator_joining_stake, ctx);
}
</code></pre>



</details>

<a name="sui_system_sui_system_state_inner_request_remove_validator"></a>

## Function `request_remove_validator`

A validator can call this function to request a removal in the next epoch.
We use the sender of <code>ctx</code> to look up the validator
(i.e. sender must match the sui_address in the validator).
At the end of the epoch, the <code><a href="../sui_system/validator.md#sui_system_validator">validator</a></code> object will be returned to the sui_address
of the validator.


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_request_remove_validator">request_remove_validator</a>(self: &<b>mut</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">sui_system::sui_system_state_inner::SuiSystemStateInnerV2</a>, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_request_remove_validator">request_remove_validator</a>(
    self: &<b>mut</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>,
    ctx: &TxContext,
) {
    // Only check min <a href="../sui_system/validator.md#sui_system_validator">validator</a> condition <b>if</b> the current number of validators satisfy the constraint.
    // This is so that <b>if</b> we somehow already are in a state where we have less than min validators, it no longer matters
    // and is ok to stay so. This is useful <b>for</b> a test setup.
    <b>if</b> (self.validators.active_validators().length() &gt;= self.parameters.min_validator_count) {
        <b>assert</b>!(
            self.validators.next_epoch_validator_count() &gt; self.parameters.min_validator_count,
            <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_ELimitExceeded">ELimitExceeded</a>,
        );
    };
    self.validators.<a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_request_remove_validator">request_remove_validator</a>(ctx)
}
</code></pre>



</details>

<a name="sui_system_sui_system_state_inner_request_set_gas_price"></a>

## Function `request_set_gas_price`

A validator can call this function to submit a new gas price quote, to be
used for the reference gas price calculation at the end of the epoch.


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_request_set_gas_price">request_set_gas_price</a>(self: &<b>mut</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">sui_system::sui_system_state_inner::SuiSystemStateInnerV2</a>, cap: &<a href="../sui_system/validator_cap.md#sui_system_validator_cap_UnverifiedValidatorOperationCap">sui_system::validator_cap::UnverifiedValidatorOperationCap</a>, new_gas_price: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_request_set_gas_price">request_set_gas_price</a>(
    self: &<b>mut</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>,
    cap: &UnverifiedValidatorOperationCap,
    new_gas_price: u64,
) {
    // Verify the represented <b>address</b> is an active or pending <a href="../sui_system/validator.md#sui_system_validator">validator</a>, and the capability is still valid.
    <b>let</b> verified_cap = self.validators.verify_cap(cap, <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_ACTIVE_OR_PENDING_VALIDATOR">ACTIVE_OR_PENDING_VALIDATOR</a>);
    <b>let</b> <a href="../sui_system/validator.md#sui_system_validator">validator</a> = self.validators.get_validator_mut_with_verified_cap(&verified_cap, <b>false</b> /* include_candidate */);
    <a href="../sui_system/validator.md#sui_system_validator">validator</a>.<a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_request_set_gas_price">request_set_gas_price</a>(verified_cap, new_gas_price);
}
</code></pre>



</details>

<a name="sui_system_sui_system_state_inner_set_candidate_validator_gas_price"></a>

## Function `set_candidate_validator_gas_price`

This function is used to set new gas price for candidate validators


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_set_candidate_validator_gas_price">set_candidate_validator_gas_price</a>(self: &<b>mut</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">sui_system::sui_system_state_inner::SuiSystemStateInnerV2</a>, cap: &<a href="../sui_system/validator_cap.md#sui_system_validator_cap_UnverifiedValidatorOperationCap">sui_system::validator_cap::UnverifiedValidatorOperationCap</a>, new_gas_price: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_set_candidate_validator_gas_price">set_candidate_validator_gas_price</a>(
    self: &<b>mut</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>,
    cap: &UnverifiedValidatorOperationCap,
    new_gas_price: u64,
) {
    // Verify the represented <b>address</b> is an active or pending <a href="../sui_system/validator.md#sui_system_validator">validator</a>, and the capability is still valid.
    <b>let</b> verified_cap = self.validators.verify_cap(cap, <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_ANY_VALIDATOR">ANY_VALIDATOR</a>);
    <b>let</b> candidate = self.validators.get_validator_mut_with_verified_cap(&verified_cap, <b>true</b> /* include_candidate */);
    candidate.set_candidate_gas_price(verified_cap, new_gas_price)
}
</code></pre>



</details>

<a name="sui_system_sui_system_state_inner_request_set_commission_rate"></a>

## Function `request_set_commission_rate`

A validator can call this function to set a new commission rate, updated at the end of
the epoch.


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_request_set_commission_rate">request_set_commission_rate</a>(self: &<b>mut</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">sui_system::sui_system_state_inner::SuiSystemStateInnerV2</a>, new_commission_rate: u64, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_request_set_commission_rate">request_set_commission_rate</a>(
    self: &<b>mut</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>,
    new_commission_rate: u64,
    ctx: &TxContext,
) {
    self.validators.<a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_request_set_commission_rate">request_set_commission_rate</a>(
        new_commission_rate,
        ctx
    )
}
</code></pre>



</details>

<a name="sui_system_sui_system_state_inner_set_candidate_validator_commission_rate"></a>

## Function `set_candidate_validator_commission_rate`

This function is used to set new commission rate for candidate validators


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_set_candidate_validator_commission_rate">set_candidate_validator_commission_rate</a>(self: &<b>mut</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">sui_system::sui_system_state_inner::SuiSystemStateInnerV2</a>, new_commission_rate: u64, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_set_candidate_validator_commission_rate">set_candidate_validator_commission_rate</a>(
    self: &<b>mut</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>,
    new_commission_rate: u64,
    ctx: &TxContext,
) {
    <b>let</b> candidate = self.validators.get_validator_mut_with_ctx_including_candidates(ctx);
    candidate.set_candidate_commission_rate(new_commission_rate)
}
</code></pre>



</details>

<a name="sui_system_sui_system_state_inner_request_add_stake"></a>

## Function `request_add_stake`

Add stake to a validator's staking pool.


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_request_add_stake">request_add_stake</a>(self: &<b>mut</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">sui_system::sui_system_state_inner::SuiSystemStateInnerV2</a>, stake: <a href="../sui/coin.md#sui_coin_Coin">sui::coin::Coin</a>&lt;<a href="../sui/sui.md#sui_sui_SUI">sui::sui::SUI</a>&gt;, validator_address: <b>address</b>, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakedSui">sui_system::staking_pool::StakedSui</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_request_add_stake">request_add_stake</a>(
    self: &<b>mut</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>,
    stake: Coin&lt;SUI&gt;,
    validator_address: <b>address</b>,
    ctx: &<b>mut</b> TxContext,
) : StakedSui {
    self.validators.<a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_request_add_stake">request_add_stake</a>(
        validator_address,
        stake.into_balance(),
        ctx,
    )
}
</code></pre>



</details>

<a name="sui_system_sui_system_state_inner_request_add_stake_mul_coin"></a>

## Function `request_add_stake_mul_coin`

Add stake to a validator's staking pool using multiple coins.


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_request_add_stake_mul_coin">request_add_stake_mul_coin</a>(self: &<b>mut</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">sui_system::sui_system_state_inner::SuiSystemStateInnerV2</a>, stakes: vector&lt;<a href="../sui/coin.md#sui_coin_Coin">sui::coin::Coin</a>&lt;<a href="../sui/sui.md#sui_sui_SUI">sui::sui::SUI</a>&gt;&gt;, stake_amount: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;u64&gt;, validator_address: <b>address</b>, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakedSui">sui_system::staking_pool::StakedSui</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_request_add_stake_mul_coin">request_add_stake_mul_coin</a>(
    self: &<b>mut</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>,
    stakes: vector&lt;Coin&lt;SUI&gt;&gt;,
    stake_amount: option::Option&lt;u64&gt;,
    validator_address: <b>address</b>,
    ctx: &<b>mut</b> TxContext,
) : StakedSui {
    <b>let</b> balance = <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_extract_coin_balance">extract_coin_balance</a>(stakes, stake_amount, ctx);
    self.validators.<a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_request_add_stake">request_add_stake</a>(validator_address, balance, ctx)
}
</code></pre>



</details>

<a name="sui_system_sui_system_state_inner_request_withdraw_stake"></a>

## Function `request_withdraw_stake`

Withdraw some portion of a stake from a validator's staking pool.


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_request_withdraw_stake">request_withdraw_stake</a>(self: &<b>mut</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">sui_system::sui_system_state_inner::SuiSystemStateInnerV2</a>, staked_sui: <a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakedSui">sui_system::staking_pool::StakedSui</a>, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;<a href="../sui/sui.md#sui_sui_SUI">sui::sui::SUI</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_request_withdraw_stake">request_withdraw_stake</a>(
    self: &<b>mut</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>,
    staked_sui: StakedSui,
    ctx: &TxContext,
) : Balance&lt;SUI&gt; {
    self.validators.<a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_request_withdraw_stake">request_withdraw_stake</a>(staked_sui, ctx)
}
</code></pre>



</details>

<a name="sui_system_sui_system_state_inner_convert_to_fungible_staked_sui"></a>

## Function `convert_to_fungible_staked_sui`



<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_convert_to_fungible_staked_sui">convert_to_fungible_staked_sui</a>(self: &<b>mut</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">sui_system::sui_system_state_inner::SuiSystemStateInnerV2</a>, staked_sui: <a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakedSui">sui_system::staking_pool::StakedSui</a>, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <a href="../sui_system/staking_pool.md#sui_system_staking_pool_FungibleStakedSui">sui_system::staking_pool::FungibleStakedSui</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_convert_to_fungible_staked_sui">convert_to_fungible_staked_sui</a>(
    self: &<b>mut</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>,
    staked_sui: StakedSui,
    ctx: &<b>mut</b> TxContext,
) : FungibleStakedSui {
    self.validators.<a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_convert_to_fungible_staked_sui">convert_to_fungible_staked_sui</a>(staked_sui, ctx)
}
</code></pre>



</details>

<a name="sui_system_sui_system_state_inner_redeem_fungible_staked_sui"></a>

## Function `redeem_fungible_staked_sui`



<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_redeem_fungible_staked_sui">redeem_fungible_staked_sui</a>(self: &<b>mut</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">sui_system::sui_system_state_inner::SuiSystemStateInnerV2</a>, fungible_staked_sui: <a href="../sui_system/staking_pool.md#sui_system_staking_pool_FungibleStakedSui">sui_system::staking_pool::FungibleStakedSui</a>, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;<a href="../sui/sui.md#sui_sui_SUI">sui::sui::SUI</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_redeem_fungible_staked_sui">redeem_fungible_staked_sui</a>(
    self: &<b>mut</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>,
    fungible_staked_sui: FungibleStakedSui,
    ctx: &TxContext,
) : Balance&lt;SUI&gt; {
    self.validators.<a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_redeem_fungible_staked_sui">redeem_fungible_staked_sui</a>(fungible_staked_sui, ctx)
}
</code></pre>



</details>

<a name="sui_system_sui_system_state_inner_report_validator"></a>

## Function `report_validator`

Report a validator as a bad or non-performant actor in the system.
Succeeds if all the following are satisfied:
1. both the reporter in <code>cap</code> and the input <code>reportee_addr</code> are active validators.
2. reporter and reportee not the same address.
3. the cap object is still valid.
This function is idempotent.


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_report_validator">report_validator</a>(self: &<b>mut</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">sui_system::sui_system_state_inner::SuiSystemStateInnerV2</a>, cap: &<a href="../sui_system/validator_cap.md#sui_system_validator_cap_UnverifiedValidatorOperationCap">sui_system::validator_cap::UnverifiedValidatorOperationCap</a>, reportee_addr: <b>address</b>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_report_validator">report_validator</a>(
    self: &<b>mut</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>,
    cap: &UnverifiedValidatorOperationCap,
    reportee_addr: <b>address</b>,
) {
    // Reportee needs to be an active <a href="../sui_system/validator.md#sui_system_validator">validator</a>
    <b>assert</b>!(self.validators.is_active_validator_by_sui_address(reportee_addr), <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_ENotValidator">ENotValidator</a>);
    // Verify the represented reporter <b>address</b> is an active <a href="../sui_system/validator.md#sui_system_validator">validator</a>, and the capability is still valid.
    <b>let</b> verified_cap = self.validators.verify_cap(cap, <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_ACTIVE_VALIDATOR_ONLY">ACTIVE_VALIDATOR_ONLY</a>);
    <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_report_validator_impl">report_validator_impl</a>(verified_cap, reportee_addr, &<b>mut</b> self.validator_report_records);
}
</code></pre>



</details>

<a name="sui_system_sui_system_state_inner_undo_report_validator"></a>

## Function `undo_report_validator`

Undo a <code><a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_report_validator">report_validator</a></code> action. Aborts if
1. the reportee is not a currently active validator or
2. the sender has not previously reported the <code>reportee_addr</code>, or
3. the cap is not valid


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_undo_report_validator">undo_report_validator</a>(self: &<b>mut</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">sui_system::sui_system_state_inner::SuiSystemStateInnerV2</a>, cap: &<a href="../sui_system/validator_cap.md#sui_system_validator_cap_UnverifiedValidatorOperationCap">sui_system::validator_cap::UnverifiedValidatorOperationCap</a>, reportee_addr: <b>address</b>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_undo_report_validator">undo_report_validator</a>(
    self: &<b>mut</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>,
    cap: &UnverifiedValidatorOperationCap,
    reportee_addr: <b>address</b>,
) {
    <b>let</b> verified_cap = self.validators.verify_cap(cap, <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_ACTIVE_VALIDATOR_ONLY">ACTIVE_VALIDATOR_ONLY</a>);
    <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_undo_report_validator_impl">undo_report_validator_impl</a>(verified_cap, reportee_addr, &<b>mut</b> self.validator_report_records);
}
</code></pre>



</details>

<a name="sui_system_sui_system_state_inner_report_validator_impl"></a>

## Function `report_validator_impl`



<pre><code><b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_report_validator_impl">report_validator_impl</a>(verified_cap: <a href="../sui_system/validator_cap.md#sui_system_validator_cap_ValidatorOperationCap">sui_system::validator_cap::ValidatorOperationCap</a>, reportee_addr: <b>address</b>, validator_report_records: &<b>mut</b> <a href="../sui/vec_map.md#sui_vec_map_VecMap">sui::vec_map::VecMap</a>&lt;<b>address</b>, <a href="../sui/vec_set.md#sui_vec_set_VecSet">sui::vec_set::VecSet</a>&lt;<b>address</b>&gt;&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_report_validator_impl">report_validator_impl</a>(
    verified_cap: ValidatorOperationCap,
    reportee_addr: <b>address</b>,
    validator_report_records: &<b>mut</b> VecMap&lt;<b>address</b>, VecSet&lt;<b>address</b>&gt;&gt;,
) {
    <b>let</b> reporter_address = *verified_cap.verified_operation_cap_address();
    <b>assert</b>!(reporter_address != reportee_addr, <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_ECannotReportOneself">ECannotReportOneself</a>);
    <b>if</b> (!validator_report_records.contains(&reportee_addr)) {
        validator_report_records.insert(reportee_addr, vec_set::singleton(reporter_address));
    } <b>else</b> {
        <b>let</b> reporters = validator_report_records.get_mut(&reportee_addr);
        <b>if</b> (!reporters.contains(&reporter_address)) {
            reporters.insert(reporter_address);
        }
    }
}
</code></pre>



</details>

<a name="sui_system_sui_system_state_inner_undo_report_validator_impl"></a>

## Function `undo_report_validator_impl`



<pre><code><b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_undo_report_validator_impl">undo_report_validator_impl</a>(verified_cap: <a href="../sui_system/validator_cap.md#sui_system_validator_cap_ValidatorOperationCap">sui_system::validator_cap::ValidatorOperationCap</a>, reportee_addr: <b>address</b>, validator_report_records: &<b>mut</b> <a href="../sui/vec_map.md#sui_vec_map_VecMap">sui::vec_map::VecMap</a>&lt;<b>address</b>, <a href="../sui/vec_set.md#sui_vec_set_VecSet">sui::vec_set::VecSet</a>&lt;<b>address</b>&gt;&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_undo_report_validator_impl">undo_report_validator_impl</a>(
    verified_cap: ValidatorOperationCap,
    reportee_addr: <b>address</b>,
    validator_report_records: &<b>mut</b> VecMap&lt;<b>address</b>, VecSet&lt;<b>address</b>&gt;&gt;,
) {
    <b>assert</b>!(validator_report_records.contains(&reportee_addr), <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_EReportRecordNotFound">EReportRecordNotFound</a>);
    <b>let</b> reporters = validator_report_records.get_mut(&reportee_addr);
    <b>let</b> reporter_addr = *verified_cap.verified_operation_cap_address();
    <b>assert</b>!(reporters.contains(&reporter_addr), <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_EReportRecordNotFound">EReportRecordNotFound</a>);
    reporters.remove(&reporter_addr);
    <b>if</b> (reporters.is_empty()) {
        validator_report_records.remove(&reportee_addr);
    }
}
</code></pre>



</details>

<a name="sui_system_sui_system_state_inner_rotate_operation_cap"></a>

## Function `rotate_operation_cap`

Create a new <code>UnverifiedValidatorOperationCap</code>, transfer it to the
validator and registers it. The original object is thus revoked.


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_rotate_operation_cap">rotate_operation_cap</a>(self: &<b>mut</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">sui_system::sui_system_state_inner::SuiSystemStateInnerV2</a>, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_rotate_operation_cap">rotate_operation_cap</a>(
    self: &<b>mut</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> <a href="../sui_system/validator.md#sui_system_validator">validator</a> = self.validators.get_validator_mut_with_ctx_including_candidates(ctx);
    <a href="../sui_system/validator.md#sui_system_validator">validator</a>.new_unverified_validator_operation_cap_and_transfer(ctx);
}
</code></pre>



</details>

<a name="sui_system_sui_system_state_inner_update_validator_name"></a>

## Function `update_validator_name`

Update a validator's name.


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_update_validator_name">update_validator_name</a>(self: &<b>mut</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">sui_system::sui_system_state_inner::SuiSystemStateInnerV2</a>, name: vector&lt;u8&gt;, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_update_validator_name">update_validator_name</a>(
    self: &<b>mut</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>,
    name: vector&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> <a href="../sui_system/validator.md#sui_system_validator">validator</a> = self.validators.get_validator_mut_with_ctx_including_candidates(ctx);
    <a href="../sui_system/validator.md#sui_system_validator">validator</a>.update_name(name);
}
</code></pre>



</details>

<a name="sui_system_sui_system_state_inner_update_validator_description"></a>

## Function `update_validator_description`

Update a validator's description


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_update_validator_description">update_validator_description</a>(self: &<b>mut</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">sui_system::sui_system_state_inner::SuiSystemStateInnerV2</a>, description: vector&lt;u8&gt;, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_update_validator_description">update_validator_description</a>(
    self: &<b>mut</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>,
    description: vector&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> <a href="../sui_system/validator.md#sui_system_validator">validator</a> = self.validators.get_validator_mut_with_ctx_including_candidates(ctx);
    <a href="../sui_system/validator.md#sui_system_validator">validator</a>.update_description(description);
}
</code></pre>



</details>

<a name="sui_system_sui_system_state_inner_update_validator_image_url"></a>

## Function `update_validator_image_url`

Update a validator's image url


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_update_validator_image_url">update_validator_image_url</a>(self: &<b>mut</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">sui_system::sui_system_state_inner::SuiSystemStateInnerV2</a>, image_url: vector&lt;u8&gt;, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_update_validator_image_url">update_validator_image_url</a>(
    self: &<b>mut</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>,
    image_url: vector&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> <a href="../sui_system/validator.md#sui_system_validator">validator</a> = self.validators.get_validator_mut_with_ctx_including_candidates(ctx);
    <a href="../sui_system/validator.md#sui_system_validator">validator</a>.update_image_url(image_url);
}
</code></pre>



</details>

<a name="sui_system_sui_system_state_inner_update_validator_project_url"></a>

## Function `update_validator_project_url`

Update a validator's project url


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_update_validator_project_url">update_validator_project_url</a>(self: &<b>mut</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">sui_system::sui_system_state_inner::SuiSystemStateInnerV2</a>, project_url: vector&lt;u8&gt;, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_update_validator_project_url">update_validator_project_url</a>(
    self: &<b>mut</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>,
    project_url: vector&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> <a href="../sui_system/validator.md#sui_system_validator">validator</a> = self.validators.get_validator_mut_with_ctx_including_candidates(ctx);
    <a href="../sui_system/validator.md#sui_system_validator">validator</a>.update_project_url(project_url);
}
</code></pre>



</details>

<a name="sui_system_sui_system_state_inner_update_validator_next_epoch_network_address"></a>

## Function `update_validator_next_epoch_network_address`

Update a validator's network address.
The change will only take effects starting from the next epoch.


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_update_validator_next_epoch_network_address">update_validator_next_epoch_network_address</a>(self: &<b>mut</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">sui_system::sui_system_state_inner::SuiSystemStateInnerV2</a>, network_address: vector&lt;u8&gt;, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_update_validator_next_epoch_network_address">update_validator_next_epoch_network_address</a>(
    self: &<b>mut</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>,
    network_address: vector&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> <a href="../sui_system/validator.md#sui_system_validator">validator</a> = self.validators.get_validator_mut_with_ctx(ctx);
    <a href="../sui_system/validator.md#sui_system_validator">validator</a>.update_next_epoch_network_address(network_address);
    <b>let</b> <a href="../sui_system/validator.md#sui_system_validator">validator</a> :&Validator = <a href="../sui_system/validator.md#sui_system_validator">validator</a>; // Force immutability <b>for</b> the following call
    self.validators.assert_no_pending_or_active_duplicates(<a href="../sui_system/validator.md#sui_system_validator">validator</a>);
}
</code></pre>



</details>

<a name="sui_system_sui_system_state_inner_update_candidate_validator_network_address"></a>

## Function `update_candidate_validator_network_address`

Update candidate validator's network address.


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_update_candidate_validator_network_address">update_candidate_validator_network_address</a>(self: &<b>mut</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">sui_system::sui_system_state_inner::SuiSystemStateInnerV2</a>, network_address: vector&lt;u8&gt;, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_update_candidate_validator_network_address">update_candidate_validator_network_address</a>(
    self: &<b>mut</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>,
    network_address: vector&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> candidate = self.validators.get_validator_mut_with_ctx_including_candidates(ctx);
    candidate.update_candidate_network_address(network_address);
}
</code></pre>



</details>

<a name="sui_system_sui_system_state_inner_update_validator_next_epoch_p2p_address"></a>

## Function `update_validator_next_epoch_p2p_address`

Update a validator's p2p address.
The change will only take effects starting from the next epoch.


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_update_validator_next_epoch_p2p_address">update_validator_next_epoch_p2p_address</a>(self: &<b>mut</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">sui_system::sui_system_state_inner::SuiSystemStateInnerV2</a>, p2p_address: vector&lt;u8&gt;, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_update_validator_next_epoch_p2p_address">update_validator_next_epoch_p2p_address</a>(
    self: &<b>mut</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>,
    p2p_address: vector&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> <a href="../sui_system/validator.md#sui_system_validator">validator</a> = self.validators.get_validator_mut_with_ctx(ctx);
    <a href="../sui_system/validator.md#sui_system_validator">validator</a>.update_next_epoch_p2p_address(p2p_address);
    <b>let</b> <a href="../sui_system/validator.md#sui_system_validator">validator</a> :&Validator = <a href="../sui_system/validator.md#sui_system_validator">validator</a>; // Force immutability <b>for</b> the following call
    self.validators.assert_no_pending_or_active_duplicates(<a href="../sui_system/validator.md#sui_system_validator">validator</a>);
}
</code></pre>



</details>

<a name="sui_system_sui_system_state_inner_update_candidate_validator_p2p_address"></a>

## Function `update_candidate_validator_p2p_address`

Update candidate validator's p2p address.


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_update_candidate_validator_p2p_address">update_candidate_validator_p2p_address</a>(self: &<b>mut</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">sui_system::sui_system_state_inner::SuiSystemStateInnerV2</a>, p2p_address: vector&lt;u8&gt;, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_update_candidate_validator_p2p_address">update_candidate_validator_p2p_address</a>(
    self: &<b>mut</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>,
    p2p_address: vector&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> candidate = self.validators.get_validator_mut_with_ctx_including_candidates(ctx);
    candidate.update_candidate_p2p_address(p2p_address);
}
</code></pre>



</details>

<a name="sui_system_sui_system_state_inner_update_validator_next_epoch_primary_address"></a>

## Function `update_validator_next_epoch_primary_address`

Update a validator's narwhal primary address.
The change will only take effects starting from the next epoch.


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_update_validator_next_epoch_primary_address">update_validator_next_epoch_primary_address</a>(self: &<b>mut</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">sui_system::sui_system_state_inner::SuiSystemStateInnerV2</a>, primary_address: vector&lt;u8&gt;, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_update_validator_next_epoch_primary_address">update_validator_next_epoch_primary_address</a>(
    self: &<b>mut</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>,
    primary_address: vector&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> <a href="../sui_system/validator.md#sui_system_validator">validator</a> = self.validators.get_validator_mut_with_ctx(ctx);
    <a href="../sui_system/validator.md#sui_system_validator">validator</a>.update_next_epoch_primary_address(primary_address);
}
</code></pre>



</details>

<a name="sui_system_sui_system_state_inner_update_candidate_validator_primary_address"></a>

## Function `update_candidate_validator_primary_address`

Update candidate validator's narwhal primary address.


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_update_candidate_validator_primary_address">update_candidate_validator_primary_address</a>(self: &<b>mut</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">sui_system::sui_system_state_inner::SuiSystemStateInnerV2</a>, primary_address: vector&lt;u8&gt;, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_update_candidate_validator_primary_address">update_candidate_validator_primary_address</a>(
    self: &<b>mut</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>,
    primary_address: vector&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> candidate = self.validators.get_validator_mut_with_ctx_including_candidates(ctx);
    candidate.update_candidate_primary_address(primary_address);
}
</code></pre>



</details>

<a name="sui_system_sui_system_state_inner_update_validator_next_epoch_worker_address"></a>

## Function `update_validator_next_epoch_worker_address`

Update a validator's narwhal worker address.
The change will only take effects starting from the next epoch.


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_update_validator_next_epoch_worker_address">update_validator_next_epoch_worker_address</a>(self: &<b>mut</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">sui_system::sui_system_state_inner::SuiSystemStateInnerV2</a>, worker_address: vector&lt;u8&gt;, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_update_validator_next_epoch_worker_address">update_validator_next_epoch_worker_address</a>(
    self: &<b>mut</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>,
    worker_address: vector&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> <a href="../sui_system/validator.md#sui_system_validator">validator</a> = self.validators.get_validator_mut_with_ctx(ctx);
    <a href="../sui_system/validator.md#sui_system_validator">validator</a>.update_next_epoch_worker_address(worker_address);
}
</code></pre>



</details>

<a name="sui_system_sui_system_state_inner_update_candidate_validator_worker_address"></a>

## Function `update_candidate_validator_worker_address`

Update candidate validator's narwhal worker address.


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_update_candidate_validator_worker_address">update_candidate_validator_worker_address</a>(self: &<b>mut</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">sui_system::sui_system_state_inner::SuiSystemStateInnerV2</a>, worker_address: vector&lt;u8&gt;, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_update_candidate_validator_worker_address">update_candidate_validator_worker_address</a>(
    self: &<b>mut</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>,
    worker_address: vector&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> candidate = self.validators.get_validator_mut_with_ctx_including_candidates(ctx);
    candidate.update_candidate_worker_address(worker_address);
}
</code></pre>



</details>

<a name="sui_system_sui_system_state_inner_update_validator_next_epoch_protocol_pubkey"></a>

## Function `update_validator_next_epoch_protocol_pubkey`

Update a validator's public key of protocol key and proof of possession.
The change will only take effects starting from the next epoch.


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_update_validator_next_epoch_protocol_pubkey">update_validator_next_epoch_protocol_pubkey</a>(self: &<b>mut</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">sui_system::sui_system_state_inner::SuiSystemStateInnerV2</a>, protocol_pubkey: vector&lt;u8&gt;, proof_of_possession: vector&lt;u8&gt;, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_update_validator_next_epoch_protocol_pubkey">update_validator_next_epoch_protocol_pubkey</a>(
    self: &<b>mut</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>,
    protocol_pubkey: vector&lt;u8&gt;,
    proof_of_possession: vector&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> <a href="../sui_system/validator.md#sui_system_validator">validator</a> = self.validators.get_validator_mut_with_ctx(ctx);
    <a href="../sui_system/validator.md#sui_system_validator">validator</a>.update_next_epoch_protocol_pubkey(protocol_pubkey, proof_of_possession);
    <b>let</b> <a href="../sui_system/validator.md#sui_system_validator">validator</a> :&Validator = <a href="../sui_system/validator.md#sui_system_validator">validator</a>; // Force immutability <b>for</b> the following call
    self.validators.assert_no_pending_or_active_duplicates(<a href="../sui_system/validator.md#sui_system_validator">validator</a>);
}
</code></pre>



</details>

<a name="sui_system_sui_system_state_inner_update_candidate_validator_protocol_pubkey"></a>

## Function `update_candidate_validator_protocol_pubkey`

Update candidate validator's public key of protocol key and proof of possession.


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_update_candidate_validator_protocol_pubkey">update_candidate_validator_protocol_pubkey</a>(self: &<b>mut</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">sui_system::sui_system_state_inner::SuiSystemStateInnerV2</a>, protocol_pubkey: vector&lt;u8&gt;, proof_of_possession: vector&lt;u8&gt;, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_update_candidate_validator_protocol_pubkey">update_candidate_validator_protocol_pubkey</a>(
    self: &<b>mut</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>,
    protocol_pubkey: vector&lt;u8&gt;,
    proof_of_possession: vector&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> candidate = self.validators.get_validator_mut_with_ctx_including_candidates(ctx);
    candidate.update_candidate_protocol_pubkey(protocol_pubkey, proof_of_possession);
}
</code></pre>



</details>

<a name="sui_system_sui_system_state_inner_update_validator_next_epoch_worker_pubkey"></a>

## Function `update_validator_next_epoch_worker_pubkey`

Update a validator's public key of worker key.
The change will only take effects starting from the next epoch.


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_update_validator_next_epoch_worker_pubkey">update_validator_next_epoch_worker_pubkey</a>(self: &<b>mut</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">sui_system::sui_system_state_inner::SuiSystemStateInnerV2</a>, worker_pubkey: vector&lt;u8&gt;, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_update_validator_next_epoch_worker_pubkey">update_validator_next_epoch_worker_pubkey</a>(
    self: &<b>mut</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>,
    worker_pubkey: vector&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> <a href="../sui_system/validator.md#sui_system_validator">validator</a> = self.validators.get_validator_mut_with_ctx(ctx);
    <a href="../sui_system/validator.md#sui_system_validator">validator</a>.update_next_epoch_worker_pubkey(worker_pubkey);
    <b>let</b> <a href="../sui_system/validator.md#sui_system_validator">validator</a> :&Validator = <a href="../sui_system/validator.md#sui_system_validator">validator</a>; // Force immutability <b>for</b> the following call
    self.validators.assert_no_pending_or_active_duplicates(<a href="../sui_system/validator.md#sui_system_validator">validator</a>);
}
</code></pre>



</details>

<a name="sui_system_sui_system_state_inner_update_candidate_validator_worker_pubkey"></a>

## Function `update_candidate_validator_worker_pubkey`

Update candidate validator's public key of worker key.


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_update_candidate_validator_worker_pubkey">update_candidate_validator_worker_pubkey</a>(self: &<b>mut</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">sui_system::sui_system_state_inner::SuiSystemStateInnerV2</a>, worker_pubkey: vector&lt;u8&gt;, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_update_candidate_validator_worker_pubkey">update_candidate_validator_worker_pubkey</a>(
    self: &<b>mut</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>,
    worker_pubkey: vector&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> candidate = self.validators.get_validator_mut_with_ctx_including_candidates(ctx);
    candidate.update_candidate_worker_pubkey(worker_pubkey);
}
</code></pre>



</details>

<a name="sui_system_sui_system_state_inner_update_validator_next_epoch_network_pubkey"></a>

## Function `update_validator_next_epoch_network_pubkey`

Update a validator's public key of network key.
The change will only take effects starting from the next epoch.


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_update_validator_next_epoch_network_pubkey">update_validator_next_epoch_network_pubkey</a>(self: &<b>mut</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">sui_system::sui_system_state_inner::SuiSystemStateInnerV2</a>, network_pubkey: vector&lt;u8&gt;, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_update_validator_next_epoch_network_pubkey">update_validator_next_epoch_network_pubkey</a>(
    self: &<b>mut</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>,
    network_pubkey: vector&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> <a href="../sui_system/validator.md#sui_system_validator">validator</a> = self.validators.get_validator_mut_with_ctx(ctx);
    <a href="../sui_system/validator.md#sui_system_validator">validator</a>.update_next_epoch_network_pubkey(network_pubkey);
    <b>let</b> <a href="../sui_system/validator.md#sui_system_validator">validator</a> :&Validator = <a href="../sui_system/validator.md#sui_system_validator">validator</a>; // Force immutability <b>for</b> the following call
    self.validators.assert_no_pending_or_active_duplicates(<a href="../sui_system/validator.md#sui_system_validator">validator</a>);
}
</code></pre>



</details>

<a name="sui_system_sui_system_state_inner_update_candidate_validator_network_pubkey"></a>

## Function `update_candidate_validator_network_pubkey`

Update candidate validator's public key of network key.


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_update_candidate_validator_network_pubkey">update_candidate_validator_network_pubkey</a>(self: &<b>mut</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">sui_system::sui_system_state_inner::SuiSystemStateInnerV2</a>, network_pubkey: vector&lt;u8&gt;, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_update_candidate_validator_network_pubkey">update_candidate_validator_network_pubkey</a>(
    self: &<b>mut</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>,
    network_pubkey: vector&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> candidate = self.validators.get_validator_mut_with_ctx_including_candidates(ctx);
    candidate.update_candidate_network_pubkey(network_pubkey);
}
</code></pre>



</details>

<a name="sui_system_sui_system_state_inner_advance_epoch"></a>

## Function `advance_epoch`

This function should be called at the end of an epoch, and advances the system to the next epoch.
It does the following things:
1. Add storage charge to the storage fund.
2. Burn the storage rebates from the storage fund. These are already refunded to transaction sender's
gas coins.
3. Distribute computation charge to validator stake.
4. Update all validators.


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_advance_epoch">advance_epoch</a>(self: &<b>mut</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">sui_system::sui_system_state_inner::SuiSystemStateInnerV2</a>, new_epoch: u64, next_protocol_version: u64, storage_reward: <a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;<a href="../sui/sui.md#sui_sui_SUI">sui::sui::SUI</a>&gt;, computation_reward: <a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;<a href="../sui/sui.md#sui_sui_SUI">sui::sui::SUI</a>&gt;, storage_rebate_amount: u64, non_refundable_storage_fee_amount: u64, storage_fund_reinvest_rate: u64, reward_slashing_rate: u64, <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_epoch_start_timestamp_ms">epoch_start_timestamp_ms</a>: u64, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;<a href="../sui/sui.md#sui_sui_SUI">sui::sui::SUI</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_advance_epoch">advance_epoch</a>(
    self: &<b>mut</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>,
    new_epoch: u64,
    next_protocol_version: u64,
    <b>mut</b> storage_reward: Balance&lt;SUI&gt;,
    <b>mut</b> computation_reward: Balance&lt;SUI&gt;,
    <b>mut</b> storage_rebate_amount: u64,
    <b>mut</b> non_refundable_storage_fee_amount: u64,
    storage_fund_reinvest_rate: u64, // share of storage fund's rewards that's reinvested
                                     // into storage fund, in basis point.
    reward_slashing_rate: u64, // how much rewards are slashed to punish a <a href="../sui_system/validator.md#sui_system_validator">validator</a>, in bps.
    <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_epoch_start_timestamp_ms">epoch_start_timestamp_ms</a>: u64, // Timestamp of the <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_epoch">epoch</a> start
    ctx: &<b>mut</b> TxContext,
) : Balance&lt;SUI&gt; {
    <b>let</b> prev_epoch_start_timestamp = self.<a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_epoch_start_timestamp_ms">epoch_start_timestamp_ms</a>;
    self.<a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_epoch_start_timestamp_ms">epoch_start_timestamp_ms</a> = <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_epoch_start_timestamp_ms">epoch_start_timestamp_ms</a>;
    <b>let</b> bps_denominator_u64 = <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_BASIS_POINT_DENOMINATOR">BASIS_POINT_DENOMINATOR</a> <b>as</b> u64;
    // Rates can't be higher than 100%.
    <b>assert</b>!(
        storage_fund_reinvest_rate &lt;= bps_denominator_u64
        && reward_slashing_rate &lt;= bps_denominator_u64,
        <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_EBpsTooLarge">EBpsTooLarge</a>,
    );
    // TODO: remove this in later upgrade.
    <b>if</b> (self.parameters.stake_subsidy_start_epoch &gt; 0) {
        self.parameters.stake_subsidy_start_epoch = 20;
    };
    // Accumulate the gas summary during safe_mode before processing any rewards:
    <b>let</b> safe_mode_storage_rewards = self.safe_mode_storage_rewards.withdraw_all();
    storage_reward.join(safe_mode_storage_rewards);
    <b>let</b> safe_mode_computation_rewards = self.safe_mode_computation_rewards.withdraw_all();
    computation_reward.join(safe_mode_computation_rewards);
    storage_rebate_amount = storage_rebate_amount + self.safe_mode_storage_rebates;
    self.safe_mode_storage_rebates = 0;
    non_refundable_storage_fee_amount = non_refundable_storage_fee_amount + self.safe_mode_non_refundable_storage_fee;
    self.safe_mode_non_refundable_storage_fee = 0;
    <b>let</b> total_validators_stake = self.validators.total_stake();
    <b>let</b> storage_fund_balance = self.<a href="../sui_system/storage_fund.md#sui_system_storage_fund">storage_fund</a>.total_balance();
    <b>let</b> total_stake = storage_fund_balance + total_validators_stake;
    <b>let</b> storage_charge = storage_reward.value();
    <b>let</b> computation_charge = computation_reward.value();
    <b>let</b> <b>mut</b> <a href="../sui_system/stake_subsidy.md#sui_system_stake_subsidy">stake_subsidy</a> = balance::zero();
    // during the transition from <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_epoch">epoch</a> N to <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_epoch">epoch</a> N + 1, ctx.<a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_epoch">epoch</a>() will <b>return</b> N
    <b>let</b> old_epoch = ctx.<a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_epoch">epoch</a>();
    // Include stake subsidy in the rewards given out to validators and stakers.
    // Delay distributing any stake subsidies until after `stake_subsidy_start_epoch`.
    // And <b>if</b> this <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_epoch">epoch</a> is shorter than the regular <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_epoch">epoch</a> duration, don't distribute any stake subsidy.
    <b>if</b> (old_epoch &gt;= self.parameters.stake_subsidy_start_epoch  &&
        <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_epoch_start_timestamp_ms">epoch_start_timestamp_ms</a> &gt;= prev_epoch_start_timestamp + self.parameters.epoch_duration_ms)
    {
        // special case <b>for</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_epoch">epoch</a> 560 -&gt; 561 change bug. add extra subsidies <b>for</b> "safe mode"
        // where reward distribution was skipped. <b>use</b> distribution counter and <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_epoch">epoch</a> check to
        // avoiding affecting devnet and testnet
        <b>if</b> (self.<a href="../sui_system/stake_subsidy.md#sui_system_stake_subsidy">stake_subsidy</a>.get_distribution_counter() == 540 && old_epoch &gt; 560) {
            // safe mode was entered on the change from 560 to 561. so 560 was the first <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_epoch">epoch</a> without proper subsidy distribution
            <b>let</b> first_safe_mode_epoch = 560;
            <b>let</b> safe_mode_epoch_count = old_epoch - first_safe_mode_epoch;
            safe_mode_epoch_count.do!(|_| {
                <a href="../sui_system/stake_subsidy.md#sui_system_stake_subsidy">stake_subsidy</a>.join(self.<a href="../sui_system/stake_subsidy.md#sui_system_stake_subsidy">stake_subsidy</a>.<a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_advance_epoch">advance_epoch</a>());
            });
            // done with catchup <b>for</b> safe mode epochs. distribution counter is now &gt;540, we won't hit this again
            // fall through to the normal logic, which will add subsidies <b>for</b> the current <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_epoch">epoch</a>
        };
        <a href="../sui_system/stake_subsidy.md#sui_system_stake_subsidy">stake_subsidy</a>.join(self.<a href="../sui_system/stake_subsidy.md#sui_system_stake_subsidy">stake_subsidy</a>.<a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_advance_epoch">advance_epoch</a>());
    };
    <b>let</b> stake_subsidy_amount = <a href="../sui_system/stake_subsidy.md#sui_system_stake_subsidy">stake_subsidy</a>.value();
    computation_reward.join(<a href="../sui_system/stake_subsidy.md#sui_system_stake_subsidy">stake_subsidy</a>);
    <b>let</b> total_stake_u128 = total_stake <b>as</b> u128;
    <b>let</b> computation_charge_u128 = computation_charge <b>as</b> u128;
    <b>let</b> storage_fund_reward_amount = storage_fund_balance <b>as</b> u128 * computation_charge_u128 / total_stake_u128;
    <b>let</b> <b>mut</b> storage_fund_reward = computation_reward.split(storage_fund_reward_amount <b>as</b> u64);
    <b>let</b> storage_fund_reinvestment_amount =
        storage_fund_reward_amount * (storage_fund_reinvest_rate <b>as</b> u128) / <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_BASIS_POINT_DENOMINATOR">BASIS_POINT_DENOMINATOR</a>;
    <b>let</b> storage_fund_reinvestment = storage_fund_reward.split(
        storage_fund_reinvestment_amount <b>as</b> u64,
    );
    self.<a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_epoch">epoch</a> = self.<a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_epoch">epoch</a> + 1;
    // Sanity check to make sure we are advancing to the right <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_epoch">epoch</a>.
    <b>assert</b>!(new_epoch == self.<a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_epoch">epoch</a>, <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_EAdvancedToWrongEpoch">EAdvancedToWrongEpoch</a>);
    <b>let</b> computation_reward_amount_before_distribution = computation_reward.value();
    <b>let</b> storage_fund_reward_amount_before_distribution = storage_fund_reward.value();
    self.validators.<a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_advance_epoch">advance_epoch</a>(
        &<b>mut</b> computation_reward,
        &<b>mut</b> storage_fund_reward,
        &<b>mut</b> self.validator_report_records,
        reward_slashing_rate,
        self.parameters.validator_low_stake_threshold,
        self.parameters.validator_very_low_stake_threshold,
        self.parameters.validator_low_stake_grace_period,
        ctx,
    );
    <b>let</b> new_total_stake = self.validators.total_stake();
    <b>let</b> computation_reward_amount_after_distribution = computation_reward.value();
    <b>let</b> storage_fund_reward_amount_after_distribution = storage_fund_reward.value();
    <b>let</b> computation_reward_distributed = computation_reward_amount_before_distribution - computation_reward_amount_after_distribution;
    <b>let</b> storage_fund_reward_distributed = storage_fund_reward_amount_before_distribution - storage_fund_reward_amount_after_distribution;
    self.<a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_protocol_version">protocol_version</a> = next_protocol_version;
    // Derive the reference gas price <b>for</b> the new <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_epoch">epoch</a>
    self.reference_gas_price = self.validators.derive_reference_gas_price();
    // Because of precision issues with integer divisions, we expect that there will be some
    // remaining balance in `storage_fund_reward` and `computation_reward`.
    // All of these go to the storage fund.
    <b>let</b> <b>mut</b> leftover_staking_rewards = storage_fund_reward;
    leftover_staking_rewards.join(computation_reward);
    <b>let</b> leftover_storage_fund_inflow = leftover_staking_rewards.value();
    <b>let</b> refunded_storage_rebate =
        self.<a href="../sui_system/storage_fund.md#sui_system_storage_fund">storage_fund</a>.<a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_advance_epoch">advance_epoch</a>(
            storage_reward,
            storage_fund_reinvestment,
            leftover_staking_rewards,
            storage_rebate_amount,
            non_refundable_storage_fee_amount,
        );
    event::emit(
        <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SystemEpochInfoEvent">SystemEpochInfoEvent</a> {
            <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_epoch">epoch</a>: self.<a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_epoch">epoch</a>,
            <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_protocol_version">protocol_version</a>: self.<a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_protocol_version">protocol_version</a>,
            reference_gas_price: self.reference_gas_price,
            total_stake: new_total_stake,
            storage_charge,
            storage_fund_reinvestment: storage_fund_reinvestment_amount <b>as</b> u64,
            storage_rebate: storage_rebate_amount,
            storage_fund_balance: self.<a href="../sui_system/storage_fund.md#sui_system_storage_fund">storage_fund</a>.total_balance(),
            stake_subsidy_amount,
            total_gas_fees: computation_charge,
            total_stake_rewards_distributed: computation_reward_distributed + storage_fund_reward_distributed,
            leftover_storage_fund_inflow,
        }
    );
    self.safe_mode = <b>false</b>;
    // Double check that the gas from safe mode <b>has</b> been processed.
    <b>assert</b>!(self.safe_mode_storage_rebates == 0
        && self.safe_mode_storage_rewards.value() == 0
        && self.safe_mode_computation_rewards.value() == 0, <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_ESafeModeGasNotProcessed">ESafeModeGasNotProcessed</a>);
    // Return the storage rebate split from storage fund that's already refunded to the transaction senders.
    // This will be burnt at the last step of <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_epoch">epoch</a> change programmable transaction.
    refunded_storage_rebate
}
</code></pre>



</details>

<a name="sui_system_sui_system_state_inner_epoch"></a>

## Function `epoch`

Return the current epoch number. Useful for applications that need a coarse-grained concept of time,
since epochs are ever-increasing and epoch changes are intended to happen every 24 hours.


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_epoch">epoch</a>(self: &<a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">sui_system::sui_system_state_inner::SuiSystemStateInnerV2</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_epoch">epoch</a>(self: &<a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>): u64 {
    self.<a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_epoch">epoch</a>
}
</code></pre>



</details>

<a name="sui_system_sui_system_state_inner_protocol_version"></a>

## Function `protocol_version`



<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_protocol_version">protocol_version</a>(self: &<a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">sui_system::sui_system_state_inner::SuiSystemStateInnerV2</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_protocol_version">protocol_version</a>(self: &<a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>): u64 {
    self.<a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_protocol_version">protocol_version</a>
}
</code></pre>



</details>

<a name="sui_system_sui_system_state_inner_system_state_version"></a>

## Function `system_state_version`



<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_system_state_version">system_state_version</a>(self: &<a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">sui_system::sui_system_state_inner::SuiSystemStateInnerV2</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_system_state_version">system_state_version</a>(self: &<a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>): u64 {
    self.<a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_system_state_version">system_state_version</a>
}
</code></pre>



</details>

<a name="sui_system_sui_system_state_inner_genesis_system_state_version"></a>

## Function `genesis_system_state_version`

This function always return the genesis system state version, which is used to create the system state in genesis.
It should never change for a given network.


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_genesis_system_state_version">genesis_system_state_version</a>(): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_genesis_system_state_version">genesis_system_state_version</a>(): u64 {
    <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SYSTEM_STATE_VERSION_V1">SYSTEM_STATE_VERSION_V1</a>
}
</code></pre>



</details>

<a name="sui_system_sui_system_state_inner_epoch_start_timestamp_ms"></a>

## Function `epoch_start_timestamp_ms`

Returns unix timestamp of the start of current epoch


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_epoch_start_timestamp_ms">epoch_start_timestamp_ms</a>(self: &<a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">sui_system::sui_system_state_inner::SuiSystemStateInnerV2</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_epoch_start_timestamp_ms">epoch_start_timestamp_ms</a>(self: &<a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>): u64 {
    self.<a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_epoch_start_timestamp_ms">epoch_start_timestamp_ms</a>
}
</code></pre>



</details>

<a name="sui_system_sui_system_state_inner_validator_stake_amount"></a>

## Function `validator_stake_amount`

Returns the total amount staked with <code>validator_addr</code>.
Aborts if <code>validator_addr</code> is not an active validator.


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_validator_stake_amount">validator_stake_amount</a>(self: &<a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">sui_system::sui_system_state_inner::SuiSystemStateInnerV2</a>, validator_addr: <b>address</b>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_validator_stake_amount">validator_stake_amount</a>(self: &<a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>, validator_addr: <b>address</b>): u64 {
    self.validators.validator_total_stake_amount(validator_addr)
}
</code></pre>



</details>

<a name="sui_system_sui_system_state_inner_active_validator_voting_powers"></a>

## Function `active_validator_voting_powers`

Returns the voting power for <code>validator_addr</code>.
Aborts if <code>validator_addr</code> is not an active validator.


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_active_validator_voting_powers">active_validator_voting_powers</a>(self: &<a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">sui_system::sui_system_state_inner::SuiSystemStateInnerV2</a>): <a href="../sui/vec_map.md#sui_vec_map_VecMap">sui::vec_map::VecMap</a>&lt;<b>address</b>, u64&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_active_validator_voting_powers">active_validator_voting_powers</a>(self: &<a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>): VecMap&lt;<b>address</b>, u64&gt; {
    <b>let</b> <b>mut</b> active_validators = <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_active_validator_addresses">active_validator_addresses</a>(self);
    <b>let</b> <b>mut</b> voting_powers = vec_map::empty();
    <b>while</b> (!vector::is_empty(&active_validators)) {
        <b>let</b> <a href="../sui_system/validator.md#sui_system_validator">validator</a> = vector::pop_back(&<b>mut</b> active_validators);
        <b>let</b> <a href="../sui_system/voting_power.md#sui_system_voting_power">voting_power</a> = <a href="../sui_system/validator_set.md#sui_system_validator_set_validator_voting_power">validator_set::validator_voting_power</a>(&self.validators, <a href="../sui_system/validator.md#sui_system_validator">validator</a>);
        vec_map::insert(&<b>mut</b> voting_powers, <a href="../sui_system/validator.md#sui_system_validator">validator</a>, <a href="../sui_system/voting_power.md#sui_system_voting_power">voting_power</a>);
    };
    voting_powers
}
</code></pre>



</details>

<a name="sui_system_sui_system_state_inner_validator_staking_pool_id"></a>

## Function `validator_staking_pool_id`

Returns the staking pool id of a given validator.
Aborts if <code>validator_addr</code> is not an active validator.


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_validator_staking_pool_id">validator_staking_pool_id</a>(self: &<a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">sui_system::sui_system_state_inner::SuiSystemStateInnerV2</a>, validator_addr: <b>address</b>): <a href="../sui/object.md#sui_object_ID">sui::object::ID</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_validator_staking_pool_id">validator_staking_pool_id</a>(self: &<a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>, validator_addr: <b>address</b>): ID {
    self.validators.<a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_validator_staking_pool_id">validator_staking_pool_id</a>(validator_addr)
}
</code></pre>



</details>

<a name="sui_system_sui_system_state_inner_validator_staking_pool_mappings"></a>

## Function `validator_staking_pool_mappings`

Returns reference to the staking pool mappings that map pool ids to active validator addresses


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_validator_staking_pool_mappings">validator_staking_pool_mappings</a>(self: &<a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">sui_system::sui_system_state_inner::SuiSystemStateInnerV2</a>): &<a href="../sui/table.md#sui_table_Table">sui::table::Table</a>&lt;<a href="../sui/object.md#sui_object_ID">sui::object::ID</a>, <b>address</b>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_validator_staking_pool_mappings">validator_staking_pool_mappings</a>(self: &<a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>): &Table&lt;ID, <b>address</b>&gt; {
    self.validators.staking_pool_mappings()
}
</code></pre>



</details>

<a name="sui_system_sui_system_state_inner_get_reporters_of"></a>

## Function `get_reporters_of`

Returns all the validators who are currently reporting <code>addr</code>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_get_reporters_of">get_reporters_of</a>(self: &<a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">sui_system::sui_system_state_inner::SuiSystemStateInnerV2</a>, addr: <b>address</b>): <a href="../sui/vec_set.md#sui_vec_set_VecSet">sui::vec_set::VecSet</a>&lt;<b>address</b>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_get_reporters_of">get_reporters_of</a>(self: &<a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>, addr: <b>address</b>): VecSet&lt;<b>address</b>&gt; {
    <b>if</b> (self.validator_report_records.contains(&addr)) {
        self.validator_report_records[&addr]
    } <b>else</b> {
        vec_set::empty()
    }
}
</code></pre>



</details>

<a name="sui_system_sui_system_state_inner_get_storage_fund_total_balance"></a>

## Function `get_storage_fund_total_balance`



<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_get_storage_fund_total_balance">get_storage_fund_total_balance</a>(self: &<a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">sui_system::sui_system_state_inner::SuiSystemStateInnerV2</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_get_storage_fund_total_balance">get_storage_fund_total_balance</a>(self: &<a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>): u64 {
    self.<a href="../sui_system/storage_fund.md#sui_system_storage_fund">storage_fund</a>.total_balance()
}
</code></pre>



</details>

<a name="sui_system_sui_system_state_inner_get_storage_fund_object_rebates"></a>

## Function `get_storage_fund_object_rebates`



<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_get_storage_fund_object_rebates">get_storage_fund_object_rebates</a>(self: &<a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">sui_system::sui_system_state_inner::SuiSystemStateInnerV2</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_get_storage_fund_object_rebates">get_storage_fund_object_rebates</a>(self: &<a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>): u64 {
    self.<a href="../sui_system/storage_fund.md#sui_system_storage_fund">storage_fund</a>.total_object_storage_rebates()
}
</code></pre>



</details>

<a name="sui_system_sui_system_state_inner_validator_address_by_pool_id"></a>

## Function `validator_address_by_pool_id`



<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_validator_address_by_pool_id">validator_address_by_pool_id</a>(self: &<b>mut</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">sui_system::sui_system_state_inner::SuiSystemStateInnerV2</a>, pool_id: &<a href="../sui/object.md#sui_object_ID">sui::object::ID</a>): <b>address</b>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_validator_address_by_pool_id">validator_address_by_pool_id</a>(self: &<b>mut</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>, pool_id: &ID): <b>address</b> {
    self.validators.<a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_validator_address_by_pool_id">validator_address_by_pool_id</a>(pool_id)
}
</code></pre>



</details>

<a name="sui_system_sui_system_state_inner_pool_exchange_rates"></a>

## Function `pool_exchange_rates`



<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_pool_exchange_rates">pool_exchange_rates</a>(self: &<b>mut</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">sui_system::sui_system_state_inner::SuiSystemStateInnerV2</a>, pool_id: &<a href="../sui/object.md#sui_object_ID">sui::object::ID</a>): &<a href="../sui/table.md#sui_table_Table">sui::table::Table</a>&lt;u64, <a href="../sui_system/staking_pool.md#sui_system_staking_pool_PoolTokenExchangeRate">sui_system::staking_pool::PoolTokenExchangeRate</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_pool_exchange_rates">pool_exchange_rates</a>(
    self: &<b>mut</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>,
    pool_id: &ID
): &Table&lt;u64, PoolTokenExchangeRate&gt;  {
    <b>let</b> validators = &<b>mut</b> self.validators;
    validators.<a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_pool_exchange_rates">pool_exchange_rates</a>(pool_id)
}
</code></pre>



</details>

<a name="sui_system_sui_system_state_inner_active_validator_addresses"></a>

## Function `active_validator_addresses`



<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_active_validator_addresses">active_validator_addresses</a>(self: &<a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">sui_system::sui_system_state_inner::SuiSystemStateInnerV2</a>): vector&lt;<b>address</b>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_active_validator_addresses">active_validator_addresses</a>(self: &<a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>): vector&lt;<b>address</b>&gt; {
    <b>let</b> <a href="../sui_system/validator_set.md#sui_system_validator_set">validator_set</a> = &self.validators;
    <a href="../sui_system/validator_set.md#sui_system_validator_set">validator_set</a>.<a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_active_validator_addresses">active_validator_addresses</a>()
}
</code></pre>



</details>

<a name="sui_system_sui_system_state_inner_extract_coin_balance"></a>

## Function `extract_coin_balance`

Extract required Balance from vector of Coin<SUI>, transfer the remainder back to sender.


<pre><code><b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_extract_coin_balance">extract_coin_balance</a>(coins: vector&lt;<a href="../sui/coin.md#sui_coin_Coin">sui::coin::Coin</a>&lt;<a href="../sui/sui.md#sui_sui_SUI">sui::sui::SUI</a>&gt;&gt;, amount: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;u64&gt;, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;<a href="../sui/sui.md#sui_sui_SUI">sui::sui::SUI</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui_system/sui_system_state_inner.md#sui_system_sui_system_state_inner_extract_coin_balance">extract_coin_balance</a>(<b>mut</b> coins: vector&lt;Coin&lt;SUI&gt;&gt;, amount: option::Option&lt;u64&gt;, ctx: &<b>mut</b> TxContext): Balance&lt;SUI&gt; {
    <b>let</b> <b>mut</b> merged_coin = coins.pop_back();
    merged_coin.join_vec(coins);
    <b>let</b> <b>mut</b> total_balance = merged_coin.into_balance();
    // <b>return</b> the full amount <b>if</b> amount is not specified
    <b>if</b> (amount.is_some()) {
        <b>let</b> amount = amount.destroy_some();
        <b>let</b> balance = total_balance.split(amount);
        // transfer back the remainder <b>if</b> non zero.
        <b>if</b> (total_balance.value() &gt; 0) {
            transfer::public_transfer(total_balance.into_coin(ctx), ctx.sender());
        } <b>else</b> {
            total_balance.destroy_zero();
        };
        balance
    } <b>else</b> {
        total_balance
    }
}
</code></pre>



</details>
