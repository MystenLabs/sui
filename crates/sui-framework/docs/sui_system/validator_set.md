---
title: Module `sui_system::validator_set`
---



-  [Struct `ValidatorSet`](#sui_system_validator_set_ValidatorSet)
-  [Struct `ValidatorEpochInfoEvent`](#sui_system_validator_set_ValidatorEpochInfoEvent)
-  [Struct `ValidatorEpochInfoEventV2`](#sui_system_validator_set_ValidatorEpochInfoEventV2)
-  [Struct `ValidatorJoinEvent`](#sui_system_validator_set_ValidatorJoinEvent)
-  [Struct `ValidatorLeaveEvent`](#sui_system_validator_set_ValidatorLeaveEvent)
-  [Constants](#@Constants_0)
-  [Function `new`](#sui_system_validator_set_new)
-  [Function `request_add_validator_candidate`](#sui_system_validator_set_request_add_validator_candidate)
-  [Function `request_remove_validator_candidate`](#sui_system_validator_set_request_remove_validator_candidate)
-  [Function `request_add_validator`](#sui_system_validator_set_request_add_validator)
-  [Function `assert_no_pending_or_active_duplicates`](#sui_system_validator_set_assert_no_pending_or_active_duplicates)
-  [Function `request_remove_validator`](#sui_system_validator_set_request_remove_validator)
-  [Function `request_add_stake`](#sui_system_validator_set_request_add_stake)
-  [Function `request_withdraw_stake`](#sui_system_validator_set_request_withdraw_stake)
-  [Function `convert_to_fungible_staked_sui`](#sui_system_validator_set_convert_to_fungible_staked_sui)
-  [Function `redeem_fungible_staked_sui`](#sui_system_validator_set_redeem_fungible_staked_sui)
-  [Function `request_set_commission_rate`](#sui_system_validator_set_request_set_commission_rate)
-  [Function `advance_epoch`](#sui_system_validator_set_advance_epoch)
-  [Function `update_and_process_low_stake_departures`](#sui_system_validator_set_update_and_process_low_stake_departures)
-  [Function `effectuate_staged_metadata`](#sui_system_validator_set_effectuate_staged_metadata)
-  [Function `derive_reference_gas_price`](#sui_system_validator_set_derive_reference_gas_price)
-  [Function `total_stake`](#sui_system_validator_set_total_stake)
-  [Function `validator_total_stake_amount`](#sui_system_validator_set_validator_total_stake_amount)
-  [Function `validator_stake_amount`](#sui_system_validator_set_validator_stake_amount)
-  [Function `validator_voting_power`](#sui_system_validator_set_validator_voting_power)
-  [Function `validator_staking_pool_id`](#sui_system_validator_set_validator_staking_pool_id)
-  [Function `staking_pool_mappings`](#sui_system_validator_set_staking_pool_mappings)
-  [Function `validator_address_by_pool_id`](#sui_system_validator_set_validator_address_by_pool_id)
-  [Function `pool_exchange_rates`](#sui_system_validator_set_pool_exchange_rates)
-  [Function `next_epoch_validator_count`](#sui_system_validator_set_next_epoch_validator_count)
-  [Function `is_active_validator_by_sui_address`](#sui_system_validator_set_is_active_validator_by_sui_address)
-  [Function `is_duplicate_with_active_validator`](#sui_system_validator_set_is_duplicate_with_active_validator)
-  [Function `is_duplicate_validator`](#sui_system_validator_set_is_duplicate_validator)
-  [Function `count_duplicates_vec`](#sui_system_validator_set_count_duplicates_vec)
-  [Function `is_duplicate_with_pending_validator`](#sui_system_validator_set_is_duplicate_with_pending_validator)
-  [Function `count_duplicates_tablevec`](#sui_system_validator_set_count_duplicates_tablevec)
-  [Function `get_candidate_or_active_validator_mut`](#sui_system_validator_set_get_candidate_or_active_validator_mut)
-  [Function `find_validator`](#sui_system_validator_set_find_validator)
-  [Function `find_validator_from_table_vec`](#sui_system_validator_set_find_validator_from_table_vec)
-  [Function `get_validator_indices`](#sui_system_validator_set_get_validator_indices)
-  [Function `get_validator_mut`](#sui_system_validator_set_get_validator_mut)
-  [Function `get_active_or_pending_or_candidate_validator_mut`](#sui_system_validator_set_get_active_or_pending_or_candidate_validator_mut)
-  [Function `get_validator_mut_with_verified_cap`](#sui_system_validator_set_get_validator_mut_with_verified_cap)
-  [Function `get_validator_mut_with_ctx`](#sui_system_validator_set_get_validator_mut_with_ctx)
-  [Function `get_validator_mut_with_ctx_including_candidates`](#sui_system_validator_set_get_validator_mut_with_ctx_including_candidates)
-  [Function `get_validator_ref`](#sui_system_validator_set_get_validator_ref)
-  [Function `get_active_or_pending_or_candidate_validator_ref`](#sui_system_validator_set_get_active_or_pending_or_candidate_validator_ref)
-  [Function `get_active_validator_ref`](#sui_system_validator_set_get_active_validator_ref)
-  [Function `get_pending_validator_ref`](#sui_system_validator_set_get_pending_validator_ref)
-  [Function `verify_cap`](#sui_system_validator_set_verify_cap)
-  [Function `process_pending_removals`](#sui_system_validator_set_process_pending_removals)
-  [Function `process_validator_departure`](#sui_system_validator_set_process_validator_departure)
-  [Function `clean_report_records_leaving_validator`](#sui_system_validator_set_clean_report_records_leaving_validator)
-  [Function `process_pending_validators`](#sui_system_validator_set_process_pending_validators)
-  [Function `sort_removal_list`](#sui_system_validator_set_sort_removal_list)
-  [Function `process_pending_stakes_and_withdraws`](#sui_system_validator_set_process_pending_stakes_and_withdraws)
-  [Function `calculate_total_stakes`](#sui_system_validator_set_calculate_total_stakes)
-  [Function `adjust_stake_and_gas_price`](#sui_system_validator_set_adjust_stake_and_gas_price)
-  [Function `compute_reward_adjustments`](#sui_system_validator_set_compute_reward_adjustments)
-  [Function `compute_slashed_validators`](#sui_system_validator_set_compute_slashed_validators)
-  [Function `compute_unadjusted_reward_distribution`](#sui_system_validator_set_compute_unadjusted_reward_distribution)
-  [Function `compute_adjusted_reward_distribution`](#sui_system_validator_set_compute_adjusted_reward_distribution)
-  [Function `distribute_reward`](#sui_system_validator_set_distribute_reward)
-  [Function `emit_validator_epoch_events`](#sui_system_validator_set_emit_validator_epoch_events)
-  [Function `sum_voting_power_by_addresses`](#sui_system_validator_set_sum_voting_power_by_addresses)
-  [Function `active_validators`](#sui_system_validator_set_active_validators)
-  [Function `is_validator_candidate`](#sui_system_validator_set_is_validator_candidate)
-  [Function `is_inactive_validator`](#sui_system_validator_set_is_inactive_validator)
-  [Function `active_validator_addresses`](#sui_system_validator_set_active_validator_addresses)


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
<b>use</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool">sui_system::staking_pool</a>;
<b>use</b> <a href="../sui_system/validator.md#sui_system_validator">sui_system::validator</a>;
<b>use</b> <a href="../sui_system/validator_cap.md#sui_system_validator_cap">sui_system::validator_cap</a>;
<b>use</b> <a href="../sui_system/validator_wrapper.md#sui_system_validator_wrapper">sui_system::validator_wrapper</a>;
<b>use</b> <a href="../sui_system/voting_power.md#sui_system_voting_power">sui_system::voting_power</a>;
</code></pre>



<a name="sui_system_validator_set_ValidatorSet"></a>

## Struct `ValidatorSet`



<pre><code><b>public</b> <b>struct</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">ValidatorSet</a> <b>has</b> store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code><a href="../sui_system/validator_set.md#sui_system_validator_set_total_stake">total_stake</a>: u64</code>
</dt>
<dd>
 Total amount of stake from all active validators at the beginning of the epoch.
</dd>
<dt>
<code><a href="../sui_system/validator_set.md#sui_system_validator_set_active_validators">active_validators</a>: vector&lt;<a href="../sui_system/validator.md#sui_system_validator_Validator">sui_system::validator::Validator</a>&gt;</code>
</dt>
<dd>
 The current list of active validators.
</dd>
<dt>
<code>pending_active_validators: <a href="../sui/table_vec.md#sui_table_vec_TableVec">sui::table_vec::TableVec</a>&lt;<a href="../sui_system/validator.md#sui_system_validator_Validator">sui_system::validator::Validator</a>&gt;</code>
</dt>
<dd>
 List of new validator candidates added during the current epoch.
 They will be processed at the end of the epoch.
</dd>
<dt>
<code>pending_removals: vector&lt;u64&gt;</code>
</dt>
<dd>
 Removal requests from the validators. Each element is an index
 pointing to <code><a href="../sui_system/validator_set.md#sui_system_validator_set_active_validators">active_validators</a></code>.
</dd>
<dt>
<code><a href="../sui_system/validator_set.md#sui_system_validator_set_staking_pool_mappings">staking_pool_mappings</a>: <a href="../sui/table.md#sui_table_Table">sui::table::Table</a>&lt;<a href="../sui/object.md#sui_object_ID">sui::object::ID</a>, <b>address</b>&gt;</code>
</dt>
<dd>
 Mappings from staking pool's ID to the sui address of a validator.
</dd>
<dt>
<code>inactive_validators: <a href="../sui/table.md#sui_table_Table">sui::table::Table</a>&lt;<a href="../sui/object.md#sui_object_ID">sui::object::ID</a>, <a href="../sui_system/validator_wrapper.md#sui_system_validator_wrapper_ValidatorWrapper">sui_system::validator_wrapper::ValidatorWrapper</a>&gt;</code>
</dt>
<dd>
 Mapping from a staking pool ID to the inactive validator that has that pool as its staking pool.
 When a validator is deactivated the validator is removed from <code><a href="../sui_system/validator_set.md#sui_system_validator_set_active_validators">active_validators</a></code> it
 is added to this table so that stakers can continue to withdraw their stake from it.
</dd>
<dt>
<code>validator_candidates: <a href="../sui/table.md#sui_table_Table">sui::table::Table</a>&lt;<b>address</b>, <a href="../sui_system/validator_wrapper.md#sui_system_validator_wrapper_ValidatorWrapper">sui_system::validator_wrapper::ValidatorWrapper</a>&gt;</code>
</dt>
<dd>
 Table storing preactive/candidate validators, mapping their addresses to their <code>Validator </code> structs.
 When an address calls <code><a href="../sui_system/validator_set.md#sui_system_validator_set_request_add_validator_candidate">request_add_validator_candidate</a></code>, they get added to this table and become a preactive
 validator.
 When the candidate has met the min stake requirement, they can call <code><a href="../sui_system/validator_set.md#sui_system_validator_set_request_add_validator">request_add_validator</a></code> to
 officially add them to the active validator set <code><a href="../sui_system/validator_set.md#sui_system_validator_set_active_validators">active_validators</a></code> next epoch.
</dd>
<dt>
<code>at_risk_validators: <a href="../sui/vec_map.md#sui_vec_map_VecMap">sui::vec_map::VecMap</a>&lt;<b>address</b>, u64&gt;</code>
</dt>
<dd>
 Table storing the number of epochs during which a validator's stake has been below the low stake threshold.
</dd>
<dt>
<code>extra_fields: <a href="../sui/bag.md#sui_bag_Bag">sui::bag::Bag</a></code>
</dt>
<dd>
 Any extra fields that's not defined statically.
</dd>
</dl>


</details>

<a name="sui_system_validator_set_ValidatorEpochInfoEvent"></a>

## Struct `ValidatorEpochInfoEvent`

Event containing staking and rewards related information of
each validator, emitted during epoch advancement.


<pre><code><b>public</b> <b>struct</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorEpochInfoEvent">ValidatorEpochInfoEvent</a> <b>has</b> <b>copy</b>, drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>epoch: u64</code>
</dt>
<dd>
</dd>
<dt>
<code>validator_address: <b>address</b></code>
</dt>
<dd>
</dd>
<dt>
<code>reference_gas_survey_quote: u64</code>
</dt>
<dd>
</dd>
<dt>
<code>stake: u64</code>
</dt>
<dd>
</dd>
<dt>
<code>commission_rate: u64</code>
</dt>
<dd>
</dd>
<dt>
<code>pool_staking_reward: u64</code>
</dt>
<dd>
</dd>
<dt>
<code>storage_fund_staking_reward: u64</code>
</dt>
<dd>
</dd>
<dt>
<code>pool_token_exchange_rate: <a href="../sui_system/staking_pool.md#sui_system_staking_pool_PoolTokenExchangeRate">sui_system::staking_pool::PoolTokenExchangeRate</a></code>
</dt>
<dd>
</dd>
<dt>
<code>tallying_rule_reporters: vector&lt;<b>address</b>&gt;</code>
</dt>
<dd>
</dd>
<dt>
<code>tallying_rule_global_score: u64</code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="sui_system_validator_set_ValidatorEpochInfoEventV2"></a>

## Struct `ValidatorEpochInfoEventV2`

V2 of ValidatorEpochInfoEvent containing more information about the validator.


<pre><code><b>public</b> <b>struct</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorEpochInfoEventV2">ValidatorEpochInfoEventV2</a> <b>has</b> <b>copy</b>, drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>epoch: u64</code>
</dt>
<dd>
</dd>
<dt>
<code>validator_address: <b>address</b></code>
</dt>
<dd>
</dd>
<dt>
<code>reference_gas_survey_quote: u64</code>
</dt>
<dd>
</dd>
<dt>
<code>stake: u64</code>
</dt>
<dd>
</dd>
<dt>
<code><a href="../sui_system/voting_power.md#sui_system_voting_power">voting_power</a>: u64</code>
</dt>
<dd>
</dd>
<dt>
<code>commission_rate: u64</code>
</dt>
<dd>
</dd>
<dt>
<code>pool_staking_reward: u64</code>
</dt>
<dd>
</dd>
<dt>
<code>storage_fund_staking_reward: u64</code>
</dt>
<dd>
</dd>
<dt>
<code>pool_token_exchange_rate: <a href="../sui_system/staking_pool.md#sui_system_staking_pool_PoolTokenExchangeRate">sui_system::staking_pool::PoolTokenExchangeRate</a></code>
</dt>
<dd>
</dd>
<dt>
<code>tallying_rule_reporters: vector&lt;<b>address</b>&gt;</code>
</dt>
<dd>
</dd>
<dt>
<code>tallying_rule_global_score: u64</code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="sui_system_validator_set_ValidatorJoinEvent"></a>

## Struct `ValidatorJoinEvent`

Event emitted every time a new validator joins the committee.
The epoch value corresponds to the first epoch this change takes place.


<pre><code><b>public</b> <b>struct</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorJoinEvent">ValidatorJoinEvent</a> <b>has</b> <b>copy</b>, drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>epoch: u64</code>
</dt>
<dd>
</dd>
<dt>
<code>validator_address: <b>address</b></code>
</dt>
<dd>
</dd>
<dt>
<code>staking_pool_id: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a></code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="sui_system_validator_set_ValidatorLeaveEvent"></a>

## Struct `ValidatorLeaveEvent`

Event emitted every time a validator leaves the committee.
The epoch value corresponds to the first epoch this change takes place.


<pre><code><b>public</b> <b>struct</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorLeaveEvent">ValidatorLeaveEvent</a> <b>has</b> <b>copy</b>, drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>epoch: u64</code>
</dt>
<dd>
</dd>
<dt>
<code>validator_address: <b>address</b></code>
</dt>
<dd>
</dd>
<dt>
<code>staking_pool_id: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a></code>
</dt>
<dd>
</dd>
<dt>
<code>is_voluntary: bool</code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="sui_system_validator_set_ACTIVE_OR_PENDING_VALIDATOR"></a>



<pre><code><b>const</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_ACTIVE_OR_PENDING_VALIDATOR">ACTIVE_OR_PENDING_VALIDATOR</a>: u8 = 2;
</code></pre>



<a name="sui_system_validator_set_ACTIVE_VALIDATOR_ONLY"></a>



<pre><code><b>const</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_ACTIVE_VALIDATOR_ONLY">ACTIVE_VALIDATOR_ONLY</a>: u8 = 1;
</code></pre>



<a name="sui_system_validator_set_ANY_VALIDATOR"></a>



<pre><code><b>const</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_ANY_VALIDATOR">ANY_VALIDATOR</a>: u8 = 3;
</code></pre>



<a name="sui_system_validator_set_BASIS_POINT_DENOMINATOR"></a>



<pre><code><b>const</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_BASIS_POINT_DENOMINATOR">BASIS_POINT_DENOMINATOR</a>: u128 = 10000;
</code></pre>



<a name="sui_system_validator_set_EAlreadyValidatorCandidate"></a>



<pre><code><b>const</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_EAlreadyValidatorCandidate">EAlreadyValidatorCandidate</a>: u64 = 6;
</code></pre>



<a name="sui_system_validator_set_EDuplicateValidator"></a>



<pre><code><b>const</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_EDuplicateValidator">EDuplicateValidator</a>: u64 = 2;
</code></pre>



<a name="sui_system_validator_set_EInvalidCap"></a>



<pre><code><b>const</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_EInvalidCap">EInvalidCap</a>: u64 = 101;
</code></pre>



<a name="sui_system_validator_set_EInvalidStakeAdjustmentAmount"></a>



<pre><code><b>const</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_EInvalidStakeAdjustmentAmount">EInvalidStakeAdjustmentAmount</a>: u64 = 1;
</code></pre>



<a name="sui_system_validator_set_EMinJoiningStakeNotReached"></a>



<pre><code><b>const</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_EMinJoiningStakeNotReached">EMinJoiningStakeNotReached</a>: u64 = 5;
</code></pre>



<a name="sui_system_validator_set_ENoPoolFound"></a>



<pre><code><b>const</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_ENoPoolFound">ENoPoolFound</a>: u64 = 3;
</code></pre>



<a name="sui_system_validator_set_ENonValidatorInReportRecords"></a>



<pre><code><b>const</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_ENonValidatorInReportRecords">ENonValidatorInReportRecords</a>: u64 = 0;
</code></pre>



<a name="sui_system_validator_set_ENotAPendingValidator"></a>



<pre><code><b>const</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_ENotAPendingValidator">ENotAPendingValidator</a>: u64 = 12;
</code></pre>



<a name="sui_system_validator_set_ENotAValidator"></a>



<pre><code><b>const</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_ENotAValidator">ENotAValidator</a>: u64 = 4;
</code></pre>



<a name="sui_system_validator_set_ENotActiveOrPendingValidator"></a>



<pre><code><b>const</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_ENotActiveOrPendingValidator">ENotActiveOrPendingValidator</a>: u64 = 9;
</code></pre>



<a name="sui_system_validator_set_ENotValidatorCandidate"></a>



<pre><code><b>const</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_ENotValidatorCandidate">ENotValidatorCandidate</a>: u64 = 8;
</code></pre>



<a name="sui_system_validator_set_EStakingBelowThreshold"></a>



<pre><code><b>const</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_EStakingBelowThreshold">EStakingBelowThreshold</a>: u64 = 10;
</code></pre>



<a name="sui_system_validator_set_EValidatorAlreadyRemoved"></a>



<pre><code><b>const</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_EValidatorAlreadyRemoved">EValidatorAlreadyRemoved</a>: u64 = 11;
</code></pre>



<a name="sui_system_validator_set_EValidatorNotCandidate"></a>



<pre><code><b>const</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_EValidatorNotCandidate">EValidatorNotCandidate</a>: u64 = 7;
</code></pre>



<a name="sui_system_validator_set_EValidatorSetEmpty"></a>



<pre><code><b>const</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_EValidatorSetEmpty">EValidatorSetEmpty</a>: u64 = 13;
</code></pre>



<a name="sui_system_validator_set_MIN_STAKING_THRESHOLD"></a>



<pre><code><b>const</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_MIN_STAKING_THRESHOLD">MIN_STAKING_THRESHOLD</a>: u64 = 1000000000;
</code></pre>



<a name="sui_system_validator_set_new"></a>

## Function `new`



<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_new">new</a>(init_active_validators: vector&lt;<a href="../sui_system/validator.md#sui_system_validator_Validator">sui_system::validator::Validator</a>&gt;, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">sui_system::validator_set::ValidatorSet</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_new">new</a>(init_active_validators: vector&lt;Validator&gt;, ctx: &<b>mut</b> TxContext): <a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">ValidatorSet</a> {
    <b>let</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_total_stake">total_stake</a> = <a href="../sui_system/validator_set.md#sui_system_validator_set_calculate_total_stakes">calculate_total_stakes</a>(&init_active_validators);
    <b>let</b> <b>mut</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_staking_pool_mappings">staking_pool_mappings</a> = table::new(ctx);
    <b>let</b> num_validators = init_active_validators.length();
    <b>let</b> <b>mut</b> i = 0;
    <b>while</b> (i &lt; num_validators) {
        <b>let</b> <a href="../sui_system/validator.md#sui_system_validator">validator</a> = &init_active_validators[i];
        <a href="../sui_system/validator_set.md#sui_system_validator_set_staking_pool_mappings">staking_pool_mappings</a>.add(staking_pool_id(<a href="../sui_system/validator.md#sui_system_validator">validator</a>), sui_address(<a href="../sui_system/validator.md#sui_system_validator">validator</a>));
        i = i + 1;
    };
    <b>let</b> <b>mut</b> validators = <a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">ValidatorSet</a> {
        <a href="../sui_system/validator_set.md#sui_system_validator_set_total_stake">total_stake</a>,
        <a href="../sui_system/validator_set.md#sui_system_validator_set_active_validators">active_validators</a>: init_active_validators,
        pending_active_validators: table_vec::empty(ctx),
        pending_removals: vector[],
        <a href="../sui_system/validator_set.md#sui_system_validator_set_staking_pool_mappings">staking_pool_mappings</a>,
        inactive_validators: table::new(ctx),
        validator_candidates: table::new(ctx),
        at_risk_validators: vec_map::empty(),
        extra_fields: bag::new(ctx),
    };
    <a href="../sui_system/voting_power.md#sui_system_voting_power_set_voting_power">voting_power::set_voting_power</a>(&<b>mut</b> validators.<a href="../sui_system/validator_set.md#sui_system_validator_set_active_validators">active_validators</a>);
    validators
}
</code></pre>



</details>

<a name="sui_system_validator_set_request_add_validator_candidate"></a>

## Function `request_add_validator_candidate`

Called by <code><a href="../sui_system/sui_system.md#sui_system_sui_system">sui_system</a></code> to add a new validator candidate.


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_request_add_validator_candidate">request_add_validator_candidate</a>(self: &<b>mut</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">sui_system::validator_set::ValidatorSet</a>, <a href="../sui_system/validator.md#sui_system_validator">validator</a>: <a href="../sui_system/validator.md#sui_system_validator_Validator">sui_system::validator::Validator</a>, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_request_add_validator_candidate">request_add_validator_candidate</a>(
    self: &<b>mut</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">ValidatorSet</a>,
    <a href="../sui_system/validator.md#sui_system_validator">validator</a>: Validator,
    ctx: &<b>mut</b> TxContext,
) {
    // The next assertions are not critical <b>for</b> the protocol, but they are here to catch problematic configs earlier.
    <b>assert</b>!(
        !<a href="../sui_system/validator_set.md#sui_system_validator_set_is_duplicate_with_active_validator">is_duplicate_with_active_validator</a>(self, &<a href="../sui_system/validator.md#sui_system_validator">validator</a>)
            && !<a href="../sui_system/validator_set.md#sui_system_validator_set_is_duplicate_with_pending_validator">is_duplicate_with_pending_validator</a>(self, &<a href="../sui_system/validator.md#sui_system_validator">validator</a>),
        <a href="../sui_system/validator_set.md#sui_system_validator_set_EDuplicateValidator">EDuplicateValidator</a>
    );
    <b>let</b> validator_address = sui_address(&<a href="../sui_system/validator.md#sui_system_validator">validator</a>);
    <b>assert</b>!(
        !self.validator_candidates.contains(validator_address),
        <a href="../sui_system/validator_set.md#sui_system_validator_set_EAlreadyValidatorCandidate">EAlreadyValidatorCandidate</a>
    );
    <b>assert</b>!(<a href="../sui_system/validator.md#sui_system_validator">validator</a>.is_preactive(), <a href="../sui_system/validator_set.md#sui_system_validator_set_EValidatorNotCandidate">EValidatorNotCandidate</a>);
    // Add <a href="../sui_system/validator.md#sui_system_validator">validator</a> to the candidates mapping and the pool id mappings so that users can start
    // staking with this candidate.
    self.<a href="../sui_system/validator_set.md#sui_system_validator_set_staking_pool_mappings">staking_pool_mappings</a>.add(staking_pool_id(&<a href="../sui_system/validator.md#sui_system_validator">validator</a>), validator_address);
    self.validator_candidates.add(
        sui_address(&<a href="../sui_system/validator.md#sui_system_validator">validator</a>),
        <a href="../sui_system/validator_wrapper.md#sui_system_validator_wrapper_create_v1">validator_wrapper::create_v1</a>(<a href="../sui_system/validator.md#sui_system_validator">validator</a>, ctx),
    );
}
</code></pre>



</details>

<a name="sui_system_validator_set_request_remove_validator_candidate"></a>

## Function `request_remove_validator_candidate`

Called by <code><a href="../sui_system/sui_system.md#sui_system_sui_system">sui_system</a></code> to remove a validator candidate, and move them to <code>inactive_validators</code>.


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_request_remove_validator_candidate">request_remove_validator_candidate</a>(self: &<b>mut</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">sui_system::validator_set::ValidatorSet</a>, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_request_remove_validator_candidate">request_remove_validator_candidate</a>(self: &<b>mut</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">ValidatorSet</a>, ctx: &<b>mut</b> TxContext) {
    <b>let</b> validator_address = ctx.sender();
     <b>assert</b>!(
        self.validator_candidates.contains(validator_address),
        <a href="../sui_system/validator_set.md#sui_system_validator_set_ENotValidatorCandidate">ENotValidatorCandidate</a>
    );
    <b>let</b> wrapper = self.validator_candidates.remove(validator_address);
    <b>let</b> <b>mut</b> <a href="../sui_system/validator.md#sui_system_validator">validator</a> = wrapper.destroy();
    <b>assert</b>!(<a href="../sui_system/validator.md#sui_system_validator">validator</a>.is_preactive(), <a href="../sui_system/validator_set.md#sui_system_validator_set_EValidatorNotCandidate">EValidatorNotCandidate</a>);
    <b>let</b> staking_pool_id = staking_pool_id(&<a href="../sui_system/validator.md#sui_system_validator">validator</a>);
    // Remove the <a href="../sui_system/validator.md#sui_system_validator">validator</a>'s staking pool from mappings.
    self.<a href="../sui_system/validator_set.md#sui_system_validator_set_staking_pool_mappings">staking_pool_mappings</a>.remove(staking_pool_id);
    // Deactivate the staking pool.
    <a href="../sui_system/validator.md#sui_system_validator">validator</a>.deactivate(ctx.epoch());
    // Add to the inactive tables.
    self.inactive_validators.add(
        staking_pool_id,
        <a href="../sui_system/validator_wrapper.md#sui_system_validator_wrapper_create_v1">validator_wrapper::create_v1</a>(<a href="../sui_system/validator.md#sui_system_validator">validator</a>, ctx),
    );
}
</code></pre>



</details>

<a name="sui_system_validator_set_request_add_validator"></a>

## Function `request_add_validator`

Called by <code><a href="../sui_system/sui_system.md#sui_system_sui_system">sui_system</a></code> to add a new validator to <code>pending_active_validators</code>, which will be
processed at the end of epoch.


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_request_add_validator">request_add_validator</a>(self: &<b>mut</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">sui_system::validator_set::ValidatorSet</a>, min_joining_stake_amount: u64, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_request_add_validator">request_add_validator</a>(self: &<b>mut</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">ValidatorSet</a>, min_joining_stake_amount: u64, ctx: &TxContext) {
    <b>let</b> validator_address = ctx.sender();
    <b>assert</b>!(
        self.validator_candidates.contains(validator_address),
        <a href="../sui_system/validator_set.md#sui_system_validator_set_ENotValidatorCandidate">ENotValidatorCandidate</a>
    );
    <b>let</b> wrapper = self.validator_candidates.remove(validator_address);
    <b>let</b> <a href="../sui_system/validator.md#sui_system_validator">validator</a> = wrapper.destroy();
    <b>assert</b>!(
        !<a href="../sui_system/validator_set.md#sui_system_validator_set_is_duplicate_with_active_validator">is_duplicate_with_active_validator</a>(self, &<a href="../sui_system/validator.md#sui_system_validator">validator</a>)
            && !<a href="../sui_system/validator_set.md#sui_system_validator_set_is_duplicate_with_pending_validator">is_duplicate_with_pending_validator</a>(self, &<a href="../sui_system/validator.md#sui_system_validator">validator</a>),
        <a href="../sui_system/validator_set.md#sui_system_validator_set_EDuplicateValidator">EDuplicateValidator</a>
    );
    <b>assert</b>!(<a href="../sui_system/validator.md#sui_system_validator">validator</a>.is_preactive(), <a href="../sui_system/validator_set.md#sui_system_validator_set_EValidatorNotCandidate">EValidatorNotCandidate</a>);
    <b>assert</b>!(<a href="../sui_system/validator.md#sui_system_validator">validator</a>.total_stake_amount() &gt;= min_joining_stake_amount, <a href="../sui_system/validator_set.md#sui_system_validator_set_EMinJoiningStakeNotReached">EMinJoiningStakeNotReached</a>);
    self.pending_active_validators.push_back(<a href="../sui_system/validator.md#sui_system_validator">validator</a>);
}
</code></pre>



</details>

<a name="sui_system_validator_set_assert_no_pending_or_active_duplicates"></a>

## Function `assert_no_pending_or_active_duplicates`



<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_assert_no_pending_or_active_duplicates">assert_no_pending_or_active_duplicates</a>(self: &<a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">sui_system::validator_set::ValidatorSet</a>, <a href="../sui_system/validator.md#sui_system_validator">validator</a>: &<a href="../sui_system/validator.md#sui_system_validator_Validator">sui_system::validator::Validator</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_assert_no_pending_or_active_duplicates">assert_no_pending_or_active_duplicates</a>(self: &<a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">ValidatorSet</a>, <a href="../sui_system/validator.md#sui_system_validator">validator</a>: &Validator) {
    // Validator here must be active or pending, and thus must be identified <b>as</b> duplicate exactly once.
    <b>assert</b>!(
        <a href="../sui_system/validator_set.md#sui_system_validator_set_count_duplicates_vec">count_duplicates_vec</a>(&self.<a href="../sui_system/validator_set.md#sui_system_validator_set_active_validators">active_validators</a>, <a href="../sui_system/validator.md#sui_system_validator">validator</a>) +
            <a href="../sui_system/validator_set.md#sui_system_validator_set_count_duplicates_tablevec">count_duplicates_tablevec</a>(&self.pending_active_validators, <a href="../sui_system/validator.md#sui_system_validator">validator</a>) == 1,
        <a href="../sui_system/validator_set.md#sui_system_validator_set_EDuplicateValidator">EDuplicateValidator</a>
    );
}
</code></pre>



</details>

<a name="sui_system_validator_set_request_remove_validator"></a>

## Function `request_remove_validator`

Called by <code><a href="../sui_system/sui_system.md#sui_system_sui_system">sui_system</a></code>, to remove a validator.
The index of the validator is added to <code>pending_removals</code> and
will be processed at the end of epoch.
Only an active validator can request to be removed.


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_request_remove_validator">request_remove_validator</a>(self: &<b>mut</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">sui_system::validator_set::ValidatorSet</a>, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_request_remove_validator">request_remove_validator</a>(
    self: &<b>mut</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">ValidatorSet</a>,
    ctx: &TxContext,
) {
    <b>let</b> validator_address = ctx.sender();
    <b>let</b> <b>mut</b> validator_index_opt = <a href="../sui_system/validator_set.md#sui_system_validator_set_find_validator">find_validator</a>(&self.<a href="../sui_system/validator_set.md#sui_system_validator_set_active_validators">active_validators</a>, validator_address);
    <b>assert</b>!(validator_index_opt.is_some(), <a href="../sui_system/validator_set.md#sui_system_validator_set_ENotAValidator">ENotAValidator</a>);
    <b>let</b> validator_index = validator_index_opt.extract();
    <b>assert</b>!(
        !self.pending_removals.contains(&validator_index),
        <a href="../sui_system/validator_set.md#sui_system_validator_set_EValidatorAlreadyRemoved">EValidatorAlreadyRemoved</a>
    );
    self.pending_removals.push_back(validator_index);
}
</code></pre>



</details>

<a name="sui_system_validator_set_request_add_stake"></a>

## Function `request_add_stake`

Called by <code><a href="../sui_system/sui_system.md#sui_system_sui_system">sui_system</a></code>, to add a new stake to the validator.
This request is added to the validator's staking pool's pending stake entries, processed at the end
of the epoch.
Aborts in case the staking amount is smaller than MIN_STAKING_THRESHOLD


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_request_add_stake">request_add_stake</a>(self: &<b>mut</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">sui_system::validator_set::ValidatorSet</a>, validator_address: <b>address</b>, stake: <a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;<a href="../sui/sui.md#sui_sui_SUI">sui::sui::SUI</a>&gt;, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakedSui">sui_system::staking_pool::StakedSui</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_request_add_stake">request_add_stake</a>(
    self: &<b>mut</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">ValidatorSet</a>,
    validator_address: <b>address</b>,
    stake: Balance&lt;SUI&gt;,
    ctx: &<b>mut</b> TxContext,
) : StakedSui {
    <b>let</b> sui_amount = stake.value();
    <b>assert</b>!(sui_amount &gt;= <a href="../sui_system/validator_set.md#sui_system_validator_set_MIN_STAKING_THRESHOLD">MIN_STAKING_THRESHOLD</a>, <a href="../sui_system/validator_set.md#sui_system_validator_set_EStakingBelowThreshold">EStakingBelowThreshold</a>);
    <b>let</b> <a href="../sui_system/validator.md#sui_system_validator">validator</a> = <a href="../sui_system/validator_set.md#sui_system_validator_set_get_candidate_or_active_validator_mut">get_candidate_or_active_validator_mut</a>(self, validator_address);
    <a href="../sui_system/validator.md#sui_system_validator">validator</a>.<a href="../sui_system/validator_set.md#sui_system_validator_set_request_add_stake">request_add_stake</a>(stake, ctx.sender(), ctx)
}
</code></pre>



</details>

<a name="sui_system_validator_set_request_withdraw_stake"></a>

## Function `request_withdraw_stake`

Called by <code><a href="../sui_system/sui_system.md#sui_system_sui_system">sui_system</a></code>, to withdraw some share of a stake from the validator. The share to withdraw
is denoted by <code>principal_withdraw_amount</code>. One of two things occurs in this function:
1. If the <code>staked_sui</code> is staked with an active validator, the request is added to the validator's
staking pool's pending stake withdraw entries, processed at the end of the epoch.
2. If the <code>staked_sui</code> was staked with a validator that is no longer active,
the stake and any rewards corresponding to it will be immediately processed.


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_request_withdraw_stake">request_withdraw_stake</a>(self: &<b>mut</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">sui_system::validator_set::ValidatorSet</a>, staked_sui: <a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakedSui">sui_system::staking_pool::StakedSui</a>, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;<a href="../sui/sui.md#sui_sui_SUI">sui::sui::SUI</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_request_withdraw_stake">request_withdraw_stake</a>(
    self: &<b>mut</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">ValidatorSet</a>,
    staked_sui: StakedSui,
    ctx: &TxContext,
) : Balance&lt;SUI&gt; {
    <b>let</b> staking_pool_id = pool_id(&staked_sui);
    <b>let</b> <a href="../sui_system/validator.md#sui_system_validator">validator</a> =
        <b>if</b> (self.<a href="../sui_system/validator_set.md#sui_system_validator_set_staking_pool_mappings">staking_pool_mappings</a>.contains(staking_pool_id)) { // This is an active <a href="../sui_system/validator.md#sui_system_validator">validator</a>.
            <b>let</b> validator_address = self.<a href="../sui_system/validator_set.md#sui_system_validator_set_staking_pool_mappings">staking_pool_mappings</a>[pool_id(&staked_sui)];
            <a href="../sui_system/validator_set.md#sui_system_validator_set_get_candidate_or_active_validator_mut">get_candidate_or_active_validator_mut</a>(self, validator_address)
        } <b>else</b> { // This is an inactive pool.
            <b>assert</b>!(self.inactive_validators.contains(staking_pool_id), <a href="../sui_system/validator_set.md#sui_system_validator_set_ENoPoolFound">ENoPoolFound</a>);
            <b>let</b> wrapper = &<b>mut</b> self.inactive_validators[staking_pool_id];
            wrapper.load_validator_maybe_upgrade()
        };
    <a href="../sui_system/validator.md#sui_system_validator">validator</a>.<a href="../sui_system/validator_set.md#sui_system_validator_set_request_withdraw_stake">request_withdraw_stake</a>(staked_sui, ctx)
}
</code></pre>



</details>

<a name="sui_system_validator_set_convert_to_fungible_staked_sui"></a>

## Function `convert_to_fungible_staked_sui`



<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_convert_to_fungible_staked_sui">convert_to_fungible_staked_sui</a>(self: &<b>mut</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">sui_system::validator_set::ValidatorSet</a>, staked_sui: <a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakedSui">sui_system::staking_pool::StakedSui</a>, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <a href="../sui_system/staking_pool.md#sui_system_staking_pool_FungibleStakedSui">sui_system::staking_pool::FungibleStakedSui</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_convert_to_fungible_staked_sui">convert_to_fungible_staked_sui</a>(
    self: &<b>mut</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">ValidatorSet</a>,
    staked_sui: StakedSui,
    ctx: &<b>mut</b> TxContext,
) : FungibleStakedSui {
    <b>let</b> staking_pool_id = pool_id(&staked_sui);
    <b>let</b> <a href="../sui_system/validator.md#sui_system_validator">validator</a> =
        <b>if</b> (self.<a href="../sui_system/validator_set.md#sui_system_validator_set_staking_pool_mappings">staking_pool_mappings</a>.contains(staking_pool_id)) { // This is an active <a href="../sui_system/validator.md#sui_system_validator">validator</a>.
            <b>let</b> validator_address = self.<a href="../sui_system/validator_set.md#sui_system_validator_set_staking_pool_mappings">staking_pool_mappings</a>[staking_pool_id];
            <a href="../sui_system/validator_set.md#sui_system_validator_set_get_candidate_or_active_validator_mut">get_candidate_or_active_validator_mut</a>(self, validator_address)
        } <b>else</b> { // This is an inactive pool.
            <b>assert</b>!(self.inactive_validators.contains(staking_pool_id), <a href="../sui_system/validator_set.md#sui_system_validator_set_ENoPoolFound">ENoPoolFound</a>);
            <b>let</b> wrapper = &<b>mut</b> self.inactive_validators[staking_pool_id];
            wrapper.load_validator_maybe_upgrade()
        };
    <a href="../sui_system/validator.md#sui_system_validator">validator</a>.<a href="../sui_system/validator_set.md#sui_system_validator_set_convert_to_fungible_staked_sui">convert_to_fungible_staked_sui</a>(staked_sui, ctx)
}
</code></pre>



</details>

<a name="sui_system_validator_set_redeem_fungible_staked_sui"></a>

## Function `redeem_fungible_staked_sui`



<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_redeem_fungible_staked_sui">redeem_fungible_staked_sui</a>(self: &<b>mut</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">sui_system::validator_set::ValidatorSet</a>, fungible_staked_sui: <a href="../sui_system/staking_pool.md#sui_system_staking_pool_FungibleStakedSui">sui_system::staking_pool::FungibleStakedSui</a>, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;<a href="../sui/sui.md#sui_sui_SUI">sui::sui::SUI</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_redeem_fungible_staked_sui">redeem_fungible_staked_sui</a>(
    self: &<b>mut</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">ValidatorSet</a>,
    fungible_staked_sui: FungibleStakedSui,
    ctx: &TxContext,
) : Balance&lt;SUI&gt; {
    <b>let</b> staking_pool_id = fungible_staked_sui_pool_id(&fungible_staked_sui);
    <b>let</b> <a href="../sui_system/validator.md#sui_system_validator">validator</a> =
        <b>if</b> (self.<a href="../sui_system/validator_set.md#sui_system_validator_set_staking_pool_mappings">staking_pool_mappings</a>.contains(staking_pool_id)) { // This is an active <a href="../sui_system/validator.md#sui_system_validator">validator</a>.
            <b>let</b> validator_address = self.<a href="../sui_system/validator_set.md#sui_system_validator_set_staking_pool_mappings">staking_pool_mappings</a>[staking_pool_id];
            <a href="../sui_system/validator_set.md#sui_system_validator_set_get_candidate_or_active_validator_mut">get_candidate_or_active_validator_mut</a>(self, validator_address)
        } <b>else</b> { // This is an inactive pool.
            <b>assert</b>!(self.inactive_validators.contains(staking_pool_id), <a href="../sui_system/validator_set.md#sui_system_validator_set_ENoPoolFound">ENoPoolFound</a>);
            <b>let</b> wrapper = &<b>mut</b> self.inactive_validators[staking_pool_id];
            wrapper.load_validator_maybe_upgrade()
        };
    <a href="../sui_system/validator.md#sui_system_validator">validator</a>.<a href="../sui_system/validator_set.md#sui_system_validator_set_redeem_fungible_staked_sui">redeem_fungible_staked_sui</a>(fungible_staked_sui, ctx)
}
</code></pre>



</details>

<a name="sui_system_validator_set_request_set_commission_rate"></a>

## Function `request_set_commission_rate`



<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_request_set_commission_rate">request_set_commission_rate</a>(self: &<b>mut</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">sui_system::validator_set::ValidatorSet</a>, new_commission_rate: u64, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_request_set_commission_rate">request_set_commission_rate</a>(
    self: &<b>mut</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">ValidatorSet</a>,
    new_commission_rate: u64,
    ctx: &TxContext,
) {
    <b>let</b> validator_address = ctx.sender();
    <b>let</b> <a href="../sui_system/validator.md#sui_system_validator">validator</a> = <a href="../sui_system/validator_set.md#sui_system_validator_set_get_validator_mut">get_validator_mut</a>(&<b>mut</b> self.<a href="../sui_system/validator_set.md#sui_system_validator_set_active_validators">active_validators</a>, validator_address);
    <a href="../sui_system/validator.md#sui_system_validator">validator</a>.<a href="../sui_system/validator_set.md#sui_system_validator_set_request_set_commission_rate">request_set_commission_rate</a>(new_commission_rate);
}
</code></pre>



</details>

<a name="sui_system_validator_set_advance_epoch"></a>

## Function `advance_epoch`

Update the validator set at the end of epoch.
It does the following things:
1. Distribute stake award.
2. Process pending stake deposits and withdraws for each validator (<code>adjust_stake</code>).
3. Process pending stake deposits, and withdraws.
4. Process pending validator application and withdraws.
5. At the end, we calculate the total stake for the new epoch.


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_advance_epoch">advance_epoch</a>(self: &<b>mut</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">sui_system::validator_set::ValidatorSet</a>, computation_reward: &<b>mut</b> <a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;<a href="../sui/sui.md#sui_sui_SUI">sui::sui::SUI</a>&gt;, storage_fund_reward: &<b>mut</b> <a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;<a href="../sui/sui.md#sui_sui_SUI">sui::sui::SUI</a>&gt;, validator_report_records: &<b>mut</b> <a href="../sui/vec_map.md#sui_vec_map_VecMap">sui::vec_map::VecMap</a>&lt;<b>address</b>, <a href="../sui/vec_set.md#sui_vec_set_VecSet">sui::vec_set::VecSet</a>&lt;<b>address</b>&gt;&gt;, reward_slashing_rate: u64, low_stake_threshold: u64, very_low_stake_threshold: u64, low_stake_grace_period: u64, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_advance_epoch">advance_epoch</a>(
    self: &<b>mut</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">ValidatorSet</a>,
    computation_reward: &<b>mut</b> Balance&lt;SUI&gt;,
    storage_fund_reward: &<b>mut</b> Balance&lt;SUI&gt;,
    validator_report_records: &<b>mut</b> VecMap&lt;<b>address</b>, VecSet&lt;<b>address</b>&gt;&gt;,
    reward_slashing_rate: u64,
    low_stake_threshold: u64,
    very_low_stake_threshold: u64,
    low_stake_grace_period: u64,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> new_epoch = ctx.epoch() + 1;
    <b>let</b> total_voting_power = <a href="../sui_system/voting_power.md#sui_system_voting_power_total_voting_power">voting_power::total_voting_power</a>();
    // Compute the reward distribution without taking into account the tallying rule slashing.
    <b>let</b> (unadjusted_staking_reward_amounts, unadjusted_storage_fund_reward_amounts) = <a href="../sui_system/validator_set.md#sui_system_validator_set_compute_unadjusted_reward_distribution">compute_unadjusted_reward_distribution</a>(
        &self.<a href="../sui_system/validator_set.md#sui_system_validator_set_active_validators">active_validators</a>,
        total_voting_power,
        computation_reward.value(),
        storage_fund_reward.value(),
    );
    // Use the tallying rule report records <b>for</b> the epoch to compute validators that will be
    // punished.
    <b>let</b> slashed_validators = <a href="../sui_system/validator_set.md#sui_system_validator_set_compute_slashed_validators">compute_slashed_validators</a>(self, *validator_report_records);
    <b>let</b> total_slashed_validator_voting_power = <a href="../sui_system/validator_set.md#sui_system_validator_set_sum_voting_power_by_addresses">sum_voting_power_by_addresses</a>(&self.<a href="../sui_system/validator_set.md#sui_system_validator_set_active_validators">active_validators</a>, &slashed_validators);
    // Compute the reward adjustments of slashed validators, to be taken into
    // account in adjusted reward computation.
    <b>let</b> (total_staking_reward_adjustment, individual_staking_reward_adjustments,
         total_storage_fund_reward_adjustment, individual_storage_fund_reward_adjustments
        ) =
        <a href="../sui_system/validator_set.md#sui_system_validator_set_compute_reward_adjustments">compute_reward_adjustments</a>(
            <a href="../sui_system/validator_set.md#sui_system_validator_set_get_validator_indices">get_validator_indices</a>(&self.<a href="../sui_system/validator_set.md#sui_system_validator_set_active_validators">active_validators</a>, &slashed_validators),
            reward_slashing_rate,
            &unadjusted_staking_reward_amounts,
            &unadjusted_storage_fund_reward_amounts,
        );
    // Compute the adjusted amounts of stake each <a href="../sui_system/validator.md#sui_system_validator">validator</a> should get given the tallying rule
    // reward adjustments we computed before.
    // `<a href="../sui_system/validator_set.md#sui_system_validator_set_compute_adjusted_reward_distribution">compute_adjusted_reward_distribution</a>` must be called before `<a href="../sui_system/validator_set.md#sui_system_validator_set_distribute_reward">distribute_reward</a>` and `<a href="../sui_system/validator_set.md#sui_system_validator_set_adjust_stake_and_gas_price">adjust_stake_and_gas_price</a>` to
    // make sure we are using the current epoch's stake information to compute reward distribution.
    <b>let</b> (adjusted_staking_reward_amounts, adjusted_storage_fund_reward_amounts) = <a href="../sui_system/validator_set.md#sui_system_validator_set_compute_adjusted_reward_distribution">compute_adjusted_reward_distribution</a>(
        &self.<a href="../sui_system/validator_set.md#sui_system_validator_set_active_validators">active_validators</a>,
        total_voting_power,
        total_slashed_validator_voting_power,
        unadjusted_staking_reward_amounts,
        unadjusted_storage_fund_reward_amounts,
        total_staking_reward_adjustment,
        individual_staking_reward_adjustments,
        total_storage_fund_reward_adjustment,
        individual_storage_fund_reward_adjustments
    );
    // Distribute the rewards before adjusting stake so that we immediately start compounding
    // the rewards <b>for</b> validators and stakers.
    <a href="../sui_system/validator_set.md#sui_system_validator_set_distribute_reward">distribute_reward</a>(
        &<b>mut</b> self.<a href="../sui_system/validator_set.md#sui_system_validator_set_active_validators">active_validators</a>,
        &adjusted_staking_reward_amounts,
        &adjusted_storage_fund_reward_amounts,
        computation_reward,
        storage_fund_reward,
        ctx
    );
    <a href="../sui_system/validator_set.md#sui_system_validator_set_adjust_stake_and_gas_price">adjust_stake_and_gas_price</a>(&<b>mut</b> self.<a href="../sui_system/validator_set.md#sui_system_validator_set_active_validators">active_validators</a>);
    <a href="../sui_system/validator_set.md#sui_system_validator_set_process_pending_stakes_and_withdraws">process_pending_stakes_and_withdraws</a>(&<b>mut</b> self.<a href="../sui_system/validator_set.md#sui_system_validator_set_active_validators">active_validators</a>, ctx);
    // Emit events after we have processed all the rewards distribution and pending stakes.
    <a href="../sui_system/validator_set.md#sui_system_validator_set_emit_validator_epoch_events">emit_validator_epoch_events</a>(new_epoch, &self.<a href="../sui_system/validator_set.md#sui_system_validator_set_active_validators">active_validators</a>, &adjusted_staking_reward_amounts,
        &adjusted_storage_fund_reward_amounts, validator_report_records, &slashed_validators);
    // Note that all their staged next epoch metadata will be effectuated below.
    <a href="../sui_system/validator_set.md#sui_system_validator_set_process_pending_validators">process_pending_validators</a>(self, new_epoch);
    <a href="../sui_system/validator_set.md#sui_system_validator_set_process_pending_removals">process_pending_removals</a>(self, validator_report_records, ctx);
    // kick low stake validators out.
    <a href="../sui_system/validator_set.md#sui_system_validator_set_update_and_process_low_stake_departures">update_and_process_low_stake_departures</a>(
        self,
        low_stake_threshold,
        very_low_stake_threshold,
        low_stake_grace_period,
        validator_report_records,
        ctx
    );
    self.<a href="../sui_system/validator_set.md#sui_system_validator_set_total_stake">total_stake</a> = <a href="../sui_system/validator_set.md#sui_system_validator_set_calculate_total_stakes">calculate_total_stakes</a>(&self.<a href="../sui_system/validator_set.md#sui_system_validator_set_active_validators">active_validators</a>);
    <a href="../sui_system/voting_power.md#sui_system_voting_power_set_voting_power">voting_power::set_voting_power</a>(&<b>mut</b> self.<a href="../sui_system/validator_set.md#sui_system_validator_set_active_validators">active_validators</a>);
    // At this point, self.<a href="../sui_system/validator_set.md#sui_system_validator_set_active_validators">active_validators</a> are updated <b>for</b> next epoch.
    // Now we process the staged <a href="../sui_system/validator.md#sui_system_validator">validator</a> metadata.
    <a href="../sui_system/validator_set.md#sui_system_validator_set_effectuate_staged_metadata">effectuate_staged_metadata</a>(self);
}
</code></pre>



</details>

<a name="sui_system_validator_set_update_and_process_low_stake_departures"></a>

## Function `update_and_process_low_stake_departures`



<pre><code><b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_update_and_process_low_stake_departures">update_and_process_low_stake_departures</a>(self: &<b>mut</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">sui_system::validator_set::ValidatorSet</a>, low_stake_threshold: u64, very_low_stake_threshold: u64, low_stake_grace_period: u64, validator_report_records: &<b>mut</b> <a href="../sui/vec_map.md#sui_vec_map_VecMap">sui::vec_map::VecMap</a>&lt;<b>address</b>, <a href="../sui/vec_set.md#sui_vec_set_VecSet">sui::vec_set::VecSet</a>&lt;<b>address</b>&gt;&gt;, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_update_and_process_low_stake_departures">update_and_process_low_stake_departures</a>(
    self: &<b>mut</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">ValidatorSet</a>,
    low_stake_threshold: u64,
    very_low_stake_threshold: u64,
    low_stake_grace_period: u64,
    validator_report_records: &<b>mut</b> VecMap&lt;<b>address</b>, VecSet&lt;<b>address</b>&gt;&gt;,
    ctx: &<b>mut</b> TxContext
) {
    // Iterate through all the active validators, record their low stake status, and kick them out <b>if</b> the condition is met.
    <b>let</b> <b>mut</b> i = self.<a href="../sui_system/validator_set.md#sui_system_validator_set_active_validators">active_validators</a>.length();
    <b>while</b> (i &gt; 0) {
        i = i - 1;
        <b>let</b> validator_ref = &self.<a href="../sui_system/validator_set.md#sui_system_validator_set_active_validators">active_validators</a>[i];
        <b>let</b> validator_address = validator_ref.sui_address();
        <b>let</b> stake = validator_ref.total_stake_amount();
        <b>if</b> (stake &gt;= low_stake_threshold) {
            // The <a href="../sui_system/validator.md#sui_system_validator">validator</a> is safe. We remove their <b>entry</b> from the at_risk map <b>if</b> there exists one.
            <b>if</b> (self.at_risk_validators.contains(&validator_address)) {
               self.at_risk_validators.remove(&validator_address);
            }
        } <b>else</b> <b>if</b> (stake &gt;= very_low_stake_threshold) {
            // The stake is a bit below the threshold so we increment the <b>entry</b> of the <a href="../sui_system/validator.md#sui_system_validator">validator</a> in the map.
            <b>let</b> new_low_stake_period =
                <b>if</b> (self.at_risk_validators.contains(&validator_address)) {
                    <b>let</b> num_epochs = &<b>mut</b> self.at_risk_validators[&validator_address];
                    *num_epochs = *num_epochs + 1;
                    *num_epochs
                } <b>else</b> {
                    self.at_risk_validators.insert(validator_address, 1);
                    1
                };
            // If the grace period <b>has</b> passed, the <a href="../sui_system/validator.md#sui_system_validator">validator</a> <b>has</b> to leave us.
            <b>if</b> (new_low_stake_period &gt; low_stake_grace_period) {
                <b>let</b> <a href="../sui_system/validator.md#sui_system_validator">validator</a> = self.<a href="../sui_system/validator_set.md#sui_system_validator_set_active_validators">active_validators</a>.remove(i);
                <a href="../sui_system/validator_set.md#sui_system_validator_set_process_validator_departure">process_validator_departure</a>(self, <a href="../sui_system/validator.md#sui_system_validator">validator</a>, validator_report_records, <b>false</b> /* the <a href="../sui_system/validator.md#sui_system_validator">validator</a> is kicked out involuntarily */, ctx);
            }
        } <b>else</b> {
            // The <a href="../sui_system/validator.md#sui_system_validator">validator</a>'s stake is lower than the very low threshold so we kick them out immediately.
            <b>let</b> <a href="../sui_system/validator.md#sui_system_validator">validator</a> = self.<a href="../sui_system/validator_set.md#sui_system_validator_set_active_validators">active_validators</a>.remove(i);
            <a href="../sui_system/validator_set.md#sui_system_validator_set_process_validator_departure">process_validator_departure</a>(self, <a href="../sui_system/validator.md#sui_system_validator">validator</a>, validator_report_records, <b>false</b> /* the <a href="../sui_system/validator.md#sui_system_validator">validator</a> is kicked out involuntarily */, ctx);
        }
    }
}
</code></pre>



</details>

<a name="sui_system_validator_set_effectuate_staged_metadata"></a>

## Function `effectuate_staged_metadata`

Effectutate pending next epoch metadata if they are staged.


<pre><code><b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_effectuate_staged_metadata">effectuate_staged_metadata</a>(self: &<b>mut</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">sui_system::validator_set::ValidatorSet</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_effectuate_staged_metadata">effectuate_staged_metadata</a>(
    self: &<b>mut</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">ValidatorSet</a>,
) {
    <b>let</b> num_validators = self.<a href="../sui_system/validator_set.md#sui_system_validator_set_active_validators">active_validators</a>.length();
    <b>let</b> <b>mut</b> i = 0;
    <b>while</b> (i &lt; num_validators) {
        <b>let</b> <a href="../sui_system/validator.md#sui_system_validator">validator</a> = &<b>mut</b> self.<a href="../sui_system/validator_set.md#sui_system_validator_set_active_validators">active_validators</a>[i];
        <a href="../sui_system/validator.md#sui_system_validator">validator</a>.<a href="../sui_system/validator_set.md#sui_system_validator_set_effectuate_staged_metadata">effectuate_staged_metadata</a>();
        i = i + 1;
    }
}
</code></pre>



</details>

<a name="sui_system_validator_set_derive_reference_gas_price"></a>

## Function `derive_reference_gas_price`

Called by <code><a href="../sui_system/sui_system.md#sui_system_sui_system">sui_system</a></code> to derive reference gas price for the new epoch.
Derive the reference gas price based on the gas price quote submitted by each validator.
The returned gas price should be greater than or equal to 2/3 of the validators submitted
gas price, weighted by stake.


<pre><code><b>public</b> <b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_derive_reference_gas_price">derive_reference_gas_price</a>(self: &<a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">sui_system::validator_set::ValidatorSet</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_derive_reference_gas_price">derive_reference_gas_price</a>(self: &<a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">ValidatorSet</a>): u64 {
    <b>let</b> vs = &self.<a href="../sui_system/validator_set.md#sui_system_validator_set_active_validators">active_validators</a>;
    <b>let</b> num_validators = vs.length();
    <b>let</b> <b>mut</b> entries = vector[];
    <b>let</b> <b>mut</b> i = 0;
    <b>while</b> (i &lt; num_validators) {
        <b>let</b> v = &vs[i];
        entries.push_back(
            pq::new_entry(v.gas_price(), v.<a href="../sui_system/voting_power.md#sui_system_voting_power">voting_power</a>())
        );
        i = i + 1;
    };
    // Build a priority queue that will pop entries with gas price from the highest to the lowest.
    <b>let</b> <b>mut</b> pq = pq::new(entries);
    <b>let</b> <b>mut</b> sum = 0;
    <b>let</b> threshold = <a href="../sui_system/voting_power.md#sui_system_voting_power_total_voting_power">voting_power::total_voting_power</a>() - <a href="../sui_system/voting_power.md#sui_system_voting_power_quorum_threshold">voting_power::quorum_threshold</a>();
    <b>let</b> <b>mut</b> result = 0;
    <b>while</b> (sum &lt; threshold) {
        <b>let</b> (gas_price, <a href="../sui_system/voting_power.md#sui_system_voting_power">voting_power</a>) = pq.pop_max();
        result = gas_price;
        sum = sum + <a href="../sui_system/voting_power.md#sui_system_voting_power">voting_power</a>;
    };
    result
}
</code></pre>



</details>

<a name="sui_system_validator_set_total_stake"></a>

## Function `total_stake`



<pre><code><b>public</b> <b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_total_stake">total_stake</a>(self: &<a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">sui_system::validator_set::ValidatorSet</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_total_stake">total_stake</a>(self: &<a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">ValidatorSet</a>): u64 {
    self.<a href="../sui_system/validator_set.md#sui_system_validator_set_total_stake">total_stake</a>
}
</code></pre>



</details>

<a name="sui_system_validator_set_validator_total_stake_amount"></a>

## Function `validator_total_stake_amount`



<pre><code><b>public</b> <b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_validator_total_stake_amount">validator_total_stake_amount</a>(self: &<a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">sui_system::validator_set::ValidatorSet</a>, validator_address: <b>address</b>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_validator_total_stake_amount">validator_total_stake_amount</a>(self: &<a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">ValidatorSet</a>, validator_address: <b>address</b>): u64 {
    <b>let</b> <a href="../sui_system/validator.md#sui_system_validator">validator</a> = <a href="../sui_system/validator_set.md#sui_system_validator_set_get_validator_ref">get_validator_ref</a>(&self.<a href="../sui_system/validator_set.md#sui_system_validator_set_active_validators">active_validators</a>, validator_address);
    <a href="../sui_system/validator.md#sui_system_validator">validator</a>.total_stake_amount()
}
</code></pre>



</details>

<a name="sui_system_validator_set_validator_stake_amount"></a>

## Function `validator_stake_amount`



<pre><code><b>public</b> <b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_validator_stake_amount">validator_stake_amount</a>(self: &<a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">sui_system::validator_set::ValidatorSet</a>, validator_address: <b>address</b>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_validator_stake_amount">validator_stake_amount</a>(self: &<a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">ValidatorSet</a>, validator_address: <b>address</b>): u64 {
    <b>let</b> <a href="../sui_system/validator.md#sui_system_validator">validator</a> = <a href="../sui_system/validator_set.md#sui_system_validator_set_get_validator_ref">get_validator_ref</a>(&self.<a href="../sui_system/validator_set.md#sui_system_validator_set_active_validators">active_validators</a>, validator_address);
    <a href="../sui_system/validator.md#sui_system_validator">validator</a>.stake_amount()
}
</code></pre>



</details>

<a name="sui_system_validator_set_validator_voting_power"></a>

## Function `validator_voting_power`



<pre><code><b>public</b> <b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_validator_voting_power">validator_voting_power</a>(self: &<a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">sui_system::validator_set::ValidatorSet</a>, validator_address: <b>address</b>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_validator_voting_power">validator_voting_power</a>(self: &<a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">ValidatorSet</a>, validator_address: <b>address</b>): u64 {
    <b>let</b> <a href="../sui_system/validator.md#sui_system_validator">validator</a> = <a href="../sui_system/validator_set.md#sui_system_validator_set_get_validator_ref">get_validator_ref</a>(&self.<a href="../sui_system/validator_set.md#sui_system_validator_set_active_validators">active_validators</a>, validator_address);
    <a href="../sui_system/validator.md#sui_system_validator">validator</a>.<a href="../sui_system/voting_power.md#sui_system_voting_power">voting_power</a>()
}
</code></pre>



</details>

<a name="sui_system_validator_set_validator_staking_pool_id"></a>

## Function `validator_staking_pool_id`



<pre><code><b>public</b> <b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_validator_staking_pool_id">validator_staking_pool_id</a>(self: &<a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">sui_system::validator_set::ValidatorSet</a>, validator_address: <b>address</b>): <a href="../sui/object.md#sui_object_ID">sui::object::ID</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_validator_staking_pool_id">validator_staking_pool_id</a>(self: &<a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">ValidatorSet</a>, validator_address: <b>address</b>): ID {
    <b>let</b> <a href="../sui_system/validator.md#sui_system_validator">validator</a> = <a href="../sui_system/validator_set.md#sui_system_validator_set_get_validator_ref">get_validator_ref</a>(&self.<a href="../sui_system/validator_set.md#sui_system_validator_set_active_validators">active_validators</a>, validator_address);
    <a href="../sui_system/validator.md#sui_system_validator">validator</a>.staking_pool_id()
}
</code></pre>



</details>

<a name="sui_system_validator_set_staking_pool_mappings"></a>

## Function `staking_pool_mappings`



<pre><code><b>public</b> <b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_staking_pool_mappings">staking_pool_mappings</a>(self: &<a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">sui_system::validator_set::ValidatorSet</a>): &<a href="../sui/table.md#sui_table_Table">sui::table::Table</a>&lt;<a href="../sui/object.md#sui_object_ID">sui::object::ID</a>, <b>address</b>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_staking_pool_mappings">staking_pool_mappings</a>(self: &<a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">ValidatorSet</a>): &Table&lt;ID, <b>address</b>&gt; {
    &self.<a href="../sui_system/validator_set.md#sui_system_validator_set_staking_pool_mappings">staking_pool_mappings</a>
}
</code></pre>



</details>

<a name="sui_system_validator_set_validator_address_by_pool_id"></a>

## Function `validator_address_by_pool_id`



<pre><code><b>public</b> <b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_validator_address_by_pool_id">validator_address_by_pool_id</a>(self: &<b>mut</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">sui_system::validator_set::ValidatorSet</a>, pool_id: &<a href="../sui/object.md#sui_object_ID">sui::object::ID</a>): <b>address</b>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_validator_address_by_pool_id">validator_address_by_pool_id</a>(self: &<b>mut</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">ValidatorSet</a>, pool_id: &ID): <b>address</b> {
    // If the pool id is recorded in the mapping, then it must be either candidate or active.
    <b>if</b> (self.<a href="../sui_system/validator_set.md#sui_system_validator_set_staking_pool_mappings">staking_pool_mappings</a>.contains(*pool_id)) {
        self.<a href="../sui_system/validator_set.md#sui_system_validator_set_staking_pool_mappings">staking_pool_mappings</a>[*pool_id]
    } <b>else</b> { // otherwise it's inactive
        <b>let</b> wrapper = &<b>mut</b> self.inactive_validators[*pool_id];
        <b>let</b> <a href="../sui_system/validator.md#sui_system_validator">validator</a> = wrapper.load_validator_maybe_upgrade();
        <a href="../sui_system/validator.md#sui_system_validator">validator</a>.sui_address()
    }
}
</code></pre>



</details>

<a name="sui_system_validator_set_pool_exchange_rates"></a>

## Function `pool_exchange_rates`



<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_pool_exchange_rates">pool_exchange_rates</a>(self: &<b>mut</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">sui_system::validator_set::ValidatorSet</a>, pool_id: &<a href="../sui/object.md#sui_object_ID">sui::object::ID</a>): &<a href="../sui/table.md#sui_table_Table">sui::table::Table</a>&lt;u64, <a href="../sui_system/staking_pool.md#sui_system_staking_pool_PoolTokenExchangeRate">sui_system::staking_pool::PoolTokenExchangeRate</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_pool_exchange_rates">pool_exchange_rates</a>(
    self: &<b>mut</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">ValidatorSet</a>, pool_id: &ID
) : &Table&lt;u64, PoolTokenExchangeRate&gt; {
    <b>let</b> <a href="../sui_system/validator.md#sui_system_validator">validator</a> =
        // If the pool id is recorded in the mapping, then it must be either candidate or active.
        <b>if</b> (self.<a href="../sui_system/validator_set.md#sui_system_validator_set_staking_pool_mappings">staking_pool_mappings</a>.contains(*pool_id)) {
            <b>let</b> validator_address = self.<a href="../sui_system/validator_set.md#sui_system_validator_set_staking_pool_mappings">staking_pool_mappings</a>[*pool_id];
            <a href="../sui_system/validator_set.md#sui_system_validator_set_get_active_or_pending_or_candidate_validator_ref">get_active_or_pending_or_candidate_validator_ref</a>(self, validator_address, <a href="../sui_system/validator_set.md#sui_system_validator_set_ANY_VALIDATOR">ANY_VALIDATOR</a>)
        } <b>else</b> { // otherwise it's inactive
            <b>let</b> wrapper = &<b>mut</b> self.inactive_validators[*pool_id];
            wrapper.load_validator_maybe_upgrade()
        };
	<a href="../sui_system/validator.md#sui_system_validator">validator</a>.get_staking_pool_ref().exchange_rates()
}
</code></pre>



</details>

<a name="sui_system_validator_set_next_epoch_validator_count"></a>

## Function `next_epoch_validator_count`

Get the total number of validators in the next epoch.


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_next_epoch_validator_count">next_epoch_validator_count</a>(self: &<a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">sui_system::validator_set::ValidatorSet</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_next_epoch_validator_count">next_epoch_validator_count</a>(self: &<a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">ValidatorSet</a>): u64 {
    self.<a href="../sui_system/validator_set.md#sui_system_validator_set_active_validators">active_validators</a>.length() - self.pending_removals.length() + self.pending_active_validators.length()
}
</code></pre>



</details>

<a name="sui_system_validator_set_is_active_validator_by_sui_address"></a>

## Function `is_active_validator_by_sui_address`

Returns true iff the address exists in active validators.


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_is_active_validator_by_sui_address">is_active_validator_by_sui_address</a>(self: &<a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">sui_system::validator_set::ValidatorSet</a>, validator_address: <b>address</b>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_is_active_validator_by_sui_address">is_active_validator_by_sui_address</a>(
    self: &<a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">ValidatorSet</a>,
    validator_address: <b>address</b>,
): bool {
   <a href="../sui_system/validator_set.md#sui_system_validator_set_find_validator">find_validator</a>(&self.<a href="../sui_system/validator_set.md#sui_system_validator_set_active_validators">active_validators</a>, validator_address).is_some()
}
</code></pre>



</details>

<a name="sui_system_validator_set_is_duplicate_with_active_validator"></a>

## Function `is_duplicate_with_active_validator`

Checks whether <code>new_validator</code> is duplicate with any currently active validators.
It differs from <code><a href="../sui_system/validator_set.md#sui_system_validator_set_is_active_validator_by_sui_address">is_active_validator_by_sui_address</a></code> in that the former checks
only the sui address but this function looks at more metadata.


<pre><code><b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_is_duplicate_with_active_validator">is_duplicate_with_active_validator</a>(self: &<a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">sui_system::validator_set::ValidatorSet</a>, new_validator: &<a href="../sui_system/validator.md#sui_system_validator_Validator">sui_system::validator::Validator</a>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_is_duplicate_with_active_validator">is_duplicate_with_active_validator</a>(self: &<a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">ValidatorSet</a>, new_validator: &Validator): bool {
    <a href="../sui_system/validator_set.md#sui_system_validator_set_is_duplicate_validator">is_duplicate_validator</a>(&self.<a href="../sui_system/validator_set.md#sui_system_validator_set_active_validators">active_validators</a>, new_validator)
}
</code></pre>



</details>

<a name="sui_system_validator_set_is_duplicate_validator"></a>

## Function `is_duplicate_validator`



<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_is_duplicate_validator">is_duplicate_validator</a>(validators: &vector&lt;<a href="../sui_system/validator.md#sui_system_validator_Validator">sui_system::validator::Validator</a>&gt;, new_validator: &<a href="../sui_system/validator.md#sui_system_validator_Validator">sui_system::validator::Validator</a>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_is_duplicate_validator">is_duplicate_validator</a>(validators: &vector&lt;Validator&gt;, new_validator: &Validator): bool {
    <a href="../sui_system/validator_set.md#sui_system_validator_set_count_duplicates_vec">count_duplicates_vec</a>(validators, new_validator) &gt; 0
}
</code></pre>



</details>

<a name="sui_system_validator_set_count_duplicates_vec"></a>

## Function `count_duplicates_vec`



<pre><code><b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_count_duplicates_vec">count_duplicates_vec</a>(validators: &vector&lt;<a href="../sui_system/validator.md#sui_system_validator_Validator">sui_system::validator::Validator</a>&gt;, <a href="../sui_system/validator.md#sui_system_validator">validator</a>: &<a href="../sui_system/validator.md#sui_system_validator_Validator">sui_system::validator::Validator</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_count_duplicates_vec">count_duplicates_vec</a>(validators: &vector&lt;Validator&gt;, <a href="../sui_system/validator.md#sui_system_validator">validator</a>: &Validator): u64 {
    <b>let</b> len = validators.length();
    <b>let</b> <b>mut</b> i = 0;
    <b>let</b> <b>mut</b> result = 0;
    <b>while</b> (i &lt; len) {
        <b>let</b> v = &validators[i];
        <b>if</b> (v.is_duplicate(<a href="../sui_system/validator.md#sui_system_validator">validator</a>)) {
            result = result + 1;
        };
        i = i + 1;
    };
    result
}
</code></pre>



</details>

<a name="sui_system_validator_set_is_duplicate_with_pending_validator"></a>

## Function `is_duplicate_with_pending_validator`

Checks whether <code>new_validator</code> is duplicate with any currently pending validators.


<pre><code><b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_is_duplicate_with_pending_validator">is_duplicate_with_pending_validator</a>(self: &<a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">sui_system::validator_set::ValidatorSet</a>, new_validator: &<a href="../sui_system/validator.md#sui_system_validator_Validator">sui_system::validator::Validator</a>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_is_duplicate_with_pending_validator">is_duplicate_with_pending_validator</a>(self: &<a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">ValidatorSet</a>, new_validator: &Validator): bool {
    <a href="../sui_system/validator_set.md#sui_system_validator_set_count_duplicates_tablevec">count_duplicates_tablevec</a>(&self.pending_active_validators, new_validator) &gt; 0
}
</code></pre>



</details>

<a name="sui_system_validator_set_count_duplicates_tablevec"></a>

## Function `count_duplicates_tablevec`



<pre><code><b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_count_duplicates_tablevec">count_duplicates_tablevec</a>(validators: &<a href="../sui/table_vec.md#sui_table_vec_TableVec">sui::table_vec::TableVec</a>&lt;<a href="../sui_system/validator.md#sui_system_validator_Validator">sui_system::validator::Validator</a>&gt;, <a href="../sui_system/validator.md#sui_system_validator">validator</a>: &<a href="../sui_system/validator.md#sui_system_validator_Validator">sui_system::validator::Validator</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_count_duplicates_tablevec">count_duplicates_tablevec</a>(validators: &TableVec&lt;Validator&gt;, <a href="../sui_system/validator.md#sui_system_validator">validator</a>: &Validator): u64 {
    <b>let</b> len = validators.length();
    <b>let</b> <b>mut</b> i = 0;
    <b>let</b> <b>mut</b> result = 0;
    <b>while</b> (i &lt; len) {
        <b>let</b> v = &validators[i];
        <b>if</b> (v.is_duplicate(<a href="../sui_system/validator.md#sui_system_validator">validator</a>)) {
            result = result + 1;
        };
        i = i + 1;
    };
    result
}
</code></pre>



</details>

<a name="sui_system_validator_set_get_candidate_or_active_validator_mut"></a>

## Function `get_candidate_or_active_validator_mut`

Get mutable reference to either a candidate or an active validator by address.


<pre><code><b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_get_candidate_or_active_validator_mut">get_candidate_or_active_validator_mut</a>(self: &<b>mut</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">sui_system::validator_set::ValidatorSet</a>, validator_address: <b>address</b>): &<b>mut</b> <a href="../sui_system/validator.md#sui_system_validator_Validator">sui_system::validator::Validator</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_get_candidate_or_active_validator_mut">get_candidate_or_active_validator_mut</a>(self: &<b>mut</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">ValidatorSet</a>, validator_address: <b>address</b>): &<b>mut</b> Validator {
    <b>if</b> (self.validator_candidates.contains(validator_address)) {
        <b>let</b> wrapper = &<b>mut</b> self.validator_candidates[validator_address];
        <b>return</b> wrapper.load_validator_maybe_upgrade()
    };
    <a href="../sui_system/validator_set.md#sui_system_validator_set_get_validator_mut">get_validator_mut</a>(&<b>mut</b> self.<a href="../sui_system/validator_set.md#sui_system_validator_set_active_validators">active_validators</a>, validator_address)
}
</code></pre>



</details>

<a name="sui_system_validator_set_find_validator"></a>

## Function `find_validator`

Find validator by <code>validator_address</code>, in <code>validators</code>.
Returns (true, index) if the validator is found, and the index is its index in the list.
If not found, returns (false, 0).


<pre><code><b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_find_validator">find_validator</a>(validators: &vector&lt;<a href="../sui_system/validator.md#sui_system_validator_Validator">sui_system::validator::Validator</a>&gt;, validator_address: <b>address</b>): <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;u64&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_find_validator">find_validator</a>(validators: &vector&lt;Validator&gt;, validator_address: <b>address</b>): Option&lt;u64&gt; {
    <b>let</b> length = validators.length();
    <b>let</b> <b>mut</b> i = 0;
    <b>while</b> (i &lt; length) {
        <b>let</b> v = &validators[i];
        <b>if</b> (v.sui_address() == validator_address) {
            <b>return</b> option::some(i)
        };
        i = i + 1;
    };
    option::none()
}
</code></pre>



</details>

<a name="sui_system_validator_set_find_validator_from_table_vec"></a>

## Function `find_validator_from_table_vec`

Find validator by <code>validator_address</code>, in <code>validators</code>.
Returns (true, index) if the validator is found, and the index is its index in the list.
If not found, returns (false, 0).


<pre><code><b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_find_validator_from_table_vec">find_validator_from_table_vec</a>(validators: &<a href="../sui/table_vec.md#sui_table_vec_TableVec">sui::table_vec::TableVec</a>&lt;<a href="../sui_system/validator.md#sui_system_validator_Validator">sui_system::validator::Validator</a>&gt;, validator_address: <b>address</b>): <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;u64&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_find_validator_from_table_vec">find_validator_from_table_vec</a>(validators: &TableVec&lt;Validator&gt;, validator_address: <b>address</b>): Option&lt;u64&gt; {
    <b>let</b> length = validators.length();
    <b>let</b> <b>mut</b> i = 0;
    <b>while</b> (i &lt; length) {
        <b>let</b> v = &validators[i];
        <b>if</b> (v.sui_address() == validator_address) {
            <b>return</b> option::some(i)
        };
        i = i + 1;
    };
    option::none()
}
</code></pre>



</details>

<a name="sui_system_validator_set_get_validator_indices"></a>

## Function `get_validator_indices`

Given a vector of validator addresses, return their indices in the validator set.
Aborts if any address isn't in the given validator set.


<pre><code><b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_get_validator_indices">get_validator_indices</a>(validators: &vector&lt;<a href="../sui_system/validator.md#sui_system_validator_Validator">sui_system::validator::Validator</a>&gt;, validator_addresses: &vector&lt;<b>address</b>&gt;): vector&lt;u64&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_get_validator_indices">get_validator_indices</a>(validators: &vector&lt;Validator&gt;, validator_addresses: &vector&lt;<b>address</b>&gt;): vector&lt;u64&gt; {
    <b>let</b> length = validator_addresses.length();
    <b>let</b> <b>mut</b> i = 0;
    <b>let</b> <b>mut</b> res = vector[];
    <b>while</b> (i &lt; length) {
        <b>let</b> addr = validator_addresses[i];
        <b>let</b> index_opt = <a href="../sui_system/validator_set.md#sui_system_validator_set_find_validator">find_validator</a>(validators, addr);
        <b>assert</b>!(index_opt.is_some(), <a href="../sui_system/validator_set.md#sui_system_validator_set_ENotAValidator">ENotAValidator</a>);
        res.push_back(index_opt.destroy_some());
        i = i + 1;
    };
    res
}
</code></pre>



</details>

<a name="sui_system_validator_set_get_validator_mut"></a>

## Function `get_validator_mut`



<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_get_validator_mut">get_validator_mut</a>(validators: &<b>mut</b> vector&lt;<a href="../sui_system/validator.md#sui_system_validator_Validator">sui_system::validator::Validator</a>&gt;, validator_address: <b>address</b>): &<b>mut</b> <a href="../sui_system/validator.md#sui_system_validator_Validator">sui_system::validator::Validator</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_get_validator_mut">get_validator_mut</a>(
    validators: &<b>mut</b> vector&lt;Validator&gt;,
    validator_address: <b>address</b>,
): &<b>mut</b> Validator {
    <b>let</b> <b>mut</b> validator_index_opt = <a href="../sui_system/validator_set.md#sui_system_validator_set_find_validator">find_validator</a>(validators, validator_address);
    <b>assert</b>!(validator_index_opt.is_some(), <a href="../sui_system/validator_set.md#sui_system_validator_set_ENotAValidator">ENotAValidator</a>);
    <b>let</b> validator_index = validator_index_opt.extract();
    &<b>mut</b> validators[validator_index]
}
</code></pre>



</details>

<a name="sui_system_validator_set_get_active_or_pending_or_candidate_validator_mut"></a>

## Function `get_active_or_pending_or_candidate_validator_mut`

Get mutable reference to an active or (if active does not exist) pending or (if pending and
active do not exist) or candidate validator by address.
Note: this function should be called carefully, only after verifying the transaction
sender has the ability to modify the <code>Validator</code>.


<pre><code><b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_get_active_or_pending_or_candidate_validator_mut">get_active_or_pending_or_candidate_validator_mut</a>(self: &<b>mut</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">sui_system::validator_set::ValidatorSet</a>, validator_address: <b>address</b>, include_candidate: bool): &<b>mut</b> <a href="../sui_system/validator.md#sui_system_validator_Validator">sui_system::validator::Validator</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_get_active_or_pending_or_candidate_validator_mut">get_active_or_pending_or_candidate_validator_mut</a>(
    self: &<b>mut</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">ValidatorSet</a>,
    validator_address: <b>address</b>,
    include_candidate: bool,
): &<b>mut</b> Validator {
    <b>let</b> <b>mut</b> validator_index_opt = <a href="../sui_system/validator_set.md#sui_system_validator_set_find_validator">find_validator</a>(&self.<a href="../sui_system/validator_set.md#sui_system_validator_set_active_validators">active_validators</a>, validator_address);
    <b>if</b> (validator_index_opt.is_some()) {
        <b>let</b> validator_index = validator_index_opt.extract();
        <b>return</b> &<b>mut</b> self.<a href="../sui_system/validator_set.md#sui_system_validator_set_active_validators">active_validators</a>[validator_index]
    };
    <b>let</b> <b>mut</b> validator_index_opt = <a href="../sui_system/validator_set.md#sui_system_validator_set_find_validator_from_table_vec">find_validator_from_table_vec</a>(&self.pending_active_validators, validator_address);
    // consider both pending validators and the candidate ones
    <b>if</b> (validator_index_opt.is_some()) {
        <b>let</b> validator_index = validator_index_opt.extract();
        <b>return</b> &<b>mut</b> self.pending_active_validators[validator_index]
    };
    <b>assert</b>!(include_candidate, <a href="../sui_system/validator_set.md#sui_system_validator_set_ENotActiveOrPendingValidator">ENotActiveOrPendingValidator</a>);
    <b>let</b> wrapper = &<b>mut</b> self.validator_candidates[validator_address];
    wrapper.load_validator_maybe_upgrade()
}
</code></pre>



</details>

<a name="sui_system_validator_set_get_validator_mut_with_verified_cap"></a>

## Function `get_validator_mut_with_verified_cap`



<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_get_validator_mut_with_verified_cap">get_validator_mut_with_verified_cap</a>(self: &<b>mut</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">sui_system::validator_set::ValidatorSet</a>, verified_cap: &<a href="../sui_system/validator_cap.md#sui_system_validator_cap_ValidatorOperationCap">sui_system::validator_cap::ValidatorOperationCap</a>, include_candidate: bool): &<b>mut</b> <a href="../sui_system/validator.md#sui_system_validator_Validator">sui_system::validator::Validator</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_get_validator_mut_with_verified_cap">get_validator_mut_with_verified_cap</a>(
    self: &<b>mut</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">ValidatorSet</a>,
    verified_cap: &ValidatorOperationCap,
    include_candidate: bool,
): &<b>mut</b> Validator {
    <a href="../sui_system/validator_set.md#sui_system_validator_set_get_active_or_pending_or_candidate_validator_mut">get_active_or_pending_or_candidate_validator_mut</a>(self, *verified_cap.verified_operation_cap_address(), include_candidate)
}
</code></pre>



</details>

<a name="sui_system_validator_set_get_validator_mut_with_ctx"></a>

## Function `get_validator_mut_with_ctx`



<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_get_validator_mut_with_ctx">get_validator_mut_with_ctx</a>(self: &<b>mut</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">sui_system::validator_set::ValidatorSet</a>, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): &<b>mut</b> <a href="../sui_system/validator.md#sui_system_validator_Validator">sui_system::validator::Validator</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_get_validator_mut_with_ctx">get_validator_mut_with_ctx</a>(
    self: &<b>mut</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">ValidatorSet</a>,
    ctx: &TxContext,
): &<b>mut</b> Validator {
    <b>let</b> validator_address = ctx.sender();
    <a href="../sui_system/validator_set.md#sui_system_validator_set_get_active_or_pending_or_candidate_validator_mut">get_active_or_pending_or_candidate_validator_mut</a>(self, validator_address, <b>false</b>)
}
</code></pre>



</details>

<a name="sui_system_validator_set_get_validator_mut_with_ctx_including_candidates"></a>

## Function `get_validator_mut_with_ctx_including_candidates`



<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_get_validator_mut_with_ctx_including_candidates">get_validator_mut_with_ctx_including_candidates</a>(self: &<b>mut</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">sui_system::validator_set::ValidatorSet</a>, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): &<b>mut</b> <a href="../sui_system/validator.md#sui_system_validator_Validator">sui_system::validator::Validator</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_get_validator_mut_with_ctx_including_candidates">get_validator_mut_with_ctx_including_candidates</a>(
    self: &<b>mut</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">ValidatorSet</a>,
    ctx: &TxContext,
): &<b>mut</b> Validator {
    <b>let</b> validator_address = ctx.sender();
    <a href="../sui_system/validator_set.md#sui_system_validator_set_get_active_or_pending_or_candidate_validator_mut">get_active_or_pending_or_candidate_validator_mut</a>(self, validator_address, <b>true</b>)
}
</code></pre>



</details>

<a name="sui_system_validator_set_get_validator_ref"></a>

## Function `get_validator_ref`



<pre><code><b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_get_validator_ref">get_validator_ref</a>(validators: &vector&lt;<a href="../sui_system/validator.md#sui_system_validator_Validator">sui_system::validator::Validator</a>&gt;, validator_address: <b>address</b>): &<a href="../sui_system/validator.md#sui_system_validator_Validator">sui_system::validator::Validator</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_get_validator_ref">get_validator_ref</a>(
    validators: &vector&lt;Validator&gt;,
    validator_address: <b>address</b>,
): &Validator {
    <b>let</b> <b>mut</b> validator_index_opt = <a href="../sui_system/validator_set.md#sui_system_validator_set_find_validator">find_validator</a>(validators, validator_address);
    <b>assert</b>!(validator_index_opt.is_some(), <a href="../sui_system/validator_set.md#sui_system_validator_set_ENotAValidator">ENotAValidator</a>);
    <b>let</b> validator_index = validator_index_opt.extract();
    &validators[validator_index]
}
</code></pre>



</details>

<a name="sui_system_validator_set_get_active_or_pending_or_candidate_validator_ref"></a>

## Function `get_active_or_pending_or_candidate_validator_ref`



<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_get_active_or_pending_or_candidate_validator_ref">get_active_or_pending_or_candidate_validator_ref</a>(self: &<b>mut</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">sui_system::validator_set::ValidatorSet</a>, validator_address: <b>address</b>, which_validator: u8): &<a href="../sui_system/validator.md#sui_system_validator_Validator">sui_system::validator::Validator</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_get_active_or_pending_or_candidate_validator_ref">get_active_or_pending_or_candidate_validator_ref</a>(
    self: &<b>mut</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">ValidatorSet</a>,
    validator_address: <b>address</b>,
    which_validator: u8,
): &Validator {
    <b>let</b> <b>mut</b> validator_index_opt = <a href="../sui_system/validator_set.md#sui_system_validator_set_find_validator">find_validator</a>(&self.<a href="../sui_system/validator_set.md#sui_system_validator_set_active_validators">active_validators</a>, validator_address);
    <b>if</b> (validator_index_opt.is_some() || which_validator == <a href="../sui_system/validator_set.md#sui_system_validator_set_ACTIVE_VALIDATOR_ONLY">ACTIVE_VALIDATOR_ONLY</a>) {
        <b>let</b> validator_index = validator_index_opt.extract();
        <b>return</b> &self.<a href="../sui_system/validator_set.md#sui_system_validator_set_active_validators">active_validators</a>[validator_index]
    };
    <b>let</b> <b>mut</b> validator_index_opt = <a href="../sui_system/validator_set.md#sui_system_validator_set_find_validator_from_table_vec">find_validator_from_table_vec</a>(&self.pending_active_validators, validator_address);
    <b>if</b> (validator_index_opt.is_some() || which_validator == <a href="../sui_system/validator_set.md#sui_system_validator_set_ACTIVE_OR_PENDING_VALIDATOR">ACTIVE_OR_PENDING_VALIDATOR</a>) {
        <b>let</b> validator_index = validator_index_opt.extract();
        <b>return</b> &self.pending_active_validators[validator_index]
    };
    self.validator_candidates[validator_address].load_validator_maybe_upgrade()
}
</code></pre>



</details>

<a name="sui_system_validator_set_get_active_validator_ref"></a>

## Function `get_active_validator_ref`



<pre><code><b>public</b> <b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_get_active_validator_ref">get_active_validator_ref</a>(self: &<a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">sui_system::validator_set::ValidatorSet</a>, validator_address: <b>address</b>): &<a href="../sui_system/validator.md#sui_system_validator_Validator">sui_system::validator::Validator</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_get_active_validator_ref">get_active_validator_ref</a>(
    self: &<a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">ValidatorSet</a>,
    validator_address: <b>address</b>,
): &Validator {
    <b>let</b> <b>mut</b> validator_index_opt = <a href="../sui_system/validator_set.md#sui_system_validator_set_find_validator">find_validator</a>(&self.<a href="../sui_system/validator_set.md#sui_system_validator_set_active_validators">active_validators</a>, validator_address);
    <b>assert</b>!(validator_index_opt.is_some(), <a href="../sui_system/validator_set.md#sui_system_validator_set_ENotAValidator">ENotAValidator</a>);
    <b>let</b> validator_index = validator_index_opt.extract();
    &self.<a href="../sui_system/validator_set.md#sui_system_validator_set_active_validators">active_validators</a>[validator_index]
}
</code></pre>



</details>

<a name="sui_system_validator_set_get_pending_validator_ref"></a>

## Function `get_pending_validator_ref`



<pre><code><b>public</b> <b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_get_pending_validator_ref">get_pending_validator_ref</a>(self: &<a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">sui_system::validator_set::ValidatorSet</a>, validator_address: <b>address</b>): &<a href="../sui_system/validator.md#sui_system_validator_Validator">sui_system::validator::Validator</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_get_pending_validator_ref">get_pending_validator_ref</a>(
    self: &<a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">ValidatorSet</a>,
    validator_address: <b>address</b>,
): &Validator {
    <b>let</b> <b>mut</b> validator_index_opt = <a href="../sui_system/validator_set.md#sui_system_validator_set_find_validator_from_table_vec">find_validator_from_table_vec</a>(&self.pending_active_validators, validator_address);
    <b>assert</b>!(validator_index_opt.is_some(), <a href="../sui_system/validator_set.md#sui_system_validator_set_ENotAPendingValidator">ENotAPendingValidator</a>);
    <b>let</b> validator_index = validator_index_opt.extract();
    &self.pending_active_validators[validator_index]
}
</code></pre>



</details>

<a name="sui_system_validator_set_verify_cap"></a>

## Function `verify_cap`

Verify the capability is valid for a Validator.
If <code>active_validator_only</code> is true, only verify the Cap for an active validator.
Otherwise, verify the Cap for au either active or pending validator.


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_verify_cap">verify_cap</a>(self: &<b>mut</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">sui_system::validator_set::ValidatorSet</a>, cap: &<a href="../sui_system/validator_cap.md#sui_system_validator_cap_UnverifiedValidatorOperationCap">sui_system::validator_cap::UnverifiedValidatorOperationCap</a>, which_validator: u8): <a href="../sui_system/validator_cap.md#sui_system_validator_cap_ValidatorOperationCap">sui_system::validator_cap::ValidatorOperationCap</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_verify_cap">verify_cap</a>(
    self: &<b>mut</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">ValidatorSet</a>,
    cap: &UnverifiedValidatorOperationCap,
    which_validator: u8,
): ValidatorOperationCap {
    <b>let</b> cap_address = *cap.unverified_operation_cap_address();
    <b>let</b> <a href="../sui_system/validator.md#sui_system_validator">validator</a> =
        <b>if</b> (which_validator == <a href="../sui_system/validator_set.md#sui_system_validator_set_ACTIVE_VALIDATOR_ONLY">ACTIVE_VALIDATOR_ONLY</a>)
            <a href="../sui_system/validator_set.md#sui_system_validator_set_get_active_validator_ref">get_active_validator_ref</a>(self, cap_address)
        <b>else</b>
            <a href="../sui_system/validator_set.md#sui_system_validator_set_get_active_or_pending_or_candidate_validator_ref">get_active_or_pending_or_candidate_validator_ref</a>(self, cap_address, which_validator);
    <b>assert</b>!(<a href="../sui_system/validator.md#sui_system_validator">validator</a>.operation_cap_id() == &object::id(cap), <a href="../sui_system/validator_set.md#sui_system_validator_set_EInvalidCap">EInvalidCap</a>);
    <a href="../sui_system/validator_cap.md#sui_system_validator_cap_new_from_unverified">validator_cap::new_from_unverified</a>(cap)
}
</code></pre>



</details>

<a name="sui_system_validator_set_process_pending_removals"></a>

## Function `process_pending_removals`

Process the pending withdraw requests. For each pending request, the validator
is removed from <code>validators</code> and its staking pool is put into the <code>inactive_validators</code> table.


<pre><code><b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_process_pending_removals">process_pending_removals</a>(self: &<b>mut</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">sui_system::validator_set::ValidatorSet</a>, validator_report_records: &<b>mut</b> <a href="../sui/vec_map.md#sui_vec_map_VecMap">sui::vec_map::VecMap</a>&lt;<b>address</b>, <a href="../sui/vec_set.md#sui_vec_set_VecSet">sui::vec_set::VecSet</a>&lt;<b>address</b>&gt;&gt;, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_process_pending_removals">process_pending_removals</a>(
    self: &<b>mut</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">ValidatorSet</a>,
    validator_report_records: &<b>mut</b> VecMap&lt;<b>address</b>, VecSet&lt;<b>address</b>&gt;&gt;,
    ctx: &<b>mut</b> TxContext,
) {
    <a href="../sui_system/validator_set.md#sui_system_validator_set_sort_removal_list">sort_removal_list</a>(&<b>mut</b> self.pending_removals);
    <b>while</b> (!self.pending_removals.is_empty()) {
        <b>let</b> index = self.pending_removals.pop_back();
        <b>let</b> <a href="../sui_system/validator.md#sui_system_validator">validator</a> = self.<a href="../sui_system/validator_set.md#sui_system_validator_set_active_validators">active_validators</a>.remove(index);
        <a href="../sui_system/validator_set.md#sui_system_validator_set_process_validator_departure">process_validator_departure</a>(self, <a href="../sui_system/validator.md#sui_system_validator">validator</a>, validator_report_records, <b>true</b> /* the <a href="../sui_system/validator.md#sui_system_validator">validator</a> removes itself voluntarily */, ctx);
    }
}
</code></pre>



</details>

<a name="sui_system_validator_set_process_validator_departure"></a>

## Function `process_validator_departure`



<pre><code><b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_process_validator_departure">process_validator_departure</a>(self: &<b>mut</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">sui_system::validator_set::ValidatorSet</a>, <a href="../sui_system/validator.md#sui_system_validator">validator</a>: <a href="../sui_system/validator.md#sui_system_validator_Validator">sui_system::validator::Validator</a>, validator_report_records: &<b>mut</b> <a href="../sui/vec_map.md#sui_vec_map_VecMap">sui::vec_map::VecMap</a>&lt;<b>address</b>, <a href="../sui/vec_set.md#sui_vec_set_VecSet">sui::vec_set::VecSet</a>&lt;<b>address</b>&gt;&gt;, is_voluntary: bool, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_process_validator_departure">process_validator_departure</a>(
    self: &<b>mut</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">ValidatorSet</a>,
    <b>mut</b> <a href="../sui_system/validator.md#sui_system_validator">validator</a>: Validator,
    validator_report_records: &<b>mut</b> VecMap&lt;<b>address</b>, VecSet&lt;<b>address</b>&gt;&gt;,
    is_voluntary: bool,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> new_epoch = ctx.epoch() + 1;
    <b>let</b> validator_address = <a href="../sui_system/validator.md#sui_system_validator">validator</a>.sui_address();
    <b>let</b> validator_pool_id = staking_pool_id(&<a href="../sui_system/validator.md#sui_system_validator">validator</a>);
    // Remove the <a href="../sui_system/validator.md#sui_system_validator">validator</a> from our tables.
    self.<a href="../sui_system/validator_set.md#sui_system_validator_set_staking_pool_mappings">staking_pool_mappings</a>.remove(validator_pool_id);
    <b>if</b> (self.at_risk_validators.contains(&validator_address)) {
        self.at_risk_validators.remove(&validator_address);
    };
    self.<a href="../sui_system/validator_set.md#sui_system_validator_set_total_stake">total_stake</a> = self.<a href="../sui_system/validator_set.md#sui_system_validator_set_total_stake">total_stake</a> - <a href="../sui_system/validator.md#sui_system_validator">validator</a>.total_stake_amount();
    <a href="../sui_system/validator_set.md#sui_system_validator_set_clean_report_records_leaving_validator">clean_report_records_leaving_validator</a>(validator_report_records, validator_address);
    event::emit(
        <a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorLeaveEvent">ValidatorLeaveEvent</a> {
            epoch: new_epoch,
            validator_address,
            staking_pool_id: staking_pool_id(&<a href="../sui_system/validator.md#sui_system_validator">validator</a>),
            is_voluntary,
        }
    );
    // Deactivate the <a href="../sui_system/validator.md#sui_system_validator">validator</a> and its staking pool
    <a href="../sui_system/validator.md#sui_system_validator">validator</a>.deactivate(new_epoch);
    self.inactive_validators.add(
        validator_pool_id,
        <a href="../sui_system/validator_wrapper.md#sui_system_validator_wrapper_create_v1">validator_wrapper::create_v1</a>(<a href="../sui_system/validator.md#sui_system_validator">validator</a>, ctx),
    );
}
</code></pre>



</details>

<a name="sui_system_validator_set_clean_report_records_leaving_validator"></a>

## Function `clean_report_records_leaving_validator`



<pre><code><b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_clean_report_records_leaving_validator">clean_report_records_leaving_validator</a>(validator_report_records: &<b>mut</b> <a href="../sui/vec_map.md#sui_vec_map_VecMap">sui::vec_map::VecMap</a>&lt;<b>address</b>, <a href="../sui/vec_set.md#sui_vec_set_VecSet">sui::vec_set::VecSet</a>&lt;<b>address</b>&gt;&gt;, leaving_validator_addr: <b>address</b>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_clean_report_records_leaving_validator">clean_report_records_leaving_validator</a>(
    validator_report_records: &<b>mut</b> VecMap&lt;<b>address</b>, VecSet&lt;<b>address</b>&gt;&gt;,
    leaving_validator_addr: <b>address</b>
) {
    // Remove the records about this <a href="../sui_system/validator.md#sui_system_validator">validator</a>
    <b>if</b> (validator_report_records.contains(&leaving_validator_addr)) {
        validator_report_records.remove(&leaving_validator_addr);
    };
    // Remove the reports submitted by this <a href="../sui_system/validator.md#sui_system_validator">validator</a>
    <b>let</b> reported_validators = validator_report_records.keys();
    <b>let</b> length = reported_validators.length();
    <b>let</b> <b>mut</b> i = 0;
    <b>while</b> (i &lt; length) {
        <b>let</b> reported_validator_addr = &reported_validators[i];
        <b>let</b> reporters = &<b>mut</b> validator_report_records[reported_validator_addr];
        <b>if</b> (reporters.contains(&leaving_validator_addr)) {
            reporters.remove(&leaving_validator_addr);
            <b>if</b> (reporters.is_empty()) {
                validator_report_records.remove(reported_validator_addr);
            };
        };
        i = i + 1;
    }
}
</code></pre>



</details>

<a name="sui_system_validator_set_process_pending_validators"></a>

## Function `process_pending_validators`

Process the pending new validators. They are activated and inserted into <code>validators</code>.


<pre><code><b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_process_pending_validators">process_pending_validators</a>(self: &<b>mut</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">sui_system::validator_set::ValidatorSet</a>, new_epoch: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_process_pending_validators">process_pending_validators</a>(
    self: &<b>mut</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">ValidatorSet</a>, new_epoch: u64,
) {
    <b>while</b> (!self.pending_active_validators.is_empty()) {
        <b>let</b> <b>mut</b> <a href="../sui_system/validator.md#sui_system_validator">validator</a> = self.pending_active_validators.pop_back();
        <a href="../sui_system/validator.md#sui_system_validator">validator</a>.activate(new_epoch);
        event::emit(
            <a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorJoinEvent">ValidatorJoinEvent</a> {
                epoch: new_epoch,
                validator_address: <a href="../sui_system/validator.md#sui_system_validator">validator</a>.sui_address(),
                staking_pool_id: staking_pool_id(&<a href="../sui_system/validator.md#sui_system_validator">validator</a>),
            }
        );
        self.<a href="../sui_system/validator_set.md#sui_system_validator_set_active_validators">active_validators</a>.push_back(<a href="../sui_system/validator.md#sui_system_validator">validator</a>);
    }
}
</code></pre>



</details>

<a name="sui_system_validator_set_sort_removal_list"></a>

## Function `sort_removal_list`

Sort all the pending removal indexes.


<pre><code><b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_sort_removal_list">sort_removal_list</a>(withdraw_list: &<b>mut</b> vector&lt;u64&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_sort_removal_list">sort_removal_list</a>(withdraw_list: &<b>mut</b> vector&lt;u64&gt;) {
    <b>let</b> length = withdraw_list.length();
    <b>let</b> <b>mut</b> i = 1;
    <b>while</b> (i &lt; length) {
        <b>let</b> cur = withdraw_list[i];
        <b>let</b> <b>mut</b> j = i;
        <b>while</b> (j &gt; 0) {
            j = j - 1;
            <b>if</b> (withdraw_list[j] &gt; cur) {
                withdraw_list.swap(j, j + 1);
            } <b>else</b> {
                <b>break</b>
            };
        };
        i = i + 1;
    };
}
</code></pre>



</details>

<a name="sui_system_validator_set_process_pending_stakes_and_withdraws"></a>

## Function `process_pending_stakes_and_withdraws`

Process all active validators' pending stake deposits and withdraws.


<pre><code><b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_process_pending_stakes_and_withdraws">process_pending_stakes_and_withdraws</a>(validators: &<b>mut</b> vector&lt;<a href="../sui_system/validator.md#sui_system_validator_Validator">sui_system::validator::Validator</a>&gt;, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_process_pending_stakes_and_withdraws">process_pending_stakes_and_withdraws</a>(
    validators: &<b>mut</b> vector&lt;Validator&gt;, ctx: &TxContext
) {
    <b>let</b> length = validators.length();
    <b>let</b> <b>mut</b> i = 0;
    <b>while</b> (i &lt; length) {
        <b>let</b> <a href="../sui_system/validator.md#sui_system_validator">validator</a> = &<b>mut</b> validators[i];
        <a href="../sui_system/validator.md#sui_system_validator">validator</a>.<a href="../sui_system/validator_set.md#sui_system_validator_set_process_pending_stakes_and_withdraws">process_pending_stakes_and_withdraws</a>(ctx);
        i = i + 1;
    }
}
</code></pre>



</details>

<a name="sui_system_validator_set_calculate_total_stakes"></a>

## Function `calculate_total_stakes`

Calculate the total active validator stake.


<pre><code><b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_calculate_total_stakes">calculate_total_stakes</a>(validators: &vector&lt;<a href="../sui_system/validator.md#sui_system_validator_Validator">sui_system::validator::Validator</a>&gt;): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_calculate_total_stakes">calculate_total_stakes</a>(validators: &vector&lt;Validator&gt;): u64 {
    <b>let</b> <b>mut</b> stake = 0;
    <b>let</b> length = validators.length();
    <b>let</b> <b>mut</b> i = 0;
    <b>while</b> (i &lt; length) {
        <b>let</b> v = &validators[i];
        stake = stake + v.total_stake_amount();
        i = i + 1;
    };
    stake
}
</code></pre>



</details>

<a name="sui_system_validator_set_adjust_stake_and_gas_price"></a>

## Function `adjust_stake_and_gas_price`

Process the pending stake changes for each validator.


<pre><code><b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_adjust_stake_and_gas_price">adjust_stake_and_gas_price</a>(validators: &<b>mut</b> vector&lt;<a href="../sui_system/validator.md#sui_system_validator_Validator">sui_system::validator::Validator</a>&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_adjust_stake_and_gas_price">adjust_stake_and_gas_price</a>(validators: &<b>mut</b> vector&lt;Validator&gt;) {
    <b>let</b> length = validators.length();
    <b>let</b> <b>mut</b> i = 0;
    <b>while</b> (i &lt; length) {
        <b>let</b> <a href="../sui_system/validator.md#sui_system_validator">validator</a> = &<b>mut</b> validators[i];
        <a href="../sui_system/validator.md#sui_system_validator">validator</a>.<a href="../sui_system/validator_set.md#sui_system_validator_set_adjust_stake_and_gas_price">adjust_stake_and_gas_price</a>();
        i = i + 1;
    }
}
</code></pre>



</details>

<a name="sui_system_validator_set_compute_reward_adjustments"></a>

## Function `compute_reward_adjustments`

Compute both the individual reward adjustments and total reward adjustment for staking rewards
as well as storage fund rewards.


<pre><code><b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_compute_reward_adjustments">compute_reward_adjustments</a>(slashed_validator_indices: vector&lt;u64&gt;, reward_slashing_rate: u64, unadjusted_staking_reward_amounts: &vector&lt;u64&gt;, unadjusted_storage_fund_reward_amounts: &vector&lt;u64&gt;): (u64, <a href="../sui/vec_map.md#sui_vec_map_VecMap">sui::vec_map::VecMap</a>&lt;u64, u64&gt;, u64, <a href="../sui/vec_map.md#sui_vec_map_VecMap">sui::vec_map::VecMap</a>&lt;u64, u64&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_compute_reward_adjustments">compute_reward_adjustments</a>(
    <b>mut</b> slashed_validator_indices: vector&lt;u64&gt;,
    reward_slashing_rate: u64,
    unadjusted_staking_reward_amounts: &vector&lt;u64&gt;,
    unadjusted_storage_fund_reward_amounts: &vector&lt;u64&gt;,
): (
    u64, // sum of staking reward adjustments
    VecMap&lt;u64, u64&gt;, // mapping of individual <a href="../sui_system/validator.md#sui_system_validator">validator</a>'s staking reward adjustment from index -&gt; amount
    u64, // sum of storage fund reward adjustments
    VecMap&lt;u64, u64&gt;, // mapping of individual <a href="../sui_system/validator.md#sui_system_validator">validator</a>'s storage fund reward adjustment from index -&gt; amount
) {
    <b>let</b> <b>mut</b> total_staking_reward_adjustment = 0;
    <b>let</b> <b>mut</b> individual_staking_reward_adjustments = vec_map::empty();
    <b>let</b> <b>mut</b> total_storage_fund_reward_adjustment = 0;
    <b>let</b> <b>mut</b> individual_storage_fund_reward_adjustments = vec_map::empty();
    <b>while</b> (!slashed_validator_indices.is_empty()) {
        <b>let</b> validator_index = slashed_validator_indices.pop_back();
        // Use the slashing rate to compute the amount of staking rewards slashed from this punished <a href="../sui_system/validator.md#sui_system_validator">validator</a>.
        <b>let</b> unadjusted_staking_reward = unadjusted_staking_reward_amounts[validator_index];
        <b>let</b> staking_reward_adjustment_u128 =
            unadjusted_staking_reward <b>as</b> u128 * (reward_slashing_rate <b>as</b> u128)
            / <a href="../sui_system/validator_set.md#sui_system_validator_set_BASIS_POINT_DENOMINATOR">BASIS_POINT_DENOMINATOR</a>;
        // Insert into individual mapping and record into the total adjustment sum.
        individual_staking_reward_adjustments.insert(validator_index, staking_reward_adjustment_u128 <b>as</b> u64);
        total_staking_reward_adjustment = total_staking_reward_adjustment + (staking_reward_adjustment_u128 <b>as</b> u64);
        // Do the same thing <b>for</b> storage fund rewards.
        <b>let</b> unadjusted_storage_fund_reward = unadjusted_storage_fund_reward_amounts[validator_index];
        <b>let</b> storage_fund_reward_adjustment_u128 =
            unadjusted_storage_fund_reward <b>as</b> u128 * (reward_slashing_rate <b>as</b> u128)
            / <a href="../sui_system/validator_set.md#sui_system_validator_set_BASIS_POINT_DENOMINATOR">BASIS_POINT_DENOMINATOR</a>;
        individual_storage_fund_reward_adjustments.insert(validator_index, storage_fund_reward_adjustment_u128 <b>as</b> u64);
        total_storage_fund_reward_adjustment = total_storage_fund_reward_adjustment + (storage_fund_reward_adjustment_u128 <b>as</b> u64);
    };
    (
        total_staking_reward_adjustment, individual_staking_reward_adjustments,
        total_storage_fund_reward_adjustment, individual_storage_fund_reward_adjustments
    )
}
</code></pre>



</details>

<a name="sui_system_validator_set_compute_slashed_validators"></a>

## Function `compute_slashed_validators`

Process the validator report records of the epoch and return the addresses of the
non-performant validators according to the input threshold.


<pre><code><b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_compute_slashed_validators">compute_slashed_validators</a>(self: &<a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">sui_system::validator_set::ValidatorSet</a>, validator_report_records: <a href="../sui/vec_map.md#sui_vec_map_VecMap">sui::vec_map::VecMap</a>&lt;<b>address</b>, <a href="../sui/vec_set.md#sui_vec_set_VecSet">sui::vec_set::VecSet</a>&lt;<b>address</b>&gt;&gt;): vector&lt;<b>address</b>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_compute_slashed_validators">compute_slashed_validators</a>(
    self: &<a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">ValidatorSet</a>,
    <b>mut</b> validator_report_records: VecMap&lt;<b>address</b>, VecSet&lt;<b>address</b>&gt;&gt;,
): vector&lt;<b>address</b>&gt; {
    <b>let</b> <b>mut</b> slashed_validators = vector[];
    <b>while</b> (!validator_report_records.is_empty()) {
        <b>let</b> (validator_address, reporters) = validator_report_records.pop();
        <b>assert</b>!(
            <a href="../sui_system/validator_set.md#sui_system_validator_set_is_active_validator_by_sui_address">is_active_validator_by_sui_address</a>(self, validator_address),
            <a href="../sui_system/validator_set.md#sui_system_validator_set_ENonValidatorInReportRecords">ENonValidatorInReportRecords</a>,
        );
        // Sum up the voting power of validators that have reported this <a href="../sui_system/validator.md#sui_system_validator">validator</a> and check <b>if</b> it <b>has</b>
        // passed the slashing threshold.
        <b>let</b> reporter_votes = <a href="../sui_system/validator_set.md#sui_system_validator_set_sum_voting_power_by_addresses">sum_voting_power_by_addresses</a>(&self.<a href="../sui_system/validator_set.md#sui_system_validator_set_active_validators">active_validators</a>, &reporters.into_keys());
        <b>if</b> (reporter_votes &gt;= <a href="../sui_system/voting_power.md#sui_system_voting_power_quorum_threshold">voting_power::quorum_threshold</a>()) {
            slashed_validators.push_back(validator_address);
        }
    };
    slashed_validators
}
</code></pre>



