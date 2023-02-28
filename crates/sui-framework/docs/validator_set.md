
<a name="0x2_validator_set"></a>

# Module `0x2::validator_set`



-  [Struct `ValidatorSet`](#0x2_validator_set_ValidatorSet)
-  [Struct `DelegationRequestEvent`](#0x2_validator_set_DelegationRequestEvent)
-  [Struct `ValidatorEpochInfo`](#0x2_validator_set_ValidatorEpochInfo)
-  [Constants](#@Constants_0)
-  [Function `new`](#0x2_validator_set_new)
-  [Function `request_add_validator`](#0x2_validator_set_request_add_validator)
-  [Function `request_remove_validator`](#0x2_validator_set_request_remove_validator)
-  [Function `request_add_stake`](#0x2_validator_set_request_add_stake)
-  [Function `request_withdraw_stake`](#0x2_validator_set_request_withdraw_stake)
-  [Function `request_add_delegation`](#0x2_validator_set_request_add_delegation)
-  [Function `request_withdraw_delegation`](#0x2_validator_set_request_withdraw_delegation)
-  [Function `request_set_gas_price`](#0x2_validator_set_request_set_gas_price)
-  [Function `request_set_commission_rate`](#0x2_validator_set_request_set_commission_rate)
-  [Function `advance_epoch`](#0x2_validator_set_advance_epoch)
-  [Function `derive_reference_gas_price`](#0x2_validator_set_derive_reference_gas_price)
-  [Function `total_validator_stake`](#0x2_validator_set_total_validator_stake)
-  [Function `total_delegation_stake`](#0x2_validator_set_total_delegation_stake)
-  [Function `validator_total_stake_amount`](#0x2_validator_set_validator_total_stake_amount)
-  [Function `validator_stake_amount`](#0x2_validator_set_validator_stake_amount)
-  [Function `validator_delegate_amount`](#0x2_validator_set_validator_delegate_amount)
-  [Function `validator_staking_pool_id`](#0x2_validator_set_validator_staking_pool_id)
-  [Function `staking_pool_mappings`](#0x2_validator_set_staking_pool_mappings)
-  [Function `next_epoch_validator_count`](#0x2_validator_set_next_epoch_validator_count)
-  [Function `is_active_validator`](#0x2_validator_set_is_active_validator)
-  [Function `contains_duplicate_validator`](#0x2_validator_set_contains_duplicate_validator)
-  [Function `find_validator`](#0x2_validator_set_find_validator)
-  [Function `get_validator_indices`](#0x2_validator_set_get_validator_indices)
-  [Function `get_validator_mut`](#0x2_validator_set_get_validator_mut)
-  [Function `get_validator_ref`](#0x2_validator_set_get_validator_ref)
-  [Function `process_pending_removals`](#0x2_validator_set_process_pending_removals)
-  [Function `process_pending_validators`](#0x2_validator_set_process_pending_validators)
-  [Function `sort_removal_list`](#0x2_validator_set_sort_removal_list)
-  [Function `process_pending_delegations_and_withdraws`](#0x2_validator_set_process_pending_delegations_and_withdraws)
-  [Function `calculate_total_stakes`](#0x2_validator_set_calculate_total_stakes)
-  [Function `adjust_stake_and_gas_price`](#0x2_validator_set_adjust_stake_and_gas_price)
-  [Function `compute_reward_adjustments`](#0x2_validator_set_compute_reward_adjustments)
-  [Function `compute_slashed_validators_and_total_stake`](#0x2_validator_set_compute_slashed_validators_and_total_stake)
-  [Function `compute_unadjusted_reward_distribution`](#0x2_validator_set_compute_unadjusted_reward_distribution)
-  [Function `compute_adjusted_reward_distribution`](#0x2_validator_set_compute_adjusted_reward_distribution)
-  [Function `distribute_reward`](#0x2_validator_set_distribute_reward)
-  [Function `derive_next_epoch_validators`](#0x2_validator_set_derive_next_epoch_validators)
-  [Function `emit_validator_epoch_events`](#0x2_validator_set_emit_validator_epoch_events)
-  [Function `sum_voting_power_by_addresses`](#0x2_validator_set_sum_voting_power_by_addresses)
-  [Function `active_validators`](#0x2_validator_set_active_validators)


<pre><code><b>use</b> <a href="">0x1::option</a>;
<b>use</b> <a href="">0x1::vector</a>;
<b>use</b> <a href="balance.md#0x2_balance">0x2::balance</a>;
<b>use</b> <a href="epoch_time_lock.md#0x2_epoch_time_lock">0x2::epoch_time_lock</a>;
<b>use</b> <a href="event.md#0x2_event">0x2::event</a>;
<b>use</b> <a href="object.md#0x2_object">0x2::object</a>;
<b>use</b> <a href="priority_queue.md#0x2_priority_queue">0x2::priority_queue</a>;
<b>use</b> <a href="stake.md#0x2_stake">0x2::stake</a>;
<b>use</b> <a href="staking_pool.md#0x2_staking_pool">0x2::staking_pool</a>;
<b>use</b> <a href="sui.md#0x2_sui">0x2::sui</a>;
<b>use</b> <a href="table.md#0x2_table">0x2::table</a>;
<b>use</b> <a href="tx_context.md#0x2_tx_context">0x2::tx_context</a>;
<b>use</b> <a href="validator.md#0x2_validator">0x2::validator</a>;
<b>use</b> <a href="vec_map.md#0x2_vec_map">0x2::vec_map</a>;
<b>use</b> <a href="vec_set.md#0x2_vec_set">0x2::vec_set</a>;
<b>use</b> <a href="voting_power.md#0x2_voting_power">0x2::voting_power</a>;
</code></pre>



<a name="0x2_validator_set_ValidatorSet"></a>

## Struct `ValidatorSet`



<pre><code><b>struct</b> <a href="validator_set.md#0x2_validator_set_ValidatorSet">ValidatorSet</a> <b>has</b> store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>total_validator_stake: u64</code>
</dt>
<dd>
 Total amount of stake from all active validators (not including delegation),
 at the beginning of the epoch.
</dd>
<dt>
<code>total_delegation_stake: u64</code>
</dt>
<dd>
 Total amount of stake from delegation, at the beginning of the epoch.
</dd>
<dt>
<code>active_validators: <a href="">vector</a>&lt;<a href="validator.md#0x2_validator_Validator">validator::Validator</a>&gt;</code>
</dt>
<dd>
 The current list of active validators.
</dd>
<dt>
<code>pending_validators: <a href="">vector</a>&lt;<a href="validator.md#0x2_validator_Validator">validator::Validator</a>&gt;</code>
</dt>
<dd>
 List of new validator candidates added during the current epoch.
 They will be processed at the end of the epoch.
</dd>
<dt>
<code>pending_removals: <a href="">vector</a>&lt;u64&gt;</code>
</dt>
<dd>
 Removal requests from the validators. Each element is an index
 pointing to <code>active_validators</code>.
</dd>
<dt>
<code>next_epoch_validators: <a href="">vector</a>&lt;<a href="validator.md#0x2_validator_ValidatorMetadata">validator::ValidatorMetadata</a>&gt;</code>
</dt>
<dd>
 The metadata of the validator set for the next epoch. This is kept up-to-dated.
 Every time a change request is received, this set is updated.
 TODO: This is currently not used. We may use it latter for enforcing min/max stake.
</dd>
<dt>
<code>staking_pool_mappings: <a href="table.md#0x2_table_Table">table::Table</a>&lt;<a href="object.md#0x2_object_ID">object::ID</a>, <b>address</b>&gt;</code>
</dt>
<dd>
 Mappings from staking pool's ID to the sui address of a validator.
</dd>
</dl>


</details>

<a name="0x2_validator_set_DelegationRequestEvent"></a>

## Struct `DelegationRequestEvent`

Event emitted when a new delegation request is received.


<pre><code><b>struct</b> <a href="validator_set.md#0x2_validator_set_DelegationRequestEvent">DelegationRequestEvent</a> <b>has</b> <b>copy</b>, drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>validator_address: <b>address</b></code>
</dt>
<dd>

</dd>
<dt>
<code>delegator_address: <b>address</b></code>
</dt>
<dd>

</dd>
<dt>
<code>epoch: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>amount: u64</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x2_validator_set_ValidatorEpochInfo"></a>

## Struct `ValidatorEpochInfo`

Event containing staking and rewards related information of
each validator, emitted during epoch advancement.


<pre><code><b>struct</b> <a href="validator_set.md#0x2_validator_set_ValidatorEpochInfo">ValidatorEpochInfo</a> <b>has</b> <b>copy</b>, drop
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
<code>validator_stake: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>delegated_stake: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>commission_rate: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>stake_rewards: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>pool_token_exchange_rate: <a href="staking_pool.md#0x2_staking_pool_PoolTokenExchangeRate">staking_pool::PoolTokenExchangeRate</a></code>
</dt>
<dd>

</dd>
<dt>
<code>tallying_rule_reporters: <a href="">vector</a>&lt;<b>address</b>&gt;</code>
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

<a name="@Constants_0"></a>

## Constants


<a name="0x2_validator_set_BASIS_POINT_DENOMINATOR"></a>



<pre><code><b>const</b> <a href="validator_set.md#0x2_validator_set_BASIS_POINT_DENOMINATOR">BASIS_POINT_DENOMINATOR</a>: u128 = 10000;
</code></pre>



<a name="0x2_validator_set_EDuplicateValidator"></a>



<pre><code><b>const</b> <a href="validator_set.md#0x2_validator_set_EDuplicateValidator">EDuplicateValidator</a>: u64 = 2;
</code></pre>



<a name="0x2_validator_set_EInvalidStakeAdjustmentAmount"></a>



<pre><code><b>const</b> <a href="validator_set.md#0x2_validator_set_EInvalidStakeAdjustmentAmount">EInvalidStakeAdjustmentAmount</a>: u64 = 1;
</code></pre>



<a name="0x2_validator_set_ENonValidatorInReportRecords"></a>



<pre><code><b>const</b> <a href="validator_set.md#0x2_validator_set_ENonValidatorInReportRecords">ENonValidatorInReportRecords</a>: u64 = 0;
</code></pre>



<a name="0x2_validator_set_new"></a>

## Function `new`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator_set.md#0x2_validator_set_new">new</a>(init_active_validators: <a href="">vector</a>&lt;<a href="validator.md#0x2_validator_Validator">validator::Validator</a>&gt;, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="validator_set.md#0x2_validator_set_ValidatorSet">validator_set::ValidatorSet</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator_set.md#0x2_validator_set_new">new</a>(init_active_validators: <a href="">vector</a>&lt;Validator&gt;, ctx: &<b>mut</b> TxContext): <a href="validator_set.md#0x2_validator_set_ValidatorSet">ValidatorSet</a> {
    <b>let</b> (total_validator_stake, total_delegation_stake) =
        <a href="validator_set.md#0x2_validator_set_calculate_total_stakes">calculate_total_stakes</a>(&init_active_validators);
    <b>let</b> staking_pool_mappings = <a href="table.md#0x2_table_new">table::new</a>(ctx);
    <b>let</b> num_validators = <a href="_length">vector::length</a>(&init_active_validators);
    <b>let</b> i = 0;
    <b>while</b> (i &lt; num_validators) {
        <b>let</b> <a href="validator.md#0x2_validator">validator</a> = <a href="_borrow">vector::borrow</a>(&init_active_validators, i);
        <a href="table.md#0x2_table_add">table::add</a>(&<b>mut</b> staking_pool_mappings, staking_pool_id(<a href="validator.md#0x2_validator">validator</a>), sui_address(<a href="validator.md#0x2_validator">validator</a>));
        i = i + 1;
    };
    <b>let</b> validators = <a href="validator_set.md#0x2_validator_set_ValidatorSet">ValidatorSet</a> {
        total_validator_stake,
        total_delegation_stake,
        active_validators: init_active_validators,
        pending_validators: <a href="_empty">vector::empty</a>(),
        pending_removals: <a href="_empty">vector::empty</a>(),
        next_epoch_validators: <a href="_empty">vector::empty</a>(),
        staking_pool_mappings,
    };
    validators.next_epoch_validators = <a href="validator_set.md#0x2_validator_set_derive_next_epoch_validators">derive_next_epoch_validators</a>(&validators);
    <a href="voting_power.md#0x2_voting_power_set_voting_power">voting_power::set_voting_power</a>(&<b>mut</b> validators.active_validators);
    validators
}
</code></pre>



</details>

<a name="0x2_validator_set_request_add_validator"></a>

## Function `request_add_validator`

Called by <code><a href="sui_system.md#0x2_sui_system">sui_system</a></code>, add a new validator to <code>pending_validators</code>, which will be
processed at the end of epoch.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator_set.md#0x2_validator_set_request_add_validator">request_add_validator</a>(self: &<b>mut</b> <a href="validator_set.md#0x2_validator_set_ValidatorSet">validator_set::ValidatorSet</a>, <a href="validator.md#0x2_validator">validator</a>: <a href="validator.md#0x2_validator_Validator">validator::Validator</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator_set.md#0x2_validator_set_request_add_validator">request_add_validator</a>(self: &<b>mut</b> <a href="validator_set.md#0x2_validator_set_ValidatorSet">ValidatorSet</a>, <a href="validator.md#0x2_validator">validator</a>: Validator) {
    <b>assert</b>!(
        !<a href="validator_set.md#0x2_validator_set_contains_duplicate_validator">contains_duplicate_validator</a>(&self.active_validators, &<a href="validator.md#0x2_validator">validator</a>)
            && !<a href="validator_set.md#0x2_validator_set_contains_duplicate_validator">contains_duplicate_validator</a>(&self.pending_validators, &<a href="validator.md#0x2_validator">validator</a>),
        <a href="validator_set.md#0x2_validator_set_EDuplicateValidator">EDuplicateValidator</a>
    );
    <a href="_push_back">vector::push_back</a>(&<b>mut</b> self.pending_validators, <a href="validator.md#0x2_validator">validator</a>);
    self.next_epoch_validators = <a href="validator_set.md#0x2_validator_set_derive_next_epoch_validators">derive_next_epoch_validators</a>(self);
}
</code></pre>



</details>

<a name="0x2_validator_set_request_remove_validator"></a>

## Function `request_remove_validator`

Called by <code><a href="sui_system.md#0x2_sui_system">sui_system</a></code>, to remove a validator.
The index of the validator is added to <code>pending_removals</code> and
will be processed at the end of epoch.
Only an active validator can request to be removed.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator_set.md#0x2_validator_set_request_remove_validator">request_remove_validator</a>(self: &<b>mut</b> <a href="validator_set.md#0x2_validator_set_ValidatorSet">validator_set::ValidatorSet</a>, ctx: &<a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator_set.md#0x2_validator_set_request_remove_validator">request_remove_validator</a>(
    self: &<b>mut</b> <a href="validator_set.md#0x2_validator_set_ValidatorSet">ValidatorSet</a>,
    ctx: &TxContext,
) {
    <b>let</b> validator_address = <a href="tx_context.md#0x2_tx_context_sender">tx_context::sender</a>(ctx);
    <b>let</b> validator_index_opt = <a href="validator_set.md#0x2_validator_set_find_validator">find_validator</a>(&self.active_validators, validator_address);
    <b>assert</b>!(<a href="_is_some">option::is_some</a>(&validator_index_opt), 0);
    <b>let</b> validator_index = <a href="_extract">option::extract</a>(&<b>mut</b> validator_index_opt);
    <b>assert</b>!(
        !<a href="_contains">vector::contains</a>(&self.pending_removals, &validator_index),
        0
    );
    <a href="_push_back">vector::push_back</a>(&<b>mut</b> self.pending_removals, validator_index);
    self.next_epoch_validators = <a href="validator_set.md#0x2_validator_set_derive_next_epoch_validators">derive_next_epoch_validators</a>(self);
}
</code></pre>



</details>

<a name="0x2_validator_set_request_add_stake"></a>

## Function `request_add_stake`

Called by <code><a href="sui_system.md#0x2_sui_system">sui_system</a></code>, to add more stake to a validator.
The new stake will be added to the validator's pending stake, which will be processed
at the end of epoch.
TODO: impl max stake requirement.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator_set.md#0x2_validator_set_request_add_stake">request_add_stake</a>(self: &<b>mut</b> <a href="validator_set.md#0x2_validator_set_ValidatorSet">validator_set::ValidatorSet</a>, new_stake: <a href="balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="sui.md#0x2_sui_SUI">sui::SUI</a>&gt;, coin_locked_until_epoch: <a href="_Option">option::Option</a>&lt;<a href="epoch_time_lock.md#0x2_epoch_time_lock_EpochTimeLock">epoch_time_lock::EpochTimeLock</a>&gt;, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator_set.md#0x2_validator_set_request_add_stake">request_add_stake</a>(
    self: &<b>mut</b> <a href="validator_set.md#0x2_validator_set_ValidatorSet">ValidatorSet</a>,
    new_stake: Balance&lt;SUI&gt;,
    coin_locked_until_epoch: Option&lt;EpochTimeLock&gt;,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> validator_address = <a href="tx_context.md#0x2_tx_context_sender">tx_context::sender</a>(ctx);
    <b>let</b> <a href="validator.md#0x2_validator">validator</a> = <a href="validator_set.md#0x2_validator_set_get_validator_mut">get_validator_mut</a>(&<b>mut</b> self.active_validators, validator_address);
    <a href="validator.md#0x2_validator_request_add_stake">validator::request_add_stake</a>(<a href="validator.md#0x2_validator">validator</a>, new_stake, coin_locked_until_epoch, ctx);
    self.next_epoch_validators = <a href="validator_set.md#0x2_validator_set_derive_next_epoch_validators">derive_next_epoch_validators</a>(self);
}
</code></pre>



</details>

<a name="0x2_validator_set_request_withdraw_stake"></a>

## Function `request_withdraw_stake`

Called by <code><a href="sui_system.md#0x2_sui_system">sui_system</a></code>, to withdraw stake from a validator.
We send a withdraw request to the validator which will be processed at the end of epoch.
The remaining stake of the validator cannot be lower than <code>min_validator_stake</code>.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator_set.md#0x2_validator_set_request_withdraw_stake">request_withdraw_stake</a>(self: &<b>mut</b> <a href="validator_set.md#0x2_validator_set_ValidatorSet">validator_set::ValidatorSet</a>, <a href="stake.md#0x2_stake">stake</a>: &<b>mut</b> <a href="stake.md#0x2_stake_Stake">stake::Stake</a>, withdraw_amount: u64, min_validator_stake: u64, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator_set.md#0x2_validator_set_request_withdraw_stake">request_withdraw_stake</a>(
    self: &<b>mut</b> <a href="validator_set.md#0x2_validator_set_ValidatorSet">ValidatorSet</a>,
    <a href="stake.md#0x2_stake">stake</a>: &<b>mut</b> Stake,
    withdraw_amount: u64,
    min_validator_stake: u64,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> validator_address = <a href="tx_context.md#0x2_tx_context_sender">tx_context::sender</a>(ctx);
    <b>let</b> <a href="validator.md#0x2_validator">validator</a> = <a href="validator_set.md#0x2_validator_set_get_validator_mut">get_validator_mut</a>(&<b>mut</b> self.active_validators, validator_address);
    <a href="validator.md#0x2_validator_request_withdraw_stake">validator::request_withdraw_stake</a>(<a href="validator.md#0x2_validator">validator</a>, <a href="stake.md#0x2_stake">stake</a>, withdraw_amount, min_validator_stake, ctx);
    self.next_epoch_validators = <a href="validator_set.md#0x2_validator_set_derive_next_epoch_validators">derive_next_epoch_validators</a>(self);
}
</code></pre>



</details>

<a name="0x2_validator_set_request_add_delegation"></a>

## Function `request_add_delegation`

Called by <code><a href="sui_system.md#0x2_sui_system">sui_system</a></code>, to add a new delegation to the validator.
This request is added to the validator's staking pool's pending delegation entries, processed at the end
of the epoch.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator_set.md#0x2_validator_set_request_add_delegation">request_add_delegation</a>(self: &<b>mut</b> <a href="validator_set.md#0x2_validator_set_ValidatorSet">validator_set::ValidatorSet</a>, validator_address: <b>address</b>, delegated_stake: <a href="balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="sui.md#0x2_sui_SUI">sui::SUI</a>&gt;, locking_period: <a href="_Option">option::Option</a>&lt;<a href="epoch_time_lock.md#0x2_epoch_time_lock_EpochTimeLock">epoch_time_lock::EpochTimeLock</a>&gt;, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator_set.md#0x2_validator_set_request_add_delegation">request_add_delegation</a>(
    self: &<b>mut</b> <a href="validator_set.md#0x2_validator_set_ValidatorSet">ValidatorSet</a>,
    validator_address: <b>address</b>,
    delegated_stake: Balance&lt;SUI&gt;,
    locking_period: Option&lt;EpochTimeLock&gt;,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> <a href="validator.md#0x2_validator">validator</a> = <a href="validator_set.md#0x2_validator_set_get_validator_mut">get_validator_mut</a>(&<b>mut</b> self.active_validators, validator_address);
    <b>let</b> delegator_address = <a href="tx_context.md#0x2_tx_context_sender">tx_context::sender</a>(ctx);
    <b>let</b> amount = <a href="balance.md#0x2_balance_value">balance::value</a>(&delegated_stake);
    <a href="validator.md#0x2_validator_request_add_delegation">validator::request_add_delegation</a>(<a href="validator.md#0x2_validator">validator</a>, delegated_stake, locking_period, <a href="tx_context.md#0x2_tx_context_sender">tx_context::sender</a>(ctx), ctx);
    self.next_epoch_validators = <a href="validator_set.md#0x2_validator_set_derive_next_epoch_validators">derive_next_epoch_validators</a>(self);
    <a href="event.md#0x2_event_emit">event::emit</a>(
        <a href="validator_set.md#0x2_validator_set_DelegationRequestEvent">DelegationRequestEvent</a> {
            validator_address,
            delegator_address,
            epoch: <a href="tx_context.md#0x2_tx_context_epoch">tx_context::epoch</a>(ctx),
            amount,
        }
    );
}
</code></pre>



</details>

<a name="0x2_validator_set_request_withdraw_delegation"></a>

## Function `request_withdraw_delegation`

Called by <code><a href="sui_system.md#0x2_sui_system">sui_system</a></code>, to withdraw some share of a delegation from the validator. The share to withdraw
is denoted by <code>principal_withdraw_amount</code>.
This request is added to the validator's staking pool's pending delegation withdraw entries, processed at the end
of the epoch.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator_set.md#0x2_validator_set_request_withdraw_delegation">request_withdraw_delegation</a>(self: &<b>mut</b> <a href="validator_set.md#0x2_validator_set_ValidatorSet">validator_set::ValidatorSet</a>, staked_sui: <a href="staking_pool.md#0x2_staking_pool_StakedSui">staking_pool::StakedSui</a>, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator_set.md#0x2_validator_set_request_withdraw_delegation">request_withdraw_delegation</a>(
    self: &<b>mut</b> <a href="validator_set.md#0x2_validator_set_ValidatorSet">ValidatorSet</a>,
    staked_sui: StakedSui,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> validator_address = *<a href="table.md#0x2_table_borrow">table::borrow</a>(&self.staking_pool_mappings, pool_id(&staked_sui));
    <b>let</b> validator_index_opt = <a href="validator_set.md#0x2_validator_set_find_validator">find_validator</a>(&self.active_validators, validator_address);

    <b>assert</b>!(<a href="_is_some">option::is_some</a>(&validator_index_opt), 0);

    <b>let</b> validator_index = <a href="_extract">option::extract</a>(&<b>mut</b> validator_index_opt);
    <b>let</b> <a href="validator.md#0x2_validator">validator</a> = <a href="_borrow_mut">vector::borrow_mut</a>(&<b>mut</b> self.active_validators, validator_index);
    <a href="validator.md#0x2_validator_request_withdraw_delegation">validator::request_withdraw_delegation</a>(<a href="validator.md#0x2_validator">validator</a>, staked_sui, ctx);
    self.next_epoch_validators = <a href="validator_set.md#0x2_validator_set_derive_next_epoch_validators">derive_next_epoch_validators</a>(self);
}
</code></pre>



</details>

<a name="0x2_validator_set_request_set_gas_price"></a>

## Function `request_set_gas_price`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator_set.md#0x2_validator_set_request_set_gas_price">request_set_gas_price</a>(self: &<b>mut</b> <a href="validator_set.md#0x2_validator_set_ValidatorSet">validator_set::ValidatorSet</a>, new_gas_price: u64, ctx: &<a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator_set.md#0x2_validator_set_request_set_gas_price">request_set_gas_price</a>(
    self: &<b>mut</b> <a href="validator_set.md#0x2_validator_set_ValidatorSet">ValidatorSet</a>,
    new_gas_price: u64,
    ctx: &TxContext,
) {
    <b>let</b> validator_address = <a href="tx_context.md#0x2_tx_context_sender">tx_context::sender</a>(ctx);
    <b>let</b> <a href="validator.md#0x2_validator">validator</a> = <a href="validator_set.md#0x2_validator_set_get_validator_mut">get_validator_mut</a>(&<b>mut</b> self.active_validators, validator_address);
    <a href="validator.md#0x2_validator_request_set_gas_price">validator::request_set_gas_price</a>(<a href="validator.md#0x2_validator">validator</a>, new_gas_price);
}
</code></pre>



</details>

<a name="0x2_validator_set_request_set_commission_rate"></a>

## Function `request_set_commission_rate`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator_set.md#0x2_validator_set_request_set_commission_rate">request_set_commission_rate</a>(self: &<b>mut</b> <a href="validator_set.md#0x2_validator_set_ValidatorSet">validator_set::ValidatorSet</a>, new_commission_rate: u64, ctx: &<a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator_set.md#0x2_validator_set_request_set_commission_rate">request_set_commission_rate</a>(
    self: &<b>mut</b> <a href="validator_set.md#0x2_validator_set_ValidatorSet">ValidatorSet</a>,
    new_commission_rate: u64,
    ctx: &TxContext,
) {
    <b>let</b> validator_address = <a href="tx_context.md#0x2_tx_context_sender">tx_context::sender</a>(ctx);
    <b>let</b> <a href="validator.md#0x2_validator">validator</a> = <a href="validator_set.md#0x2_validator_set_get_validator_mut">get_validator_mut</a>(&<b>mut</b> self.active_validators, validator_address);
    <a href="validator.md#0x2_validator_request_set_commission_rate">validator::request_set_commission_rate</a>(<a href="validator.md#0x2_validator">validator</a>, new_commission_rate);
}
</code></pre>



</details>

<a name="0x2_validator_set_advance_epoch"></a>

## Function `advance_epoch`

Update the validator set at the end of epoch.
It does the following things:
1. Distribute stake award.
2. Process pending stake deposits and withdraws for each validator (<code>adjust_stake</code>).
3. Process pending delegation deposits, and withdraws.
4. Process pending validator application and withdraws.
5. At the end, we calculate the total stake for the new epoch.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator_set.md#0x2_validator_set_advance_epoch">advance_epoch</a>(self: &<b>mut</b> <a href="validator_set.md#0x2_validator_set_ValidatorSet">validator_set::ValidatorSet</a>, computation_reward: &<b>mut</b> <a href="balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="sui.md#0x2_sui_SUI">sui::SUI</a>&gt;, storage_fund_reward: &<b>mut</b> <a href="balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="sui.md#0x2_sui_SUI">sui::SUI</a>&gt;, validator_report_records: <a href="vec_map.md#0x2_vec_map_VecMap">vec_map::VecMap</a>&lt;<b>address</b>, <a href="vec_set.md#0x2_vec_set_VecSet">vec_set::VecSet</a>&lt;<b>address</b>&gt;&gt;, reward_slashing_rate: u64, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator_set.md#0x2_validator_set_advance_epoch">advance_epoch</a>(
    self: &<b>mut</b> <a href="validator_set.md#0x2_validator_set_ValidatorSet">ValidatorSet</a>,
    computation_reward: &<b>mut</b> Balance&lt;SUI&gt;,
    storage_fund_reward: &<b>mut</b> Balance&lt;SUI&gt;,
    validator_report_records: VecMap&lt;<b>address</b>, VecSet&lt;<b>address</b>&gt;&gt;,
    reward_slashing_rate: u64,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> new_epoch = <a href="tx_context.md#0x2_tx_context_epoch">tx_context::epoch</a>(ctx) + 1;
    <b>let</b> total_stake = self.total_validator_stake + self.total_delegation_stake;

    // Compute the reward distribution without taking into account the tallying rule slashing.
    <b>let</b> (unadjusted_staking_reward_amounts, unadjusted_storage_fund_reward_amounts) = <a href="validator_set.md#0x2_validator_set_compute_unadjusted_reward_distribution">compute_unadjusted_reward_distribution</a>(
        &self.active_validators,
        total_stake,
        <a href="balance.md#0x2_balance_value">balance::value</a>(computation_reward),
        <a href="balance.md#0x2_balance_value">balance::value</a>(storage_fund_reward),
    );

    // Use the tallying rule report records for the epoch <b>to</b> compute validators that will be
    // punished and the sum of their stakes.
    <b>let</b> (slashed_validators, total_slashed_validator_stake) =
        <a href="validator_set.md#0x2_validator_set_compute_slashed_validators_and_total_stake">compute_slashed_validators_and_total_stake</a>(
            self,
            <b>copy</b> validator_report_records,
        );

    // Compute the reward adjustments of slashed validators, <b>to</b> be taken into
    // account in adjusted reward computation.
    <b>let</b> (total_staking_reward_adjustment, individual_staking_reward_adjustments,
         total_storage_fund_reward_adjustment, individual_storage_fund_reward_adjustments
        ) =
        <a href="validator_set.md#0x2_validator_set_compute_reward_adjustments">compute_reward_adjustments</a>(
            <a href="validator_set.md#0x2_validator_set_get_validator_indices">get_validator_indices</a>(&self.active_validators, &slashed_validators),
            reward_slashing_rate,
            &unadjusted_staking_reward_amounts,
            &unadjusted_storage_fund_reward_amounts,
        );

    // Compute the adjusted amounts of <a href="stake.md#0x2_stake">stake</a> each <a href="validator.md#0x2_validator">validator</a> should get given the tallying rule
    // reward adjustments we computed before.
    // `compute_adjusted_reward_distribution` must be called before `distribute_reward` and `adjust_stake_and_gas_price` <b>to</b>
    // make sure we are using the current epoch's <a href="stake.md#0x2_stake">stake</a> information <b>to</b> compute reward distribution.
    <b>let</b> (adjusted_staking_reward_amounts, adjusted_storage_fund_reward_amounts) = <a href="validator_set.md#0x2_validator_set_compute_adjusted_reward_distribution">compute_adjusted_reward_distribution</a>(
        &self.active_validators,
        total_stake,
        total_slashed_validator_stake,
        unadjusted_staking_reward_amounts,
        unadjusted_storage_fund_reward_amounts,
        total_staking_reward_adjustment,
        individual_staking_reward_adjustments,
        total_storage_fund_reward_adjustment,
        individual_storage_fund_reward_adjustments
    );

    // Distribute the rewards before adjusting <a href="stake.md#0x2_stake">stake</a> so that we immediately start compounding
    // the rewards for validators and delegators.
    <a href="validator_set.md#0x2_validator_set_distribute_reward">distribute_reward</a>(
        &<b>mut</b> self.active_validators,
        &adjusted_staking_reward_amounts,
        &adjusted_storage_fund_reward_amounts,
        computation_reward,
        storage_fund_reward,
        ctx
    );

    <a href="validator_set.md#0x2_validator_set_adjust_stake_and_gas_price">adjust_stake_and_gas_price</a>(&<b>mut</b> self.active_validators);

    <a href="validator_set.md#0x2_validator_set_process_pending_delegations_and_withdraws">process_pending_delegations_and_withdraws</a>(&<b>mut</b> self.active_validators, ctx);

    // Emit events after we have processed all the rewards distribution and pending delegations.
    <a href="validator_set.md#0x2_validator_set_emit_validator_epoch_events">emit_validator_epoch_events</a>(new_epoch, &self.active_validators, &adjusted_staking_reward_amounts,
        &validator_report_records, &slashed_validators);

    <a href="validator_set.md#0x2_validator_set_process_pending_validators">process_pending_validators</a>(self);

    <a href="validator_set.md#0x2_validator_set_process_pending_removals">process_pending_removals</a>(self, ctx);

    self.next_epoch_validators = <a href="validator_set.md#0x2_validator_set_derive_next_epoch_validators">derive_next_epoch_validators</a>(self);

    <b>let</b> (validator_stake, delegation_stake) = <a href="validator_set.md#0x2_validator_set_calculate_total_stakes">calculate_total_stakes</a>(&self.active_validators);
    self.total_validator_stake = validator_stake;
    self.total_delegation_stake = delegation_stake;

    <a href="voting_power.md#0x2_voting_power_set_voting_power">voting_power::set_voting_power</a>(&<b>mut</b> self.active_validators);
}
</code></pre>



</details>

<a name="0x2_validator_set_derive_reference_gas_price"></a>

## Function `derive_reference_gas_price`

Called by <code><a href="sui_system.md#0x2_sui_system">sui_system</a></code> to derive reference gas price for the new epoch.
Derive the reference gas price based on the gas price quote submitted by each validator.
The returned gas price should be greater than or equal to 2/3 of the validators submitted
gas price, weighted by stake.


<pre><code><b>public</b> <b>fun</b> <a href="validator_set.md#0x2_validator_set_derive_reference_gas_price">derive_reference_gas_price</a>(self: &<a href="validator_set.md#0x2_validator_set_ValidatorSet">validator_set::ValidatorSet</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="validator_set.md#0x2_validator_set_derive_reference_gas_price">derive_reference_gas_price</a>(self: &<a href="validator_set.md#0x2_validator_set_ValidatorSet">ValidatorSet</a>): u64 {
    <b>let</b> vs = &self.active_validators;
    <b>let</b> num_validators = <a href="_length">vector::length</a>(vs);
    <b>let</b> entries = <a href="_empty">vector::empty</a>();
    <b>let</b> i = 0;
    <b>while</b> (i &lt; num_validators) {
        <b>let</b> v = <a href="_borrow">vector::borrow</a>(vs, i);
        <a href="_push_back">vector::push_back</a>(
            &<b>mut</b> entries,
            pq::new_entry(<a href="validator.md#0x2_validator_gas_price">validator::gas_price</a>(v), <a href="validator.md#0x2_validator_voting_power">validator::voting_power</a>(v))
        );
        i = i + 1;
    };
    // Build a priority queue that will pop entries <b>with</b> gas price from the highest <b>to</b> the lowest.
    <b>let</b> pq = pq::new(entries);
    <b>let</b> sum = 0;
    <b>let</b> threshold = <a href="voting_power.md#0x2_voting_power_total_voting_power">voting_power::total_voting_power</a>() - <a href="voting_power.md#0x2_voting_power_quorum_threshold">voting_power::quorum_threshold</a>();
    <b>let</b> result = 0;
    <b>while</b> (sum &lt; threshold) {
        <b>let</b> (gas_price, <a href="stake.md#0x2_stake">stake</a>) = pq::pop_max(&<b>mut</b> pq);
        result = gas_price;
        sum = sum + <a href="stake.md#0x2_stake">stake</a>;
    };
    result
}
</code></pre>



</details>

<a name="0x2_validator_set_total_validator_stake"></a>

## Function `total_validator_stake`



<pre><code><b>public</b> <b>fun</b> <a href="validator_set.md#0x2_validator_set_total_validator_stake">total_validator_stake</a>(self: &<a href="validator_set.md#0x2_validator_set_ValidatorSet">validator_set::ValidatorSet</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="validator_set.md#0x2_validator_set_total_validator_stake">total_validator_stake</a>(self: &<a href="validator_set.md#0x2_validator_set_ValidatorSet">ValidatorSet</a>): u64 {
    self.total_validator_stake
}
</code></pre>



</details>

<a name="0x2_validator_set_total_delegation_stake"></a>

## Function `total_delegation_stake`



<pre><code><b>public</b> <b>fun</b> <a href="validator_set.md#0x2_validator_set_total_delegation_stake">total_delegation_stake</a>(self: &<a href="validator_set.md#0x2_validator_set_ValidatorSet">validator_set::ValidatorSet</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="validator_set.md#0x2_validator_set_total_delegation_stake">total_delegation_stake</a>(self: &<a href="validator_set.md#0x2_validator_set_ValidatorSet">ValidatorSet</a>): u64 {
    self.total_delegation_stake
}
</code></pre>



</details>

<a name="0x2_validator_set_validator_total_stake_amount"></a>

## Function `validator_total_stake_amount`



<pre><code><b>public</b> <b>fun</b> <a href="validator_set.md#0x2_validator_set_validator_total_stake_amount">validator_total_stake_amount</a>(self: &<a href="validator_set.md#0x2_validator_set_ValidatorSet">validator_set::ValidatorSet</a>, validator_address: <b>address</b>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="validator_set.md#0x2_validator_set_validator_total_stake_amount">validator_total_stake_amount</a>(self: &<a href="validator_set.md#0x2_validator_set_ValidatorSet">ValidatorSet</a>, validator_address: <b>address</b>): u64 {
    <b>let</b> <a href="validator.md#0x2_validator">validator</a> = <a href="validator_set.md#0x2_validator_set_get_validator_ref">get_validator_ref</a>(&self.active_validators, validator_address);
    <a href="validator.md#0x2_validator_total_stake_amount">validator::total_stake_amount</a>(<a href="validator.md#0x2_validator">validator</a>)
}
</code></pre>



</details>

<a name="0x2_validator_set_validator_stake_amount"></a>

## Function `validator_stake_amount`



<pre><code><b>public</b> <b>fun</b> <a href="validator_set.md#0x2_validator_set_validator_stake_amount">validator_stake_amount</a>(self: &<a href="validator_set.md#0x2_validator_set_ValidatorSet">validator_set::ValidatorSet</a>, validator_address: <b>address</b>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="validator_set.md#0x2_validator_set_validator_stake_amount">validator_stake_amount</a>(self: &<a href="validator_set.md#0x2_validator_set_ValidatorSet">ValidatorSet</a>, validator_address: <b>address</b>): u64 {
    <b>let</b> <a href="validator.md#0x2_validator">validator</a> = <a href="validator_set.md#0x2_validator_set_get_validator_ref">get_validator_ref</a>(&self.active_validators, validator_address);
    <a href="validator.md#0x2_validator_stake_amount">validator::stake_amount</a>(<a href="validator.md#0x2_validator">validator</a>)
}
</code></pre>



</details>

<a name="0x2_validator_set_validator_delegate_amount"></a>

## Function `validator_delegate_amount`



<pre><code><b>public</b> <b>fun</b> <a href="validator_set.md#0x2_validator_set_validator_delegate_amount">validator_delegate_amount</a>(self: &<a href="validator_set.md#0x2_validator_set_ValidatorSet">validator_set::ValidatorSet</a>, validator_address: <b>address</b>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="validator_set.md#0x2_validator_set_validator_delegate_amount">validator_delegate_amount</a>(self: &<a href="validator_set.md#0x2_validator_set_ValidatorSet">ValidatorSet</a>, validator_address: <b>address</b>): u64 {
    <b>let</b> <a href="validator.md#0x2_validator">validator</a> = <a href="validator_set.md#0x2_validator_set_get_validator_ref">get_validator_ref</a>(&self.active_validators, validator_address);
    <a href="validator.md#0x2_validator_delegate_amount">validator::delegate_amount</a>(<a href="validator.md#0x2_validator">validator</a>)
}
</code></pre>



</details>

<a name="0x2_validator_set_validator_staking_pool_id"></a>

## Function `validator_staking_pool_id`



<pre><code><b>public</b> <b>fun</b> <a href="validator_set.md#0x2_validator_set_validator_staking_pool_id">validator_staking_pool_id</a>(self: &<a href="validator_set.md#0x2_validator_set_ValidatorSet">validator_set::ValidatorSet</a>, validator_address: <b>address</b>): <a href="object.md#0x2_object_ID">object::ID</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="validator_set.md#0x2_validator_set_validator_staking_pool_id">validator_staking_pool_id</a>(self: &<a href="validator_set.md#0x2_validator_set_ValidatorSet">ValidatorSet</a>, validator_address: <b>address</b>): ID {
    <b>let</b> <a href="validator.md#0x2_validator">validator</a> = <a href="validator_set.md#0x2_validator_set_get_validator_ref">get_validator_ref</a>(&self.active_validators, validator_address);
    <a href="validator.md#0x2_validator_staking_pool_id">validator::staking_pool_id</a>(<a href="validator.md#0x2_validator">validator</a>)
}
</code></pre>



</details>

<a name="0x2_validator_set_staking_pool_mappings"></a>

## Function `staking_pool_mappings`



<pre><code><b>public</b> <b>fun</b> <a href="validator_set.md#0x2_validator_set_staking_pool_mappings">staking_pool_mappings</a>(self: &<a href="validator_set.md#0x2_validator_set_ValidatorSet">validator_set::ValidatorSet</a>): &<a href="table.md#0x2_table_Table">table::Table</a>&lt;<a href="object.md#0x2_object_ID">object::ID</a>, <b>address</b>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="validator_set.md#0x2_validator_set_staking_pool_mappings">staking_pool_mappings</a>(self: &<a href="validator_set.md#0x2_validator_set_ValidatorSet">ValidatorSet</a>): &Table&lt;ID, <b>address</b>&gt; {
    &self.staking_pool_mappings
}
</code></pre>



</details>

<a name="0x2_validator_set_next_epoch_validator_count"></a>

## Function `next_epoch_validator_count`

Get the total number of validators in the next epoch.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator_set.md#0x2_validator_set_next_epoch_validator_count">next_epoch_validator_count</a>(self: &<a href="validator_set.md#0x2_validator_set_ValidatorSet">validator_set::ValidatorSet</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator_set.md#0x2_validator_set_next_epoch_validator_count">next_epoch_validator_count</a>(self: &<a href="validator_set.md#0x2_validator_set_ValidatorSet">ValidatorSet</a>): u64 {
    <a href="_length">vector::length</a>(&self.next_epoch_validators)
}
</code></pre>



</details>

<a name="0x2_validator_set_is_active_validator"></a>

## Function `is_active_validator`

Returns true iff <code>validator_address</code> is a member of the active validators.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator_set.md#0x2_validator_set_is_active_validator">is_active_validator</a>(self: &<a href="validator_set.md#0x2_validator_set_ValidatorSet">validator_set::ValidatorSet</a>, validator_address: <b>address</b>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator_set.md#0x2_validator_set_is_active_validator">is_active_validator</a>(
    self: &<a href="validator_set.md#0x2_validator_set_ValidatorSet">ValidatorSet</a>,
    validator_address: <b>address</b>,
): bool {
    <a href="_is_some">option::is_some</a>(&<a href="validator_set.md#0x2_validator_set_find_validator">find_validator</a>(&self.active_validators, validator_address))
}
</code></pre>



</details>

<a name="0x2_validator_set_contains_duplicate_validator"></a>

## Function `contains_duplicate_validator`

Checks whether a duplicate of <code>new_validator</code> is already in <code>validators</code>.
Two validators duplicate if they share the same sui_address or same IP or same name.


<pre><code><b>fun</b> <a href="validator_set.md#0x2_validator_set_contains_duplicate_validator">contains_duplicate_validator</a>(validators: &<a href="">vector</a>&lt;<a href="validator.md#0x2_validator_Validator">validator::Validator</a>&gt;, new_validator: &<a href="validator.md#0x2_validator_Validator">validator::Validator</a>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="validator_set.md#0x2_validator_set_contains_duplicate_validator">contains_duplicate_validator</a>(validators: &<a href="">vector</a>&lt;Validator&gt;, new_validator: &Validator): bool {
    <b>let</b> len = <a href="_length">vector::length</a>(validators);
    <b>let</b> i = 0;
    <b>while</b> (i &lt; len) {
        <b>let</b> v = <a href="_borrow">vector::borrow</a>(validators, i);
        <b>if</b> (<a href="validator.md#0x2_validator_is_duplicate">validator::is_duplicate</a>(v, new_validator)) {
            <b>return</b> <b>true</b>
        };
        i = i + 1;
    };
    <b>false</b>
}
</code></pre>



</details>

<a name="0x2_validator_set_find_validator"></a>

## Function `find_validator`

Find validator by <code>validator_address</code>, in <code>validators</code>.
Returns (true, index) if the validator is found, and the index is its index in the list.
If not found, returns (false, 0).


<pre><code><b>fun</b> <a href="validator_set.md#0x2_validator_set_find_validator">find_validator</a>(validators: &<a href="">vector</a>&lt;<a href="validator.md#0x2_validator_Validator">validator::Validator</a>&gt;, validator_address: <b>address</b>): <a href="_Option">option::Option</a>&lt;u64&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="validator_set.md#0x2_validator_set_find_validator">find_validator</a>(validators: &<a href="">vector</a>&lt;Validator&gt;, validator_address: <b>address</b>): Option&lt;u64&gt; {
    <b>let</b> length = <a href="_length">vector::length</a>(validators);
    <b>let</b> i = 0;
    <b>while</b> (i &lt; length) {
        <b>let</b> v = <a href="_borrow">vector::borrow</a>(validators, i);
        <b>if</b> (<a href="validator.md#0x2_validator_sui_address">validator::sui_address</a>(v) == validator_address) {
            <b>return</b> <a href="_some">option::some</a>(i)
        };
        i = i + 1;
    };
    <a href="_none">option::none</a>()
}
</code></pre>



</details>

<a name="0x2_validator_set_get_validator_indices"></a>

## Function `get_validator_indices`

Given a vector of validator addresses, return their indices in the validator set.
Aborts if any address isn't in the given validator set.


<pre><code><b>fun</b> <a href="validator_set.md#0x2_validator_set_get_validator_indices">get_validator_indices</a>(validators: &<a href="">vector</a>&lt;<a href="validator.md#0x2_validator_Validator">validator::Validator</a>&gt;, validator_addresses: &<a href="">vector</a>&lt;<b>address</b>&gt;): <a href="">vector</a>&lt;u64&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="validator_set.md#0x2_validator_set_get_validator_indices">get_validator_indices</a>(validators: &<a href="">vector</a>&lt;Validator&gt;, validator_addresses: &<a href="">vector</a>&lt;<b>address</b>&gt;): <a href="">vector</a>&lt;u64&gt; {
    <b>let</b> length = <a href="_length">vector::length</a>(validator_addresses);
    <b>let</b> i = 0;
    <b>let</b> res = <a href="">vector</a>[];
    <b>while</b> (i &lt; length) {
        <b>let</b> addr = *<a href="_borrow">vector::borrow</a>(validator_addresses, i);
        <b>let</b> index_opt = <a href="validator_set.md#0x2_validator_set_find_validator">find_validator</a>(validators, addr);
        <b>assert</b>!(<a href="_is_some">option::is_some</a>(&index_opt), 0);
        <a href="_push_back">vector::push_back</a>(&<b>mut</b> res, <a href="_destroy_some">option::destroy_some</a>(index_opt));
        i = i + 1;
    };
    res
}
</code></pre>



</details>

<a name="0x2_validator_set_get_validator_mut"></a>

## Function `get_validator_mut`



<pre><code><b>fun</b> <a href="validator_set.md#0x2_validator_set_get_validator_mut">get_validator_mut</a>(validators: &<b>mut</b> <a href="">vector</a>&lt;<a href="validator.md#0x2_validator_Validator">validator::Validator</a>&gt;, validator_address: <b>address</b>): &<b>mut</b> <a href="validator.md#0x2_validator_Validator">validator::Validator</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="validator_set.md#0x2_validator_set_get_validator_mut">get_validator_mut</a>(
    validators: &<b>mut</b> <a href="">vector</a>&lt;Validator&gt;,
    validator_address: <b>address</b>,
): &<b>mut</b> Validator {
    <b>let</b> validator_index_opt = <a href="validator_set.md#0x2_validator_set_find_validator">find_validator</a>(validators, validator_address);
    <b>assert</b>!(<a href="_is_some">option::is_some</a>(&validator_index_opt), 0);
    <b>let</b> validator_index = <a href="_extract">option::extract</a>(&<b>mut</b> validator_index_opt);
    <a href="_borrow_mut">vector::borrow_mut</a>(validators, validator_index)
}
</code></pre>



</details>

<a name="0x2_validator_set_get_validator_ref"></a>

## Function `get_validator_ref`



<pre><code><b>fun</b> <a href="validator_set.md#0x2_validator_set_get_validator_ref">get_validator_ref</a>(validators: &<a href="">vector</a>&lt;<a href="validator.md#0x2_validator_Validator">validator::Validator</a>&gt;, validator_address: <b>address</b>): &<a href="validator.md#0x2_validator_Validator">validator::Validator</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="validator_set.md#0x2_validator_set_get_validator_ref">get_validator_ref</a>(
    validators: &<a href="">vector</a>&lt;Validator&gt;,
    validator_address: <b>address</b>,
): &Validator {
    <b>let</b> validator_index_opt = <a href="validator_set.md#0x2_validator_set_find_validator">find_validator</a>(validators, validator_address);
    <b>assert</b>!(<a href="_is_some">option::is_some</a>(&validator_index_opt), 0);
    <b>let</b> validator_index = <a href="_extract">option::extract</a>(&<b>mut</b> validator_index_opt);
    <a href="_borrow">vector::borrow</a>(validators, validator_index)
}
</code></pre>



</details>

<a name="0x2_validator_set_process_pending_removals"></a>

## Function `process_pending_removals`

Process the pending withdraw requests. For each pending request, the validator
is removed from <code>validators</code> and sent back to the address of the validator.


<pre><code><b>fun</b> <a href="validator_set.md#0x2_validator_set_process_pending_removals">process_pending_removals</a>(self: &<b>mut</b> <a href="validator_set.md#0x2_validator_set_ValidatorSet">validator_set::ValidatorSet</a>, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="validator_set.md#0x2_validator_set_process_pending_removals">process_pending_removals</a>(
    self: &<b>mut</b> <a href="validator_set.md#0x2_validator_set_ValidatorSet">ValidatorSet</a>,
    ctx: &<b>mut</b> TxContext,
) {
    <a href="validator_set.md#0x2_validator_set_sort_removal_list">sort_removal_list</a>(&<b>mut</b> self.pending_removals);
    <b>while</b> (!<a href="_is_empty">vector::is_empty</a>(&self.pending_removals)) {
        <b>let</b> index = <a href="_pop_back">vector::pop_back</a>(&<b>mut</b> self.pending_removals);
        <b>let</b> <a href="validator.md#0x2_validator">validator</a> = <a href="_remove">vector::remove</a>(&<b>mut</b> self.active_validators, index);
        <a href="table.md#0x2_table_remove">table::remove</a>(&<b>mut</b> self.staking_pool_mappings, staking_pool_id(&<a href="validator.md#0x2_validator">validator</a>));
        self.total_delegation_stake = self.total_delegation_stake - <a href="validator.md#0x2_validator_delegate_amount">validator::delegate_amount</a>(&<a href="validator.md#0x2_validator">validator</a>);
        <a href="validator.md#0x2_validator_destroy">validator::destroy</a>(<a href="validator.md#0x2_validator">validator</a>, ctx);
    }
}
</code></pre>



</details>

<a name="0x2_validator_set_process_pending_validators"></a>

## Function `process_pending_validators`

Process the pending new validators. They are simply inserted into <code>validators</code>.


<pre><code><b>fun</b> <a href="validator_set.md#0x2_validator_set_process_pending_validators">process_pending_validators</a>(self: &<b>mut</b> <a href="validator_set.md#0x2_validator_set_ValidatorSet">validator_set::ValidatorSet</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="validator_set.md#0x2_validator_set_process_pending_validators">process_pending_validators</a>(
    self: &<b>mut</b> <a href="validator_set.md#0x2_validator_set_ValidatorSet">ValidatorSet</a>,
) {
    <b>while</b> (!<a href="_is_empty">vector::is_empty</a>(&self.pending_validators)) {
        <b>let</b> <a href="validator.md#0x2_validator">validator</a> = <a href="_pop_back">vector::pop_back</a>(&<b>mut</b> self.pending_validators);
        <a href="table.md#0x2_table_add">table::add</a>(&<b>mut</b> self.staking_pool_mappings, staking_pool_id(&<a href="validator.md#0x2_validator">validator</a>), sui_address(&<a href="validator.md#0x2_validator">validator</a>));
        <a href="_push_back">vector::push_back</a>(&<b>mut</b> self.active_validators, <a href="validator.md#0x2_validator">validator</a>);
    }
}
</code></pre>



</details>

<a name="0x2_validator_set_sort_removal_list"></a>

## Function `sort_removal_list`

Sort all the pending removal indexes.


<pre><code><b>fun</b> <a href="validator_set.md#0x2_validator_set_sort_removal_list">sort_removal_list</a>(withdraw_list: &<b>mut</b> <a href="">vector</a>&lt;u64&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="validator_set.md#0x2_validator_set_sort_removal_list">sort_removal_list</a>(withdraw_list: &<b>mut</b> <a href="">vector</a>&lt;u64&gt;) {
    <b>let</b> length = <a href="_length">vector::length</a>(withdraw_list);
    <b>let</b> i = 1;
    <b>while</b> (i &lt; length) {
        <b>let</b> cur = *<a href="_borrow">vector::borrow</a>(withdraw_list, i);
        <b>let</b> j = i;
        <b>while</b> (j &gt; 0) {
            j = j - 1;
            <b>if</b> (*<a href="_borrow">vector::borrow</a>(withdraw_list, j) &gt; cur) {
                <a href="_swap">vector::swap</a>(withdraw_list, j, j + 1);
            } <b>else</b> {
                <b>break</b>
            };
        };
        i = i + 1;
    };
}
</code></pre>



</details>

<a name="0x2_validator_set_process_pending_delegations_and_withdraws"></a>

## Function `process_pending_delegations_and_withdraws`

Process all active validators' pending delegation deposits and withdraws.


<pre><code><b>fun</b> <a href="validator_set.md#0x2_validator_set_process_pending_delegations_and_withdraws">process_pending_delegations_and_withdraws</a>(validators: &<b>mut</b> <a href="">vector</a>&lt;<a href="validator.md#0x2_validator_Validator">validator::Validator</a>&gt;, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="validator_set.md#0x2_validator_set_process_pending_delegations_and_withdraws">process_pending_delegations_and_withdraws</a>(
    validators: &<b>mut</b> <a href="">vector</a>&lt;Validator&gt;, ctx: &<b>mut</b> TxContext
) {
    <b>let</b> length = <a href="_length">vector::length</a>(validators);
    <b>let</b> i = 0;
    <b>while</b> (i &lt; length) {
        <b>let</b> <a href="validator.md#0x2_validator">validator</a> = <a href="_borrow_mut">vector::borrow_mut</a>(validators, i);
        <a href="validator.md#0x2_validator_process_pending_delegations_and_withdraws">validator::process_pending_delegations_and_withdraws</a>(<a href="validator.md#0x2_validator">validator</a>, ctx);
        i = i + 1;
    }
}
</code></pre>



</details>

<a name="0x2_validator_set_calculate_total_stakes"></a>

## Function `calculate_total_stakes`

Calculate the total active validator and delegated stake.


<pre><code><b>fun</b> <a href="validator_set.md#0x2_validator_set_calculate_total_stakes">calculate_total_stakes</a>(validators: &<a href="">vector</a>&lt;<a href="validator.md#0x2_validator_Validator">validator::Validator</a>&gt;): (u64, u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="validator_set.md#0x2_validator_set_calculate_total_stakes">calculate_total_stakes</a>(validators: &<a href="">vector</a>&lt;Validator&gt;): (u64, u64) {
    <b>let</b> validator_state = 0;
    <b>let</b> delegate_stake = 0;
    <b>let</b> length = <a href="_length">vector::length</a>(validators);
    <b>let</b> i = 0;
    <b>while</b> (i &lt; length) {
        <b>let</b> v = <a href="_borrow">vector::borrow</a>(validators, i);
        validator_state = validator_state + <a href="validator.md#0x2_validator_stake_amount">validator::stake_amount</a>(v);
        delegate_stake = delegate_stake + <a href="validator.md#0x2_validator_delegate_amount">validator::delegate_amount</a>(v);
        i = i + 1;
    };
    (validator_state, delegate_stake)
}
</code></pre>



</details>

<a name="0x2_validator_set_adjust_stake_and_gas_price"></a>

## Function `adjust_stake_and_gas_price`

Process the pending stake changes for each validator.


<pre><code><b>fun</b> <a href="validator_set.md#0x2_validator_set_adjust_stake_and_gas_price">adjust_stake_and_gas_price</a>(validators: &<b>mut</b> <a href="">vector</a>&lt;<a href="validator.md#0x2_validator_Validator">validator::Validator</a>&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="validator_set.md#0x2_validator_set_adjust_stake_and_gas_price">adjust_stake_and_gas_price</a>(validators: &<b>mut</b> <a href="">vector</a>&lt;Validator&gt;) {
    <b>let</b> length = <a href="_length">vector::length</a>(validators);
    <b>let</b> i = 0;
    <b>while</b> (i &lt; length) {
        <b>let</b> <a href="validator.md#0x2_validator">validator</a> = <a href="_borrow_mut">vector::borrow_mut</a>(validators, i);
        <a href="validator.md#0x2_validator_adjust_stake_and_gas_price">validator::adjust_stake_and_gas_price</a>(<a href="validator.md#0x2_validator">validator</a>);
        i = i + 1;
    }
}
</code></pre>



</details>

<a name="0x2_validator_set_compute_reward_adjustments"></a>

## Function `compute_reward_adjustments`

Compute both the individual reward adjustments and total reward adjustment for staking rewards
as well as storage fund rewards.


<pre><code><b>fun</b> <a href="validator_set.md#0x2_validator_set_compute_reward_adjustments">compute_reward_adjustments</a>(slashed_validator_indices: <a href="">vector</a>&lt;u64&gt;, reward_slashing_rate: u64, unadjusted_staking_reward_amounts: &<a href="">vector</a>&lt;u64&gt;, unadjusted_storage_fund_reward_amounts: &<a href="">vector</a>&lt;u64&gt;): (u64, <a href="vec_map.md#0x2_vec_map_VecMap">vec_map::VecMap</a>&lt;u64, u64&gt;, u64, <a href="vec_map.md#0x2_vec_map_VecMap">vec_map::VecMap</a>&lt;u64, u64&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="validator_set.md#0x2_validator_set_compute_reward_adjustments">compute_reward_adjustments</a>(
    slashed_validator_indices: <a href="">vector</a>&lt;u64&gt;,
    reward_slashing_rate: u64,
    unadjusted_staking_reward_amounts: &<a href="">vector</a>&lt;u64&gt;,
    unadjusted_storage_fund_reward_amounts: &<a href="">vector</a>&lt;u64&gt;,
): (
    u64, // sum of staking reward adjustments
    VecMap&lt;u64, u64&gt;, // mapping of individual <a href="validator.md#0x2_validator">validator</a>'s staking reward adjustment from index -&gt; amount
    u64, // sum of storage fund reward adjustments
    VecMap&lt;u64, u64&gt;, // mapping of individual <a href="validator.md#0x2_validator">validator</a>'s storage fund reward adjustment from index -&gt; amount
) {
    <b>let</b> total_staking_reward_adjustment = 0;
    <b>let</b> individual_staking_reward_adjustments = <a href="vec_map.md#0x2_vec_map_empty">vec_map::empty</a>();
    <b>let</b> total_storage_fund_reward_adjustment = 0;
    <b>let</b> individual_storage_fund_reward_adjustments = <a href="vec_map.md#0x2_vec_map_empty">vec_map::empty</a>();

    <b>while</b> (!<a href="_is_empty">vector::is_empty</a>(&<b>mut</b> slashed_validator_indices)) {
        <b>let</b> validator_index = <a href="_pop_back">vector::pop_back</a>(&<b>mut</b> slashed_validator_indices);

        // Use the slashing rate <b>to</b> compute the amount of staking rewards slashed from this punished <a href="validator.md#0x2_validator">validator</a>.
        <b>let</b> unadjusted_staking_reward = *<a href="_borrow">vector::borrow</a>(unadjusted_staking_reward_amounts, validator_index);
        <b>let</b> staking_reward_adjustment_u128 =
            (unadjusted_staking_reward <b>as</b> u128) * (reward_slashing_rate <b>as</b> u128)
            / <a href="validator_set.md#0x2_validator_set_BASIS_POINT_DENOMINATOR">BASIS_POINT_DENOMINATOR</a>;

        // Insert into individual mapping and record into the total adjustment sum.
        <a href="vec_map.md#0x2_vec_map_insert">vec_map::insert</a>(&<b>mut</b> individual_staking_reward_adjustments, validator_index, (staking_reward_adjustment_u128 <b>as</b> u64));
        total_staking_reward_adjustment = total_staking_reward_adjustment + (staking_reward_adjustment_u128 <b>as</b> u64);

        // Do the same thing for storage fund rewards.
        <b>let</b> unadjusted_storage_fund_reward = *<a href="_borrow">vector::borrow</a>(unadjusted_storage_fund_reward_amounts, validator_index);
        <b>let</b> storage_fund_reward_adjustment_u128 =
            (unadjusted_storage_fund_reward <b>as</b> u128) * (reward_slashing_rate <b>as</b> u128)
            / <a href="validator_set.md#0x2_validator_set_BASIS_POINT_DENOMINATOR">BASIS_POINT_DENOMINATOR</a>;
        <a href="vec_map.md#0x2_vec_map_insert">vec_map::insert</a>(&<b>mut</b> individual_storage_fund_reward_adjustments, validator_index, (storage_fund_reward_adjustment_u128 <b>as</b> u64));
        total_storage_fund_reward_adjustment = total_storage_fund_reward_adjustment + (storage_fund_reward_adjustment_u128 <b>as</b> u64);
    };

    (
        total_staking_reward_adjustment, individual_staking_reward_adjustments,
        total_storage_fund_reward_adjustment, individual_storage_fund_reward_adjustments
    )
}
</code></pre>



</details>

<a name="0x2_validator_set_compute_slashed_validators_and_total_stake"></a>

## Function `compute_slashed_validators_and_total_stake`

Process the validator report records of the epoch and return the addresses of the
non-performant validators according to the input threshold.


<pre><code><b>fun</b> <a href="validator_set.md#0x2_validator_set_compute_slashed_validators_and_total_stake">compute_slashed_validators_and_total_stake</a>(self: &<a href="validator_set.md#0x2_validator_set_ValidatorSet">validator_set::ValidatorSet</a>, validator_report_records: <a href="vec_map.md#0x2_vec_map_VecMap">vec_map::VecMap</a>&lt;<b>address</b>, <a href="vec_set.md#0x2_vec_set_VecSet">vec_set::VecSet</a>&lt;<b>address</b>&gt;&gt;): (<a href="">vector</a>&lt;<b>address</b>&gt;, u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="validator_set.md#0x2_validator_set_compute_slashed_validators_and_total_stake">compute_slashed_validators_and_total_stake</a>(
    self: &<a href="validator_set.md#0x2_validator_set_ValidatorSet">ValidatorSet</a>,
    validator_report_records: VecMap&lt;<b>address</b>, VecSet&lt;<b>address</b>&gt;&gt;,
): (<a href="">vector</a>&lt;<b>address</b>&gt;, u64) {
    <b>let</b> slashed_validators = <a href="">vector</a>[];
    <b>let</b> sum_of_stake = 0;
    <b>while</b> (!<a href="vec_map.md#0x2_vec_map_is_empty">vec_map::is_empty</a>(&validator_report_records)) {
        <b>let</b> (validator_address, reporters) = <a href="vec_map.md#0x2_vec_map_pop">vec_map::pop</a>(&<b>mut</b> validator_report_records);
        <b>assert</b>!(
            <a href="validator_set.md#0x2_validator_set_is_active_validator">is_active_validator</a>(self, validator_address),
            <a href="validator_set.md#0x2_validator_set_ENonValidatorInReportRecords">ENonValidatorInReportRecords</a>,
        );
        // Sum up the voting power of validators that have reported this <a href="validator.md#0x2_validator">validator</a> and check <b>if</b> it <b>has</b>
        // passed the slashing threshold.
        <b>let</b> reporter_votes = <a href="validator_set.md#0x2_validator_set_sum_voting_power_by_addresses">sum_voting_power_by_addresses</a>(&self.active_validators, &<a href="vec_set.md#0x2_vec_set_into_keys">vec_set::into_keys</a>(reporters));
        <b>if</b> (reporter_votes &gt;= <a href="voting_power.md#0x2_voting_power_quorum_threshold">voting_power::quorum_threshold</a>()) {
            sum_of_stake = sum_of_stake + <a href="validator_set.md#0x2_validator_set_validator_total_stake_amount">validator_total_stake_amount</a>(self, validator_address);
            <a href="_push_back">vector::push_back</a>(&<b>mut</b> slashed_validators, validator_address);
        }
    };
    (slashed_validators, sum_of_stake)
}
</code></pre>



</details>

<a name="0x2_validator_set_compute_unadjusted_reward_distribution"></a>

## Function `compute_unadjusted_reward_distribution`

Given the current list of active validators, the total stake and total reward,
calculate the amount of reward each validator should get, without taking into
account the tallyig rule results.
Returns the unadjusted amounts of staking reward and storage fund reward for each validator.


<pre><code><b>fun</b> <a href="validator_set.md#0x2_validator_set_compute_unadjusted_reward_distribution">compute_unadjusted_reward_distribution</a>(validators: &<a href="">vector</a>&lt;<a href="validator.md#0x2_validator_Validator">validator::Validator</a>&gt;, total_stake: u64, total_staking_reward: u64, total_storage_fund_reward: u64): (<a href="">vector</a>&lt;u64&gt;, <a href="">vector</a>&lt;u64&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="validator_set.md#0x2_validator_set_compute_unadjusted_reward_distribution">compute_unadjusted_reward_distribution</a>(
    validators: &<a href="">vector</a>&lt;Validator&gt;,
    total_stake: u64,
    total_staking_reward: u64,
    total_storage_fund_reward: u64,
): (<a href="">vector</a>&lt;u64&gt;, <a href="">vector</a>&lt;u64&gt;) {
    <b>let</b> staking_reward_amounts = <a href="_empty">vector::empty</a>();
    <b>let</b> storage_fund_reward_amounts = <a href="_empty">vector::empty</a>();
    <b>let</b> length = <a href="_length">vector::length</a>(validators);
    <b>let</b> storage_fund_reward_per_validator = total_storage_fund_reward / length;
    <b>let</b> i = 0;
    <b>while</b> (i &lt; length) {
        <b>let</b> <a href="validator.md#0x2_validator">validator</a> = <a href="_borrow">vector::borrow</a>(validators, i);
        // Integer divisions will truncate the results. Because of this, we expect that at the end
        // there will be some reward remaining in `total_staking_reward`.
        // Use u128 <b>to</b> avoid multiplication overflow.
        <b>let</b> stake_amount: u128 = (<a href="validator.md#0x2_validator_total_stake_amount">validator::total_stake_amount</a>(<a href="validator.md#0x2_validator">validator</a>) <b>as</b> u128);
        <b>let</b> reward_amount = stake_amount * (total_staking_reward <b>as</b> u128) / (total_stake <b>as</b> u128);
        <a href="_push_back">vector::push_back</a>(&<b>mut</b> staking_reward_amounts, (reward_amount <b>as</b> u64));
        // Storage fund's share of the rewards are equally distributed among validators.
        <a href="_push_back">vector::push_back</a>(&<b>mut</b> storage_fund_reward_amounts, storage_fund_reward_per_validator);
        i = i + 1;
    };
    (staking_reward_amounts, storage_fund_reward_amounts)
}
</code></pre>



</details>

<a name="0x2_validator_set_compute_adjusted_reward_distribution"></a>

## Function `compute_adjusted_reward_distribution`

Use the reward adjustment info to compute the adjusted rewards each validator should get.
Returns the staking rewards each validator gets and the storage fund rewards each validator gets.
The staking rewards are shared with the delegators while the storage fund ones are not.


<pre><code><b>fun</b> <a href="validator_set.md#0x2_validator_set_compute_adjusted_reward_distribution">compute_adjusted_reward_distribution</a>(validators: &<a href="">vector</a>&lt;<a href="validator.md#0x2_validator_Validator">validator::Validator</a>&gt;, total_stake: u64, total_slashed_validator_stake: u64, unadjusted_staking_reward_amounts: <a href="">vector</a>&lt;u64&gt;, unadjusted_storage_fund_reward_amounts: <a href="">vector</a>&lt;u64&gt;, total_staking_reward_adjustment: u64, individual_staking_reward_adjustments: <a href="vec_map.md#0x2_vec_map_VecMap">vec_map::VecMap</a>&lt;u64, u64&gt;, total_storage_fund_reward_adjustment: u64, individual_storage_fund_reward_adjustments: <a href="vec_map.md#0x2_vec_map_VecMap">vec_map::VecMap</a>&lt;u64, u64&gt;): (<a href="">vector</a>&lt;u64&gt;, <a href="">vector</a>&lt;u64&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="validator_set.md#0x2_validator_set_compute_adjusted_reward_distribution">compute_adjusted_reward_distribution</a>(
    validators: &<a href="">vector</a>&lt;Validator&gt;,
    total_stake: u64,
    total_slashed_validator_stake: u64,
    unadjusted_staking_reward_amounts: <a href="">vector</a>&lt;u64&gt;,
    unadjusted_storage_fund_reward_amounts: <a href="">vector</a>&lt;u64&gt;,
    total_staking_reward_adjustment: u64,
    individual_staking_reward_adjustments: VecMap&lt;u64, u64&gt;,
    total_storage_fund_reward_adjustment: u64,
    individual_storage_fund_reward_adjustments: VecMap&lt;u64, u64&gt;,
): (<a href="">vector</a>&lt;u64&gt;, <a href="">vector</a>&lt;u64&gt;) {
    <b>let</b> total_unslashed_validator_stake = total_stake - total_slashed_validator_stake;
    <b>let</b> adjusted_staking_reward_amounts = <a href="_empty">vector::empty</a>();
    <b>let</b> adjusted_storage_fund_reward_amounts = <a href="_empty">vector::empty</a>();

    <b>let</b> length = <a href="_length">vector::length</a>(validators);
    <b>let</b> num_unslashed_validators = length - <a href="vec_map.md#0x2_vec_map_size">vec_map::size</a>(&individual_staking_reward_adjustments);

    <b>let</b> i = 0;
    <b>while</b> (i &lt; length) {
        <b>let</b> <a href="validator.md#0x2_validator">validator</a> = <a href="_borrow">vector::borrow</a>(validators, i);
        // Integer divisions will truncate the results. Because of this, we expect that at the end
        // there will be some reward remaining in `total_reward`.
        // Use u128 <b>to</b> avoid multiplication overflow.
        <b>let</b> stake_amount: u128 = (<a href="validator.md#0x2_validator_total_stake_amount">validator::total_stake_amount</a>(<a href="validator.md#0x2_validator">validator</a>) <b>as</b> u128);

        // Compute adjusted staking reward.
        <b>let</b> unadjusted_staking_reward_amount = *<a href="_borrow">vector::borrow</a>(&unadjusted_staking_reward_amounts, i);
        <b>let</b> adjusted_staking_reward_amount =
            // If the <a href="validator.md#0x2_validator">validator</a> is one of the slashed ones, then subtract the adjustment.
            <b>if</b> (<a href="vec_map.md#0x2_vec_map_contains">vec_map::contains</a>(&individual_staking_reward_adjustments, &i)) {
                <b>let</b> adjustment = *<a href="vec_map.md#0x2_vec_map_get">vec_map::get</a>(&individual_staking_reward_adjustments, &i);
                unadjusted_staking_reward_amount - adjustment
            } <b>else</b> {
                // Otherwise the slashed rewards should be distributed among the unslashed
                // validators so add the corresponding adjustment.
                <b>let</b> adjustment = (total_staking_reward_adjustment <b>as</b> u128) * stake_amount
                               / (total_unslashed_validator_stake <b>as</b> u128);
                unadjusted_staking_reward_amount + (adjustment <b>as</b> u64)
            };
        <a href="_push_back">vector::push_back</a>(&<b>mut</b> adjusted_staking_reward_amounts, adjusted_staking_reward_amount);

        // Compute adjusted storage fund reward.
        <b>let</b> unadjusted_storage_fund_reward_amount = *<a href="_borrow">vector::borrow</a>(&unadjusted_storage_fund_reward_amounts, i);
        <b>let</b> adjusted_storage_fund_reward_amount =
            // If the <a href="validator.md#0x2_validator">validator</a> is one of the slashed ones, then subtract the adjustment.
            <b>if</b> (<a href="vec_map.md#0x2_vec_map_contains">vec_map::contains</a>(&individual_storage_fund_reward_adjustments, &i)) {
                <b>let</b> adjustment = *<a href="vec_map.md#0x2_vec_map_get">vec_map::get</a>(&individual_storage_fund_reward_adjustments, &i);
                unadjusted_storage_fund_reward_amount - adjustment
            } <b>else</b> {
                // Otherwise the slashed rewards should be equally distributed among the unslashed validators.
                <b>let</b> adjustment = total_storage_fund_reward_adjustment / num_unslashed_validators;
                unadjusted_storage_fund_reward_amount + adjustment
            };
        <a href="_push_back">vector::push_back</a>(&<b>mut</b> adjusted_storage_fund_reward_amounts, adjusted_storage_fund_reward_amount);

        i = i + 1;
    };

    (adjusted_staking_reward_amounts, adjusted_storage_fund_reward_amounts)
}
</code></pre>



</details>

<a name="0x2_validator_set_distribute_reward"></a>

## Function `distribute_reward`



<pre><code><b>fun</b> <a href="validator_set.md#0x2_validator_set_distribute_reward">distribute_reward</a>(validators: &<b>mut</b> <a href="">vector</a>&lt;<a href="validator.md#0x2_validator_Validator">validator::Validator</a>&gt;, adjusted_staking_reward_amounts: &<a href="">vector</a>&lt;u64&gt;, adjusted_storage_fund_reward_amounts: &<a href="">vector</a>&lt;u64&gt;, staking_rewards: &<b>mut</b> <a href="balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="sui.md#0x2_sui_SUI">sui::SUI</a>&gt;, storage_fund_reward: &<b>mut</b> <a href="balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="sui.md#0x2_sui_SUI">sui::SUI</a>&gt;, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="validator_set.md#0x2_validator_set_distribute_reward">distribute_reward</a>(
    validators: &<b>mut</b> <a href="">vector</a>&lt;Validator&gt;,
    adjusted_staking_reward_amounts: &<a href="">vector</a>&lt;u64&gt;,
    adjusted_storage_fund_reward_amounts: &<a href="">vector</a>&lt;u64&gt;,
    staking_rewards: &<b>mut</b> Balance&lt;SUI&gt;,
    storage_fund_reward: &<b>mut</b> Balance&lt;SUI&gt;,
    ctx: &<b>mut</b> TxContext
) {
    <b>let</b> new_epoch = <a href="tx_context.md#0x2_tx_context_epoch">tx_context::epoch</a>(ctx) + 1;
    <b>let</b> length = <a href="_length">vector::length</a>(validators);
    <b>assert</b>!(length &gt; 0, 0);
    <b>let</b> i = 0;
    <b>while</b> (i &lt; length) {
        <b>let</b> <a href="validator.md#0x2_validator">validator</a> = <a href="_borrow_mut">vector::borrow_mut</a>(validators, i);
        <b>let</b> staking_reward_amount = *<a href="_borrow">vector::borrow</a>(adjusted_staking_reward_amounts, i);
        <b>let</b> combined_stake = <a href="validator.md#0x2_validator_total_stake_amount">validator::total_stake_amount</a>(<a href="validator.md#0x2_validator">validator</a>);
        <b>let</b> self_stake = <a href="validator.md#0x2_validator_stake_amount">validator::stake_amount</a>(<a href="validator.md#0x2_validator">validator</a>);
        <b>let</b> validator_reward_amount = (staking_reward_amount <b>as</b> u128) * (self_stake <b>as</b> u128) / (combined_stake <b>as</b> u128);
        <b>let</b> validator_reward = <a href="balance.md#0x2_balance_split">balance::split</a>(staking_rewards, (validator_reward_amount <b>as</b> u64));

        <b>let</b> delegator_reward_amount = staking_reward_amount - (validator_reward_amount <b>as</b> u64);
        <b>let</b> delegator_reward = <a href="balance.md#0x2_balance_split">balance::split</a>(staking_rewards, delegator_reward_amount);

        // Validator takes a cut of the rewards <b>as</b> commission.
        <b>let</b> commission_amount = (delegator_reward_amount <b>as</b> u128) * (<a href="validator.md#0x2_validator_commission_rate">validator::commission_rate</a>(<a href="validator.md#0x2_validator">validator</a>) <b>as</b> u128) / <a href="validator_set.md#0x2_validator_set_BASIS_POINT_DENOMINATOR">BASIS_POINT_DENOMINATOR</a>;
        <a href="balance.md#0x2_balance_join">balance::join</a>(&<b>mut</b> validator_reward, <a href="balance.md#0x2_balance_split">balance::split</a>(&<b>mut</b> delegator_reward, (commission_amount <b>as</b> u64)));

        // Add storage fund rewards <b>to</b> the <a href="validator.md#0x2_validator">validator</a>'s reward.
        <a href="balance.md#0x2_balance_join">balance::join</a>(&<b>mut</b> validator_reward, <a href="balance.md#0x2_balance_split">balance::split</a>(storage_fund_reward, *<a href="_borrow">vector::borrow</a>(adjusted_storage_fund_reward_amounts, i)));

        // Add rewards <b>to</b> the <a href="validator.md#0x2_validator">validator</a>.
        <a href="validator.md#0x2_validator_request_add_stake">validator::request_add_stake</a>(<a href="validator.md#0x2_validator">validator</a>, validator_reward, <a href="_none">option::none</a>(), ctx);
        // Add rewards <b>to</b> delegation staking pool <b>to</b> auto compound for delegators.
        <a href="validator.md#0x2_validator_deposit_delegation_rewards">validator::deposit_delegation_rewards</a>(<a href="validator.md#0x2_validator">validator</a>, delegator_reward, new_epoch);
        i = i + 1;
    }
}
</code></pre>



</details>

<a name="0x2_validator_set_derive_next_epoch_validators"></a>

## Function `derive_next_epoch_validators`

Upon any change to the validator set, derive and update the metadata of the validators for the new epoch.
TODO: If we want to enforce a % on stake threshold, this is the function to do it.


<pre><code><b>fun</b> <a href="validator_set.md#0x2_validator_set_derive_next_epoch_validators">derive_next_epoch_validators</a>(self: &<a href="validator_set.md#0x2_validator_set_ValidatorSet">validator_set::ValidatorSet</a>): <a href="">vector</a>&lt;<a href="validator.md#0x2_validator_ValidatorMetadata">validator::ValidatorMetadata</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="validator_set.md#0x2_validator_set_derive_next_epoch_validators">derive_next_epoch_validators</a>(self: &<a href="validator_set.md#0x2_validator_set_ValidatorSet">ValidatorSet</a>): <a href="">vector</a>&lt;ValidatorMetadata&gt; {
    <b>let</b> active_count = <a href="_length">vector::length</a>(&self.active_validators);
    <b>let</b> removal_count = <a href="_length">vector::length</a>(&self.pending_removals);
    <b>let</b> result = <a href="_empty">vector::empty</a>();
    <b>while</b> (active_count &gt; 0) {
        <b>if</b> (removal_count &gt; 0) {
            <b>let</b> removal_index = *<a href="_borrow">vector::borrow</a>(&self.pending_removals, removal_count - 1);
            <b>if</b> (removal_index == active_count - 1) {
                // This <a href="validator.md#0x2_validator">validator</a> will be removed, and hence we won't add it <b>to</b> the new <a href="validator.md#0x2_validator">validator</a> set.
                removal_count = removal_count - 1;
                active_count = active_count - 1;
                <b>continue</b>
            };
        };
        <b>let</b> metadata = <a href="validator.md#0x2_validator_metadata">validator::metadata</a>(
            <a href="_borrow">vector::borrow</a>(&self.active_validators, active_count - 1),
        );
        <a href="_push_back">vector::push_back</a>(&<b>mut</b> result, *metadata);
        active_count = active_count - 1;
    };
    <b>let</b> i = 0;
    <b>let</b> pending_count = <a href="_length">vector::length</a>(&self.pending_validators);
    <b>while</b> (i &lt; pending_count) {
        <b>let</b> metadata = <a href="validator.md#0x2_validator_metadata">validator::metadata</a>(
            <a href="_borrow">vector::borrow</a>(&self.pending_validators, i),
        );
        <a href="_push_back">vector::push_back</a>(&<b>mut</b> result, *metadata);
        i = i + 1;
    };
    result
}
</code></pre>



</details>

<a name="0x2_validator_set_emit_validator_epoch_events"></a>

## Function `emit_validator_epoch_events`

Emit events containing information of each validator for the epoch,
including stakes, rewards, performance, etc.


<pre><code><b>fun</b> <a href="validator_set.md#0x2_validator_set_emit_validator_epoch_events">emit_validator_epoch_events</a>(new_epoch: u64, vs: &<a href="">vector</a>&lt;<a href="validator.md#0x2_validator_Validator">validator::Validator</a>&gt;, reward_amounts: &<a href="">vector</a>&lt;u64&gt;, report_records: &<a href="vec_map.md#0x2_vec_map_VecMap">vec_map::VecMap</a>&lt;<b>address</b>, <a href="vec_set.md#0x2_vec_set_VecSet">vec_set::VecSet</a>&lt;<b>address</b>&gt;&gt;, slashed_validators: &<a href="">vector</a>&lt;<b>address</b>&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="validator_set.md#0x2_validator_set_emit_validator_epoch_events">emit_validator_epoch_events</a>(
    new_epoch: u64,
    vs: &<a href="">vector</a>&lt;Validator&gt;,
    reward_amounts: &<a href="">vector</a>&lt;u64&gt;,
    report_records: &VecMap&lt;<b>address</b>, VecSet&lt;<b>address</b>&gt;&gt;,
    slashed_validators: &<a href="">vector</a>&lt;<b>address</b>&gt;,
) {
    <b>let</b> num_validators = <a href="_length">vector::length</a>(vs);
    <b>let</b> i = 0;
    <b>while</b> (i &lt; num_validators) {
        <b>let</b> v = <a href="_borrow">vector::borrow</a>(vs, i);
        <b>let</b> validator_address = <a href="validator.md#0x2_validator_sui_address">validator::sui_address</a>(v);
        <b>let</b> tallying_rule_reporters =
            <b>if</b> (<a href="vec_map.md#0x2_vec_map_contains">vec_map::contains</a>(report_records, &validator_address)) {
                <a href="vec_set.md#0x2_vec_set_into_keys">vec_set::into_keys</a>(*<a href="vec_map.md#0x2_vec_map_get">vec_map::get</a>(report_records, &validator_address))
            } <b>else</b> {
                <a href="">vector</a>[]
            };
        <b>let</b> tallying_rule_global_score =
            <b>if</b> (<a href="_contains">vector::contains</a>(slashed_validators, &validator_address)) 0
            <b>else</b> 1;
        <a href="event.md#0x2_event_emit">event::emit</a>(
            <a href="validator_set.md#0x2_validator_set_ValidatorEpochInfo">ValidatorEpochInfo</a> {
                epoch: new_epoch,
                validator_address,
                reference_gas_survey_quote: <a href="validator.md#0x2_validator_gas_price">validator::gas_price</a>(v),
                validator_stake: <a href="validator.md#0x2_validator_stake_amount">validator::stake_amount</a>(v),
                delegated_stake: <a href="validator.md#0x2_validator_delegate_amount">validator::delegate_amount</a>(v),
                commission_rate: <a href="validator.md#0x2_validator_commission_rate">validator::commission_rate</a>(v),
                stake_rewards: *<a href="_borrow">vector::borrow</a>(reward_amounts, i),
                pool_token_exchange_rate: <a href="validator.md#0x2_validator_pool_token_exchange_rate_at_epoch">validator::pool_token_exchange_rate_at_epoch</a>(v, new_epoch),
                tallying_rule_reporters,
                tallying_rule_global_score,
            }
        );
        i = i + 1;
    }
}
</code></pre>



</details>

<a name="0x2_validator_set_sum_voting_power_by_addresses"></a>

## Function `sum_voting_power_by_addresses`

Sum up the total stake of a given list of validator addresses.


<pre><code><b>public</b> <b>fun</b> <a href="validator_set.md#0x2_validator_set_sum_voting_power_by_addresses">sum_voting_power_by_addresses</a>(vs: &<a href="">vector</a>&lt;<a href="validator.md#0x2_validator_Validator">validator::Validator</a>&gt;, addresses: &<a href="">vector</a>&lt;<b>address</b>&gt;): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="validator_set.md#0x2_validator_set_sum_voting_power_by_addresses">sum_voting_power_by_addresses</a>(vs: &<a href="">vector</a>&lt;Validator&gt;, addresses: &<a href="">vector</a>&lt;<b>address</b>&gt;): u64 {
    <b>let</b> sum = 0;
    <b>let</b> i = 0;
    <b>let</b> length = <a href="_length">vector::length</a>(addresses);
    <b>while</b> (i &lt; length) {
        <b>let</b> <a href="validator.md#0x2_validator">validator</a> = <a href="validator_set.md#0x2_validator_set_get_validator_ref">get_validator_ref</a>(vs, *<a href="_borrow">vector::borrow</a>(addresses, i));
        sum = sum + <a href="validator.md#0x2_validator_voting_power">validator::voting_power</a>(<a href="validator.md#0x2_validator">validator</a>);
        i = i + 1;
    };
    sum
}
</code></pre>



</details>

<a name="0x2_validator_set_active_validators"></a>

## Function `active_validators`

Return the active validators in <code>self</code>


<pre><code><b>public</b> <b>fun</b> <a href="validator_set.md#0x2_validator_set_active_validators">active_validators</a>(self: &<a href="validator_set.md#0x2_validator_set_ValidatorSet">validator_set::ValidatorSet</a>): &<a href="">vector</a>&lt;<a href="validator.md#0x2_validator_Validator">validator::Validator</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="validator_set.md#0x2_validator_set_active_validators">active_validators</a>(self: &<a href="validator_set.md#0x2_validator_set_ValidatorSet">ValidatorSet</a>): &<a href="">vector</a>&lt;Validator&gt; {
    &self.active_validators
}
</code></pre>



</details>
