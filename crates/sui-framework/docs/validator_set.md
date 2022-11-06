
<a name="0x2_validator_set"></a>

# Module `0x2::validator_set`



-  [Struct `ValidatorSet`](#0x2_validator_set_ValidatorSet)
-  [Struct `ValidatorPair`](#0x2_validator_set_ValidatorPair)
-  [Constants](#@Constants_0)
-  [Function `new`](#0x2_validator_set_new)
-  [Function `next_epoch_validator_count`](#0x2_validator_set_next_epoch_validator_count)
-  [Function `request_add_validator`](#0x2_validator_set_request_add_validator)
-  [Function `request_remove_validator`](#0x2_validator_set_request_remove_validator)
-  [Function `request_add_stake`](#0x2_validator_set_request_add_stake)
-  [Function `request_withdraw_stake`](#0x2_validator_set_request_withdraw_stake)
-  [Function `is_active_validator`](#0x2_validator_set_is_active_validator)
-  [Function `request_add_delegation`](#0x2_validator_set_request_add_delegation)
-  [Function `request_set_gas_price`](#0x2_validator_set_request_set_gas_price)
-  [Function `request_set_commission_rate`](#0x2_validator_set_request_set_commission_rate)
-  [Function `request_withdraw_delegation`](#0x2_validator_set_request_withdraw_delegation)
-  [Function `request_switch_delegation`](#0x2_validator_set_request_switch_delegation)
-  [Function `process_delegation_switches`](#0x2_validator_set_process_delegation_switches)
-  [Function `process_pending_delegations`](#0x2_validator_set_process_pending_delegations)
-  [Function `advance_epoch`](#0x2_validator_set_advance_epoch)
-  [Function `derive_reference_gas_price`](#0x2_validator_set_derive_reference_gas_price)
-  [Function `total_validator_stake`](#0x2_validator_set_total_validator_stake)
-  [Function `total_delegation_stake`](#0x2_validator_set_total_delegation_stake)
-  [Function `validator_stake_amount`](#0x2_validator_set_validator_stake_amount)
-  [Function `validator_delegate_amount`](#0x2_validator_set_validator_delegate_amount)
-  [Function `contains_duplicate_validator`](#0x2_validator_set_contains_duplicate_validator)
-  [Function `find_validator`](#0x2_validator_set_find_validator)
-  [Function `get_validator_mut`](#0x2_validator_set_get_validator_mut)
-  [Function `get_validator_ref`](#0x2_validator_set_get_validator_ref)
-  [Function `process_pending_removals`](#0x2_validator_set_process_pending_removals)
-  [Function `process_pending_validators`](#0x2_validator_set_process_pending_validators)
-  [Function `sort_removal_list`](#0x2_validator_set_sort_removal_list)
-  [Function `calculate_total_stake_and_quorum_threshold`](#0x2_validator_set_calculate_total_stake_and_quorum_threshold)
-  [Function `calculate_quorum_threshold`](#0x2_validator_set_calculate_quorum_threshold)
-  [Function `adjust_stake_and_gas_price`](#0x2_validator_set_adjust_stake_and_gas_price)
-  [Function `compute_reward_distribution`](#0x2_validator_set_compute_reward_distribution)
-  [Function `distribute_reward`](#0x2_validator_set_distribute_reward)
-  [Function `derive_next_epoch_validators`](#0x2_validator_set_derive_next_epoch_validators)


<pre><code><b>use</b> <a href="">0x1::option</a>;
<b>use</b> <a href="">0x1::vector</a>;
<b>use</b> <a href="balance.md#0x2_balance">0x2::balance</a>;
<b>use</b> <a href="epoch_time_lock.md#0x2_epoch_time_lock">0x2::epoch_time_lock</a>;
<b>use</b> <a href="priority_queue.md#0x2_priority_queue">0x2::priority_queue</a>;
<b>use</b> <a href="stake.md#0x2_stake">0x2::stake</a>;
<b>use</b> <a href="staking_pool.md#0x2_staking_pool">0x2::staking_pool</a>;
<b>use</b> <a href="sui.md#0x2_sui">0x2::sui</a>;
<b>use</b> <a href="tx_context.md#0x2_tx_context">0x2::tx_context</a>;
<b>use</b> <a href="validator.md#0x2_validator">0x2::validator</a>;
<b>use</b> <a href="vec_map.md#0x2_vec_map">0x2::vec_map</a>;
<b>use</b> <a href="vec_set.md#0x2_vec_set">0x2::vec_set</a>;
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
<code>quorum_stake_threshold: u64</code>
</dt>
<dd>
 The amount of accumulated stake to reach a quorum among all active validators.
 This is always 2/3 of total stake. Keep it here to reduce potential inconsistencies
 among validators.
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
 Everytime a change request is received, this set is updated.
</dd>
<dt>
<code>pending_delegation_switches: <a href="vec_map.md#0x2_vec_map_VecMap">vec_map::VecMap</a>&lt;<a href="validator_set.md#0x2_validator_set_ValidatorPair">validator_set::ValidatorPair</a>, <a href="">vector</a>&lt;<a href="staking_pool.md#0x2_staking_pool_PendingWithdrawEntry">staking_pool::PendingWithdrawEntry</a>&gt;&gt;</code>
</dt>
<dd>
 Delegation switches requested during the current epoch, processed at epoch boundaries
 so that all the rewards with be added to the new delegation.
</dd>
</dl>


</details>

<a name="0x2_validator_set_ValidatorPair"></a>

## Struct `ValidatorPair`



<pre><code><b>struct</b> <a href="validator_set.md#0x2_validator_set_ValidatorPair">ValidatorPair</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>from: <b>address</b></code>
</dt>
<dd>

</dd>
<dt>
<code><b>to</b>: <b>address</b></code>
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



<a name="0x2_validator_set_new"></a>

## Function `new`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator_set.md#0x2_validator_set_new">new</a>(init_active_validators: <a href="">vector</a>&lt;<a href="validator.md#0x2_validator_Validator">validator::Validator</a>&gt;): <a href="validator_set.md#0x2_validator_set_ValidatorSet">validator_set::ValidatorSet</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator_set.md#0x2_validator_set_new">new</a>(init_active_validators: <a href="">vector</a>&lt;Validator&gt;): <a href="validator_set.md#0x2_validator_set_ValidatorSet">ValidatorSet</a> {
    <b>let</b> (total_validator_stake, total_delegation_stake, quorum_stake_threshold) = <a href="validator_set.md#0x2_validator_set_calculate_total_stake_and_quorum_threshold">calculate_total_stake_and_quorum_threshold</a>(&init_active_validators);
    <b>let</b> validators = <a href="validator_set.md#0x2_validator_set_ValidatorSet">ValidatorSet</a> {
        total_validator_stake,
        total_delegation_stake,
        quorum_stake_threshold,
        active_validators: init_active_validators,
        pending_validators: <a href="_empty">vector::empty</a>(),
        pending_removals: <a href="_empty">vector::empty</a>(),
        next_epoch_validators: <a href="_empty">vector::empty</a>(),
        pending_delegation_switches: <a href="vec_map.md#0x2_vec_map_empty">vec_map::empty</a>(),
    };
    validators.next_epoch_validators = <a href="validator_set.md#0x2_validator_set_derive_next_epoch_validators">derive_next_epoch_validators</a>(&validators);
    validators
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

<a name="0x2_validator_set_request_add_validator"></a>

## Function `request_add_validator`

Called by <code>SuiSystem</code>, add a new validator to <code>pending_validators</code>, which will be
processed at the end of epoch.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator_set.md#0x2_validator_set_request_add_validator">request_add_validator</a>(self: &<b>mut</b> <a href="validator_set.md#0x2_validator_set_ValidatorSet">validator_set::ValidatorSet</a>, <a href="validator.md#0x2_validator">validator</a>: <a href="validator.md#0x2_validator_Validator">validator::Validator</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator_set.md#0x2_validator_set_request_add_validator">request_add_validator</a>(self: &<b>mut</b> <a href="validator_set.md#0x2_validator_set_ValidatorSet">ValidatorSet</a>, <a href="validator.md#0x2_validator">validator</a>: Validator) {
    <b>assert</b>!(
        !<a href="validator_set.md#0x2_validator_set_contains_duplicate_validator">contains_duplicate_validator</a>(&self.active_validators, &<a href="validator.md#0x2_validator">validator</a>)
            && !<a href="validator_set.md#0x2_validator_set_contains_duplicate_validator">contains_duplicate_validator</a>(&self.pending_validators, &<a href="validator.md#0x2_validator">validator</a>),
        0
    );
    <a href="_push_back">vector::push_back</a>(&<b>mut</b> self.pending_validators, <a href="validator.md#0x2_validator">validator</a>);
    self.next_epoch_validators = <a href="validator_set.md#0x2_validator_set_derive_next_epoch_validators">derive_next_epoch_validators</a>(self);
}
</code></pre>



</details>

<a name="0x2_validator_set_request_remove_validator"></a>

## Function `request_remove_validator`

Called by <code>SuiSystem</code>, to remove a validator.
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

Called by <code>SuiSystem</code>, to add more stake to a validator.
The new stake will be added to the validator's pending stake, which will be processed
at the end of epoch.
The total stake of the validator cannot exceed <code>max_validator_stake</code> with the <code>new_stake</code>.


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

Called by <code>SuiSystem</code>, to withdraw stake from a validator.
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

<a name="0x2_validator_set_is_active_validator"></a>

## Function `is_active_validator`



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

<a name="0x2_validator_set_request_add_delegation"></a>

## Function `request_add_delegation`



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
    <a href="validator.md#0x2_validator_request_add_delegation">validator::request_add_delegation</a>(<a href="validator.md#0x2_validator">validator</a>, delegated_stake, locking_period, <a href="tx_context.md#0x2_tx_context_sender">tx_context::sender</a>(ctx), ctx);
    self.next_epoch_validators = <a href="validator_set.md#0x2_validator_set_derive_next_epoch_validators">derive_next_epoch_validators</a>(self);
}
</code></pre>



</details>

<a name="0x2_validator_set_request_set_gas_price"></a>

## Function `request_set_gas_price`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator_set.md#0x2_validator_set_request_set_gas_price">request_set_gas_price</a>(self: &<b>mut</b> <a href="validator_set.md#0x2_validator_set_ValidatorSet">validator_set::ValidatorSet</a>, new_gas_price: u64, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator_set.md#0x2_validator_set_request_set_gas_price">request_set_gas_price</a>(
    self: &<b>mut</b> <a href="validator_set.md#0x2_validator_set_ValidatorSet">ValidatorSet</a>,
    new_gas_price: u64,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> validator_address = <a href="tx_context.md#0x2_tx_context_sender">tx_context::sender</a>(ctx);
    <b>let</b> <a href="validator.md#0x2_validator">validator</a> = <a href="validator_set.md#0x2_validator_set_get_validator_mut">get_validator_mut</a>(&<b>mut</b> self.active_validators, validator_address);
    <a href="validator.md#0x2_validator_request_set_gas_price">validator::request_set_gas_price</a>(<a href="validator.md#0x2_validator">validator</a>, new_gas_price);
}
</code></pre>



</details>

<a name="0x2_validator_set_request_set_commission_rate"></a>

## Function `request_set_commission_rate`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator_set.md#0x2_validator_set_request_set_commission_rate">request_set_commission_rate</a>(self: &<b>mut</b> <a href="validator_set.md#0x2_validator_set_ValidatorSet">validator_set::ValidatorSet</a>, new_commission_rate: u64, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator_set.md#0x2_validator_set_request_set_commission_rate">request_set_commission_rate</a>(
    self: &<b>mut</b> <a href="validator_set.md#0x2_validator_set_ValidatorSet">ValidatorSet</a>,
    new_commission_rate: u64,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> validator_address = <a href="tx_context.md#0x2_tx_context_sender">tx_context::sender</a>(ctx);
    <b>let</b> <a href="validator.md#0x2_validator">validator</a> = <a href="validator_set.md#0x2_validator_set_get_validator_mut">get_validator_mut</a>(&<b>mut</b> self.active_validators, validator_address);
    <a href="validator.md#0x2_validator_request_set_commission_rate">validator::request_set_commission_rate</a>(<a href="validator.md#0x2_validator">validator</a>, new_commission_rate);
}
</code></pre>



</details>

<a name="0x2_validator_set_request_withdraw_delegation"></a>

## Function `request_withdraw_delegation`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator_set.md#0x2_validator_set_request_withdraw_delegation">request_withdraw_delegation</a>(self: &<b>mut</b> <a href="validator_set.md#0x2_validator_set_ValidatorSet">validator_set::ValidatorSet</a>, delegation: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_Delegation">staking_pool::Delegation</a>, staked_sui: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_StakedSui">staking_pool::StakedSui</a>, withdraw_pool_token_amount: u64, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator_set.md#0x2_validator_set_request_withdraw_delegation">request_withdraw_delegation</a>(
    self: &<b>mut</b> <a href="validator_set.md#0x2_validator_set_ValidatorSet">ValidatorSet</a>,
    delegation: &<b>mut</b> Delegation,
    staked_sui: &<b>mut</b> StakedSui,
    withdraw_pool_token_amount: u64,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> validator_address = <a href="staking_pool.md#0x2_staking_pool_validator_address">staking_pool::validator_address</a>(delegation);
    <b>let</b> validator_index_opt = <a href="validator_set.md#0x2_validator_set_find_validator">find_validator</a>(&self.active_validators, validator_address);

    <b>assert</b>!(<a href="_is_some">option::is_some</a>(&validator_index_opt), 0);

    <b>let</b> validator_index = <a href="_extract">option::extract</a>(&<b>mut</b> validator_index_opt);
    <b>let</b> <a href="validator.md#0x2_validator">validator</a> = <a href="_borrow_mut">vector::borrow_mut</a>(&<b>mut</b> self.active_validators, validator_index);
    <a href="validator.md#0x2_validator_request_withdraw_delegation">validator::request_withdraw_delegation</a>(<a href="validator.md#0x2_validator">validator</a>, delegation, staked_sui, withdraw_pool_token_amount, ctx);
    self.next_epoch_validators = <a href="validator_set.md#0x2_validator_set_derive_next_epoch_validators">derive_next_epoch_validators</a>(self);
}
</code></pre>



</details>

<a name="0x2_validator_set_request_switch_delegation"></a>

## Function `request_switch_delegation`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator_set.md#0x2_validator_set_request_switch_delegation">request_switch_delegation</a>(self: &<b>mut</b> <a href="validator_set.md#0x2_validator_set_ValidatorSet">validator_set::ValidatorSet</a>, delegation: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_Delegation">staking_pool::Delegation</a>, staked_sui: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_StakedSui">staking_pool::StakedSui</a>, new_validator_address: <b>address</b>, switch_pool_token_amount: u64, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator_set.md#0x2_validator_set_request_switch_delegation">request_switch_delegation</a>(
    self: &<b>mut</b> <a href="validator_set.md#0x2_validator_set_ValidatorSet">ValidatorSet</a>,
    delegation: &<b>mut</b> Delegation,
    staked_sui: &<b>mut</b> StakedSui,
    new_validator_address: <b>address</b>,
    switch_pool_token_amount: u64,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> current_validator_address = <a href="staking_pool.md#0x2_staking_pool_validator_address">staking_pool::validator_address</a>(delegation);

    // check that the validators are not the same and they are both active.
    <b>assert</b>!(current_validator_address != new_validator_address, 0);
    <b>assert</b>!(<a href="validator_set.md#0x2_validator_set_is_active_validator">is_active_validator</a>(self, new_validator_address), 0);
    <b>let</b> current_validator_index_opt = <a href="validator_set.md#0x2_validator_set_find_validator">find_validator</a>(&self.active_validators, current_validator_address);
    <b>assert</b>!(<a href="_is_some">option::is_some</a>(&current_validator_index_opt), 0);

    // withdraw principal from the current <a href="validator.md#0x2_validator">validator</a>'s pool
    <b>let</b> current_validator_index = <a href="_extract">option::extract</a>(&<b>mut</b> current_validator_index_opt);
    <b>let</b> current_validator = <a href="_borrow_mut">vector::borrow_mut</a>(&<b>mut</b> self.active_validators, current_validator_index);
    <b>let</b> (current_validator_pool_token, principal_stake, time_lock) =
        <a href="staking_pool.md#0x2_staking_pool_withdraw_principal">staking_pool::withdraw_principal</a>(<a href="validator.md#0x2_validator_get_staking_pool_mut_ref">validator::get_staking_pool_mut_ref</a>(current_validator), delegation, staked_sui, switch_pool_token_amount);
    <b>let</b> principal_sui_amount = <a href="balance.md#0x2_balance_value">balance::value</a>(&principal_stake);
    <a href="validator.md#0x2_validator_decrease_next_epoch_delegation">validator::decrease_next_epoch_delegation</a>(current_validator, principal_sui_amount);

    // and deposit into the new <a href="validator.md#0x2_validator">validator</a>'s pool
    <a href="validator_set.md#0x2_validator_set_request_add_delegation">request_add_delegation</a>(self, new_validator_address, principal_stake, time_lock, ctx);

    <b>let</b> delegator = <a href="tx_context.md#0x2_tx_context_sender">tx_context::sender</a>(ctx);

    // add pending switch entry, <b>to</b> be processed at epoch boundaries.
    <b>let</b> key = <a href="validator_set.md#0x2_validator_set_ValidatorPair">ValidatorPair</a> { from: current_validator_address, <b>to</b>: new_validator_address };
    <b>let</b> entry = <a href="staking_pool.md#0x2_staking_pool_new_pending_withdraw_entry">staking_pool::new_pending_withdraw_entry</a>(delegator,principal_sui_amount, current_validator_pool_token);
    <b>if</b> (!<a href="vec_map.md#0x2_vec_map_contains">vec_map::contains</a>(&self.pending_delegation_switches, &key)) {
        <a href="vec_map.md#0x2_vec_map_insert">vec_map::insert</a>(&<b>mut</b> self.pending_delegation_switches, key, <a href="_singleton">vector::singleton</a>(entry));
    } <b>else</b> {
        <b>let</b> entries = <a href="vec_map.md#0x2_vec_map_get_mut">vec_map::get_mut</a>(&<b>mut</b> self.pending_delegation_switches, &key);
        <a href="_push_back">vector::push_back</a>(entries, entry);
    };

    self.next_epoch_validators = <a href="validator_set.md#0x2_validator_set_derive_next_epoch_validators">derive_next_epoch_validators</a>(self);
}
</code></pre>



</details>

<a name="0x2_validator_set_process_delegation_switches"></a>

## Function `process_delegation_switches`



<pre><code><b>fun</b> <a href="validator_set.md#0x2_validator_set_process_delegation_switches">process_delegation_switches</a>(self: &<b>mut</b> <a href="validator_set.md#0x2_validator_set_ValidatorSet">validator_set::ValidatorSet</a>, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="validator_set.md#0x2_validator_set_process_delegation_switches">process_delegation_switches</a>(self: &<b>mut</b> <a href="validator_set.md#0x2_validator_set_ValidatorSet">ValidatorSet</a>, ctx: &<b>mut</b> TxContext) {
    // for each pair of (from, <b>to</b>) validators, complete the delegation switch
    <b>while</b> (!<a href="vec_map.md#0x2_vec_map_is_empty">vec_map::is_empty</a>(&self.pending_delegation_switches)) {
        <b>let</b> (<a href="validator_set.md#0x2_validator_set_ValidatorPair">ValidatorPair</a> { from, <b>to</b> }, entries) = <a href="vec_map.md#0x2_vec_map_pop">vec_map::pop</a>(&<b>mut</b> self.pending_delegation_switches);
        <b>let</b> from_validator = <a href="validator_set.md#0x2_validator_set_get_validator_mut">get_validator_mut</a>(&<b>mut</b> self.active_validators, from);
        <b>let</b> from_pool = <a href="validator.md#0x2_validator_get_staking_pool_mut_ref">validator::get_staking_pool_mut_ref</a>(from_validator);
        // withdraw rewards from the <b>old</b> <a href="validator.md#0x2_validator">validator</a>'s pool
        <b>let</b> (delegators, rewards, rewards_withdraw_amount) = <a href="staking_pool.md#0x2_staking_pool_batch_rewards_withdraws">staking_pool::batch_rewards_withdraws</a>(from_pool, entries);
        <a href="validator.md#0x2_validator_decrease_next_epoch_delegation">validator::decrease_next_epoch_delegation</a>(from_validator, rewards_withdraw_amount);

        <b>assert</b>!(<a href="_length">vector::length</a>(&delegators) == <a href="_length">vector::length</a>(&rewards), 0);

        <b>let</b> to_validator = <a href="validator_set.md#0x2_validator_set_get_validator_mut">get_validator_mut</a>(&<b>mut</b> self.active_validators, <b>to</b>);
        // add delegations <b>to</b> the new <a href="validator.md#0x2_validator">validator</a>
        <b>while</b> (!<a href="_is_empty">vector::is_empty</a>(&rewards)) {
            <b>let</b> delegator = <a href="_pop_back">vector::pop_back</a>(&<b>mut</b> delegators);
            <b>let</b> new_stake = <a href="_pop_back">vector::pop_back</a>(&<b>mut</b> rewards);
            <a href="validator.md#0x2_validator_request_add_delegation">validator::request_add_delegation</a>(
                to_validator,
                new_stake,
                <a href="_none">option::none</a>(), // no time lock for rewards
                delegator,
                ctx
            );
        };
        <a href="_destroy_empty">vector::destroy_empty</a>(rewards);
    };
}
</code></pre>



</details>

<a name="0x2_validator_set_process_pending_delegations"></a>

## Function `process_pending_delegations`



<pre><code><b>fun</b> <a href="validator_set.md#0x2_validator_set_process_pending_delegations">process_pending_delegations</a>(validators: &<b>mut</b> <a href="">vector</a>&lt;<a href="validator.md#0x2_validator_Validator">validator::Validator</a>&gt;, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="validator_set.md#0x2_validator_set_process_pending_delegations">process_pending_delegations</a>(validators: &<b>mut</b> <a href="">vector</a>&lt;Validator&gt;, ctx: &<b>mut</b> TxContext) {
    <b>let</b> length = <a href="_length">vector::length</a>(validators);
    <b>let</b> i = 0;
    <b>while</b> (i &lt; length) {
        <b>let</b> <a href="validator.md#0x2_validator">validator</a> = <a href="_borrow_mut">vector::borrow_mut</a>(validators, i);
        <a href="validator.md#0x2_validator_process_pending_delegations">validator::process_pending_delegations</a>(<a href="validator.md#0x2_validator">validator</a>, ctx);
        i = i + 1;
    }
}
</code></pre>



</details>

<a name="0x2_validator_set_advance_epoch"></a>

## Function `advance_epoch`

Update the validator set at the end of epoch.
It does the following things:
1. Distribute stake award.
2. Process pending stake deposits and withdraws for each validator (<code>adjust_stake</code>).
3. Process pending validator application and withdraws.
4. At the end, we calculate the total stake for the new epoch.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator_set.md#0x2_validator_set_advance_epoch">advance_epoch</a>(self: &<b>mut</b> <a href="validator_set.md#0x2_validator_set_ValidatorSet">validator_set::ValidatorSet</a>, validator_reward: &<b>mut</b> <a href="balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="sui.md#0x2_sui_SUI">sui::SUI</a>&gt;, delegator_reward: &<b>mut</b> <a href="balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="sui.md#0x2_sui_SUI">sui::SUI</a>&gt;, _validator_report_records: &<a href="vec_map.md#0x2_vec_map_VecMap">vec_map::VecMap</a>&lt;<b>address</b>, <a href="vec_set.md#0x2_vec_set_VecSet">vec_set::VecSet</a>&lt;<b>address</b>&gt;&gt;, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator_set.md#0x2_validator_set_advance_epoch">advance_epoch</a>(
    self: &<b>mut</b> <a href="validator_set.md#0x2_validator_set_ValidatorSet">ValidatorSet</a>,
    validator_reward: &<b>mut</b> Balance&lt;SUI&gt;,
    delegator_reward: &<b>mut</b> Balance&lt;SUI&gt;,
    _validator_report_records: &VecMap&lt;<b>address</b>, VecSet&lt;<b>address</b>&gt;&gt;,
    ctx: &<b>mut</b> TxContext,
) {
    // `compute_reward_distribution` must be called before `adjust_stake` <b>to</b> make sure we are using the current
    // epoch's <a href="stake.md#0x2_stake">stake</a> information <b>to</b> compute reward distribution.
    <b>let</b> (validator_reward_amounts, delegator_reward_amounts) = <a href="validator_set.md#0x2_validator_set_compute_reward_distribution">compute_reward_distribution</a>(
        &self.active_validators,
        self.total_validator_stake,
        <a href="balance.md#0x2_balance_value">balance::value</a>(validator_reward),
        self.total_delegation_stake,
        <a href="balance.md#0x2_balance_value">balance::value</a>(delegator_reward),
    );

    // `adjust_stake_and_gas_price` must be called before `distribute_reward`, because reward distribution goes <b>to</b>
    // each <a href="validator.md#0x2_validator">validator</a>'s pending <a href="stake.md#0x2_stake">stake</a>, and that shouldn't be available in the next epoch.
    <a href="validator_set.md#0x2_validator_set_adjust_stake_and_gas_price">adjust_stake_and_gas_price</a>(&<b>mut</b> self.active_validators);

    // TODO: <b>use</b> `validator_report_records` and punish validators whose numbers of reports receives are greater than
    // some threshold.
    <a href="validator_set.md#0x2_validator_set_distribute_reward">distribute_reward</a>(
        &<b>mut</b> self.active_validators,
        &validator_reward_amounts,
        validator_reward,
        &delegator_reward_amounts,
        delegator_reward,
        ctx
    );

    <a href="validator_set.md#0x2_validator_set_process_delegation_switches">process_delegation_switches</a>(self, ctx);

    <a href="validator_set.md#0x2_validator_set_process_pending_delegations">process_pending_delegations</a>(&<b>mut</b> self.active_validators, ctx);

    <a href="validator_set.md#0x2_validator_set_process_pending_validators">process_pending_validators</a>(&<b>mut</b> self.active_validators, &<b>mut</b> self.pending_validators);

    <a href="validator_set.md#0x2_validator_set_process_pending_removals">process_pending_removals</a>(self, ctx);

    self.next_epoch_validators = <a href="validator_set.md#0x2_validator_set_derive_next_epoch_validators">derive_next_epoch_validators</a>(self);

    <b>let</b> (validator_stake, delegation_stake, quorum_stake_threshold) = <a href="validator_set.md#0x2_validator_set_calculate_total_stake_and_quorum_threshold">calculate_total_stake_and_quorum_threshold</a>(&self.active_validators);
    self.total_validator_stake = validator_stake;
    self.total_delegation_stake = delegation_stake;
    self.quorum_stake_threshold = quorum_stake_threshold;
}
</code></pre>



</details>

<a name="0x2_validator_set_derive_reference_gas_price"></a>

## Function `derive_reference_gas_price`

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
            // Count both self and delegated <a href="stake.md#0x2_stake">stake</a>
            pq::new_entry(<a href="validator.md#0x2_validator_gas_price">validator::gas_price</a>(v), <a href="validator.md#0x2_validator_stake_amount">validator::stake_amount</a>(v) + <a href="validator.md#0x2_validator_delegate_amount">validator::delegate_amount</a>(v))
        );
        i = i + 1;
    };
    // Build a priority queue that will pop entries <b>with</b> gas price from the highest <b>to</b> the lowest.
    <b>let</b> pq = pq::new(entries);
    <b>let</b> sum = 0;
    <b>let</b> threshold = (<a href="validator_set.md#0x2_validator_set_total_validator_stake">total_validator_stake</a>(self) + <a href="validator_set.md#0x2_validator_set_total_delegation_stake">total_delegation_stake</a>(self)) / 3;
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
        self.total_delegation_stake = self.total_delegation_stake - <a href="validator.md#0x2_validator_delegate_amount">validator::delegate_amount</a>(&<a href="validator.md#0x2_validator">validator</a>);
        <a href="validator.md#0x2_validator_destroy">validator::destroy</a>(<a href="validator.md#0x2_validator">validator</a>, ctx);
    }
}
</code></pre>



</details>

<a name="0x2_validator_set_process_pending_validators"></a>

## Function `process_pending_validators`

Process the pending new validators. They are simply inserted into <code>validators</code>.


<pre><code><b>fun</b> <a href="validator_set.md#0x2_validator_set_process_pending_validators">process_pending_validators</a>(validators: &<b>mut</b> <a href="">vector</a>&lt;<a href="validator.md#0x2_validator_Validator">validator::Validator</a>&gt;, pending_validators: &<b>mut</b> <a href="">vector</a>&lt;<a href="validator.md#0x2_validator_Validator">validator::Validator</a>&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="validator_set.md#0x2_validator_set_process_pending_validators">process_pending_validators</a>(
    validators: &<b>mut</b> <a href="">vector</a>&lt;Validator&gt;, pending_validators: &<b>mut</b> <a href="">vector</a>&lt;Validator&gt;
) {
    <b>while</b> (!<a href="_is_empty">vector::is_empty</a>(pending_validators)) {
        <b>let</b> v = <a href="_pop_back">vector::pop_back</a>(pending_validators);
        <a href="_push_back">vector::push_back</a>(validators, v);
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

<a name="0x2_validator_set_calculate_total_stake_and_quorum_threshold"></a>

## Function `calculate_total_stake_and_quorum_threshold`

Calculate the total active stake, and the amount of stake to reach quorum.


<pre><code><b>fun</b> <a href="validator_set.md#0x2_validator_set_calculate_total_stake_and_quorum_threshold">calculate_total_stake_and_quorum_threshold</a>(validators: &<a href="">vector</a>&lt;<a href="validator.md#0x2_validator_Validator">validator::Validator</a>&gt;): (u64, u64, u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="validator_set.md#0x2_validator_set_calculate_total_stake_and_quorum_threshold">calculate_total_stake_and_quorum_threshold</a>(validators: &<a href="">vector</a>&lt;Validator&gt;): (u64, u64, u64) {
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
    <b>let</b> total_stake = validator_state + delegate_stake;
    (validator_state, delegate_stake, (total_stake + 1) * 2 / 3)
}
</code></pre>



</details>

<a name="0x2_validator_set_calculate_quorum_threshold"></a>

## Function `calculate_quorum_threshold`

Calculate the required percentage threshold to reach quorum.
With 3f + 1 validators, we can tolerate up to f byzantine ones.
Hence (2f + 1) / total is our threshold.


<pre><code><b>fun</b> <a href="validator_set.md#0x2_validator_set_calculate_quorum_threshold">calculate_quorum_threshold</a>(validators: &<a href="">vector</a>&lt;<a href="validator.md#0x2_validator_Validator">validator::Validator</a>&gt;): u8
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="validator_set.md#0x2_validator_set_calculate_quorum_threshold">calculate_quorum_threshold</a>(validators: &<a href="">vector</a>&lt;Validator&gt;): u8 {
    <b>let</b> count = <a href="_length">vector::length</a>(validators);
    <b>let</b> threshold = (2 * count / 3 + 1) * 100 / count;
    (threshold <b>as</b> u8)
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

<a name="0x2_validator_set_compute_reward_distribution"></a>

## Function `compute_reward_distribution`

Given the current list of active validators, the total stake and total reward,
calculate the amount of reward each validator should get.
Returns the amount of reward for each validator, as well as a remaining reward
due to integer division loss.


<pre><code><b>fun</b> <a href="validator_set.md#0x2_validator_set_compute_reward_distribution">compute_reward_distribution</a>(validators: &<a href="">vector</a>&lt;<a href="validator.md#0x2_validator_Validator">validator::Validator</a>&gt;, total_stake: u64, total_reward: u64, total_delegation_stake: u64, total_delegation_reward: u64): (<a href="">vector</a>&lt;u64&gt;, <a href="">vector</a>&lt;u64&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="validator_set.md#0x2_validator_set_compute_reward_distribution">compute_reward_distribution</a>(
    validators: &<a href="">vector</a>&lt;Validator&gt;,
    total_stake: u64,
    total_reward: u64,
    total_delegation_stake: u64,
    total_delegation_reward: u64,
): (<a href="">vector</a>&lt;u64&gt;, <a href="">vector</a>&lt;u64&gt;) {
    <b>let</b> validator_reward_amounts = <a href="_empty">vector::empty</a>();
    <b>let</b> delegator_reward_amounts = <a href="_empty">vector::empty</a>();
    <b>let</b> length = <a href="_length">vector::length</a>(validators);
    <b>let</b> i = 0;
    <b>while</b> (i &lt; length) {
        <b>let</b> <a href="validator.md#0x2_validator">validator</a> = <a href="_borrow">vector::borrow</a>(validators, i);
        // Integer divisions will truncate the results. Because of this, we expect that at the end
        // there will be some reward remaining in `total_reward`.
        // Use u128 <b>to</b> avoid multiplication overflow.
        <b>let</b> stake_amount: u128 = (<a href="validator.md#0x2_validator_stake_amount">validator::stake_amount</a>(<a href="validator.md#0x2_validator">validator</a>) <b>as</b> u128);
        <b>let</b> reward_amount = stake_amount * (total_reward <b>as</b> u128) / (total_stake <b>as</b> u128);
        <a href="_push_back">vector::push_back</a>(&<b>mut</b> validator_reward_amounts, (reward_amount <b>as</b> u64));

        <b>let</b> delegation_stake_amount: u128 = (<a href="validator.md#0x2_validator_delegate_amount">validator::delegate_amount</a>(<a href="validator.md#0x2_validator">validator</a>) <b>as</b> u128);
        <b>let</b> delegation_reward_amount =
            <b>if</b> (total_delegation_stake == 0) 0
            <b>else</b> delegation_stake_amount * (total_delegation_reward <b>as</b> u128) / (total_delegation_stake <b>as</b> u128);
        <a href="_push_back">vector::push_back</a>(&<b>mut</b> delegator_reward_amounts, (delegation_reward_amount <b>as</b> u64));

        i = i + 1;
    };
    (validator_reward_amounts, delegator_reward_amounts)
}
</code></pre>



</details>

<a name="0x2_validator_set_distribute_reward"></a>

## Function `distribute_reward`



<pre><code><b>fun</b> <a href="validator_set.md#0x2_validator_set_distribute_reward">distribute_reward</a>(validators: &<b>mut</b> <a href="">vector</a>&lt;<a href="validator.md#0x2_validator_Validator">validator::Validator</a>&gt;, validator_reward_amounts: &<a href="">vector</a>&lt;u64&gt;, validator_rewards: &<b>mut</b> <a href="balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="sui.md#0x2_sui_SUI">sui::SUI</a>&gt;, delegator_reward_amounts: &<a href="">vector</a>&lt;u64&gt;, delegator_rewards: &<b>mut</b> <a href="balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="sui.md#0x2_sui_SUI">sui::SUI</a>&gt;, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="validator_set.md#0x2_validator_set_distribute_reward">distribute_reward</a>(
    validators: &<b>mut</b> <a href="">vector</a>&lt;Validator&gt;,
    validator_reward_amounts: &<a href="">vector</a>&lt;u64&gt;,
    validator_rewards: &<b>mut</b> Balance&lt;SUI&gt;,
    delegator_reward_amounts: &<a href="">vector</a>&lt;u64&gt;,
    delegator_rewards: &<b>mut</b> Balance&lt;SUI&gt;,
    ctx: &<b>mut</b> TxContext
) {
    <b>let</b> length = <a href="_length">vector::length</a>(validators);
    <b>let</b> i = 0;
    <b>while</b> (i &lt; length) {
        <b>let</b> <a href="validator.md#0x2_validator">validator</a> = <a href="_borrow_mut">vector::borrow_mut</a>(validators, i);
        <b>let</b> validator_reward_amount = *<a href="_borrow">vector::borrow</a>(validator_reward_amounts, i);
        <b>let</b> validator_reward = <a href="balance.md#0x2_balance_split">balance::split</a>(validator_rewards, validator_reward_amount);

        <b>let</b> delegator_reward_amount = *<a href="_borrow">vector::borrow</a>(delegator_reward_amounts, i);
        <b>let</b> delegator_reward = <a href="balance.md#0x2_balance_split">balance::split</a>(delegator_rewards, delegator_reward_amount);

        // Validator takes a cut of the rewards <b>as</b> commission.
        <b>let</b> commission_amount = (delegator_reward_amount <b>as</b> u128) * (<a href="validator.md#0x2_validator_commission_rate">validator::commission_rate</a>(<a href="validator.md#0x2_validator">validator</a>) <b>as</b> u128) / <a href="validator_set.md#0x2_validator_set_BASIS_POINT_DENOMINATOR">BASIS_POINT_DENOMINATOR</a>;
        <a href="balance.md#0x2_balance_join">balance::join</a>(&<b>mut</b> validator_reward, <a href="balance.md#0x2_balance_split">balance::split</a>(&<b>mut</b> delegator_reward, (commission_amount <b>as</b> u64)));

        // Add rewards <b>to</b> the <a href="validator.md#0x2_validator">validator</a>. Because reward goes <b>to</b> pending <a href="stake.md#0x2_stake">stake</a>, it's the same <b>as</b> calling `request_add_stake`.
        <a href="validator.md#0x2_validator_request_add_stake">validator::request_add_stake</a>(<a href="validator.md#0x2_validator">validator</a>, validator_reward, <a href="_none">option::none</a>(), ctx);
        // Add rewards <b>to</b> delegation staking pool <b>to</b> auto compound for delegators.
        <a href="validator.md#0x2_validator_distribute_rewards">validator::distribute_rewards</a>(<a href="validator.md#0x2_validator">validator</a>, delegator_reward, ctx);
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