</details>

<a name="sui_system_validator_set_compute_unadjusted_reward_distribution"></a>

## Function `compute_unadjusted_reward_distribution`

Given the current list of active validators, the total stake and total reward,
calculate the amount of reward each validator should get, without taking into
account the tallying rule results.
Returns the unadjusted amounts of staking reward and storage fund reward for each validator.


<pre><code><b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_compute_unadjusted_reward_distribution">compute_unadjusted_reward_distribution</a>(validators: &vector&lt;<a href="../sui_system/validator.md#sui_system_validator_Validator">sui_system::validator::Validator</a>&gt;, total_voting_power: u64, total_staking_reward: u64, total_storage_fund_reward: u64): (vector&lt;u64&gt;, vector&lt;u64&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_compute_unadjusted_reward_distribution">compute_unadjusted_reward_distribution</a>(
    validators: &vector&lt;Validator&gt;,
    total_voting_power: u64,
    total_staking_reward: u64,
    total_storage_fund_reward: u64,
): (vector&lt;u64&gt;, vector&lt;u64&gt;) {
    <b>let</b> <b>mut</b> staking_reward_amounts = vector[];
    <b>let</b> <b>mut</b> storage_fund_reward_amounts = vector[];
    <b>let</b> length = validators.length();
    <b>let</b> storage_fund_reward_per_validator = total_storage_fund_reward / length;
    <b>let</b> <b>mut</b> i = 0;
    <b>while</b> (i &lt; length) {
        <b>let</b> <a href="../sui_system/validator.md#sui_system_validator">validator</a> = &validators[i];
        // Integer divisions will truncate the results. Because of this, we expect that at the end
        // there will be some reward remaining in `total_staking_reward`.
        // Use u128 to avoid multiplication overflow.
        <b>let</b> <a href="../sui_system/voting_power.md#sui_system_voting_power">voting_power</a>: u128 = <a href="../sui_system/validator.md#sui_system_validator">validator</a>.<a href="../sui_system/voting_power.md#sui_system_voting_power">voting_power</a>() <b>as</b> u128;
        <b>let</b> reward_amount = <a href="../sui_system/voting_power.md#sui_system_voting_power">voting_power</a> * (total_staking_reward <b>as</b> u128) / (total_voting_power <b>as</b> u128);
        staking_reward_amounts.push_back(reward_amount <b>as</b> u64);
        // Storage fund's share of the rewards are equally distributed among validators.
        storage_fund_reward_amounts.push_back(storage_fund_reward_per_validator);
        i = i + 1;
    };
    (staking_reward_amounts, storage_fund_reward_amounts)
}
</code></pre>



