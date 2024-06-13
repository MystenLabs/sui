---
title: Module `0x3::sui_system_state_inner`
---



-  [Struct `SystemParameters`](#0x3_sui_system_state_inner_SystemParameters)
-  [Struct `SystemParametersV2`](#0x3_sui_system_state_inner_SystemParametersV2)
-  [Struct `SuiSystemStateInner`](#0x3_sui_system_state_inner_SuiSystemStateInner)
-  [Struct `SuiSystemStateInnerV2`](#0x3_sui_system_state_inner_SuiSystemStateInnerV2)
-  [Struct `SystemEpochInfoEvent`](#0x3_sui_system_state_inner_SystemEpochInfoEvent)
-  [Constants](#@Constants_0)
-  [Function `create`](#0x3_sui_system_state_inner_create)
-  [Function `create_system_parameters`](#0x3_sui_system_state_inner_create_system_parameters)
-  [Function `v1_to_v2`](#0x3_sui_system_state_inner_v1_to_v2)
-  [Function `request_add_validator_candidate`](#0x3_sui_system_state_inner_request_add_validator_candidate)
-  [Function `request_remove_validator_candidate`](#0x3_sui_system_state_inner_request_remove_validator_candidate)
-  [Function `request_add_validator`](#0x3_sui_system_state_inner_request_add_validator)
-  [Function `request_remove_validator`](#0x3_sui_system_state_inner_request_remove_validator)
-  [Function `request_set_gas_price`](#0x3_sui_system_state_inner_request_set_gas_price)
-  [Function `set_candidate_validator_gas_price`](#0x3_sui_system_state_inner_set_candidate_validator_gas_price)
-  [Function `request_set_commission_rate`](#0x3_sui_system_state_inner_request_set_commission_rate)
-  [Function `set_candidate_validator_commission_rate`](#0x3_sui_system_state_inner_set_candidate_validator_commission_rate)
-  [Function `request_add_stake`](#0x3_sui_system_state_inner_request_add_stake)
-  [Function `request_add_stake_mul_coin`](#0x3_sui_system_state_inner_request_add_stake_mul_coin)
-  [Function `request_withdraw_stake`](#0x3_sui_system_state_inner_request_withdraw_stake)
-  [Function `report_validator`](#0x3_sui_system_state_inner_report_validator)
-  [Function `undo_report_validator`](#0x3_sui_system_state_inner_undo_report_validator)
-  [Function `report_validator_impl`](#0x3_sui_system_state_inner_report_validator_impl)
-  [Function `undo_report_validator_impl`](#0x3_sui_system_state_inner_undo_report_validator_impl)
-  [Function `rotate_operation_cap`](#0x3_sui_system_state_inner_rotate_operation_cap)
-  [Function `update_validator_name`](#0x3_sui_system_state_inner_update_validator_name)
-  [Function `update_validator_description`](#0x3_sui_system_state_inner_update_validator_description)
-  [Function `update_validator_image_url`](#0x3_sui_system_state_inner_update_validator_image_url)
-  [Function `update_validator_project_url`](#0x3_sui_system_state_inner_update_validator_project_url)
-  [Function `update_validator_next_epoch_network_address`](#0x3_sui_system_state_inner_update_validator_next_epoch_network_address)
-  [Function `update_candidate_validator_network_address`](#0x3_sui_system_state_inner_update_candidate_validator_network_address)
-  [Function `update_validator_next_epoch_p2p_address`](#0x3_sui_system_state_inner_update_validator_next_epoch_p2p_address)
-  [Function `update_candidate_validator_p2p_address`](#0x3_sui_system_state_inner_update_candidate_validator_p2p_address)
-  [Function `update_validator_next_epoch_primary_address`](#0x3_sui_system_state_inner_update_validator_next_epoch_primary_address)
-  [Function `update_candidate_validator_primary_address`](#0x3_sui_system_state_inner_update_candidate_validator_primary_address)
-  [Function `update_validator_next_epoch_worker_address`](#0x3_sui_system_state_inner_update_validator_next_epoch_worker_address)
-  [Function `update_candidate_validator_worker_address`](#0x3_sui_system_state_inner_update_candidate_validator_worker_address)
-  [Function `update_validator_next_epoch_protocol_pubkey`](#0x3_sui_system_state_inner_update_validator_next_epoch_protocol_pubkey)
-  [Function `update_candidate_validator_protocol_pubkey`](#0x3_sui_system_state_inner_update_candidate_validator_protocol_pubkey)
-  [Function `update_validator_next_epoch_worker_pubkey`](#0x3_sui_system_state_inner_update_validator_next_epoch_worker_pubkey)
-  [Function `update_candidate_validator_worker_pubkey`](#0x3_sui_system_state_inner_update_candidate_validator_worker_pubkey)
-  [Function `update_validator_next_epoch_network_pubkey`](#0x3_sui_system_state_inner_update_validator_next_epoch_network_pubkey)
-  [Function `update_candidate_validator_network_pubkey`](#0x3_sui_system_state_inner_update_candidate_validator_network_pubkey)
-  [Function `advance_epoch`](#0x3_sui_system_state_inner_advance_epoch)
-  [Function `epoch`](#0x3_sui_system_state_inner_epoch)
-  [Function `protocol_version`](#0x3_sui_system_state_inner_protocol_version)
-  [Function `system_state_version`](#0x3_sui_system_state_inner_system_state_version)
-  [Function `genesis_system_state_version`](#0x3_sui_system_state_inner_genesis_system_state_version)
-  [Function `epoch_start_timestamp_ms`](#0x3_sui_system_state_inner_epoch_start_timestamp_ms)
-  [Function `validator_stake_amount`](#0x3_sui_system_state_inner_validator_stake_amount)
-  [Function `active_validator_voting_powers`](#0x3_sui_system_state_inner_active_validator_voting_powers)
-  [Function `validator_staking_pool_id`](#0x3_sui_system_state_inner_validator_staking_pool_id)
-  [Function `validator_staking_pool_mappings`](#0x3_sui_system_state_inner_validator_staking_pool_mappings)
-  [Function `get_reporters_of`](#0x3_sui_system_state_inner_get_reporters_of)
-  [Function `get_storage_fund_total_balance`](#0x3_sui_system_state_inner_get_storage_fund_total_balance)
-  [Function `get_storage_fund_object_rebates`](#0x3_sui_system_state_inner_get_storage_fund_object_rebates)
-  [Function `pool_exchange_rates`](#0x3_sui_system_state_inner_pool_exchange_rates)
-  [Function `active_validator_addresses`](#0x3_sui_system_state_inner_active_validator_addresses)
-  [Function `extract_coin_balance`](#0x3_sui_system_state_inner_extract_coin_balance)


<pre><code><b>use</b> <a href="../move-stdlib/option.md#0x1_option">0x1::option</a>;
<b>use</b> <a href="../move-stdlib/vector.md#0x1_vector">0x1::vector</a>;
<b>use</b> <a href="../sui-framework/bag.md#0x2_bag">0x2::bag</a>;
<b>use</b> <a href="../sui-framework/balance.md#0x2_balance">0x2::balance</a>;
<b>use</b> <a href="../sui-framework/coin.md#0x2_coin">0x2::coin</a>;
<b>use</b> <a href="../sui-framework/event.md#0x2_event">0x2::event</a>;
<b>use</b> <a href="../sui-framework/object.md#0x2_object">0x2::object</a>;
<b>use</b> <a href="../sui-framework/pay.md#0x2_pay">0x2::pay</a>;
<b>use</b> <a href="../sui-framework/sui.md#0x2_sui">0x2::sui</a>;
<b>use</b> <a href="../sui-framework/table.md#0x2_table">0x2::table</a>;
<b>use</b> <a href="../sui-framework/transfer.md#0x2_transfer">0x2::transfer</a>;
<b>use</b> <a href="../sui-framework/tx_context.md#0x2_tx_context">0x2::tx_context</a>;
<b>use</b> <a href="../sui-framework/vec_map.md#0x2_vec_map">0x2::vec_map</a>;
<b>use</b> <a href="../sui-framework/vec_set.md#0x2_vec_set">0x2::vec_set</a>;
<b>use</b> <a href="stake_subsidy.md#0x3_stake_subsidy">0x3::stake_subsidy</a>;
<b>use</b> <a href="staking_pool.md#0x3_staking_pool">0x3::staking_pool</a>;
<b>use</b> <a href="storage_fund.md#0x3_storage_fund">0x3::storage_fund</a>;
<b>use</b> <a href="validator.md#0x3_validator">0x3::validator</a>;
<b>use</b> <a href="validator_cap.md#0x3_validator_cap">0x3::validator_cap</a>;
<b>use</b> <a href="validator_set.md#0x3_validator_set">0x3::validator_set</a>;
</code></pre>



<a name="0x3_sui_system_state_inner_SystemParameters"></a>

## Struct `SystemParameters`

A list of system config parameters.


<pre><code><b>struct</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SystemParameters">SystemParameters</a> <b>has</b> store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>epoch_duration_ms: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>
 The duration of an epoch, in milliseconds.
</dd>
<dt>
<code>stake_subsidy_start_epoch: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>
 The starting epoch in which stake subsidies start being paid out
</dd>
<dt>
<code>max_validator_count: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>
 Maximum number of active validators at any moment.
 We do not allow the number of validators in any epoch to go above this.
</dd>
<dt>
<code>min_validator_joining_stake: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>
 Lower-bound on the amount of stake required to become a validator.
</dd>
<dt>
<code>validator_low_stake_threshold: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>
 Validators with stake amount below <code>validator_low_stake_threshold</code> are considered to
 have low stake and will be escorted out of the validator set after being below this
 threshold for more than <code>validator_low_stake_grace_period</code> number of epochs.
</dd>
<dt>
<code>validator_very_low_stake_threshold: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>
 Validators with stake below <code>validator_very_low_stake_threshold</code> will be removed
 immediately at epoch change, no grace period.
</dd>
<dt>
<code>validator_low_stake_grace_period: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>
 A validator can have stake below <code>validator_low_stake_threshold</code>
 for this many epochs before being kicked out.
</dd>
<dt>
<code>extra_fields: <a href="../sui-framework/bag.md#0x2_bag_Bag">bag::Bag</a></code>
</dt>
<dd>
 Any extra fields that's not defined statically.
</dd>
</dl>


</details>

<a name="0x3_sui_system_state_inner_SystemParametersV2"></a>

## Struct `SystemParametersV2`

Added min_validator_count.


<pre><code><b>struct</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SystemParametersV2">SystemParametersV2</a> <b>has</b> store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>epoch_duration_ms: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>
 The duration of an epoch, in milliseconds.
</dd>
<dt>
<code>stake_subsidy_start_epoch: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>
 The starting epoch in which stake subsidies start being paid out
</dd>
<dt>
<code>min_validator_count: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>
 Minimum number of active validators at any moment.
</dd>
<dt>
<code>max_validator_count: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>
 Maximum number of active validators at any moment.
 We do not allow the number of validators in any epoch to go above this.
</dd>
<dt>
<code>min_validator_joining_stake: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>
 Lower-bound on the amount of stake required to become a validator.
</dd>
<dt>
<code>validator_low_stake_threshold: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>
 Validators with stake amount below <code>validator_low_stake_threshold</code> are considered to
 have low stake and will be escorted out of the validator set after being below this
 threshold for more than <code>validator_low_stake_grace_period</code> number of epochs.
</dd>
<dt>
<code>validator_very_low_stake_threshold: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>
 Validators with stake below <code>validator_very_low_stake_threshold</code> will be removed
 immediately at epoch change, no grace period.
</dd>
<dt>
<code>validator_low_stake_grace_period: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>
 A validator can have stake below <code>validator_low_stake_threshold</code>
 for this many epochs before being kicked out.
</dd>
<dt>
<code>extra_fields: <a href="../sui-framework/bag.md#0x2_bag_Bag">bag::Bag</a></code>
</dt>
<dd>
 Any extra fields that's not defined statically.
</dd>
</dl>


</details>

<a name="0x3_sui_system_state_inner_SuiSystemStateInner"></a>

## Struct `SuiSystemStateInner`

The top-level object containing all information of the Sui system.


<pre><code><b>struct</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">SuiSystemStateInner</a> <b>has</b> store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>epoch: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>
 The current epoch ID, starting from 0.
</dd>
<dt>
<code>protocol_version: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>
 The current protocol version, starting from 1.
</dd>
<dt>
<code>system_state_version: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>
 The current version of the system state data structure type.
 This is always the same as SuiSystemState.version. Keeping a copy here so that
 we know what version it is by inspecting SuiSystemStateInner as well.
</dd>
<dt>
<code>validators: <a href="validator_set.md#0x3_validator_set_ValidatorSet">validator_set::ValidatorSet</a></code>
</dt>
<dd>
 Contains all information about the validators.
</dd>
<dt>
<code><a href="storage_fund.md#0x3_storage_fund">storage_fund</a>: <a href="storage_fund.md#0x3_storage_fund_StorageFund">storage_fund::StorageFund</a></code>
</dt>
<dd>
 The storage fund.
</dd>
<dt>
<code>parameters: <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SystemParameters">sui_system_state_inner::SystemParameters</a></code>
</dt>
<dd>
 A list of system config parameters.
</dd>
<dt>
<code>reference_gas_price: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>
 The reference gas price for the current epoch.
</dd>
<dt>
<code>validator_report_records: <a href="../sui-framework/vec_map.md#0x2_vec_map_VecMap">vec_map::VecMap</a>&lt;<b>address</b>, <a href="../sui-framework/vec_set.md#0x2_vec_set_VecSet">vec_set::VecSet</a>&lt;<b>address</b>&gt;&gt;</code>
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
<code><a href="stake_subsidy.md#0x3_stake_subsidy">stake_subsidy</a>: <a href="stake_subsidy.md#0x3_stake_subsidy_StakeSubsidy">stake_subsidy::StakeSubsidy</a></code>
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
 The rest of the fields starting with <code>safe_mode_</code> are accmulated during safe mode
 when advance_epoch_safe_mode is executed. They will eventually be processed once we
 are out of safe mode.
</dd>
<dt>
<code>safe_mode_storage_rewards: <a href="../sui-framework/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="../sui-framework/sui.md#0x2_sui_SUI">sui::SUI</a>&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>safe_mode_computation_rewards: <a href="../sui-framework/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="../sui-framework/sui.md#0x2_sui_SUI">sui::SUI</a>&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>safe_mode_storage_rebates: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>

</dd>
<dt>
<code>safe_mode_non_refundable_storage_fee: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>

</dd>
<dt>
<code>epoch_start_timestamp_ms: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>
 Unix timestamp of the current epoch start
</dd>
<dt>
<code>extra_fields: <a href="../sui-framework/bag.md#0x2_bag_Bag">bag::Bag</a></code>
</dt>
<dd>
 Any extra fields that's not defined statically.
</dd>
</dl>


</details>

<a name="0x3_sui_system_state_inner_SuiSystemStateInnerV2"></a>

## Struct `SuiSystemStateInnerV2`

Uses SystemParametersV2 as the parameters.


<pre><code><b>struct</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a> <b>has</b> store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>epoch: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>
 The current epoch ID, starting from 0.
</dd>
<dt>
<code>protocol_version: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>
 The current protocol version, starting from 1.
</dd>
<dt>
<code>system_state_version: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>
 The current version of the system state data structure type.
 This is always the same as SuiSystemState.version. Keeping a copy here so that
 we know what version it is by inspecting SuiSystemStateInner as well.
</dd>
<dt>
<code>validators: <a href="validator_set.md#0x3_validator_set_ValidatorSet">validator_set::ValidatorSet</a></code>
</dt>
<dd>
 Contains all information about the validators.
</dd>
<dt>
<code><a href="storage_fund.md#0x3_storage_fund">storage_fund</a>: <a href="storage_fund.md#0x3_storage_fund_StorageFund">storage_fund::StorageFund</a></code>
</dt>
<dd>
 The storage fund.
</dd>
<dt>
<code>parameters: <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SystemParametersV2">sui_system_state_inner::SystemParametersV2</a></code>
</dt>
<dd>
 A list of system config parameters.
</dd>
<dt>
<code>reference_gas_price: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>
 The reference gas price for the current epoch.
</dd>
<dt>
<code>validator_report_records: <a href="../sui-framework/vec_map.md#0x2_vec_map_VecMap">vec_map::VecMap</a>&lt;<b>address</b>, <a href="../sui-framework/vec_set.md#0x2_vec_set_VecSet">vec_set::VecSet</a>&lt;<b>address</b>&gt;&gt;</code>
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
<code><a href="stake_subsidy.md#0x3_stake_subsidy">stake_subsidy</a>: <a href="stake_subsidy.md#0x3_stake_subsidy_StakeSubsidy">stake_subsidy::StakeSubsidy</a></code>
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
 The rest of the fields starting with <code>safe_mode_</code> are accmulated during safe mode
 when advance_epoch_safe_mode is executed. They will eventually be processed once we
 are out of safe mode.
</dd>
<dt>
<code>safe_mode_storage_rewards: <a href="../sui-framework/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="../sui-framework/sui.md#0x2_sui_SUI">sui::SUI</a>&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>safe_mode_computation_rewards: <a href="../sui-framework/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="../sui-framework/sui.md#0x2_sui_SUI">sui::SUI</a>&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>safe_mode_storage_rebates: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>

</dd>
<dt>
<code>safe_mode_non_refundable_storage_fee: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>

</dd>
<dt>
<code>epoch_start_timestamp_ms: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>
 Unix timestamp of the current epoch start
</dd>
<dt>
<code>extra_fields: <a href="../sui-framework/bag.md#0x2_bag_Bag">bag::Bag</a></code>
</dt>
<dd>
 Any extra fields that's not defined statically.
</dd>
</dl>


</details>

<a name="0x3_sui_system_state_inner_SystemEpochInfoEvent"></a>

## Struct `SystemEpochInfoEvent`

Event containing system-level epoch information, emitted during
the epoch advancement transaction.


<pre><code><b>struct</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SystemEpochInfoEvent">SystemEpochInfoEvent</a> <b>has</b> <b>copy</b>, drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>epoch: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>

</dd>
<dt>
<code>protocol_version: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>

</dd>
<dt>
<code>reference_gas_price: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>

</dd>
<dt>
<code>total_stake: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>

</dd>
<dt>
<code>storage_fund_reinvestment: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>

</dd>
<dt>
<code>storage_charge: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>

</dd>
<dt>
<code>storage_rebate: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>

</dd>
<dt>
<code>storage_fund_balance: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>

</dd>
<dt>
<code>stake_subsidy_amount: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>

</dd>
<dt>
<code>total_gas_fees: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>

</dd>
<dt>
<code>total_stake_rewards_distributed: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>

</dd>
<dt>
<code>leftover_storage_fund_inflow: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0x3_sui_system_state_inner_ENotSystemAddress"></a>



<pre><code><b>const</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_ENotSystemAddress">ENotSystemAddress</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 2;
</code></pre>



<a name="0x3_sui_system_state_inner_ACTIVE_OR_PENDING_VALIDATOR"></a>



<pre><code><b>const</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_ACTIVE_OR_PENDING_VALIDATOR">ACTIVE_OR_PENDING_VALIDATOR</a>: u8 = 2;
</code></pre>



<a name="0x3_sui_system_state_inner_ACTIVE_VALIDATOR_ONLY"></a>



<pre><code><b>const</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_ACTIVE_VALIDATOR_ONLY">ACTIVE_VALIDATOR_ONLY</a>: u8 = 1;
</code></pre>



<a name="0x3_sui_system_state_inner_ANY_VALIDATOR"></a>



<pre><code><b>const</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_ANY_VALIDATOR">ANY_VALIDATOR</a>: u8 = 3;
</code></pre>



<a name="0x3_sui_system_state_inner_BASIS_POINT_DENOMINATOR"></a>



<pre><code><b>const</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_BASIS_POINT_DENOMINATOR">BASIS_POINT_DENOMINATOR</a>: u128 = 10000;
</code></pre>



<a name="0x3_sui_system_state_inner_EAdvancedToWrongEpoch"></a>



<pre><code><b>const</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_EAdvancedToWrongEpoch">EAdvancedToWrongEpoch</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 8;
</code></pre>



<a name="0x3_sui_system_state_inner_EBpsTooLarge"></a>



<pre><code><b>const</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_EBpsTooLarge">EBpsTooLarge</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 5;
</code></pre>



<a name="0x3_sui_system_state_inner_ECannotReportOneself"></a>



<pre><code><b>const</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_ECannotReportOneself">ECannotReportOneself</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 3;
</code></pre>



<a name="0x3_sui_system_state_inner_ELimitExceeded"></a>



<pre><code><b>const</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_ELimitExceeded">ELimitExceeded</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 1;
</code></pre>



<a name="0x3_sui_system_state_inner_ENotValidator"></a>



<pre><code><b>const</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_ENotValidator">ENotValidator</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 0;
</code></pre>



<a name="0x3_sui_system_state_inner_EReportRecordNotFound"></a>



<pre><code><b>const</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_EReportRecordNotFound">EReportRecordNotFound</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 4;
</code></pre>



<a name="0x3_sui_system_state_inner_ESafeModeGasNotProcessed"></a>



<pre><code><b>const</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_ESafeModeGasNotProcessed">ESafeModeGasNotProcessed</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 7;
</code></pre>



<a name="0x3_sui_system_state_inner_EStakeWithdrawBeforeActivation"></a>



<pre><code><b>const</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_EStakeWithdrawBeforeActivation">EStakeWithdrawBeforeActivation</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 6;
</code></pre>



<a name="0x3_sui_system_state_inner_SYSTEM_STATE_VERSION_V1"></a>



<pre><code><b>const</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SYSTEM_STATE_VERSION_V1">SYSTEM_STATE_VERSION_V1</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 1;
</code></pre>



<a name="0x3_sui_system_state_inner_create"></a>

## Function `create`

Create a new SuiSystemState object and make it shared.
This function will be called only once in genesis.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_create">create</a>(validators: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="validator.md#0x3_validator_Validator">validator::Validator</a>&gt;, initial_storage_fund: <a href="../sui-framework/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="../sui-framework/sui.md#0x2_sui_SUI">sui::SUI</a>&gt;, protocol_version: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>, epoch_start_timestamp_ms: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>, parameters: <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SystemParameters">sui_system_state_inner::SystemParameters</a>, <a href="stake_subsidy.md#0x3_stake_subsidy">stake_subsidy</a>: <a href="stake_subsidy.md#0x3_stake_subsidy_StakeSubsidy">stake_subsidy::StakeSubsidy</a>, ctx: &<b>mut</b> <a href="../sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">sui_system_state_inner::SuiSystemStateInner</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_create">create</a>(
    validators: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;Validator&gt;,
    initial_storage_fund: Balance&lt;SUI&gt;,
    protocol_version: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>,
    epoch_start_timestamp_ms: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>,
    parameters: <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SystemParameters">SystemParameters</a>,
    <a href="stake_subsidy.md#0x3_stake_subsidy">stake_subsidy</a>: StakeSubsidy,
    ctx: &<b>mut</b> TxContext,
): <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">SuiSystemStateInner</a> {
    <b>let</b> validators = <a href="validator_set.md#0x3_validator_set_new">validator_set::new</a>(validators, ctx);
    <b>let</b> reference_gas_price = validators.derive_reference_gas_price();
    // This type is fixed <b>as</b> it's created at <a href="genesis.md#0x3_genesis">genesis</a>. It should not be updated during type upgrade.
    <b>let</b> system_state = <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">SuiSystemStateInner</a> {
        epoch: 0,
        protocol_version,
        system_state_version: <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_genesis_system_state_version">genesis_system_state_version</a>(),
        validators,
        <a href="storage_fund.md#0x3_storage_fund">storage_fund</a>: <a href="storage_fund.md#0x3_storage_fund_new">storage_fund::new</a>(initial_storage_fund),
        parameters,
        reference_gas_price,
        validator_report_records: <a href="../sui-framework/vec_map.md#0x2_vec_map_empty">vec_map::empty</a>(),
        <a href="stake_subsidy.md#0x3_stake_subsidy">stake_subsidy</a>,
        safe_mode: <b>false</b>,
        safe_mode_storage_rewards: <a href="../sui-framework/balance.md#0x2_balance_zero">balance::zero</a>(),
        safe_mode_computation_rewards: <a href="../sui-framework/balance.md#0x2_balance_zero">balance::zero</a>(),
        safe_mode_storage_rebates: 0,
        safe_mode_non_refundable_storage_fee: 0,
        epoch_start_timestamp_ms,
        extra_fields: <a href="../sui-framework/bag.md#0x2_bag_new">bag::new</a>(ctx),
    };
    system_state
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_create_system_parameters"></a>

## Function `create_system_parameters`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_create_system_parameters">create_system_parameters</a>(epoch_duration_ms: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>, stake_subsidy_start_epoch: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>, max_validator_count: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>, min_validator_joining_stake: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>, validator_low_stake_threshold: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>, validator_very_low_stake_threshold: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>, validator_low_stake_grace_period: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>, ctx: &<b>mut</b> <a href="../sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SystemParameters">sui_system_state_inner::SystemParameters</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_create_system_parameters">create_system_parameters</a>(
    epoch_duration_ms: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>,
    stake_subsidy_start_epoch: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>,

    // Validator committee parameters
    max_validator_count: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>,
    min_validator_joining_stake: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>,
    validator_low_stake_threshold: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>,
    validator_very_low_stake_threshold: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>,
    validator_low_stake_grace_period: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>,
    ctx: &<b>mut</b> TxContext,
): <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SystemParameters">SystemParameters</a> {
    <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SystemParameters">SystemParameters</a> {
        epoch_duration_ms,
        stake_subsidy_start_epoch,
        max_validator_count,
        min_validator_joining_stake,
        validator_low_stake_threshold,
        validator_very_low_stake_threshold,
        validator_low_stake_grace_period,
        extra_fields: <a href="../sui-framework/bag.md#0x2_bag_new">bag::new</a>(ctx),
    }
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_v1_to_v2"></a>

## Function `v1_to_v2`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_v1_to_v2">v1_to_v2</a>(self: <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">sui_system_state_inner::SuiSystemStateInner</a>): <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">sui_system_state_inner::SuiSystemStateInnerV2</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_v1_to_v2">v1_to_v2</a>(self: <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">SuiSystemStateInner</a>): <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a> {
    <b>let</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">SuiSystemStateInner</a> {
        epoch,
        protocol_version,
        system_state_version: _,
        validators,
        <a href="storage_fund.md#0x3_storage_fund">storage_fund</a>,
        parameters,
        reference_gas_price,
        validator_report_records,
        <a href="stake_subsidy.md#0x3_stake_subsidy">stake_subsidy</a>,
        safe_mode,
        safe_mode_storage_rewards,
        safe_mode_computation_rewards,
        safe_mode_storage_rebates,
        safe_mode_non_refundable_storage_fee,
        epoch_start_timestamp_ms,
        extra_fields: state_extra_fields,
    } = self;
    <b>let</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SystemParameters">SystemParameters</a> {
        epoch_duration_ms,
        stake_subsidy_start_epoch,
        max_validator_count,
        min_validator_joining_stake,
        validator_low_stake_threshold,
        validator_very_low_stake_threshold,
        validator_low_stake_grace_period,
        extra_fields: param_extra_fields,
    } = parameters;
    <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a> {
        epoch,
        protocol_version,
        system_state_version: 2,
        validators,
        <a href="storage_fund.md#0x3_storage_fund">storage_fund</a>,
        parameters: <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SystemParametersV2">SystemParametersV2</a> {
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
        <a href="stake_subsidy.md#0x3_stake_subsidy">stake_subsidy</a>,
        safe_mode,
        safe_mode_storage_rewards,
        safe_mode_computation_rewards,
        safe_mode_storage_rebates,
        safe_mode_non_refundable_storage_fee,
        epoch_start_timestamp_ms,
        extra_fields: state_extra_fields
    }
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_request_add_validator_candidate"></a>

## Function `request_add_validator_candidate`

Can be called by anyone who wishes to become a validator candidate and starts accuring delegated
stakes in their staking pool. Once they have at least <code>MIN_VALIDATOR_JOINING_STAKE</code> amount of stake they
can call <code>request_add_validator</code> to officially become an active validator at the next epoch.
Aborts if the caller is already a pending or active validator, or a validator candidate.
Note: <code>proof_of_possession</code> MUST be a valid signature using sui_address and protocol_pubkey_bytes.
To produce a valid PoP, run [fn test_proof_of_possession].


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_request_add_validator_candidate">request_add_validator_candidate</a>(self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">sui_system_state_inner::SuiSystemStateInnerV2</a>, pubkey_bytes: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, network_pubkey_bytes: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, worker_pubkey_bytes: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, proof_of_possession: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, name: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, description: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, image_url: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, project_url: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, net_address: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, p2p_address: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, primary_address: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, worker_address: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, gas_price: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>, commission_rate: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>, ctx: &<b>mut</b> <a href="../sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_request_add_validator_candidate">request_add_validator_candidate</a>(
    self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>,
    pubkey_bytes: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    network_pubkey_bytes: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    worker_pubkey_bytes: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    proof_of_possession: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    name: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    description: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    image_url: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    project_url: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    net_address: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    p2p_address: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    primary_address: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    worker_address: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    gas_price: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>,
    commission_rate: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> <a href="validator.md#0x3_validator">validator</a> = <a href="validator.md#0x3_validator_new">validator::new</a>(
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

    self.validators.<a href="sui_system_state_inner.md#0x3_sui_system_state_inner_request_add_validator_candidate">request_add_validator_candidate</a>(<a href="validator.md#0x3_validator">validator</a>, ctx);
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_request_remove_validator_candidate"></a>

## Function `request_remove_validator_candidate`

Called by a validator candidate to remove themselves from the candidacy. After this call
their staking pool becomes deactivate.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_request_remove_validator_candidate">request_remove_validator_candidate</a>(self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">sui_system_state_inner::SuiSystemStateInnerV2</a>, ctx: &<b>mut</b> <a href="../sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_request_remove_validator_candidate">request_remove_validator_candidate</a>(
    self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>,
    ctx: &<b>mut</b> TxContext,
) {
    self.validators.<a href="sui_system_state_inner.md#0x3_sui_system_state_inner_request_remove_validator_candidate">request_remove_validator_candidate</a>(ctx);
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_request_add_validator"></a>

## Function `request_add_validator`

Called by a validator candidate to add themselves to the active validator set beginning next epoch.
Aborts if the validator is a duplicate with one of the pending or active validators, or if the amount of
stake the validator has doesn't meet the min threshold, or if the number of new validators for the next
epoch has already reached the maximum.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_request_add_validator">request_add_validator</a>(self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">sui_system_state_inner::SuiSystemStateInnerV2</a>, ctx: &<a href="../sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_request_add_validator">request_add_validator</a>(
    self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>,
    ctx: &TxContext,
) {
    <b>assert</b>!(
        self.validators.next_epoch_validator_count() &lt; self.parameters.max_validator_count,
        <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_ELimitExceeded">ELimitExceeded</a>,
    );

    self.validators.<a href="sui_system_state_inner.md#0x3_sui_system_state_inner_request_add_validator">request_add_validator</a>(self.parameters.min_validator_joining_stake, ctx);
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_request_remove_validator"></a>

## Function `request_remove_validator`

A validator can call this function to request a removal in the next epoch.
We use the sender of <code>ctx</code> to look up the validator
(i.e. sender must match the sui_address in the validator).
At the end of the epoch, the <code><a href="validator.md#0x3_validator">validator</a></code> object will be returned to the sui_address
of the validator.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_request_remove_validator">request_remove_validator</a>(self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">sui_system_state_inner::SuiSystemStateInnerV2</a>, ctx: &<a href="../sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_request_remove_validator">request_remove_validator</a>(
    self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>,
    ctx: &TxContext,
) {
    // Only check <b>min</b> <a href="validator.md#0x3_validator">validator</a> condition <b>if</b> the current number of validators satisfy the constraint.
    // This is so that <b>if</b> we somehow already are in a state <b>where</b> we have less than <b>min</b> validators, it no longer matters
    // and is ok <b>to</b> stay so. This is useful for a test setup.
    <b>if</b> (self.validators.active_validators().length() &gt;= self.parameters.min_validator_count) {
        <b>assert</b>!(
            self.validators.next_epoch_validator_count() &gt; self.parameters.min_validator_count,
            <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_ELimitExceeded">ELimitExceeded</a>,
        );
    };

    self.validators.<a href="sui_system_state_inner.md#0x3_sui_system_state_inner_request_remove_validator">request_remove_validator</a>(ctx)
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_request_set_gas_price"></a>

## Function `request_set_gas_price`

A validator can call this function to submit a new gas price quote, to be
used for the reference gas price calculation at the end of the epoch.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_request_set_gas_price">request_set_gas_price</a>(self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">sui_system_state_inner::SuiSystemStateInnerV2</a>, cap: &<a href="validator_cap.md#0x3_validator_cap_UnverifiedValidatorOperationCap">validator_cap::UnverifiedValidatorOperationCap</a>, new_gas_price: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_request_set_gas_price">request_set_gas_price</a>(
    self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>,
    cap: &UnverifiedValidatorOperationCap,
    new_gas_price: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>,
) {
    // Verify the represented <b>address</b> is an active or pending <a href="validator.md#0x3_validator">validator</a>, and the capability is still valid.
    <b>let</b> verified_cap = self.validators.verify_cap(cap, <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_ACTIVE_OR_PENDING_VALIDATOR">ACTIVE_OR_PENDING_VALIDATOR</a>);
    <b>let</b> <a href="validator.md#0x3_validator">validator</a> = self.validators.get_validator_mut_with_verified_cap(&verified_cap, <b>false</b> /* include_candidate */);

    <a href="validator.md#0x3_validator">validator</a>.<a href="sui_system_state_inner.md#0x3_sui_system_state_inner_request_set_gas_price">request_set_gas_price</a>(verified_cap, new_gas_price);
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_set_candidate_validator_gas_price"></a>

## Function `set_candidate_validator_gas_price`

This function is used to set new gas price for candidate validators


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_set_candidate_validator_gas_price">set_candidate_validator_gas_price</a>(self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">sui_system_state_inner::SuiSystemStateInnerV2</a>, cap: &<a href="validator_cap.md#0x3_validator_cap_UnverifiedValidatorOperationCap">validator_cap::UnverifiedValidatorOperationCap</a>, new_gas_price: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_set_candidate_validator_gas_price">set_candidate_validator_gas_price</a>(
    self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>,
    cap: &UnverifiedValidatorOperationCap,
    new_gas_price: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>,
) {
    // Verify the represented <b>address</b> is an active or pending <a href="validator.md#0x3_validator">validator</a>, and the capability is still valid.
    <b>let</b> verified_cap = self.validators.verify_cap(cap, <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_ANY_VALIDATOR">ANY_VALIDATOR</a>);
    <b>let</b> candidate = self.validators.get_validator_mut_with_verified_cap(&verified_cap, <b>true</b> /* include_candidate */);
    candidate.set_candidate_gas_price(verified_cap, new_gas_price)
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_request_set_commission_rate"></a>

## Function `request_set_commission_rate`

A validator can call this function to set a new commission rate, updated at the end of
the epoch.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_request_set_commission_rate">request_set_commission_rate</a>(self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">sui_system_state_inner::SuiSystemStateInnerV2</a>, new_commission_rate: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>, ctx: &<a href="../sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_request_set_commission_rate">request_set_commission_rate</a>(
    self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>,
    new_commission_rate: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>,
    ctx: &TxContext,
) {
    self.validators.<a href="sui_system_state_inner.md#0x3_sui_system_state_inner_request_set_commission_rate">request_set_commission_rate</a>(
        new_commission_rate,
        ctx
    )
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_set_candidate_validator_commission_rate"></a>

## Function `set_candidate_validator_commission_rate`

This function is used to set new commission rate for candidate validators


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_set_candidate_validator_commission_rate">set_candidate_validator_commission_rate</a>(self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">sui_system_state_inner::SuiSystemStateInnerV2</a>, new_commission_rate: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>, ctx: &<a href="../sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_set_candidate_validator_commission_rate">set_candidate_validator_commission_rate</a>(
    self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>,
    new_commission_rate: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>,
    ctx: &TxContext,
) {
    <b>let</b> candidate = self.validators.get_validator_mut_with_ctx_including_candidates(ctx);
    candidate.set_candidate_commission_rate(new_commission_rate)
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_request_add_stake"></a>

## Function `request_add_stake`

Add stake to a validator's staking pool.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_request_add_stake">request_add_stake</a>(self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">sui_system_state_inner::SuiSystemStateInnerV2</a>, stake: <a href="../sui-framework/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;<a href="../sui-framework/sui.md#0x2_sui_SUI">sui::SUI</a>&gt;, validator_address: <b>address</b>, ctx: &<b>mut</b> <a href="../sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="staking_pool.md#0x3_staking_pool_StakedSui">staking_pool::StakedSui</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_request_add_stake">request_add_stake</a>(
    self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>,
    stake: Coin&lt;SUI&gt;,
    validator_address: <b>address</b>,
    ctx: &<b>mut</b> TxContext,
) : StakedSui {
    self.validators.<a href="sui_system_state_inner.md#0x3_sui_system_state_inner_request_add_stake">request_add_stake</a>(
        validator_address,
        stake.into_balance(),
        ctx,
    )
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_request_add_stake_mul_coin"></a>

## Function `request_add_stake_mul_coin`

Add stake to a validator's staking pool using multiple coins.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_request_add_stake_mul_coin">request_add_stake_mul_coin</a>(self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">sui_system_state_inner::SuiSystemStateInnerV2</a>, stakes: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="../sui-framework/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;<a href="../sui-framework/sui.md#0x2_sui_SUI">sui::SUI</a>&gt;&gt;, stake_amount: <a href="../move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;<a href="../move-stdlib/u64.md#0x1_u64">u64</a>&gt;, validator_address: <b>address</b>, ctx: &<b>mut</b> <a href="../sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="staking_pool.md#0x3_staking_pool_StakedSui">staking_pool::StakedSui</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_request_add_stake_mul_coin">request_add_stake_mul_coin</a>(
    self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>,
    stakes: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;Coin&lt;SUI&gt;&gt;,
    stake_amount: <a href="../move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;<a href="../move-stdlib/u64.md#0x1_u64">u64</a>&gt;,
    validator_address: <b>address</b>,
    ctx: &<b>mut</b> TxContext,
) : StakedSui {
    <b>let</b> <a href="../sui-framework/balance.md#0x2_balance">balance</a> = <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_extract_coin_balance">extract_coin_balance</a>(stakes, stake_amount, ctx);
    self.validators.<a href="sui_system_state_inner.md#0x3_sui_system_state_inner_request_add_stake">request_add_stake</a>(validator_address, <a href="../sui-framework/balance.md#0x2_balance">balance</a>, ctx)
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_request_withdraw_stake"></a>

## Function `request_withdraw_stake`

Withdraw some portion of a stake from a validator's staking pool.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_request_withdraw_stake">request_withdraw_stake</a>(self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">sui_system_state_inner::SuiSystemStateInnerV2</a>, staked_sui: <a href="staking_pool.md#0x3_staking_pool_StakedSui">staking_pool::StakedSui</a>, ctx: &<a href="../sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="../sui-framework/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="../sui-framework/sui.md#0x2_sui_SUI">sui::SUI</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_request_withdraw_stake">request_withdraw_stake</a>(
    self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>,
    staked_sui: StakedSui,
    ctx: &TxContext,
) : Balance&lt;SUI&gt; {
    <b>assert</b>!(
        stake_activation_epoch(&staked_sui) &lt;= ctx.<a href="sui_system_state_inner.md#0x3_sui_system_state_inner_epoch">epoch</a>(),
        <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_EStakeWithdrawBeforeActivation">EStakeWithdrawBeforeActivation</a>
    );
    self.validators.<a href="sui_system_state_inner.md#0x3_sui_system_state_inner_request_withdraw_stake">request_withdraw_stake</a>(staked_sui, ctx)
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_report_validator"></a>

## Function `report_validator`

Report a validator as a bad or non-performant actor in the system.
Succeeds if all the following are satisfied:
1. both the reporter in <code>cap</code> and the input <code>reportee_addr</code> are active validators.
2. reporter and reportee not the same address.
3. the cap object is still valid.
This function is idempotent.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_report_validator">report_validator</a>(self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">sui_system_state_inner::SuiSystemStateInnerV2</a>, cap: &<a href="validator_cap.md#0x3_validator_cap_UnverifiedValidatorOperationCap">validator_cap::UnverifiedValidatorOperationCap</a>, reportee_addr: <b>address</b>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_report_validator">report_validator</a>(
    self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>,
    cap: &UnverifiedValidatorOperationCap,
    reportee_addr: <b>address</b>,
) {
    // Reportee needs <b>to</b> be an active <a href="validator.md#0x3_validator">validator</a>
    <b>assert</b>!(self.validators.is_active_validator_by_sui_address(reportee_addr), <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_ENotValidator">ENotValidator</a>);
    // Verify the represented reporter <b>address</b> is an active <a href="validator.md#0x3_validator">validator</a>, and the capability is still valid.
    <b>let</b> verified_cap = self.validators.verify_cap(cap, <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_ACTIVE_VALIDATOR_ONLY">ACTIVE_VALIDATOR_ONLY</a>);
    <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_report_validator_impl">report_validator_impl</a>(verified_cap, reportee_addr, &<b>mut</b> self.validator_report_records);
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_undo_report_validator"></a>

## Function `undo_report_validator`

Undo a <code>report_validator</code> action. Aborts if
1. the reportee is not a currently active validator or
2. the sender has not previously reported the <code>reportee_addr</code>, or
3. the cap is not valid


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_undo_report_validator">undo_report_validator</a>(self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">sui_system_state_inner::SuiSystemStateInnerV2</a>, cap: &<a href="validator_cap.md#0x3_validator_cap_UnverifiedValidatorOperationCap">validator_cap::UnverifiedValidatorOperationCap</a>, reportee_addr: <b>address</b>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_undo_report_validator">undo_report_validator</a>(
    self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>,
    cap: &UnverifiedValidatorOperationCap,
    reportee_addr: <b>address</b>,
) {
    <b>let</b> verified_cap = self.validators.verify_cap(cap, <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_ACTIVE_VALIDATOR_ONLY">ACTIVE_VALIDATOR_ONLY</a>);
    <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_undo_report_validator_impl">undo_report_validator_impl</a>(verified_cap, reportee_addr, &<b>mut</b> self.validator_report_records);
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_report_validator_impl"></a>

## Function `report_validator_impl`



<pre><code><b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_report_validator_impl">report_validator_impl</a>(verified_cap: <a href="validator_cap.md#0x3_validator_cap_ValidatorOperationCap">validator_cap::ValidatorOperationCap</a>, reportee_addr: <b>address</b>, validator_report_records: &<b>mut</b> <a href="../sui-framework/vec_map.md#0x2_vec_map_VecMap">vec_map::VecMap</a>&lt;<b>address</b>, <a href="../sui-framework/vec_set.md#0x2_vec_set_VecSet">vec_set::VecSet</a>&lt;<b>address</b>&gt;&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_report_validator_impl">report_validator_impl</a>(
    verified_cap: ValidatorOperationCap,
    reportee_addr: <b>address</b>,
    validator_report_records: &<b>mut</b> VecMap&lt;<b>address</b>, VecSet&lt;<b>address</b>&gt;&gt;,
) {
    <b>let</b> reporter_address = *verified_cap.verified_operation_cap_address();
    <b>assert</b>!(reporter_address != reportee_addr, <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_ECannotReportOneself">ECannotReportOneself</a>);
    <b>if</b> (!validator_report_records.contains(&reportee_addr)) {
        validator_report_records.insert(reportee_addr, <a href="../sui-framework/vec_set.md#0x2_vec_set_singleton">vec_set::singleton</a>(reporter_address));
    } <b>else</b> {
        <b>let</b> reporters = validator_report_records.get_mut(&reportee_addr);
        <b>if</b> (!reporters.contains(&reporter_address)) {
            reporters.insert(reporter_address);
        }
    }
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_undo_report_validator_impl"></a>

## Function `undo_report_validator_impl`



<pre><code><b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_undo_report_validator_impl">undo_report_validator_impl</a>(verified_cap: <a href="validator_cap.md#0x3_validator_cap_ValidatorOperationCap">validator_cap::ValidatorOperationCap</a>, reportee_addr: <b>address</b>, validator_report_records: &<b>mut</b> <a href="../sui-framework/vec_map.md#0x2_vec_map_VecMap">vec_map::VecMap</a>&lt;<b>address</b>, <a href="../sui-framework/vec_set.md#0x2_vec_set_VecSet">vec_set::VecSet</a>&lt;<b>address</b>&gt;&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_undo_report_validator_impl">undo_report_validator_impl</a>(
    verified_cap: ValidatorOperationCap,
    reportee_addr: <b>address</b>,
    validator_report_records: &<b>mut</b> VecMap&lt;<b>address</b>, VecSet&lt;<b>address</b>&gt;&gt;,
) {
    <b>assert</b>!(validator_report_records.contains(&reportee_addr), <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_EReportRecordNotFound">EReportRecordNotFound</a>);
    <b>let</b> reporters = validator_report_records.get_mut(&reportee_addr);

    <b>let</b> reporter_addr = *verified_cap.verified_operation_cap_address();
    <b>assert</b>!(reporters.contains(&reporter_addr), <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_EReportRecordNotFound">EReportRecordNotFound</a>);

    reporters.remove(&reporter_addr);
    <b>if</b> (reporters.is_empty()) {
        validator_report_records.remove(&reportee_addr);
    }
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_rotate_operation_cap"></a>

## Function `rotate_operation_cap`

Create a new <code>UnverifiedValidatorOperationCap</code>, transfer it to the
validator and registers it. The original object is thus revoked.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_rotate_operation_cap">rotate_operation_cap</a>(self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">sui_system_state_inner::SuiSystemStateInnerV2</a>, ctx: &<b>mut</b> <a href="../sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_rotate_operation_cap">rotate_operation_cap</a>(
    self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> <a href="validator.md#0x3_validator">validator</a> = self.validators.get_validator_mut_with_ctx_including_candidates(ctx);
    <a href="validator.md#0x3_validator">validator</a>.new_unverified_validator_operation_cap_and_transfer(ctx);
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_update_validator_name"></a>

## Function `update_validator_name`

Update a validator's name.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_validator_name">update_validator_name</a>(self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">sui_system_state_inner::SuiSystemStateInnerV2</a>, name: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, ctx: &<a href="../sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_validator_name">update_validator_name</a>(
    self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>,
    name: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> <a href="validator.md#0x3_validator">validator</a> = self.validators.get_validator_mut_with_ctx_including_candidates(ctx);

    <a href="validator.md#0x3_validator">validator</a>.update_name(name);
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_update_validator_description"></a>

## Function `update_validator_description`

Update a validator's description


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_validator_description">update_validator_description</a>(self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">sui_system_state_inner::SuiSystemStateInnerV2</a>, description: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, ctx: &<a href="../sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_validator_description">update_validator_description</a>(
    self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>,
    description: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> <a href="validator.md#0x3_validator">validator</a> = self.validators.get_validator_mut_with_ctx_including_candidates(ctx);
    <a href="validator.md#0x3_validator">validator</a>.update_description(description);
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_update_validator_image_url"></a>

## Function `update_validator_image_url`

Update a validator's image url


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_validator_image_url">update_validator_image_url</a>(self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">sui_system_state_inner::SuiSystemStateInnerV2</a>, image_url: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, ctx: &<a href="../sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_validator_image_url">update_validator_image_url</a>(
    self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>,
    image_url: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> <a href="validator.md#0x3_validator">validator</a> = self.validators.get_validator_mut_with_ctx_including_candidates(ctx);
    <a href="validator.md#0x3_validator">validator</a>.update_image_url(image_url);
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_update_validator_project_url"></a>

## Function `update_validator_project_url`

Update a validator's project url


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_validator_project_url">update_validator_project_url</a>(self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">sui_system_state_inner::SuiSystemStateInnerV2</a>, project_url: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, ctx: &<a href="../sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_validator_project_url">update_validator_project_url</a>(
    self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>,
    project_url: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> <a href="validator.md#0x3_validator">validator</a> = self.validators.get_validator_mut_with_ctx_including_candidates(ctx);
    <a href="validator.md#0x3_validator">validator</a>.update_project_url(project_url);
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_update_validator_next_epoch_network_address"></a>

## Function `update_validator_next_epoch_network_address`

Update a validator's network address.
The change will only take effects starting from the next epoch.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_validator_next_epoch_network_address">update_validator_next_epoch_network_address</a>(self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">sui_system_state_inner::SuiSystemStateInnerV2</a>, network_address: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, ctx: &<a href="../sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_validator_next_epoch_network_address">update_validator_next_epoch_network_address</a>(
    self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>,
    network_address: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> <a href="validator.md#0x3_validator">validator</a> = self.validators.get_validator_mut_with_ctx(ctx);
    <a href="validator.md#0x3_validator">validator</a>.update_next_epoch_network_address(network_address);
    <b>let</b> <a href="validator.md#0x3_validator">validator</a> :&Validator = <a href="validator.md#0x3_validator">validator</a>; // Force immutability for the following call
    self.validators.assert_no_pending_or_active_duplicates(<a href="validator.md#0x3_validator">validator</a>);
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_update_candidate_validator_network_address"></a>

## Function `update_candidate_validator_network_address`

Update candidate validator's network address.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_candidate_validator_network_address">update_candidate_validator_network_address</a>(self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">sui_system_state_inner::SuiSystemStateInnerV2</a>, network_address: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, ctx: &<a href="../sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_candidate_validator_network_address">update_candidate_validator_network_address</a>(
    self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>,
    network_address: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> candidate = self.validators.get_validator_mut_with_ctx_including_candidates(ctx);
    candidate.update_candidate_network_address(network_address);
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_update_validator_next_epoch_p2p_address"></a>

## Function `update_validator_next_epoch_p2p_address`

Update a validator's p2p address.
The change will only take effects starting from the next epoch.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_validator_next_epoch_p2p_address">update_validator_next_epoch_p2p_address</a>(self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">sui_system_state_inner::SuiSystemStateInnerV2</a>, p2p_address: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, ctx: &<a href="../sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_validator_next_epoch_p2p_address">update_validator_next_epoch_p2p_address</a>(
    self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>,
    p2p_address: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> <a href="validator.md#0x3_validator">validator</a> = self.validators.get_validator_mut_with_ctx(ctx);
    <a href="validator.md#0x3_validator">validator</a>.update_next_epoch_p2p_address(p2p_address);
    <b>let</b> <a href="validator.md#0x3_validator">validator</a> :&Validator = <a href="validator.md#0x3_validator">validator</a>; // Force immutability for the following call
    self.validators.assert_no_pending_or_active_duplicates(<a href="validator.md#0x3_validator">validator</a>);
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_update_candidate_validator_p2p_address"></a>

## Function `update_candidate_validator_p2p_address`

Update candidate validator's p2p address.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_candidate_validator_p2p_address">update_candidate_validator_p2p_address</a>(self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">sui_system_state_inner::SuiSystemStateInnerV2</a>, p2p_address: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, ctx: &<a href="../sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_candidate_validator_p2p_address">update_candidate_validator_p2p_address</a>(
    self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>,
    p2p_address: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> candidate = self.validators.get_validator_mut_with_ctx_including_candidates(ctx);
    candidate.update_candidate_p2p_address(p2p_address);
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_update_validator_next_epoch_primary_address"></a>

## Function `update_validator_next_epoch_primary_address`

Update a validator's narwhal primary address.
The change will only take effects starting from the next epoch.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_validator_next_epoch_primary_address">update_validator_next_epoch_primary_address</a>(self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">sui_system_state_inner::SuiSystemStateInnerV2</a>, primary_address: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, ctx: &<a href="../sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_validator_next_epoch_primary_address">update_validator_next_epoch_primary_address</a>(
    self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>,
    primary_address: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> <a href="validator.md#0x3_validator">validator</a> = self.validators.get_validator_mut_with_ctx(ctx);
    <a href="validator.md#0x3_validator">validator</a>.update_next_epoch_primary_address(primary_address);
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_update_candidate_validator_primary_address"></a>

## Function `update_candidate_validator_primary_address`

Update candidate validator's narwhal primary address.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_candidate_validator_primary_address">update_candidate_validator_primary_address</a>(self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">sui_system_state_inner::SuiSystemStateInnerV2</a>, primary_address: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, ctx: &<a href="../sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_candidate_validator_primary_address">update_candidate_validator_primary_address</a>(
    self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>,
    primary_address: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> candidate = self.validators.get_validator_mut_with_ctx_including_candidates(ctx);
    candidate.update_candidate_primary_address(primary_address);
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_update_validator_next_epoch_worker_address"></a>

## Function `update_validator_next_epoch_worker_address`

Update a validator's narwhal worker address.
The change will only take effects starting from the next epoch.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_validator_next_epoch_worker_address">update_validator_next_epoch_worker_address</a>(self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">sui_system_state_inner::SuiSystemStateInnerV2</a>, worker_address: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, ctx: &<a href="../sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_validator_next_epoch_worker_address">update_validator_next_epoch_worker_address</a>(
    self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>,
    worker_address: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> <a href="validator.md#0x3_validator">validator</a> = self.validators.get_validator_mut_with_ctx(ctx);
    <a href="validator.md#0x3_validator">validator</a>.update_next_epoch_worker_address(worker_address);
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_update_candidate_validator_worker_address"></a>

## Function `update_candidate_validator_worker_address`

Update candidate validator's narwhal worker address.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_candidate_validator_worker_address">update_candidate_validator_worker_address</a>(self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">sui_system_state_inner::SuiSystemStateInnerV2</a>, worker_address: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, ctx: &<a href="../sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_candidate_validator_worker_address">update_candidate_validator_worker_address</a>(
    self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>,
    worker_address: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> candidate = self.validators.get_validator_mut_with_ctx_including_candidates(ctx);
    candidate.update_candidate_worker_address(worker_address);
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_update_validator_next_epoch_protocol_pubkey"></a>

## Function `update_validator_next_epoch_protocol_pubkey`

Update a validator's public key of protocol key and proof of possession.
The change will only take effects starting from the next epoch.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_validator_next_epoch_protocol_pubkey">update_validator_next_epoch_protocol_pubkey</a>(self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">sui_system_state_inner::SuiSystemStateInnerV2</a>, protocol_pubkey: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, proof_of_possession: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, ctx: &<a href="../sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_validator_next_epoch_protocol_pubkey">update_validator_next_epoch_protocol_pubkey</a>(
    self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>,
    protocol_pubkey: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    proof_of_possession: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> <a href="validator.md#0x3_validator">validator</a> = self.validators.get_validator_mut_with_ctx(ctx);
    <a href="validator.md#0x3_validator">validator</a>.update_next_epoch_protocol_pubkey(protocol_pubkey, proof_of_possession);
    <b>let</b> <a href="validator.md#0x3_validator">validator</a> :&Validator = <a href="validator.md#0x3_validator">validator</a>; // Force immutability for the following call
    self.validators.assert_no_pending_or_active_duplicates(<a href="validator.md#0x3_validator">validator</a>);
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_update_candidate_validator_protocol_pubkey"></a>

## Function `update_candidate_validator_protocol_pubkey`

Update candidate validator's public key of protocol key and proof of possession.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_candidate_validator_protocol_pubkey">update_candidate_validator_protocol_pubkey</a>(self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">sui_system_state_inner::SuiSystemStateInnerV2</a>, protocol_pubkey: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, proof_of_possession: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, ctx: &<a href="../sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_candidate_validator_protocol_pubkey">update_candidate_validator_protocol_pubkey</a>(
    self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>,
    protocol_pubkey: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    proof_of_possession: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> candidate = self.validators.get_validator_mut_with_ctx_including_candidates(ctx);
    candidate.update_candidate_protocol_pubkey(protocol_pubkey, proof_of_possession);
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_update_validator_next_epoch_worker_pubkey"></a>

## Function `update_validator_next_epoch_worker_pubkey`

Update a validator's public key of worker key.
The change will only take effects starting from the next epoch.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_validator_next_epoch_worker_pubkey">update_validator_next_epoch_worker_pubkey</a>(self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">sui_system_state_inner::SuiSystemStateInnerV2</a>, worker_pubkey: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, ctx: &<a href="../sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_validator_next_epoch_worker_pubkey">update_validator_next_epoch_worker_pubkey</a>(
    self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>,
    worker_pubkey: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> <a href="validator.md#0x3_validator">validator</a> = self.validators.get_validator_mut_with_ctx(ctx);
    <a href="validator.md#0x3_validator">validator</a>.update_next_epoch_worker_pubkey(worker_pubkey);
    <b>let</b> <a href="validator.md#0x3_validator">validator</a> :&Validator = <a href="validator.md#0x3_validator">validator</a>; // Force immutability for the following call
    self.validators.assert_no_pending_or_active_duplicates(<a href="validator.md#0x3_validator">validator</a>);
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_update_candidate_validator_worker_pubkey"></a>

## Function `update_candidate_validator_worker_pubkey`

Update candidate validator's public key of worker key.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_candidate_validator_worker_pubkey">update_candidate_validator_worker_pubkey</a>(self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">sui_system_state_inner::SuiSystemStateInnerV2</a>, worker_pubkey: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, ctx: &<a href="../sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_candidate_validator_worker_pubkey">update_candidate_validator_worker_pubkey</a>(
    self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>,
    worker_pubkey: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> candidate = self.validators.get_validator_mut_with_ctx_including_candidates(ctx);
    candidate.update_candidate_worker_pubkey(worker_pubkey);
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_update_validator_next_epoch_network_pubkey"></a>

## Function `update_validator_next_epoch_network_pubkey`

Update a validator's public key of network key.
The change will only take effects starting from the next epoch.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_validator_next_epoch_network_pubkey">update_validator_next_epoch_network_pubkey</a>(self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">sui_system_state_inner::SuiSystemStateInnerV2</a>, network_pubkey: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, ctx: &<a href="../sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_validator_next_epoch_network_pubkey">update_validator_next_epoch_network_pubkey</a>(
    self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>,
    network_pubkey: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> <a href="validator.md#0x3_validator">validator</a> = self.validators.get_validator_mut_with_ctx(ctx);
    <a href="validator.md#0x3_validator">validator</a>.update_next_epoch_network_pubkey(network_pubkey);
    <b>let</b> <a href="validator.md#0x3_validator">validator</a> :&Validator = <a href="validator.md#0x3_validator">validator</a>; // Force immutability for the following call
    self.validators.assert_no_pending_or_active_duplicates(<a href="validator.md#0x3_validator">validator</a>);
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_update_candidate_validator_network_pubkey"></a>

## Function `update_candidate_validator_network_pubkey`

Update candidate validator's public key of network key.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_candidate_validator_network_pubkey">update_candidate_validator_network_pubkey</a>(self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">sui_system_state_inner::SuiSystemStateInnerV2</a>, network_pubkey: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, ctx: &<a href="../sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_candidate_validator_network_pubkey">update_candidate_validator_network_pubkey</a>(
    self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>,
    network_pubkey: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> candidate = self.validators.get_validator_mut_with_ctx_including_candidates(ctx);
    candidate.update_candidate_network_pubkey(network_pubkey);
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_advance_epoch"></a>

## Function `advance_epoch`

This function should be called at the end of an epoch, and advances the system to the next epoch.
It does the following things:
1. Add storage charge to the storage fund.
2. Burn the storage rebates from the storage fund. These are already refunded to transaction sender's
gas coins.
3. Distribute computation charge to validator stake.
4. Update all validators.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_advance_epoch">advance_epoch</a>(self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">sui_system_state_inner::SuiSystemStateInnerV2</a>, new_epoch: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>, next_protocol_version: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>, storage_reward: <a href="../sui-framework/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="../sui-framework/sui.md#0x2_sui_SUI">sui::SUI</a>&gt;, computation_reward: <a href="../sui-framework/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="../sui-framework/sui.md#0x2_sui_SUI">sui::SUI</a>&gt;, storage_rebate_amount: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>, non_refundable_storage_fee_amount: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>, storage_fund_reinvest_rate: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>, reward_slashing_rate: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>, epoch_start_timestamp_ms: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>, ctx: &<b>mut</b> <a href="../sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="../sui-framework/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="../sui-framework/sui.md#0x2_sui_SUI">sui::SUI</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_advance_epoch">advance_epoch</a>(
    self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>,
    new_epoch: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>,
    next_protocol_version: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>,
    <b>mut</b> storage_reward: Balance&lt;SUI&gt;,
    <b>mut</b> computation_reward: Balance&lt;SUI&gt;,
    <b>mut</b> storage_rebate_amount: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>,
    <b>mut</b> non_refundable_storage_fee_amount: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>,
    storage_fund_reinvest_rate: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>, // share of storage fund's rewards that's reinvested
                                     // into storage fund, in basis point.
    reward_slashing_rate: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>, // how much rewards are slashed <b>to</b> punish a <a href="validator.md#0x3_validator">validator</a>, in bps.
    epoch_start_timestamp_ms: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>, // Timestamp of the epoch start
    ctx: &<b>mut</b> TxContext,
) : Balance&lt;SUI&gt; {
    <b>let</b> prev_epoch_start_timestamp = self.epoch_start_timestamp_ms;
    self.epoch_start_timestamp_ms = epoch_start_timestamp_ms;

    <b>let</b> bps_denominator_u64 = <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_BASIS_POINT_DENOMINATOR">BASIS_POINT_DENOMINATOR</a> <b>as</b> <a href="../move-stdlib/u64.md#0x1_u64">u64</a>;
    // Rates can't be higher than 100%.
    <b>assert</b>!(
        storage_fund_reinvest_rate &lt;= bps_denominator_u64
        && reward_slashing_rate &lt;= bps_denominator_u64,
        <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_EBpsTooLarge">EBpsTooLarge</a>,
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
    <b>let</b> storage_fund_balance = self.<a href="storage_fund.md#0x3_storage_fund">storage_fund</a>.total_balance();
    <b>let</b> total_stake = storage_fund_balance + total_validators_stake;

    <b>let</b> storage_charge = storage_reward.value();
    <b>let</b> computation_charge = computation_reward.value();

    // Include stake subsidy in the rewards given out <b>to</b> validators and stakers.
    // Delay distributing any stake subsidies until after `stake_subsidy_start_epoch`.
    // And <b>if</b> this epoch is shorter than the regular epoch duration, don't distribute any stake subsidy.
    <b>let</b> <a href="stake_subsidy.md#0x3_stake_subsidy">stake_subsidy</a> =
        <b>if</b> (ctx.<a href="sui_system_state_inner.md#0x3_sui_system_state_inner_epoch">epoch</a>() &gt;= self.parameters.stake_subsidy_start_epoch  &&
            epoch_start_timestamp_ms &gt;= prev_epoch_start_timestamp + self.parameters.epoch_duration_ms)
        {
            self.<a href="stake_subsidy.md#0x3_stake_subsidy">stake_subsidy</a>.<a href="sui_system_state_inner.md#0x3_sui_system_state_inner_advance_epoch">advance_epoch</a>()
        } <b>else</b> {
            <a href="../sui-framework/balance.md#0x2_balance_zero">balance::zero</a>()
        };

    <b>let</b> stake_subsidy_amount = <a href="stake_subsidy.md#0x3_stake_subsidy">stake_subsidy</a>.value();
    computation_reward.join(<a href="stake_subsidy.md#0x3_stake_subsidy">stake_subsidy</a>);

    <b>let</b> total_stake_u128 = total_stake <b>as</b> u128;
    <b>let</b> computation_charge_u128 = computation_charge <b>as</b> u128;

    <b>let</b> storage_fund_reward_amount = storage_fund_balance <b>as</b> u128 * computation_charge_u128 / total_stake_u128;
    <b>let</b> <b>mut</b> storage_fund_reward = computation_reward.split(storage_fund_reward_amount <b>as</b> <a href="../move-stdlib/u64.md#0x1_u64">u64</a>);
    <b>let</b> storage_fund_reinvestment_amount =
        storage_fund_reward_amount * (storage_fund_reinvest_rate <b>as</b> u128) / <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_BASIS_POINT_DENOMINATOR">BASIS_POINT_DENOMINATOR</a>;
    <b>let</b> storage_fund_reinvestment = storage_fund_reward.split(
        storage_fund_reinvestment_amount <b>as</b> <a href="../move-stdlib/u64.md#0x1_u64">u64</a>,
    );

    self.epoch = self.epoch + 1;
    // Sanity check <b>to</b> make sure we are advancing <b>to</b> the right epoch.
    <b>assert</b>!(new_epoch == self.epoch, <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_EAdvancedToWrongEpoch">EAdvancedToWrongEpoch</a>);

    <b>let</b> computation_reward_amount_before_distribution = computation_reward.value();
    <b>let</b> storage_fund_reward_amount_before_distribution = storage_fund_reward.value();

    self.validators.<a href="sui_system_state_inner.md#0x3_sui_system_state_inner_advance_epoch">advance_epoch</a>(
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

    self.protocol_version = next_protocol_version;

    // Derive the reference gas price for the new epoch
    self.reference_gas_price = self.validators.derive_reference_gas_price();
    // Because of precision issues <b>with</b> integer divisions, we expect that there will be some
    // remaining <a href="../sui-framework/balance.md#0x2_balance">balance</a> in `storage_fund_reward` and `computation_reward`.
    // All of these go <b>to</b> the storage fund.
    <b>let</b> <b>mut</b> leftover_staking_rewards = storage_fund_reward;
    leftover_staking_rewards.join(computation_reward);
    <b>let</b> leftover_storage_fund_inflow = leftover_staking_rewards.value();

    <b>let</b> refunded_storage_rebate =
        self.<a href="storage_fund.md#0x3_storage_fund">storage_fund</a>.<a href="sui_system_state_inner.md#0x3_sui_system_state_inner_advance_epoch">advance_epoch</a>(
            storage_reward,
            storage_fund_reinvestment,
            leftover_staking_rewards,
            storage_rebate_amount,
            non_refundable_storage_fee_amount,
        );

    <a href="../sui-framework/event.md#0x2_event_emit">event::emit</a>(
        <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SystemEpochInfoEvent">SystemEpochInfoEvent</a> {
            epoch: self.epoch,
            protocol_version: self.protocol_version,
            reference_gas_price: self.reference_gas_price,
            total_stake: new_total_stake,
            storage_charge,
            storage_fund_reinvestment: storage_fund_reinvestment_amount <b>as</b> <a href="../move-stdlib/u64.md#0x1_u64">u64</a>,
            storage_rebate: storage_rebate_amount,
            storage_fund_balance: self.<a href="storage_fund.md#0x3_storage_fund">storage_fund</a>.total_balance(),
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
        && self.safe_mode_computation_rewards.value() == 0, <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_ESafeModeGasNotProcessed">ESafeModeGasNotProcessed</a>);

    // Return the storage rebate split from storage fund that's already refunded <b>to</b> the transaction senders.
    // This will be burnt at the last step of epoch change programmable transaction.
    refunded_storage_rebate
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_epoch"></a>

## Function `epoch`

Return the current epoch number. Useful for applications that need a coarse-grained concept of time,
since epochs are ever-increasing and epoch changes are intended to happen every 24 hours.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_epoch">epoch</a>(self: &<a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">sui_system_state_inner::SuiSystemStateInnerV2</a>): <a href="../move-stdlib/u64.md#0x1_u64">u64</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_epoch">epoch</a>(self: &<a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>): <a href="../move-stdlib/u64.md#0x1_u64">u64</a> {
    self.epoch
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_protocol_version"></a>

## Function `protocol_version`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_protocol_version">protocol_version</a>(self: &<a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">sui_system_state_inner::SuiSystemStateInnerV2</a>): <a href="../move-stdlib/u64.md#0x1_u64">u64</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_protocol_version">protocol_version</a>(self: &<a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>): <a href="../move-stdlib/u64.md#0x1_u64">u64</a> {
    self.protocol_version
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_system_state_version"></a>

## Function `system_state_version`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_system_state_version">system_state_version</a>(self: &<a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">sui_system_state_inner::SuiSystemStateInnerV2</a>): <a href="../move-stdlib/u64.md#0x1_u64">u64</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_system_state_version">system_state_version</a>(self: &<a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>): <a href="../move-stdlib/u64.md#0x1_u64">u64</a> {
    self.system_state_version
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_genesis_system_state_version"></a>

## Function `genesis_system_state_version`

This function always return the genesis system state version, which is used to create the system state in genesis.
It should never change for a given network.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_genesis_system_state_version">genesis_system_state_version</a>(): <a href="../move-stdlib/u64.md#0x1_u64">u64</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_genesis_system_state_version">genesis_system_state_version</a>(): <a href="../move-stdlib/u64.md#0x1_u64">u64</a> {
    <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SYSTEM_STATE_VERSION_V1">SYSTEM_STATE_VERSION_V1</a>
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_epoch_start_timestamp_ms"></a>

## Function `epoch_start_timestamp_ms`

Returns unix timestamp of the start of current epoch


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_epoch_start_timestamp_ms">epoch_start_timestamp_ms</a>(self: &<a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">sui_system_state_inner::SuiSystemStateInnerV2</a>): <a href="../move-stdlib/u64.md#0x1_u64">u64</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_epoch_start_timestamp_ms">epoch_start_timestamp_ms</a>(self: &<a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>): <a href="../move-stdlib/u64.md#0x1_u64">u64</a> {
    self.epoch_start_timestamp_ms
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_validator_stake_amount"></a>

## Function `validator_stake_amount`

Returns the total amount staked with <code>validator_addr</code>.
Aborts if <code>validator_addr</code> is not an active validator.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_validator_stake_amount">validator_stake_amount</a>(self: &<a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">sui_system_state_inner::SuiSystemStateInnerV2</a>, validator_addr: <b>address</b>): <a href="../move-stdlib/u64.md#0x1_u64">u64</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_validator_stake_amount">validator_stake_amount</a>(self: &<a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>, validator_addr: <b>address</b>): <a href="../move-stdlib/u64.md#0x1_u64">u64</a> {
    self.validators.validator_total_stake_amount(validator_addr)
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_active_validator_voting_powers"></a>

## Function `active_validator_voting_powers`

Returns the voting power for <code>validator_addr</code>.
Aborts if <code>validator_addr</code> is not an active validator.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_active_validator_voting_powers">active_validator_voting_powers</a>(self: &<a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">sui_system_state_inner::SuiSystemStateInnerV2</a>): <a href="../sui-framework/vec_map.md#0x2_vec_map_VecMap">vec_map::VecMap</a>&lt;<b>address</b>, <a href="../move-stdlib/u64.md#0x1_u64">u64</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_active_validator_voting_powers">active_validator_voting_powers</a>(self: &<a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>): VecMap&lt;<b>address</b>, <a href="../move-stdlib/u64.md#0x1_u64">u64</a>&gt; {
    <b>let</b> <b>mut</b> active_validators = <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_active_validator_addresses">active_validator_addresses</a>(self);
    <b>let</b> <b>mut</b> voting_powers = <a href="../sui-framework/vec_map.md#0x2_vec_map_empty">vec_map::empty</a>();
    <b>while</b> (!<a href="../move-stdlib/vector.md#0x1_vector_is_empty">vector::is_empty</a>(&active_validators)) {
        <b>let</b> <a href="validator.md#0x3_validator">validator</a> = <a href="../move-stdlib/vector.md#0x1_vector_pop_back">vector::pop_back</a>(&<b>mut</b> active_validators);
        <b>let</b> <a href="voting_power.md#0x3_voting_power">voting_power</a> = <a href="validator_set.md#0x3_validator_set_validator_voting_power">validator_set::validator_voting_power</a>(&self.validators, <a href="validator.md#0x3_validator">validator</a>);
        <a href="../sui-framework/vec_map.md#0x2_vec_map_insert">vec_map::insert</a>(&<b>mut</b> voting_powers, <a href="validator.md#0x3_validator">validator</a>, <a href="voting_power.md#0x3_voting_power">voting_power</a>);
    };
    voting_powers
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_validator_staking_pool_id"></a>

## Function `validator_staking_pool_id`

Returns the staking pool id of a given validator.
Aborts if <code>validator_addr</code> is not an active validator.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_validator_staking_pool_id">validator_staking_pool_id</a>(self: &<a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">sui_system_state_inner::SuiSystemStateInnerV2</a>, validator_addr: <b>address</b>): <a href="../sui-framework/object.md#0x2_object_ID">object::ID</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_validator_staking_pool_id">validator_staking_pool_id</a>(self: &<a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>, validator_addr: <b>address</b>): ID {

    self.validators.<a href="sui_system_state_inner.md#0x3_sui_system_state_inner_validator_staking_pool_id">validator_staking_pool_id</a>(validator_addr)
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_validator_staking_pool_mappings"></a>

## Function `validator_staking_pool_mappings`

Returns reference to the staking pool mappings that map pool ids to active validator addresses


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_validator_staking_pool_mappings">validator_staking_pool_mappings</a>(self: &<a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">sui_system_state_inner::SuiSystemStateInnerV2</a>): &<a href="../sui-framework/table.md#0x2_table_Table">table::Table</a>&lt;<a href="../sui-framework/object.md#0x2_object_ID">object::ID</a>, <b>address</b>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_validator_staking_pool_mappings">validator_staking_pool_mappings</a>(self: &<a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>): &Table&lt;ID, <b>address</b>&gt; {

    self.validators.staking_pool_mappings()
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_get_reporters_of"></a>

## Function `get_reporters_of`

Returns all the validators who are currently reporting <code>addr</code>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_get_reporters_of">get_reporters_of</a>(self: &<a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">sui_system_state_inner::SuiSystemStateInnerV2</a>, addr: <b>address</b>): <a href="../sui-framework/vec_set.md#0x2_vec_set_VecSet">vec_set::VecSet</a>&lt;<b>address</b>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_get_reporters_of">get_reporters_of</a>(self: &<a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>, addr: <b>address</b>): VecSet&lt;<b>address</b>&gt; {

    <b>if</b> (self.validator_report_records.contains(&addr)) {
        self.validator_report_records[&addr]
    } <b>else</b> {
        <a href="../sui-framework/vec_set.md#0x2_vec_set_empty">vec_set::empty</a>()
    }
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_get_storage_fund_total_balance"></a>

## Function `get_storage_fund_total_balance`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_get_storage_fund_total_balance">get_storage_fund_total_balance</a>(self: &<a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">sui_system_state_inner::SuiSystemStateInnerV2</a>): <a href="../move-stdlib/u64.md#0x1_u64">u64</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_get_storage_fund_total_balance">get_storage_fund_total_balance</a>(self: &<a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>): <a href="../move-stdlib/u64.md#0x1_u64">u64</a> {
    self.<a href="storage_fund.md#0x3_storage_fund">storage_fund</a>.total_balance()
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_get_storage_fund_object_rebates"></a>

## Function `get_storage_fund_object_rebates`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_get_storage_fund_object_rebates">get_storage_fund_object_rebates</a>(self: &<a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">sui_system_state_inner::SuiSystemStateInnerV2</a>): <a href="../move-stdlib/u64.md#0x1_u64">u64</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_get_storage_fund_object_rebates">get_storage_fund_object_rebates</a>(self: &<a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>): <a href="../move-stdlib/u64.md#0x1_u64">u64</a> {
    self.<a href="storage_fund.md#0x3_storage_fund">storage_fund</a>.total_object_storage_rebates()
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_pool_exchange_rates"></a>

## Function `pool_exchange_rates`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_pool_exchange_rates">pool_exchange_rates</a>(self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">sui_system_state_inner::SuiSystemStateInnerV2</a>, pool_id: &<a href="../sui-framework/object.md#0x2_object_ID">object::ID</a>): &<a href="../sui-framework/table.md#0x2_table_Table">table::Table</a>&lt;<a href="../move-stdlib/u64.md#0x1_u64">u64</a>, <a href="staking_pool.md#0x3_staking_pool_PoolTokenExchangeRate">staking_pool::PoolTokenExchangeRate</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_pool_exchange_rates">pool_exchange_rates</a>(
    self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>,
    pool_id: &ID
): &Table&lt;<a href="../move-stdlib/u64.md#0x1_u64">u64</a>, PoolTokenExchangeRate&gt;  {
    <b>let</b> validators = &<b>mut</b> self.validators;
    validators.<a href="sui_system_state_inner.md#0x3_sui_system_state_inner_pool_exchange_rates">pool_exchange_rates</a>(pool_id)
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_active_validator_addresses"></a>

## Function `active_validator_addresses`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_active_validator_addresses">active_validator_addresses</a>(self: &<a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">sui_system_state_inner::SuiSystemStateInnerV2</a>): <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;<b>address</b>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_active_validator_addresses">active_validator_addresses</a>(self: &<a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">SuiSystemStateInnerV2</a>): <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;<b>address</b>&gt; {
    <b>let</b> <a href="validator_set.md#0x3_validator_set">validator_set</a> = &self.validators;
    <a href="validator_set.md#0x3_validator_set">validator_set</a>.<a href="sui_system_state_inner.md#0x3_sui_system_state_inner_active_validator_addresses">active_validator_addresses</a>()
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_extract_coin_balance"></a>

## Function `extract_coin_balance`

Extract required Balance from vector of Coin<SUI>, transfer the remainder back to sender.


<pre><code><b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_extract_coin_balance">extract_coin_balance</a>(coins: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="../sui-framework/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;<a href="../sui-framework/sui.md#0x2_sui_SUI">sui::SUI</a>&gt;&gt;, amount: <a href="../move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;<a href="../move-stdlib/u64.md#0x1_u64">u64</a>&gt;, ctx: &<b>mut</b> <a href="../sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="../sui-framework/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="../sui-framework/sui.md#0x2_sui_SUI">sui::SUI</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_extract_coin_balance">extract_coin_balance</a>(<b>mut</b> coins: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;Coin&lt;SUI&gt;&gt;, amount: <a href="../move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;<a href="../move-stdlib/u64.md#0x1_u64">u64</a>&gt;, ctx: &<b>mut</b> TxContext): Balance&lt;SUI&gt; {
    <b>let</b> <b>mut</b> merged_coin = coins.pop_back();
    merged_coin.join_vec(coins);

    <b>let</b> <b>mut</b> total_balance = merged_coin.into_balance();
    // <b>return</b> the full amount <b>if</b> amount is not specified
    <b>if</b> (amount.is_some()) {
        <b>let</b> amount = amount.destroy_some();
        <b>let</b> <a href="../sui-framework/balance.md#0x2_balance">balance</a> = total_balance.split(amount);
        // <a href="../sui-framework/transfer.md#0x2_transfer">transfer</a> back the remainder <b>if</b> non zero.
        <b>if</b> (total_balance.value() &gt; 0) {
            <a href="../sui-framework/transfer.md#0x2_transfer_public_transfer">transfer::public_transfer</a>(total_balance.into_coin(ctx), ctx.sender());
        } <b>else</b> {
            total_balance.destroy_zero();
        };
        <a href="../sui-framework/balance.md#0x2_balance">balance</a>
    } <b>else</b> {
        total_balance
    }
}
</code></pre>



</details>