</details>

<a name="sui_system_validator_set_compute_adjusted_reward_distribution"></a>

## Function `compute_adjusted_reward_distribution`

Use the reward adjustment info to compute the adjusted rewards each validator should get.
Returns the staking rewards each validator gets and the storage fund rewards each validator gets.
The staking rewards are shared with the stakers while the storage fund ones are not.


<pre><code><b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_compute_adjusted_reward_distribution">compute_adjusted_reward_distribution</a>(validators: &vector&lt;<a href="../sui_system/validator.md#sui_system_validator_Validator">sui_system::validator::Validator</a>&gt;, total_voting_power: u64, total_slashed_validator_voting_power: u64, unadjusted_staking_reward_amounts: vector&lt;u64&gt;, unadjusted_storage_fund_reward_amounts: vector&lt;u64&gt;, total_staking_reward_adjustment: u64, individual_staking_reward_adjustments: <a href="../sui/vec_map.md#sui_vec_map_VecMap">sui::vec_map::VecMap</a>&lt;u64, u64&gt;, total_storage_fund_reward_adjustment: u64, individual_storage_fund_reward_adjustments: <a href="../sui/vec_map.md#sui_vec_map_VecMap">sui::vec_map::VecMap</a>&lt;u64, u64&gt;): (vector&lt;u64&gt;, vector&lt;u64&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_compute_adjusted_reward_distribution">compute_adjusted_reward_distribution</a>(
    validators: &vector&lt;Validator&gt;,
    total_voting_power: u64,
    total_slashed_validator_voting_power: u64,
    unadjusted_staking_reward_amounts: vector&lt;u64&gt;,
    unadjusted_storage_fund_reward_amounts: vector&lt;u64&gt;,
    total_staking_reward_adjustment: u64,
    individual_staking_reward_adjustments: VecMap&lt;u64, u64&gt;,
    total_storage_fund_reward_adjustment: u64,
    individual_storage_fund_reward_adjustments: VecMap&lt;u64, u64&gt;,
): (vector&lt;u64&gt;, vector&lt;u64&gt;) {
    <b>let</b> total_unslashed_validator_voting_power = total_voting_power - total_slashed_validator_voting_power;
    <b>let</b> <b>mut</b> adjusted_staking_reward_amounts = vector[];
    <b>let</b> <b>mut</b> adjusted_storage_fund_reward_amounts = vector[];
    <b>let</b> length = validators.length();
    <b>let</b> num_unslashed_validators = length - individual_staking_reward_adjustments.size();
    <b>let</b> <b>mut</b> i = 0;
    <b>while</b> (i &lt; length) {
        <b>let</b> <a href="../sui_system/validator.md#sui_system_validator">validator</a> = &validators[i];
        // Integer divisions will truncate the results. Because of this, we expect that at the end
        // there will be some reward remaining in `total_reward`.
        // Use u128 to avoid multiplication overflow.
        <b>let</b> <a href="../sui_system/voting_power.md#sui_system_voting_power">voting_power</a> = <a href="../sui_system/validator.md#sui_system_validator">validator</a>.<a href="../sui_system/voting_power.md#sui_system_voting_power">voting_power</a>() <b>as</b> u128;
        // Compute adjusted staking reward.
        <b>let</b> unadjusted_staking_reward_amount = unadjusted_staking_reward_amounts[i];
        <b>let</b> adjusted_staking_reward_amount =
            // If the <a href="../sui_system/validator.md#sui_system_validator">validator</a> is one of the slashed ones, then subtract the adjustment.
            <b>if</b> (individual_staking_reward_adjustments.contains(&i)) {
                <b>let</b> adjustment = individual_staking_reward_adjustments[&i];
                unadjusted_staking_reward_amount - adjustment
            } <b>else</b> {
                // Otherwise the slashed rewards should be distributed among the unslashed
                // validators so add the corresponding adjustment.
                <b>let</b> adjustment = total_staking_reward_adjustment <b>as</b> u128 * <a href="../sui_system/voting_power.md#sui_system_voting_power">voting_power</a>
                               / (total_unslashed_validator_voting_power <b>as</b> u128);
                unadjusted_staking_reward_amount + (adjustment <b>as</b> u64)
            };
        adjusted_staking_reward_amounts.push_back(adjusted_staking_reward_amount);
        // Compute adjusted storage fund reward.
        <b>let</b> unadjusted_storage_fund_reward_amount = unadjusted_storage_fund_reward_amounts[i];
        <b>let</b> adjusted_storage_fund_reward_amount =
            // If the <a href="../sui_system/validator.md#sui_system_validator">validator</a> is one of the slashed ones, then subtract the adjustment.
            <b>if</b> (individual_storage_fund_reward_adjustments.contains(&i)) {
                <b>let</b> adjustment = individual_storage_fund_reward_adjustments[&i];
                unadjusted_storage_fund_reward_amount - adjustment
            } <b>else</b> {
                // Otherwise the slashed rewards should be equally distributed among the unslashed validators.
                <b>let</b> adjustment = total_storage_fund_reward_adjustment / num_unslashed_validators;
                unadjusted_storage_fund_reward_amount + adjustment
            };
        adjusted_storage_fund_reward_amounts.push_back(adjusted_storage_fund_reward_amount);
        i = i + 1;
    };
    (adjusted_staking_reward_amounts, adjusted_storage_fund_reward_amounts)
}
</code></pre>



</details>

<a name="sui_system_validator_set_distribute_reward"></a>

## Function `distribute_reward`



<pre><code><b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_distribute_reward">distribute_reward</a>(validators: &<b>mut</b> vector&lt;<a href="../sui_system/validator.md#sui_system_validator_Validator">sui_system::validator::Validator</a>&gt;, adjusted_staking_reward_amounts: &vector&lt;u64&gt;, adjusted_storage_fund_reward_amounts: &vector&lt;u64&gt;, staking_rewards: &<b>mut</b> <a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;<a href="../sui/sui.md#sui_sui_SUI">sui::sui::SUI</a>&gt;, storage_fund_reward: &<b>mut</b> <a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;<a href="../sui/sui.md#sui_sui_SUI">sui::sui::SUI</a>&gt;, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_distribute_reward">distribute_reward</a>(
    validators: &<b>mut</b> vector&lt;Validator&gt;,
    adjusted_staking_reward_amounts: &vector&lt;u64&gt;,
    adjusted_storage_fund_reward_amounts: &vector&lt;u64&gt;,
    staking_rewards: &<b>mut</b> Balance&lt;SUI&gt;,
    storage_fund_reward: &<b>mut</b> Balance&lt;SUI&gt;,
    ctx: &<b>mut</b> TxContext
) {
    <b>let</b> length = validators.length();
    <b>assert</b>!(length &gt; 0, <a href="../sui_system/validator_set.md#sui_system_validator_set_EValidatorSetEmpty">EValidatorSetEmpty</a>);
    <b>let</b> <b>mut</b> i = 0;
    <b>while</b> (i &lt; length) {
        <b>let</b> <a href="../sui_system/validator.md#sui_system_validator">validator</a> = &<b>mut</b> validators[i];
        <b>let</b> staking_reward_amount = adjusted_staking_reward_amounts[i];
        <b>let</b> <b>mut</b> staker_reward = staking_rewards.split(staking_reward_amount);
        // Validator takes a cut of the rewards <b>as</b> commission.
        <b>let</b> validator_commission_amount = (staking_reward_amount <b>as</b> u128) * (<a href="../sui_system/validator.md#sui_system_validator">validator</a>.commission_rate() <b>as</b> u128) / <a href="../sui_system/validator_set.md#sui_system_validator_set_BASIS_POINT_DENOMINATOR">BASIS_POINT_DENOMINATOR</a>;
        // The <a href="../sui_system/validator.md#sui_system_validator">validator</a> reward = storage_fund_reward + commission.
        <b>let</b> <b>mut</b> validator_reward = staker_reward.split(validator_commission_amount <b>as</b> u64);
        // Add storage fund rewards to the <a href="../sui_system/validator.md#sui_system_validator">validator</a>'s reward.
        validator_reward.join(
	    	storage_fund_reward.split(adjusted_storage_fund_reward_amounts[i])
	    );
        // Add rewards to the <a href="../sui_system/validator.md#sui_system_validator">validator</a>. Don't try and distribute rewards though <b>if</b> the payout is zero.
        <b>if</b> (validator_reward.value() &gt; 0) {
            <b>let</b> validator_address = <a href="../sui_system/validator.md#sui_system_validator">validator</a>.sui_address();
            <b>let</b> rewards_stake = <a href="../sui_system/validator.md#sui_system_validator">validator</a>.<a href="../sui_system/validator_set.md#sui_system_validator_set_request_add_stake">request_add_stake</a>(validator_reward, validator_address, ctx);
            transfer::public_transfer(rewards_stake, validator_address);
        } <b>else</b> {
            validator_reward.destroy_zero();
        };
        // Add rewards to stake staking pool to auto compound <b>for</b> stakers.
        <a href="../sui_system/validator.md#sui_system_validator">validator</a>.deposit_stake_rewards(staker_reward);
        i = i + 1;
    }
}
</code></pre>



</details>

<a name="sui_system_validator_set_emit_validator_epoch_events"></a>

## Function `emit_validator_epoch_events`

Emit events containing information of each validator for the epoch,
including stakes, rewards, performance, etc.


<pre><code><b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_emit_validator_epoch_events">emit_validator_epoch_events</a>(new_epoch: u64, vs: &vector&lt;<a href="../sui_system/validator.md#sui_system_validator_Validator">sui_system::validator::Validator</a>&gt;, pool_staking_reward_amounts: &vector&lt;u64&gt;, storage_fund_staking_reward_amounts: &vector&lt;u64&gt;, report_records: &<a href="../sui/vec_map.md#sui_vec_map_VecMap">sui::vec_map::VecMap</a>&lt;<b>address</b>, <a href="../sui/vec_set.md#sui_vec_set_VecSet">sui::vec_set::VecSet</a>&lt;<b>address</b>&gt;&gt;, slashed_validators: &vector&lt;<b>address</b>&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_emit_validator_epoch_events">emit_validator_epoch_events</a>(
    new_epoch: u64,
    vs: &vector&lt;Validator&gt;,
    pool_staking_reward_amounts: &vector&lt;u64&gt;,
    storage_fund_staking_reward_amounts: &vector&lt;u64&gt;,
    report_records: &VecMap&lt;<b>address</b>, VecSet&lt;<b>address</b>&gt;&gt;,
    slashed_validators: &vector&lt;<b>address</b>&gt;,
) {
    <b>let</b> num_validators = vs.length();
    <b>let</b> <b>mut</b> i = 0;
    <b>while</b> (i &lt; num_validators) {
        <b>let</b> v = &vs[i];
        <b>let</b> validator_address = v.sui_address();
        <b>let</b> tallying_rule_reporters =
            <b>if</b> (report_records.contains(&validator_address)) {
                report_records[&validator_address].into_keys()
            } <b>else</b> {
                vector[]
            };
        <b>let</b> tallying_rule_global_score =
            <b>if</b> (slashed_validators.contains(&validator_address)) 0
            <b>else</b> 1;
        event::emit(
            <a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorEpochInfoEventV2">ValidatorEpochInfoEventV2</a> {
                epoch: new_epoch,
                validator_address,
                reference_gas_survey_quote: v.gas_price(),
                stake: v.total_stake_amount(),
                <a href="../sui_system/voting_power.md#sui_system_voting_power">voting_power</a>: v.<a href="../sui_system/voting_power.md#sui_system_voting_power">voting_power</a>(),
                commission_rate: v.commission_rate(),
                pool_staking_reward: pool_staking_reward_amounts[i],
                storage_fund_staking_reward: storage_fund_staking_reward_amounts[i],
                pool_token_exchange_rate: v.pool_token_exchange_rate_at_epoch(new_epoch),
                tallying_rule_reporters,
                tallying_rule_global_score,
            }
        );
        i = i + 1;
    }
}
</code></pre>



</details>

<a name="sui_system_validator_set_sum_voting_power_by_addresses"></a>

## Function `sum_voting_power_by_addresses`

Sum up the total stake of a given list of validator addresses.


<pre><code><b>public</b> <b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_sum_voting_power_by_addresses">sum_voting_power_by_addresses</a>(vs: &vector&lt;<a href="../sui_system/validator.md#sui_system_validator_Validator">sui_system::validator::Validator</a>&gt;, addresses: &vector&lt;<b>address</b>&gt;): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_sum_voting_power_by_addresses">sum_voting_power_by_addresses</a>(vs: &vector&lt;Validator&gt;, addresses: &vector&lt;<b>address</b>&gt;): u64 {
    <b>let</b> <b>mut</b> sum = 0;
    <b>let</b> <b>mut</b> i = 0;
    <b>let</b> length = addresses.length();
    <b>while</b> (i &lt; length) {
        <b>let</b> <a href="../sui_system/validator.md#sui_system_validator">validator</a> = <a href="../sui_system/validator_set.md#sui_system_validator_set_get_validator_ref">get_validator_ref</a>(vs, addresses[i]);
        sum = sum + <a href="../sui_system/validator.md#sui_system_validator">validator</a>.<a href="../sui_system/voting_power.md#sui_system_voting_power">voting_power</a>();
        i = i + 1;
    };
    sum
}
</code></pre>



</details>

<a name="sui_system_validator_set_active_validators"></a>

## Function `active_validators`

Return the active validators in <code>self</code>


<pre><code><b>public</b> <b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_active_validators">active_validators</a>(self: &<a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">sui_system::validator_set::ValidatorSet</a>): &vector&lt;<a href="../sui_system/validator.md#sui_system_validator_Validator">sui_system::validator::Validator</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_active_validators">active_validators</a>(self: &<a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">ValidatorSet</a>): &vector&lt;Validator&gt; {
    &self.<a href="../sui_system/validator_set.md#sui_system_validator_set_active_validators">active_validators</a>
}
</code></pre>



</details>

<a name="sui_system_validator_set_is_validator_candidate"></a>

## Function `is_validator_candidate`

Returns true if the <code>addr</code> is a validator candidate.


<pre><code><b>public</b> <b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_is_validator_candidate">is_validator_candidate</a>(self: &<a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">sui_system::validator_set::ValidatorSet</a>, addr: <b>address</b>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_is_validator_candidate">is_validator_candidate</a>(self: &<a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">ValidatorSet</a>, addr: <b>address</b>): bool {
    self.validator_candidates.contains(addr)
}
</code></pre>



</details>

<a name="sui_system_validator_set_is_inactive_validator"></a>

## Function `is_inactive_validator`

Returns true if the staking pool identified by <code>staking_pool_id</code> is of an inactive validator.


<pre><code><b>public</b> <b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_is_inactive_validator">is_inactive_validator</a>(self: &<a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">sui_system::validator_set::ValidatorSet</a>, staking_pool_id: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_is_inactive_validator">is_inactive_validator</a>(self: &<a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">ValidatorSet</a>, staking_pool_id: ID): bool {
    self.inactive_validators.contains(staking_pool_id)
}
</code></pre>



</details>

<a name="sui_system_validator_set_active_validator_addresses"></a>

## Function `active_validator_addresses`



<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_active_validator_addresses">active_validator_addresses</a>(self: &<a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">sui_system::validator_set::ValidatorSet</a>): vector&lt;<b>address</b>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/validator_set.md#sui_system_validator_set_active_validator_addresses">active_validator_addresses</a>(self: &<a href="../sui_system/validator_set.md#sui_system_validator_set_ValidatorSet">ValidatorSet</a>): vector&lt;<b>address</b>&gt; {
    <b>let</b> vs = &self.<a href="../sui_system/validator_set.md#sui_system_validator_set_active_validators">active_validators</a>;
    <b>let</b> <b>mut</b> res = vector[];
    <b>let</b> <b>mut</b> i = 0;
    <b>let</b> length = vs.length();
    <b>while</b> (i &lt; length) {
        <b>let</b> validator_address = vs[i].sui_address();
        res.push_back(validator_address);
        i = i + 1;
    };
    res
}
</code></pre>



</details>
