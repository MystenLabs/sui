---
title: Module `sui_system::staking_pool`
---



-  [Struct `StakingPool`](#sui_system_staking_pool_StakingPool)
-  [Struct `PoolTokenExchangeRate`](#sui_system_staking_pool_PoolTokenExchangeRate)
-  [Struct `StakedSui`](#sui_system_staking_pool_StakedSui)
-  [Struct `FungibleStakedSui`](#sui_system_staking_pool_FungibleStakedSui)
-  [Struct `FungibleStakedSuiData`](#sui_system_staking_pool_FungibleStakedSuiData)
-  [Struct `FungibleStakedSuiDataKey`](#sui_system_staking_pool_FungibleStakedSuiDataKey)
-  [Constants](#@Constants_0)
-  [Function `new`](#sui_system_staking_pool_new)
-  [Function `request_add_stake`](#sui_system_staking_pool_request_add_stake)
-  [Function `request_withdraw_stake`](#sui_system_staking_pool_request_withdraw_stake)
-  [Function `redeem_fungible_staked_sui`](#sui_system_staking_pool_redeem_fungible_staked_sui)
-  [Function `calculate_fungible_staked_sui_withdraw_amount`](#sui_system_staking_pool_calculate_fungible_staked_sui_withdraw_amount)
-  [Function `convert_to_fungible_staked_sui`](#sui_system_staking_pool_convert_to_fungible_staked_sui)
-  [Function `withdraw_from_principal`](#sui_system_staking_pool_withdraw_from_principal)
-  [Function `unwrap_staked_sui`](#sui_system_staking_pool_unwrap_staked_sui)
-  [Function `deposit_rewards`](#sui_system_staking_pool_deposit_rewards)
-  [Function `process_pending_stakes_and_withdraws`](#sui_system_staking_pool_process_pending_stakes_and_withdraws)
-  [Function `process_pending_stake_withdraw`](#sui_system_staking_pool_process_pending_stake_withdraw)
-  [Function `process_pending_stake`](#sui_system_staking_pool_process_pending_stake)
-  [Function `withdraw_rewards`](#sui_system_staking_pool_withdraw_rewards)
-  [Function `activate_staking_pool`](#sui_system_staking_pool_activate_staking_pool)
-  [Function `deactivate_staking_pool`](#sui_system_staking_pool_deactivate_staking_pool)
-  [Function `sui_balance`](#sui_system_staking_pool_sui_balance)
-  [Function `pool_id`](#sui_system_staking_pool_pool_id)
-  [Function `fungible_staked_sui_pool_id`](#sui_system_staking_pool_fungible_staked_sui_pool_id)
-  [Function `staked_sui_amount`](#sui_system_staking_pool_staked_sui_amount)
-  [Function `stake_activation_epoch`](#sui_system_staking_pool_stake_activation_epoch)
-  [Function `is_preactive`](#sui_system_staking_pool_is_preactive)
-  [Function `is_inactive`](#sui_system_staking_pool_is_inactive)
-  [Function `fungible_staked_sui_value`](#sui_system_staking_pool_fungible_staked_sui_value)
-  [Function `split_fungible_staked_sui`](#sui_system_staking_pool_split_fungible_staked_sui)
-  [Function `join_fungible_staked_sui`](#sui_system_staking_pool_join_fungible_staked_sui)
-  [Function `split`](#sui_system_staking_pool_split)
-  [Function `split_staked_sui`](#sui_system_staking_pool_split_staked_sui)
-  [Function `join_staked_sui`](#sui_system_staking_pool_join_staked_sui)
-  [Function `is_equal_staking_metadata`](#sui_system_staking_pool_is_equal_staking_metadata)
-  [Function `pool_token_exchange_rate_at_epoch`](#sui_system_staking_pool_pool_token_exchange_rate_at_epoch)
-  [Function `pending_stake_amount`](#sui_system_staking_pool_pending_stake_amount)
-  [Function `pending_stake_withdraw_amount`](#sui_system_staking_pool_pending_stake_withdraw_amount)
-  [Function `exchange_rates`](#sui_system_staking_pool_exchange_rates)
-  [Function `sui_amount`](#sui_system_staking_pool_sui_amount)
-  [Function `pool_token_amount`](#sui_system_staking_pool_pool_token_amount)
-  [Function `is_preactive_at_epoch`](#sui_system_staking_pool_is_preactive_at_epoch)
-  [Function `get_sui_amount`](#sui_system_staking_pool_get_sui_amount)
-  [Function `get_token_amount`](#sui_system_staking_pool_get_token_amount)
-  [Function `initial_exchange_rate`](#sui_system_staking_pool_initial_exchange_rate)
-  [Function `check_balance_invariants`](#sui_system_staking_pool_check_balance_invariants)


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
<b>use</b> <a href="../sui/sui.md#sui_sui">sui::sui</a>;
<b>use</b> <a href="../sui/table.md#sui_table">sui::table</a>;
<b>use</b> <a href="../sui/transfer.md#sui_transfer">sui::transfer</a>;
<b>use</b> <a href="../sui/tx_context.md#sui_tx_context">sui::tx_context</a>;
<b>use</b> <a href="../sui/types.md#sui_types">sui::types</a>;
<b>use</b> <a href="../sui/url.md#sui_url">sui::url</a>;
<b>use</b> <a href="../sui/vec_set.md#sui_vec_set">sui::vec_set</a>;
</code></pre>



<a name="sui_system_staking_pool_StakingPool"></a>

## Struct `StakingPool`

A staking pool embedded in each validator struct in the system state object.


<pre><code><b>public</b> <b>struct</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakingPool">StakingPool</a> <b>has</b> key, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>id: <a href="../sui/object.md#sui_object_UID">sui::object::UID</a></code>
</dt>
<dd>
</dd>
<dt>
<code>activation_epoch: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;u64&gt;</code>
</dt>
<dd>
 The epoch at which this pool became active.
 The value is <code>None</code> if the pool is pre-active and <code>Some(&lt;epoch_number&gt;)</code> if active or inactive.
</dd>
<dt>
<code>deactivation_epoch: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;u64&gt;</code>
</dt>
<dd>
 The epoch at which this staking pool ceased to be active. <code>None</code> = {pre-active, active},
 <code>Some(&lt;epoch_number&gt;)</code> if in-active, and it was de-activated at epoch <code>&lt;epoch_number&gt;</code>.
</dd>
<dt>
<code><a href="../sui_system/staking_pool.md#sui_system_staking_pool_sui_balance">sui_balance</a>: u64</code>
</dt>
<dd>
 The total number of SUI tokens in this pool, including the SUI in the rewards_pool, as well as in all the principal
 in the <code><a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakedSui">StakedSui</a></code> object, updated at epoch boundaries.
</dd>
<dt>
<code>rewards_pool: <a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;<a href="../sui/sui.md#sui_sui_SUI">sui::sui::SUI</a>&gt;</code>
</dt>
<dd>
 The epoch stake rewards will be added here at the end of each epoch.
</dd>
<dt>
<code>pool_token_balance: u64</code>
</dt>
<dd>
 Total number of pool tokens issued by the pool.
</dd>
<dt>
<code><a href="../sui_system/staking_pool.md#sui_system_staking_pool_exchange_rates">exchange_rates</a>: <a href="../sui/table.md#sui_table_Table">sui::table::Table</a>&lt;u64, <a href="../sui_system/staking_pool.md#sui_system_staking_pool_PoolTokenExchangeRate">sui_system::staking_pool::PoolTokenExchangeRate</a>&gt;</code>
</dt>
<dd>
 Exchange rate history of previous epochs. Key is the epoch number.
 The entries start from the <code>activation_epoch</code> of this pool and contains exchange rates at the beginning of each epoch,
 i.e., right after the rewards for the previous epoch have been deposited into the pool.
</dd>
<dt>
<code>pending_stake: u64</code>
</dt>
<dd>
 Pending stake amount for this epoch, emptied at epoch boundaries.
</dd>
<dt>
<code>pending_total_sui_withdraw: u64</code>
</dt>
<dd>
 Pending stake withdrawn during the current epoch, emptied at epoch boundaries.
 This includes both the principal and rewards SUI withdrawn.
</dd>
<dt>
<code>pending_pool_token_withdraw: u64</code>
</dt>
<dd>
 Pending pool token withdrawn during the current epoch, emptied at epoch boundaries.
</dd>
<dt>
<code>extra_fields: <a href="../sui/bag.md#sui_bag_Bag">sui::bag::Bag</a></code>
</dt>
<dd>
 Any extra fields that's not defined statically.
</dd>
</dl>


</details>

<a name="sui_system_staking_pool_PoolTokenExchangeRate"></a>

## Struct `PoolTokenExchangeRate`

Struct representing the exchange rate of the stake pool token to SUI.


<pre><code><b>public</b> <b>struct</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_PoolTokenExchangeRate">PoolTokenExchangeRate</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code><a href="../sui_system/staking_pool.md#sui_system_staking_pool_sui_amount">sui_amount</a>: u64</code>
</dt>
<dd>
</dd>
<dt>
<code><a href="../sui_system/staking_pool.md#sui_system_staking_pool_pool_token_amount">pool_token_amount</a>: u64</code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="sui_system_staking_pool_StakedSui"></a>

## Struct `StakedSui`

A self-custodial object holding the staked SUI tokens.


<pre><code><b>public</b> <b>struct</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakedSui">StakedSui</a> <b>has</b> key, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>id: <a href="../sui/object.md#sui_object_UID">sui::object::UID</a></code>
</dt>
<dd>
</dd>
<dt>
<code><a href="../sui_system/staking_pool.md#sui_system_staking_pool_pool_id">pool_id</a>: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a></code>
</dt>
<dd>
 ID of the staking pool we are staking with.
</dd>
<dt>
<code><a href="../sui_system/staking_pool.md#sui_system_staking_pool_stake_activation_epoch">stake_activation_epoch</a>: u64</code>
</dt>
<dd>
 The epoch at which the stake becomes active.
</dd>
<dt>
<code>principal: <a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;<a href="../sui/sui.md#sui_sui_SUI">sui::sui::SUI</a>&gt;</code>
</dt>
<dd>
 The staked SUI tokens.
</dd>
</dl>


</details>

<a name="sui_system_staking_pool_FungibleStakedSui"></a>

## Struct `FungibleStakedSui`

An alternative to <code><a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakedSui">StakedSui</a></code> that holds the pool token amount instead of the SUI balance.
StakedSui objects can be converted to FungibleStakedSuis after the initial warmup period.
The advantage of this is that you can now merge multiple StakedSui objects from different
activation epochs into a single FungibleStakedSui object.


<pre><code><b>public</b> <b>struct</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_FungibleStakedSui">FungibleStakedSui</a> <b>has</b> key, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>id: <a href="../sui/object.md#sui_object_UID">sui::object::UID</a></code>
</dt>
<dd>
</dd>
<dt>
<code><a href="../sui_system/staking_pool.md#sui_system_staking_pool_pool_id">pool_id</a>: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a></code>
</dt>
<dd>
 ID of the staking pool we are staking with.
</dd>
<dt>
<code>value: u64</code>
</dt>
<dd>
 The pool token amount.
</dd>
</dl>


</details>

<a name="sui_system_staking_pool_FungibleStakedSuiData"></a>

## Struct `FungibleStakedSuiData`

Holds useful information


<pre><code><b>public</b> <b>struct</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_FungibleStakedSuiData">FungibleStakedSuiData</a> <b>has</b> key, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>id: <a href="../sui/object.md#sui_object_UID">sui::object::UID</a></code>
</dt>
<dd>
</dd>
<dt>
<code>total_supply: u64</code>
</dt>
<dd>
 fungible_staked_sui supply
</dd>
<dt>
<code>principal: <a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;<a href="../sui/sui.md#sui_sui_SUI">sui::sui::SUI</a>&gt;</code>
</dt>
<dd>
 principal balance. Rewards are withdrawn from the reward pool
</dd>
</dl>


</details>

<a name="sui_system_staking_pool_FungibleStakedSuiDataKey"></a>

## Struct `FungibleStakedSuiDataKey`



<pre><code><b>public</b> <b>struct</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_FungibleStakedSuiDataKey">FungibleStakedSuiDataKey</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="sui_system_staking_pool_EActivationOfInactivePool"></a>



<pre><code><b>const</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_EActivationOfInactivePool">EActivationOfInactivePool</a>: u64 = 16;
</code></pre>



<a name="sui_system_staking_pool_ECannotMintFungibleStakedSuiYet"></a>



<pre><code><b>const</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_ECannotMintFungibleStakedSuiYet">ECannotMintFungibleStakedSuiYet</a>: u64 = 19;
</code></pre>



<a name="sui_system_staking_pool_EDeactivationOfInactivePool"></a>



<pre><code><b>const</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_EDeactivationOfInactivePool">EDeactivationOfInactivePool</a>: u64 = 11;
</code></pre>



<a name="sui_system_staking_pool_EDelegationOfZeroSui"></a>



<pre><code><b>const</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_EDelegationOfZeroSui">EDelegationOfZeroSui</a>: u64 = 17;
</code></pre>



<a name="sui_system_staking_pool_EDelegationToInactivePool"></a>



<pre><code><b>const</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_EDelegationToInactivePool">EDelegationToInactivePool</a>: u64 = 10;
</code></pre>



<a name="sui_system_staking_pool_EDestroyNonzeroBalance"></a>



<pre><code><b>const</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_EDestroyNonzeroBalance">EDestroyNonzeroBalance</a>: u64 = 5;
</code></pre>



<a name="sui_system_staking_pool_EIncompatibleStakedSui"></a>



<pre><code><b>const</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_EIncompatibleStakedSui">EIncompatibleStakedSui</a>: u64 = 12;
</code></pre>



<a name="sui_system_staking_pool_EInsufficientPoolTokenBalance"></a>



<pre><code><b>const</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_EInsufficientPoolTokenBalance">EInsufficientPoolTokenBalance</a>: u64 = 0;
</code></pre>



<a name="sui_system_staking_pool_EInsufficientRewardsPoolBalance"></a>



<pre><code><b>const</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_EInsufficientRewardsPoolBalance">EInsufficientRewardsPoolBalance</a>: u64 = 4;
</code></pre>



<a name="sui_system_staking_pool_EInsufficientSuiTokenBalance"></a>



<pre><code><b>const</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_EInsufficientSuiTokenBalance">EInsufficientSuiTokenBalance</a>: u64 = 3;
</code></pre>



<a name="sui_system_staking_pool_EInvariantFailure"></a>



<pre><code><b>const</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_EInvariantFailure">EInvariantFailure</a>: u64 = 20;
</code></pre>



<a name="sui_system_staking_pool_EPendingDelegationDoesNotExist"></a>



<pre><code><b>const</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_EPendingDelegationDoesNotExist">EPendingDelegationDoesNotExist</a>: u64 = 8;
</code></pre>



<a name="sui_system_staking_pool_EPoolAlreadyActive"></a>



<pre><code><b>const</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_EPoolAlreadyActive">EPoolAlreadyActive</a>: u64 = 14;
</code></pre>



<a name="sui_system_staking_pool_EPoolNotPreactive"></a>



<pre><code><b>const</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_EPoolNotPreactive">EPoolNotPreactive</a>: u64 = 15;
</code></pre>



<a name="sui_system_staking_pool_EStakedSuiBelowThreshold"></a>



<pre><code><b>const</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_EStakedSuiBelowThreshold">EStakedSuiBelowThreshold</a>: u64 = 18;
</code></pre>



<a name="sui_system_staking_pool_ETokenBalancesDoNotMatchExchangeRate"></a>



<pre><code><b>const</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_ETokenBalancesDoNotMatchExchangeRate">ETokenBalancesDoNotMatchExchangeRate</a>: u64 = 9;
</code></pre>



<a name="sui_system_staking_pool_ETokenTimeLockIsSome"></a>



<pre><code><b>const</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_ETokenTimeLockIsSome">ETokenTimeLockIsSome</a>: u64 = 6;
</code></pre>



<a name="sui_system_staking_pool_EWithdrawAmountCannotBeZero"></a>



<pre><code><b>const</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_EWithdrawAmountCannotBeZero">EWithdrawAmountCannotBeZero</a>: u64 = 2;
</code></pre>



<a name="sui_system_staking_pool_EWithdrawalInSameEpoch"></a>



<pre><code><b>const</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_EWithdrawalInSameEpoch">EWithdrawalInSameEpoch</a>: u64 = 13;
</code></pre>



<a name="sui_system_staking_pool_EWrongDelegation"></a>



<pre><code><b>const</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_EWrongDelegation">EWrongDelegation</a>: u64 = 7;
</code></pre>



<a name="sui_system_staking_pool_EWrongPool"></a>



<pre><code><b>const</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_EWrongPool">EWrongPool</a>: u64 = 1;
</code></pre>



<a name="sui_system_staking_pool_MIN_STAKING_THRESHOLD"></a>

StakedSui objects cannot be split to below this amount.


<pre><code><b>const</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_MIN_STAKING_THRESHOLD">MIN_STAKING_THRESHOLD</a>: u64 = 1000000000;
</code></pre>



<a name="sui_system_staking_pool_new"></a>

## Function `new`

Create a new, empty staking pool.


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_new">new</a>(ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakingPool">sui_system::staking_pool::StakingPool</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_new">new</a>(ctx: &<b>mut</b> TxContext) : <a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakingPool">StakingPool</a> {
    <b>let</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_exchange_rates">exchange_rates</a> = table::new(ctx);
    <a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakingPool">StakingPool</a> {
        id: object::new(ctx),
        activation_epoch: option::none(),
        deactivation_epoch: option::none(),
        <a href="../sui_system/staking_pool.md#sui_system_staking_pool_sui_balance">sui_balance</a>: 0,
        rewards_pool: balance::zero(),
        pool_token_balance: 0,
        <a href="../sui_system/staking_pool.md#sui_system_staking_pool_exchange_rates">exchange_rates</a>,
        pending_stake: 0,
        pending_total_sui_withdraw: 0,
        pending_pool_token_withdraw: 0,
        extra_fields: bag::new(ctx),
    }
}
</code></pre>



</details>

<a name="sui_system_staking_pool_request_add_stake"></a>

## Function `request_add_stake`

Request to stake to a staking pool. The stake starts counting at the beginning of the next epoch,


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_request_add_stake">request_add_stake</a>(pool: &<b>mut</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakingPool">sui_system::staking_pool::StakingPool</a>, stake: <a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;<a href="../sui/sui.md#sui_sui_SUI">sui::sui::SUI</a>&gt;, <a href="../sui_system/staking_pool.md#sui_system_staking_pool_stake_activation_epoch">stake_activation_epoch</a>: u64, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakedSui">sui_system::staking_pool::StakedSui</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_request_add_stake">request_add_stake</a>(
    pool: &<b>mut</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakingPool">StakingPool</a>,
    stake: Balance&lt;SUI&gt;,
    <a href="../sui_system/staking_pool.md#sui_system_staking_pool_stake_activation_epoch">stake_activation_epoch</a>: u64,
    ctx: &<b>mut</b> TxContext
) : <a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakedSui">StakedSui</a> {
    <b>let</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_sui_amount">sui_amount</a> = stake.value();
    <b>assert</b>!(!<a href="../sui_system/staking_pool.md#sui_system_staking_pool_is_inactive">is_inactive</a>(pool), <a href="../sui_system/staking_pool.md#sui_system_staking_pool_EDelegationToInactivePool">EDelegationToInactivePool</a>);
    <b>assert</b>!(<a href="../sui_system/staking_pool.md#sui_system_staking_pool_sui_amount">sui_amount</a> &gt; 0, <a href="../sui_system/staking_pool.md#sui_system_staking_pool_EDelegationOfZeroSui">EDelegationOfZeroSui</a>);
    <b>let</b> staked_sui = <a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakedSui">StakedSui</a> {
        id: object::new(ctx),
        <a href="../sui_system/staking_pool.md#sui_system_staking_pool_pool_id">pool_id</a>: object::id(pool),
        <a href="../sui_system/staking_pool.md#sui_system_staking_pool_stake_activation_epoch">stake_activation_epoch</a>,
        principal: stake,
    };
    pool.pending_stake = pool.pending_stake + <a href="../sui_system/staking_pool.md#sui_system_staking_pool_sui_amount">sui_amount</a>;
    staked_sui
}
</code></pre>



</details>

<a name="sui_system_staking_pool_request_withdraw_stake"></a>

## Function `request_withdraw_stake`

Request to withdraw the given stake plus rewards from a staking pool.
Both the principal and corresponding rewards in SUI are withdrawn.
A proportional amount of pool token withdraw is recorded and processed at epoch change time.


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_request_withdraw_stake">request_withdraw_stake</a>(pool: &<b>mut</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakingPool">sui_system::staking_pool::StakingPool</a>, staked_sui: <a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakedSui">sui_system::staking_pool::StakedSui</a>, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;<a href="../sui/sui.md#sui_sui_SUI">sui::sui::SUI</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_request_withdraw_stake">request_withdraw_stake</a>(
    pool: &<b>mut</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakingPool">StakingPool</a>,
    staked_sui: <a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakedSui">StakedSui</a>,
    ctx: &TxContext
) : Balance&lt;SUI&gt; {
    // stake is inactive
    <b>if</b> (staked_sui.<a href="../sui_system/staking_pool.md#sui_system_staking_pool_stake_activation_epoch">stake_activation_epoch</a> &gt; ctx.epoch()) {
        <b>let</b> principal = <a href="../sui_system/staking_pool.md#sui_system_staking_pool_unwrap_staked_sui">unwrap_staked_sui</a>(staked_sui);
        pool.pending_stake = pool.pending_stake - principal.value();
        <b>return</b> principal
    };
    <b>let</b> (pool_token_withdraw_amount, <b>mut</b> principal_withdraw) =
        <a href="../sui_system/staking_pool.md#sui_system_staking_pool_withdraw_from_principal">withdraw_from_principal</a>(pool, staked_sui);
    <b>let</b> principal_withdraw_amount = principal_withdraw.value();
    <b>let</b> rewards_withdraw = <a href="../sui_system/staking_pool.md#sui_system_staking_pool_withdraw_rewards">withdraw_rewards</a>(
        pool, principal_withdraw_amount, pool_token_withdraw_amount, ctx.epoch()
    );
    <b>let</b> total_sui_withdraw_amount = principal_withdraw_amount + rewards_withdraw.value();
    pool.pending_total_sui_withdraw = pool.pending_total_sui_withdraw + total_sui_withdraw_amount;
    pool.pending_pool_token_withdraw = pool.pending_pool_token_withdraw + pool_token_withdraw_amount;
    // If the pool is inactive, we immediately process the withdrawal.
    <b>if</b> (<a href="../sui_system/staking_pool.md#sui_system_staking_pool_is_inactive">is_inactive</a>(pool)) <a href="../sui_system/staking_pool.md#sui_system_staking_pool_process_pending_stake_withdraw">process_pending_stake_withdraw</a>(pool);
    // TODO: implement withdraw bonding period here.
    principal_withdraw.join(rewards_withdraw);
    principal_withdraw
}
</code></pre>



</details>

<a name="sui_system_staking_pool_redeem_fungible_staked_sui"></a>

## Function `redeem_fungible_staked_sui`



<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_redeem_fungible_staked_sui">redeem_fungible_staked_sui</a>(pool: &<b>mut</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakingPool">sui_system::staking_pool::StakingPool</a>, fungible_staked_sui: <a href="../sui_system/staking_pool.md#sui_system_staking_pool_FungibleStakedSui">sui_system::staking_pool::FungibleStakedSui</a>, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;<a href="../sui/sui.md#sui_sui_SUI">sui::sui::SUI</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_redeem_fungible_staked_sui">redeem_fungible_staked_sui</a>(
    pool: &<b>mut</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakingPool">StakingPool</a>,
    fungible_staked_sui: <a href="../sui_system/staking_pool.md#sui_system_staking_pool_FungibleStakedSui">FungibleStakedSui</a>,
    ctx: &TxContext
) : Balance&lt;SUI&gt; {
    <b>let</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_FungibleStakedSui">FungibleStakedSui</a> { id, <a href="../sui_system/staking_pool.md#sui_system_staking_pool_pool_id">pool_id</a>, value } = fungible_staked_sui;
    <b>assert</b>!(<a href="../sui_system/staking_pool.md#sui_system_staking_pool_pool_id">pool_id</a> == object::id(pool), <a href="../sui_system/staking_pool.md#sui_system_staking_pool_EWrongPool">EWrongPool</a>);
    object::delete(id);
    <b>let</b> latest_exchange_rate = <a href="../sui_system/staking_pool.md#sui_system_staking_pool_pool_token_exchange_rate_at_epoch">pool_token_exchange_rate_at_epoch</a>(pool, tx_context::epoch(ctx));
    <b>let</b> fungible_staked_sui_data: &<b>mut</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_FungibleStakedSuiData">FungibleStakedSuiData</a> = bag::borrow_mut(
        &<b>mut</b> pool.extra_fields,
        <a href="../sui_system/staking_pool.md#sui_system_staking_pool_FungibleStakedSuiDataKey">FungibleStakedSuiDataKey</a> {}
    );
    <b>let</b> (principal_amount, rewards_amount) = <a href="../sui_system/staking_pool.md#sui_system_staking_pool_calculate_fungible_staked_sui_withdraw_amount">calculate_fungible_staked_sui_withdraw_amount</a>(
        latest_exchange_rate,
        value,
        balance::value(&fungible_staked_sui_data.principal),
        fungible_staked_sui_data.total_supply
    );
    fungible_staked_sui_data.total_supply = fungible_staked_sui_data.total_supply - value;
    <b>let</b> <b>mut</b> sui_out = balance::split(&<b>mut</b> fungible_staked_sui_data.principal, principal_amount);
    balance::join(
        &<b>mut</b> sui_out,
        balance::split(&<b>mut</b> pool.rewards_pool, rewards_amount)
    );
    pool.pending_total_sui_withdraw = pool.pending_total_sui_withdraw + balance::value(&sui_out);
    pool.pending_pool_token_withdraw = pool.pending_pool_token_withdraw + value;
    sui_out
}
</code></pre>



</details>

<a name="sui_system_staking_pool_calculate_fungible_staked_sui_withdraw_amount"></a>

## Function `calculate_fungible_staked_sui_withdraw_amount`

written in separate function so i can test with random values
returns (principal_withdraw_amount, rewards_withdraw_amount)


<pre><code><b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_calculate_fungible_staked_sui_withdraw_amount">calculate_fungible_staked_sui_withdraw_amount</a>(latest_exchange_rate: <a href="../sui_system/staking_pool.md#sui_system_staking_pool_PoolTokenExchangeRate">sui_system::staking_pool::PoolTokenExchangeRate</a>, <a href="../sui_system/staking_pool.md#sui_system_staking_pool_fungible_staked_sui_value">fungible_staked_sui_value</a>: u64, fungible_staked_sui_data_principal_amount: u64, fungible_staked_sui_data_total_supply: u64): (u64, u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_calculate_fungible_staked_sui_withdraw_amount">calculate_fungible_staked_sui_withdraw_amount</a>(
    latest_exchange_rate: <a href="../sui_system/staking_pool.md#sui_system_staking_pool_PoolTokenExchangeRate">PoolTokenExchangeRate</a>,
    <a href="../sui_system/staking_pool.md#sui_system_staking_pool_fungible_staked_sui_value">fungible_staked_sui_value</a>: u64,
    fungible_staked_sui_data_principal_amount: u64, // fungible_staked_sui_data.principal.value()
    fungible_staked_sui_data_total_supply: u64, // fungible_staked_sui_data.total_supply
) : (u64, u64) {
    // 1. <b>if</b> the entire <a href="../sui_system/staking_pool.md#sui_system_staking_pool_FungibleStakedSuiData">FungibleStakedSuiData</a> supply is redeemed, how much sui should we receive?
    <b>let</b> total_sui_amount = <a href="../sui_system/staking_pool.md#sui_system_staking_pool_get_sui_amount">get_sui_amount</a>(&latest_exchange_rate, fungible_staked_sui_data_total_supply);
    // min with total_sui_amount to prevent underflow
    <b>let</b> fungible_staked_sui_data_principal_amount = <a href="../std/u64.md#std_u64_min">std::u64::min</a>(
        fungible_staked_sui_data_principal_amount,
        total_sui_amount
    );
    // 2. how much do we need to withdraw from the rewards pool?
    <b>let</b> total_rewards = total_sui_amount - fungible_staked_sui_data_principal_amount;
    // 3. proportionally withdraw from both wrt the <a href="../sui_system/staking_pool.md#sui_system_staking_pool_fungible_staked_sui_value">fungible_staked_sui_value</a>.
    <b>let</b> principal_withdraw_amount = ((<a href="../sui_system/staking_pool.md#sui_system_staking_pool_fungible_staked_sui_value">fungible_staked_sui_value</a> <b>as</b> u128)
        * (fungible_staked_sui_data_principal_amount <b>as</b> u128)
        / (fungible_staked_sui_data_total_supply <b>as</b> u128)) <b>as</b> u64;
    <b>let</b> rewards_withdraw_amount = ((<a href="../sui_system/staking_pool.md#sui_system_staking_pool_fungible_staked_sui_value">fungible_staked_sui_value</a> <b>as</b> u128)
        * (total_rewards <b>as</b> u128)
        / (fungible_staked_sui_data_total_supply <b>as</b> u128)) <b>as</b> u64;
    // <b>invariant</b> check, just in case
    <b>let</b> expected_sui_amount = <a href="../sui_system/staking_pool.md#sui_system_staking_pool_get_sui_amount">get_sui_amount</a>(&latest_exchange_rate, <a href="../sui_system/staking_pool.md#sui_system_staking_pool_fungible_staked_sui_value">fungible_staked_sui_value</a>);
    <b>assert</b>!(principal_withdraw_amount + rewards_withdraw_amount &lt;= expected_sui_amount, <a href="../sui_system/staking_pool.md#sui_system_staking_pool_EInvariantFailure">EInvariantFailure</a>);
    (principal_withdraw_amount, rewards_withdraw_amount)
}
</code></pre>



</details>

<a name="sui_system_staking_pool_convert_to_fungible_staked_sui"></a>

## Function `convert_to_fungible_staked_sui`

Convert the given staked SUI to an FungibleStakedSui object


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_convert_to_fungible_staked_sui">convert_to_fungible_staked_sui</a>(pool: &<b>mut</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakingPool">sui_system::staking_pool::StakingPool</a>, staked_sui: <a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakedSui">sui_system::staking_pool::StakedSui</a>, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <a href="../sui_system/staking_pool.md#sui_system_staking_pool_FungibleStakedSui">sui_system::staking_pool::FungibleStakedSui</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_convert_to_fungible_staked_sui">convert_to_fungible_staked_sui</a>(
    pool: &<b>mut</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakingPool">StakingPool</a>,
    staked_sui: <a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakedSui">StakedSui</a>,
    ctx: &<b>mut</b> TxContext
) : <a href="../sui_system/staking_pool.md#sui_system_staking_pool_FungibleStakedSui">FungibleStakedSui</a> {
    <b>let</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakedSui">StakedSui</a> { id, <a href="../sui_system/staking_pool.md#sui_system_staking_pool_pool_id">pool_id</a>, <a href="../sui_system/staking_pool.md#sui_system_staking_pool_stake_activation_epoch">stake_activation_epoch</a>, principal } = staked_sui;
    <b>assert</b>!(<a href="../sui_system/staking_pool.md#sui_system_staking_pool_pool_id">pool_id</a> == object::id(pool), <a href="../sui_system/staking_pool.md#sui_system_staking_pool_EWrongPool">EWrongPool</a>);
    <b>assert</b>!(
        tx_context::epoch(ctx) &gt;= <a href="../sui_system/staking_pool.md#sui_system_staking_pool_stake_activation_epoch">stake_activation_epoch</a>,
        <a href="../sui_system/staking_pool.md#sui_system_staking_pool_ECannotMintFungibleStakedSuiYet">ECannotMintFungibleStakedSuiYet</a>
    );
    object::delete(id);
    <b>let</b> exchange_rate_at_staking_epoch = <a href="../sui_system/staking_pool.md#sui_system_staking_pool_pool_token_exchange_rate_at_epoch">pool_token_exchange_rate_at_epoch</a>(
        pool,
        <a href="../sui_system/staking_pool.md#sui_system_staking_pool_stake_activation_epoch">stake_activation_epoch</a>
    );
    <b>let</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_pool_token_amount">pool_token_amount</a> = <a href="../sui_system/staking_pool.md#sui_system_staking_pool_get_token_amount">get_token_amount</a>(
        &exchange_rate_at_staking_epoch,
        balance::value(&principal)
    );
    <b>if</b> (!bag::contains(&pool.extra_fields, <a href="../sui_system/staking_pool.md#sui_system_staking_pool_FungibleStakedSuiDataKey">FungibleStakedSuiDataKey</a> {})) {
        bag::add(
            &<b>mut</b> pool.extra_fields,
            <a href="../sui_system/staking_pool.md#sui_system_staking_pool_FungibleStakedSuiDataKey">FungibleStakedSuiDataKey</a> {},
            <a href="../sui_system/staking_pool.md#sui_system_staking_pool_FungibleStakedSuiData">FungibleStakedSuiData</a> {
                id: object::new(ctx),
                total_supply: <a href="../sui_system/staking_pool.md#sui_system_staking_pool_pool_token_amount">pool_token_amount</a>,
                principal
            }
        );
    }
    <b>else</b> {
        <b>let</b> fungible_staked_sui_data: &<b>mut</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_FungibleStakedSuiData">FungibleStakedSuiData</a> = bag::borrow_mut(
            &<b>mut</b> pool.extra_fields,
            <a href="../sui_system/staking_pool.md#sui_system_staking_pool_FungibleStakedSuiDataKey">FungibleStakedSuiDataKey</a> {}
        );
        fungible_staked_sui_data.total_supply = fungible_staked_sui_data.total_supply + <a href="../sui_system/staking_pool.md#sui_system_staking_pool_pool_token_amount">pool_token_amount</a>;
        balance::join(&<b>mut</b> fungible_staked_sui_data.principal, principal);
    };
    <a href="../sui_system/staking_pool.md#sui_system_staking_pool_FungibleStakedSui">FungibleStakedSui</a> {
        id: object::new(ctx),
        <a href="../sui_system/staking_pool.md#sui_system_staking_pool_pool_id">pool_id</a>,
        value: <a href="../sui_system/staking_pool.md#sui_system_staking_pool_pool_token_amount">pool_token_amount</a>,
    }
}
</code></pre>



</details>

<a name="sui_system_staking_pool_withdraw_from_principal"></a>

## Function `withdraw_from_principal`

Withdraw the principal SUI stored in the StakedSui object, and calculate the corresponding amount of pool
tokens using exchange rate at staking epoch.
Returns values are amount of pool tokens withdrawn and withdrawn principal portion of SUI.


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_withdraw_from_principal">withdraw_from_principal</a>(pool: &<a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakingPool">sui_system::staking_pool::StakingPool</a>, staked_sui: <a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakedSui">sui_system::staking_pool::StakedSui</a>): (u64, <a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;<a href="../sui/sui.md#sui_sui_SUI">sui::sui::SUI</a>&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_withdraw_from_principal">withdraw_from_principal</a>(
    pool: &<a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakingPool">StakingPool</a>,
    staked_sui: <a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakedSui">StakedSui</a>,
) : (u64, Balance&lt;SUI&gt;) {
    // Check that the stake information matches the pool.
    <b>assert</b>!(staked_sui.<a href="../sui_system/staking_pool.md#sui_system_staking_pool_pool_id">pool_id</a> == object::id(pool), <a href="../sui_system/staking_pool.md#sui_system_staking_pool_EWrongPool">EWrongPool</a>);
    <b>let</b> exchange_rate_at_staking_epoch = <a href="../sui_system/staking_pool.md#sui_system_staking_pool_pool_token_exchange_rate_at_epoch">pool_token_exchange_rate_at_epoch</a>(pool, staked_sui.<a href="../sui_system/staking_pool.md#sui_system_staking_pool_stake_activation_epoch">stake_activation_epoch</a>);
    <b>let</b> principal_withdraw = <a href="../sui_system/staking_pool.md#sui_system_staking_pool_unwrap_staked_sui">unwrap_staked_sui</a>(staked_sui);
    <b>let</b> pool_token_withdraw_amount = <a href="../sui_system/staking_pool.md#sui_system_staking_pool_get_token_amount">get_token_amount</a>(
		&exchange_rate_at_staking_epoch,
		principal_withdraw.value()
	);
    (
        pool_token_withdraw_amount,
        principal_withdraw,
    )
}
</code></pre>



</details>

<a name="sui_system_staking_pool_unwrap_staked_sui"></a>

## Function `unwrap_staked_sui`



<pre><code><b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_unwrap_staked_sui">unwrap_staked_sui</a>(staked_sui: <a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakedSui">sui_system::staking_pool::StakedSui</a>): <a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;<a href="../sui/sui.md#sui_sui_SUI">sui::sui::SUI</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_unwrap_staked_sui">unwrap_staked_sui</a>(staked_sui: <a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakedSui">StakedSui</a>): Balance&lt;SUI&gt; {
    <b>let</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakedSui">StakedSui</a> {
        id,
        <a href="../sui_system/staking_pool.md#sui_system_staking_pool_pool_id">pool_id</a>: _,
        <a href="../sui_system/staking_pool.md#sui_system_staking_pool_stake_activation_epoch">stake_activation_epoch</a>: _,
        principal,
    } = staked_sui;
    object::delete(id);
    principal
}
</code></pre>



</details>

<a name="sui_system_staking_pool_deposit_rewards"></a>

## Function `deposit_rewards`

Called at epoch advancement times to add rewards (in SUI) to the staking pool.


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_deposit_rewards">deposit_rewards</a>(pool: &<b>mut</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakingPool">sui_system::staking_pool::StakingPool</a>, rewards: <a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;<a href="../sui/sui.md#sui_sui_SUI">sui::sui::SUI</a>&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_deposit_rewards">deposit_rewards</a>(pool: &<b>mut</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakingPool">StakingPool</a>, rewards: Balance&lt;SUI&gt;) {
    pool.<a href="../sui_system/staking_pool.md#sui_system_staking_pool_sui_balance">sui_balance</a> = pool.<a href="../sui_system/staking_pool.md#sui_system_staking_pool_sui_balance">sui_balance</a> + rewards.value();
    pool.rewards_pool.join(rewards);
}
</code></pre>



</details>

<a name="sui_system_staking_pool_process_pending_stakes_and_withdraws"></a>

## Function `process_pending_stakes_and_withdraws`



<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_process_pending_stakes_and_withdraws">process_pending_stakes_and_withdraws</a>(pool: &<b>mut</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakingPool">sui_system::staking_pool::StakingPool</a>, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_process_pending_stakes_and_withdraws">process_pending_stakes_and_withdraws</a>(pool: &<b>mut</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakingPool">StakingPool</a>, ctx: &TxContext) {
    <b>let</b> new_epoch = ctx.epoch() + 1;
    <a href="../sui_system/staking_pool.md#sui_system_staking_pool_process_pending_stake_withdraw">process_pending_stake_withdraw</a>(pool);
    <a href="../sui_system/staking_pool.md#sui_system_staking_pool_process_pending_stake">process_pending_stake</a>(pool);
    pool.<a href="../sui_system/staking_pool.md#sui_system_staking_pool_exchange_rates">exchange_rates</a>.add(
        new_epoch,
        <a href="../sui_system/staking_pool.md#sui_system_staking_pool_PoolTokenExchangeRate">PoolTokenExchangeRate</a> { <a href="../sui_system/staking_pool.md#sui_system_staking_pool_sui_amount">sui_amount</a>: pool.<a href="../sui_system/staking_pool.md#sui_system_staking_pool_sui_balance">sui_balance</a>, <a href="../sui_system/staking_pool.md#sui_system_staking_pool_pool_token_amount">pool_token_amount</a>: pool.pool_token_balance },
    );
    <a href="../sui_system/staking_pool.md#sui_system_staking_pool_check_balance_invariants">check_balance_invariants</a>(pool, new_epoch);
}
</code></pre>



</details>

<a name="sui_system_staking_pool_process_pending_stake_withdraw"></a>

## Function `process_pending_stake_withdraw`

Called at epoch boundaries to process pending stake withdraws requested during the epoch.
Also called immediately upon withdrawal if the pool is inactive.


<pre><code><b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_process_pending_stake_withdraw">process_pending_stake_withdraw</a>(pool: &<b>mut</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakingPool">sui_system::staking_pool::StakingPool</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_process_pending_stake_withdraw">process_pending_stake_withdraw</a>(pool: &<b>mut</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakingPool">StakingPool</a>) {
    pool.<a href="../sui_system/staking_pool.md#sui_system_staking_pool_sui_balance">sui_balance</a> = pool.<a href="../sui_system/staking_pool.md#sui_system_staking_pool_sui_balance">sui_balance</a> - pool.pending_total_sui_withdraw;
    pool.pool_token_balance = pool.pool_token_balance - pool.pending_pool_token_withdraw;
    pool.pending_total_sui_withdraw = 0;
    pool.pending_pool_token_withdraw = 0;
}
</code></pre>



</details>

<a name="sui_system_staking_pool_process_pending_stake"></a>

## Function `process_pending_stake`

Called at epoch boundaries to process the pending stake.


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_process_pending_stake">process_pending_stake</a>(pool: &<b>mut</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakingPool">sui_system::staking_pool::StakingPool</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_process_pending_stake">process_pending_stake</a>(pool: &<b>mut</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakingPool">StakingPool</a>) {
    // Use the most up to date exchange rate with the rewards deposited and withdraws effectuated.
    <b>let</b> latest_exchange_rate =
        <a href="../sui_system/staking_pool.md#sui_system_staking_pool_PoolTokenExchangeRate">PoolTokenExchangeRate</a> { <a href="../sui_system/staking_pool.md#sui_system_staking_pool_sui_amount">sui_amount</a>: pool.<a href="../sui_system/staking_pool.md#sui_system_staking_pool_sui_balance">sui_balance</a>, <a href="../sui_system/staking_pool.md#sui_system_staking_pool_pool_token_amount">pool_token_amount</a>: pool.pool_token_balance };
    pool.<a href="../sui_system/staking_pool.md#sui_system_staking_pool_sui_balance">sui_balance</a> = pool.<a href="../sui_system/staking_pool.md#sui_system_staking_pool_sui_balance">sui_balance</a> + pool.pending_stake;
    pool.pool_token_balance = <a href="../sui_system/staking_pool.md#sui_system_staking_pool_get_token_amount">get_token_amount</a>(&latest_exchange_rate, pool.<a href="../sui_system/staking_pool.md#sui_system_staking_pool_sui_balance">sui_balance</a>);
    pool.pending_stake = 0;
}
</code></pre>



</details>

<a name="sui_system_staking_pool_withdraw_rewards"></a>

## Function `withdraw_rewards`

This function does the following:
1. Calculates the total amount of SUI (including principal and rewards) that the provided pool tokens represent
at the current exchange rate.
2. Using the above number and the given <code>principal_withdraw_amount</code>, calculates the rewards portion of the
stake we should withdraw.
3. Withdraws the rewards portion from the rewards pool at the current exchange rate. We only withdraw the rewards
portion because the principal portion was already taken out of the staker's self custodied StakedSui.


<pre><code><b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_withdraw_rewards">withdraw_rewards</a>(pool: &<b>mut</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakingPool">sui_system::staking_pool::StakingPool</a>, principal_withdraw_amount: u64, pool_token_withdraw_amount: u64, epoch: u64): <a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;<a href="../sui/sui.md#sui_sui_SUI">sui::sui::SUI</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_withdraw_rewards">withdraw_rewards</a>(
    pool: &<b>mut</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakingPool">StakingPool</a>,
    principal_withdraw_amount: u64,
    pool_token_withdraw_amount: u64,
    epoch: u64,
) : Balance&lt;SUI&gt; {
    <b>let</b> exchange_rate = <a href="../sui_system/staking_pool.md#sui_system_staking_pool_pool_token_exchange_rate_at_epoch">pool_token_exchange_rate_at_epoch</a>(pool, epoch);
    <b>let</b> total_sui_withdraw_amount = <a href="../sui_system/staking_pool.md#sui_system_staking_pool_get_sui_amount">get_sui_amount</a>(&exchange_rate, pool_token_withdraw_amount);
    <b>let</b> <b>mut</b> reward_withdraw_amount =
        <b>if</b> (total_sui_withdraw_amount &gt;= principal_withdraw_amount)
            total_sui_withdraw_amount - principal_withdraw_amount
        <b>else</b> 0;
    // This may happen when we are withdrawing everything from the pool and
    // the rewards pool balance may be less than reward_withdraw_amount.
    // TODO: FIGURE OUT EXACTLY WHY THIS CAN HAPPEN.
    reward_withdraw_amount = reward_withdraw_amount.min(pool.rewards_pool.value());
    pool.rewards_pool.<a href="../sui_system/staking_pool.md#sui_system_staking_pool_split">split</a>(reward_withdraw_amount)
}
</code></pre>



</details>

<a name="sui_system_staking_pool_activate_staking_pool"></a>

## Function `activate_staking_pool`

Called by <code><a href="../sui_system/validator.md#sui_system_validator">validator</a></code> module to activate a staking pool.


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_activate_staking_pool">activate_staking_pool</a>(pool: &<b>mut</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakingPool">sui_system::staking_pool::StakingPool</a>, activation_epoch: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_activate_staking_pool">activate_staking_pool</a>(pool: &<b>mut</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakingPool">StakingPool</a>, activation_epoch: u64) {
    // Add the initial exchange rate to the table.
    pool.<a href="../sui_system/staking_pool.md#sui_system_staking_pool_exchange_rates">exchange_rates</a>.add(
        activation_epoch,
        <a href="../sui_system/staking_pool.md#sui_system_staking_pool_initial_exchange_rate">initial_exchange_rate</a>()
    );
    // Check that the pool is preactive and not inactive.
    <b>assert</b>!(<a href="../sui_system/staking_pool.md#sui_system_staking_pool_is_preactive">is_preactive</a>(pool), <a href="../sui_system/staking_pool.md#sui_system_staking_pool_EPoolAlreadyActive">EPoolAlreadyActive</a>);
    <b>assert</b>!(!<a href="../sui_system/staking_pool.md#sui_system_staking_pool_is_inactive">is_inactive</a>(pool), <a href="../sui_system/staking_pool.md#sui_system_staking_pool_EActivationOfInactivePool">EActivationOfInactivePool</a>);
    // Fill in the active epoch.
    pool.activation_epoch.fill(activation_epoch);
}
</code></pre>



</details>

<a name="sui_system_staking_pool_deactivate_staking_pool"></a>

## Function `deactivate_staking_pool`

Deactivate a staking pool by setting the <code>deactivation_epoch</code>. After
this pool deactivation, the pool stops earning rewards. Only stake
withdraws can be made to the pool.


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_deactivate_staking_pool">deactivate_staking_pool</a>(pool: &<b>mut</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakingPool">sui_system::staking_pool::StakingPool</a>, deactivation_epoch: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_deactivate_staking_pool">deactivate_staking_pool</a>(pool: &<b>mut</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakingPool">StakingPool</a>, deactivation_epoch: u64) {
    // We can't deactivate an already deactivated pool.
    <b>assert</b>!(!<a href="../sui_system/staking_pool.md#sui_system_staking_pool_is_inactive">is_inactive</a>(pool), <a href="../sui_system/staking_pool.md#sui_system_staking_pool_EDeactivationOfInactivePool">EDeactivationOfInactivePool</a>);
    pool.deactivation_epoch = option::some(deactivation_epoch);
}
</code></pre>



</details>

<a name="sui_system_staking_pool_sui_balance"></a>

## Function `sui_balance`



<pre><code><b>public</b> <b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_sui_balance">sui_balance</a>(pool: &<a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakingPool">sui_system::staking_pool::StakingPool</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_sui_balance">sui_balance</a>(pool: &<a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakingPool">StakingPool</a>): u64 { pool.<a href="../sui_system/staking_pool.md#sui_system_staking_pool_sui_balance">sui_balance</a> }
</code></pre>



</details>

<a name="sui_system_staking_pool_pool_id"></a>

## Function `pool_id`



<pre><code><b>public</b> <b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_pool_id">pool_id</a>(staked_sui: &<a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakedSui">sui_system::staking_pool::StakedSui</a>): <a href="../sui/object.md#sui_object_ID">sui::object::ID</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_pool_id">pool_id</a>(staked_sui: &<a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakedSui">StakedSui</a>): ID { staked_sui.<a href="../sui_system/staking_pool.md#sui_system_staking_pool_pool_id">pool_id</a> }
</code></pre>



</details>

<a name="sui_system_staking_pool_fungible_staked_sui_pool_id"></a>

## Function `fungible_staked_sui_pool_id`



<pre><code><b>public</b> <b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_fungible_staked_sui_pool_id">fungible_staked_sui_pool_id</a>(fungible_staked_sui: &<a href="../sui_system/staking_pool.md#sui_system_staking_pool_FungibleStakedSui">sui_system::staking_pool::FungibleStakedSui</a>): <a href="../sui/object.md#sui_object_ID">sui::object::ID</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_fungible_staked_sui_pool_id">fungible_staked_sui_pool_id</a>(fungible_staked_sui: &<a href="../sui_system/staking_pool.md#sui_system_staking_pool_FungibleStakedSui">FungibleStakedSui</a>): ID { fungible_staked_sui.<a href="../sui_system/staking_pool.md#sui_system_staking_pool_pool_id">pool_id</a> }
</code></pre>



</details>

<a name="sui_system_staking_pool_staked_sui_amount"></a>

## Function `staked_sui_amount`



<pre><code><b>public</b> <b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_staked_sui_amount">staked_sui_amount</a>(staked_sui: &<a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakedSui">sui_system::staking_pool::StakedSui</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_staked_sui_amount">staked_sui_amount</a>(staked_sui: &<a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakedSui">StakedSui</a>): u64 { staked_sui.principal.value() }
</code></pre>



</details>

<a name="sui_system_staking_pool_stake_activation_epoch"></a>

## Function `stake_activation_epoch`



<pre><code><b>public</b> <b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_stake_activation_epoch">stake_activation_epoch</a>(staked_sui: &<a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakedSui">sui_system::staking_pool::StakedSui</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_stake_activation_epoch">stake_activation_epoch</a>(staked_sui: &<a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakedSui">StakedSui</a>): u64 {
    staked_sui.<a href="../sui_system/staking_pool.md#sui_system_staking_pool_stake_activation_epoch">stake_activation_epoch</a>
}
</code></pre>



</details>

<a name="sui_system_staking_pool_is_preactive"></a>

## Function `is_preactive`

Returns true if the input staking pool is preactive.


<pre><code><b>public</b> <b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_is_preactive">is_preactive</a>(pool: &<a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakingPool">sui_system::staking_pool::StakingPool</a>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_is_preactive">is_preactive</a>(pool: &<a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakingPool">StakingPool</a>): bool{
    pool.activation_epoch.is_none()
}
</code></pre>



</details>

<a name="sui_system_staking_pool_is_inactive"></a>

## Function `is_inactive`

Returns true if the input staking pool is inactive.


<pre><code><b>public</b> <b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_is_inactive">is_inactive</a>(pool: &<a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakingPool">sui_system::staking_pool::StakingPool</a>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_is_inactive">is_inactive</a>(pool: &<a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakingPool">StakingPool</a>): bool {
    pool.deactivation_epoch.is_some()
}
</code></pre>



</details>

<a name="sui_system_staking_pool_fungible_staked_sui_value"></a>

## Function `fungible_staked_sui_value`



<pre><code><b>public</b> <b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_fungible_staked_sui_value">fungible_staked_sui_value</a>(fungible_staked_sui: &<a href="../sui_system/staking_pool.md#sui_system_staking_pool_FungibleStakedSui">sui_system::staking_pool::FungibleStakedSui</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_fungible_staked_sui_value">fungible_staked_sui_value</a>(fungible_staked_sui: &<a href="../sui_system/staking_pool.md#sui_system_staking_pool_FungibleStakedSui">FungibleStakedSui</a>): u64 { fungible_staked_sui.value }
</code></pre>



</details>

<a name="sui_system_staking_pool_split_fungible_staked_sui"></a>

## Function `split_fungible_staked_sui`



<pre><code><b>public</b> <b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_split_fungible_staked_sui">split_fungible_staked_sui</a>(fungible_staked_sui: &<b>mut</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_FungibleStakedSui">sui_system::staking_pool::FungibleStakedSui</a>, split_amount: u64, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <a href="../sui_system/staking_pool.md#sui_system_staking_pool_FungibleStakedSui">sui_system::staking_pool::FungibleStakedSui</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_split_fungible_staked_sui">split_fungible_staked_sui</a>(
    fungible_staked_sui: &<b>mut</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_FungibleStakedSui">FungibleStakedSui</a>,
    split_amount: u64,
    ctx: &<b>mut</b> TxContext
): <a href="../sui_system/staking_pool.md#sui_system_staking_pool_FungibleStakedSui">FungibleStakedSui</a> {
    <b>assert</b>!(split_amount &lt;= fungible_staked_sui.value, <a href="../sui_system/staking_pool.md#sui_system_staking_pool_EInsufficientPoolTokenBalance">EInsufficientPoolTokenBalance</a>);
    fungible_staked_sui.value = fungible_staked_sui.value - split_amount;
    <a href="../sui_system/staking_pool.md#sui_system_staking_pool_FungibleStakedSui">FungibleStakedSui</a> {
        id: object::new(ctx),
        <a href="../sui_system/staking_pool.md#sui_system_staking_pool_pool_id">pool_id</a>: fungible_staked_sui.<a href="../sui_system/staking_pool.md#sui_system_staking_pool_pool_id">pool_id</a>,
        value: split_amount,
    }
}
</code></pre>



</details>

<a name="sui_system_staking_pool_join_fungible_staked_sui"></a>

## Function `join_fungible_staked_sui`



<pre><code><b>public</b> <b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_join_fungible_staked_sui">join_fungible_staked_sui</a>(self: &<b>mut</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_FungibleStakedSui">sui_system::staking_pool::FungibleStakedSui</a>, other: <a href="../sui_system/staking_pool.md#sui_system_staking_pool_FungibleStakedSui">sui_system::staking_pool::FungibleStakedSui</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_join_fungible_staked_sui">join_fungible_staked_sui</a>(self: &<b>mut</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_FungibleStakedSui">FungibleStakedSui</a>, other: <a href="../sui_system/staking_pool.md#sui_system_staking_pool_FungibleStakedSui">FungibleStakedSui</a>) {
    <b>let</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_FungibleStakedSui">FungibleStakedSui</a> { id, <a href="../sui_system/staking_pool.md#sui_system_staking_pool_pool_id">pool_id</a>, value } = other;
    <b>assert</b>!(self.<a href="../sui_system/staking_pool.md#sui_system_staking_pool_pool_id">pool_id</a> == <a href="../sui_system/staking_pool.md#sui_system_staking_pool_pool_id">pool_id</a>, <a href="../sui_system/staking_pool.md#sui_system_staking_pool_EWrongPool">EWrongPool</a>);
    object::delete(id);
    self.value = self.value + value;
}
</code></pre>



</details>

<a name="sui_system_staking_pool_split"></a>

## Function `split`

Split StakedSui <code>self</code> to two parts, one with principal <code>split_amount</code>,
and the remaining principal is left in <code>self</code>.
All the other parameters of the StakedSui like <code><a href="../sui_system/staking_pool.md#sui_system_staking_pool_stake_activation_epoch">stake_activation_epoch</a></code> or <code><a href="../sui_system/staking_pool.md#sui_system_staking_pool_pool_id">pool_id</a></code> remain the same.


<pre><code><b>public</b> <b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_split">split</a>(self: &<b>mut</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakedSui">sui_system::staking_pool::StakedSui</a>, split_amount: u64, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakedSui">sui_system::staking_pool::StakedSui</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_split">split</a>(self: &<b>mut</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakedSui">StakedSui</a>, split_amount: u64, ctx: &<b>mut</b> TxContext): <a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakedSui">StakedSui</a> {
    <b>let</b> original_amount = self.principal.value();
    <b>assert</b>!(split_amount &lt;= original_amount, <a href="../sui_system/staking_pool.md#sui_system_staking_pool_EInsufficientSuiTokenBalance">EInsufficientSuiTokenBalance</a>);
    <b>let</b> remaining_amount = original_amount - split_amount;
    // Both resulting parts should have at least <a href="../sui_system/staking_pool.md#sui_system_staking_pool_MIN_STAKING_THRESHOLD">MIN_STAKING_THRESHOLD</a>.
    <b>assert</b>!(remaining_amount &gt;= <a href="../sui_system/staking_pool.md#sui_system_staking_pool_MIN_STAKING_THRESHOLD">MIN_STAKING_THRESHOLD</a>, <a href="../sui_system/staking_pool.md#sui_system_staking_pool_EStakedSuiBelowThreshold">EStakedSuiBelowThreshold</a>);
    <b>assert</b>!(split_amount &gt;= <a href="../sui_system/staking_pool.md#sui_system_staking_pool_MIN_STAKING_THRESHOLD">MIN_STAKING_THRESHOLD</a>, <a href="../sui_system/staking_pool.md#sui_system_staking_pool_EStakedSuiBelowThreshold">EStakedSuiBelowThreshold</a>);
    <a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakedSui">StakedSui</a> {
        id: object::new(ctx),
        <a href="../sui_system/staking_pool.md#sui_system_staking_pool_pool_id">pool_id</a>: self.<a href="../sui_system/staking_pool.md#sui_system_staking_pool_pool_id">pool_id</a>,
        <a href="../sui_system/staking_pool.md#sui_system_staking_pool_stake_activation_epoch">stake_activation_epoch</a>: self.<a href="../sui_system/staking_pool.md#sui_system_staking_pool_stake_activation_epoch">stake_activation_epoch</a>,
        principal: self.principal.<a href="../sui_system/staking_pool.md#sui_system_staking_pool_split">split</a>(split_amount),
    }
}
</code></pre>



</details>

<a name="sui_system_staking_pool_split_staked_sui"></a>

## Function `split_staked_sui`

Split the given StakedSui to the two parts, one with principal <code>split_amount</code>,
transfer the newly split part to the sender address.


<pre><code><b>public</b> <b>entry</b> <b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_split_staked_sui">split_staked_sui</a>(stake: &<b>mut</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakedSui">sui_system::staking_pool::StakedSui</a>, split_amount: u64, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>entry</b> <b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_split_staked_sui">split_staked_sui</a>(stake: &<b>mut</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakedSui">StakedSui</a>, split_amount: u64, ctx: &<b>mut</b> TxContext) {
    transfer::transfer(<a href="../sui_system/staking_pool.md#sui_system_staking_pool_split">split</a>(stake, split_amount, ctx), ctx.sender());
}
</code></pre>



</details>

<a name="sui_system_staking_pool_join_staked_sui"></a>

## Function `join_staked_sui`

Consume the staked sui <code>other</code> and add its value to <code>self</code>.
Aborts if some of the staking parameters are incompatible (pool id, stake activation epoch, etc.)


<pre><code><b>public</b> <b>entry</b> <b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_join_staked_sui">join_staked_sui</a>(self: &<b>mut</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakedSui">sui_system::staking_pool::StakedSui</a>, other: <a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakedSui">sui_system::staking_pool::StakedSui</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>entry</b> <b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_join_staked_sui">join_staked_sui</a>(self: &<b>mut</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakedSui">StakedSui</a>, other: <a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakedSui">StakedSui</a>) {
    <b>assert</b>!(<a href="../sui_system/staking_pool.md#sui_system_staking_pool_is_equal_staking_metadata">is_equal_staking_metadata</a>(self, &other), <a href="../sui_system/staking_pool.md#sui_system_staking_pool_EIncompatibleStakedSui">EIncompatibleStakedSui</a>);
    <b>let</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakedSui">StakedSui</a> {
        id,
        <a href="../sui_system/staking_pool.md#sui_system_staking_pool_pool_id">pool_id</a>: _,
        <a href="../sui_system/staking_pool.md#sui_system_staking_pool_stake_activation_epoch">stake_activation_epoch</a>: _,
        principal,
    } = other;
    id.delete();
    self.principal.join(principal);
}
</code></pre>



</details>

<a name="sui_system_staking_pool_is_equal_staking_metadata"></a>

## Function `is_equal_staking_metadata`

Returns true if all the staking parameters of the staked sui except the principal are identical


<pre><code><b>public</b> <b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_is_equal_staking_metadata">is_equal_staking_metadata</a>(self: &<a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakedSui">sui_system::staking_pool::StakedSui</a>, other: &<a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakedSui">sui_system::staking_pool::StakedSui</a>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_is_equal_staking_metadata">is_equal_staking_metadata</a>(self: &<a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakedSui">StakedSui</a>, other: &<a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakedSui">StakedSui</a>): bool {
    (self.<a href="../sui_system/staking_pool.md#sui_system_staking_pool_pool_id">pool_id</a> == other.<a href="../sui_system/staking_pool.md#sui_system_staking_pool_pool_id">pool_id</a>) &&
    (self.<a href="../sui_system/staking_pool.md#sui_system_staking_pool_stake_activation_epoch">stake_activation_epoch</a> == other.<a href="../sui_system/staking_pool.md#sui_system_staking_pool_stake_activation_epoch">stake_activation_epoch</a>)
}
</code></pre>



</details>

<a name="sui_system_staking_pool_pool_token_exchange_rate_at_epoch"></a>

## Function `pool_token_exchange_rate_at_epoch`



<pre><code><b>public</b> <b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_pool_token_exchange_rate_at_epoch">pool_token_exchange_rate_at_epoch</a>(pool: &<a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakingPool">sui_system::staking_pool::StakingPool</a>, epoch: u64): <a href="../sui_system/staking_pool.md#sui_system_staking_pool_PoolTokenExchangeRate">sui_system::staking_pool::PoolTokenExchangeRate</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_pool_token_exchange_rate_at_epoch">pool_token_exchange_rate_at_epoch</a>(pool: &<a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakingPool">StakingPool</a>, epoch: u64): <a href="../sui_system/staking_pool.md#sui_system_staking_pool_PoolTokenExchangeRate">PoolTokenExchangeRate</a> {
    // If the pool is preactive then the exchange rate is always 1:1.
    <b>if</b> (<a href="../sui_system/staking_pool.md#sui_system_staking_pool_is_preactive_at_epoch">is_preactive_at_epoch</a>(pool, epoch)) {
        <b>return</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_initial_exchange_rate">initial_exchange_rate</a>()
    };
    <b>let</b> clamped_epoch = pool.deactivation_epoch.get_with_default(epoch);
    <b>let</b> <b>mut</b> epoch = clamped_epoch.min(epoch);
    <b>let</b> activation_epoch = *pool.activation_epoch.borrow();
    // Find the latest epoch that's earlier than the given epoch with an <b>entry</b> in the table
    <b>while</b> (epoch &gt;= activation_epoch) {
        <b>if</b> (pool.<a href="../sui_system/staking_pool.md#sui_system_staking_pool_exchange_rates">exchange_rates</a>.contains(epoch)) {
            <b>return</b> pool.<a href="../sui_system/staking_pool.md#sui_system_staking_pool_exchange_rates">exchange_rates</a>[epoch]
        };
        epoch = epoch - 1;
    };
    // This line really should be unreachable. Do we want an <b>assert</b> <b>false</b> here?
    <a href="../sui_system/staking_pool.md#sui_system_staking_pool_initial_exchange_rate">initial_exchange_rate</a>()
}
</code></pre>



</details>

<a name="sui_system_staking_pool_pending_stake_amount"></a>

## Function `pending_stake_amount`

Returns the total value of the pending staking requests for this staking pool.


<pre><code><b>public</b> <b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_pending_stake_amount">pending_stake_amount</a>(<a href="../sui_system/staking_pool.md#sui_system_staking_pool">staking_pool</a>: &<a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakingPool">sui_system::staking_pool::StakingPool</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_pending_stake_amount">pending_stake_amount</a>(<a href="../sui_system/staking_pool.md#sui_system_staking_pool">staking_pool</a>: &<a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakingPool">StakingPool</a>): u64 {
    <a href="../sui_system/staking_pool.md#sui_system_staking_pool">staking_pool</a>.pending_stake
}
</code></pre>



</details>

<a name="sui_system_staking_pool_pending_stake_withdraw_amount"></a>

## Function `pending_stake_withdraw_amount`

Returns the total withdrawal from the staking pool this epoch.


<pre><code><b>public</b> <b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_pending_stake_withdraw_amount">pending_stake_withdraw_amount</a>(<a href="../sui_system/staking_pool.md#sui_system_staking_pool">staking_pool</a>: &<a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakingPool">sui_system::staking_pool::StakingPool</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_pending_stake_withdraw_amount">pending_stake_withdraw_amount</a>(<a href="../sui_system/staking_pool.md#sui_system_staking_pool">staking_pool</a>: &<a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakingPool">StakingPool</a>): u64 {
    <a href="../sui_system/staking_pool.md#sui_system_staking_pool">staking_pool</a>.pending_total_sui_withdraw
}
</code></pre>



</details>

<a name="sui_system_staking_pool_exchange_rates"></a>

## Function `exchange_rates`



<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_exchange_rates">exchange_rates</a>(pool: &<a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakingPool">sui_system::staking_pool::StakingPool</a>): &<a href="../sui/table.md#sui_table_Table">sui::table::Table</a>&lt;u64, <a href="../sui_system/staking_pool.md#sui_system_staking_pool_PoolTokenExchangeRate">sui_system::staking_pool::PoolTokenExchangeRate</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_exchange_rates">exchange_rates</a>(pool: &<a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakingPool">StakingPool</a>): &Table&lt;u64, <a href="../sui_system/staking_pool.md#sui_system_staking_pool_PoolTokenExchangeRate">PoolTokenExchangeRate</a>&gt; {
    &pool.<a href="../sui_system/staking_pool.md#sui_system_staking_pool_exchange_rates">exchange_rates</a>
}
</code></pre>



</details>

<a name="sui_system_staking_pool_sui_amount"></a>

## Function `sui_amount`



<pre><code><b>public</b> <b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_sui_amount">sui_amount</a>(exchange_rate: &<a href="../sui_system/staking_pool.md#sui_system_staking_pool_PoolTokenExchangeRate">sui_system::staking_pool::PoolTokenExchangeRate</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_sui_amount">sui_amount</a>(exchange_rate: &<a href="../sui_system/staking_pool.md#sui_system_staking_pool_PoolTokenExchangeRate">PoolTokenExchangeRate</a>): u64 {
    exchange_rate.<a href="../sui_system/staking_pool.md#sui_system_staking_pool_sui_amount">sui_amount</a>
}
</code></pre>



</details>

<a name="sui_system_staking_pool_pool_token_amount"></a>

## Function `pool_token_amount`



<pre><code><b>public</b> <b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_pool_token_amount">pool_token_amount</a>(exchange_rate: &<a href="../sui_system/staking_pool.md#sui_system_staking_pool_PoolTokenExchangeRate">sui_system::staking_pool::PoolTokenExchangeRate</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_pool_token_amount">pool_token_amount</a>(exchange_rate: &<a href="../sui_system/staking_pool.md#sui_system_staking_pool_PoolTokenExchangeRate">PoolTokenExchangeRate</a>): u64 {
    exchange_rate.<a href="../sui_system/staking_pool.md#sui_system_staking_pool_pool_token_amount">pool_token_amount</a>
}
</code></pre>



</details>

<a name="sui_system_staking_pool_is_preactive_at_epoch"></a>

## Function `is_preactive_at_epoch`

Returns true if the provided staking pool is preactive at the provided epoch.


<pre><code><b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_is_preactive_at_epoch">is_preactive_at_epoch</a>(pool: &<a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakingPool">sui_system::staking_pool::StakingPool</a>, epoch: u64): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_is_preactive_at_epoch">is_preactive_at_epoch</a>(pool: &<a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakingPool">StakingPool</a>, epoch: u64): bool{
    // Either the pool is currently preactive or the pool's starting epoch is later than the provided epoch.
    <a href="../sui_system/staking_pool.md#sui_system_staking_pool_is_preactive">is_preactive</a>(pool) || (*pool.activation_epoch.borrow() &gt; epoch)
}
</code></pre>



</details>

<a name="sui_system_staking_pool_get_sui_amount"></a>

## Function `get_sui_amount`



<pre><code><b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_get_sui_amount">get_sui_amount</a>(exchange_rate: &<a href="../sui_system/staking_pool.md#sui_system_staking_pool_PoolTokenExchangeRate">sui_system::staking_pool::PoolTokenExchangeRate</a>, token_amount: u64): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_get_sui_amount">get_sui_amount</a>(exchange_rate: &<a href="../sui_system/staking_pool.md#sui_system_staking_pool_PoolTokenExchangeRate">PoolTokenExchangeRate</a>, token_amount: u64): u64 {
    // When either amount is 0, that means we have no stakes with this pool.
    // The other amount might be non-zero when there's dust left in the pool.
    <b>if</b> (exchange_rate.<a href="../sui_system/staking_pool.md#sui_system_staking_pool_sui_amount">sui_amount</a> == 0 || exchange_rate.<a href="../sui_system/staking_pool.md#sui_system_staking_pool_pool_token_amount">pool_token_amount</a> == 0) {
        <b>return</b> token_amount
    };
    <b>let</b> res = exchange_rate.<a href="../sui_system/staking_pool.md#sui_system_staking_pool_sui_amount">sui_amount</a> <b>as</b> u128
            * (token_amount <b>as</b> u128)
            / (exchange_rate.<a href="../sui_system/staking_pool.md#sui_system_staking_pool_pool_token_amount">pool_token_amount</a> <b>as</b> u128);
    res <b>as</b> u64
}
</code></pre>



</details>

<a name="sui_system_staking_pool_get_token_amount"></a>

## Function `get_token_amount`



<pre><code><b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_get_token_amount">get_token_amount</a>(exchange_rate: &<a href="../sui_system/staking_pool.md#sui_system_staking_pool_PoolTokenExchangeRate">sui_system::staking_pool::PoolTokenExchangeRate</a>, <a href="../sui_system/staking_pool.md#sui_system_staking_pool_sui_amount">sui_amount</a>: u64): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_get_token_amount">get_token_amount</a>(exchange_rate: &<a href="../sui_system/staking_pool.md#sui_system_staking_pool_PoolTokenExchangeRate">PoolTokenExchangeRate</a>, <a href="../sui_system/staking_pool.md#sui_system_staking_pool_sui_amount">sui_amount</a>: u64): u64 {
    // When either amount is 0, that means we have no stakes with this pool.
    // The other amount might be non-zero when there's dust left in the pool.
    <b>if</b> (exchange_rate.<a href="../sui_system/staking_pool.md#sui_system_staking_pool_sui_amount">sui_amount</a> == 0 || exchange_rate.<a href="../sui_system/staking_pool.md#sui_system_staking_pool_pool_token_amount">pool_token_amount</a> == 0) {
        <b>return</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_sui_amount">sui_amount</a>
    };
    <b>let</b> res = exchange_rate.<a href="../sui_system/staking_pool.md#sui_system_staking_pool_pool_token_amount">pool_token_amount</a> <b>as</b> u128
            * (<a href="../sui_system/staking_pool.md#sui_system_staking_pool_sui_amount">sui_amount</a> <b>as</b> u128)
            / (exchange_rate.<a href="../sui_system/staking_pool.md#sui_system_staking_pool_sui_amount">sui_amount</a> <b>as</b> u128);
    res <b>as</b> u64
}
</code></pre>



</details>

<a name="sui_system_staking_pool_initial_exchange_rate"></a>

## Function `initial_exchange_rate`



<pre><code><b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_initial_exchange_rate">initial_exchange_rate</a>(): <a href="../sui_system/staking_pool.md#sui_system_staking_pool_PoolTokenExchangeRate">sui_system::staking_pool::PoolTokenExchangeRate</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_initial_exchange_rate">initial_exchange_rate</a>(): <a href="../sui_system/staking_pool.md#sui_system_staking_pool_PoolTokenExchangeRate">PoolTokenExchangeRate</a> {
    <a href="../sui_system/staking_pool.md#sui_system_staking_pool_PoolTokenExchangeRate">PoolTokenExchangeRate</a> { <a href="../sui_system/staking_pool.md#sui_system_staking_pool_sui_amount">sui_amount</a>: 0, <a href="../sui_system/staking_pool.md#sui_system_staking_pool_pool_token_amount">pool_token_amount</a>: 0 }
}
</code></pre>



</details>

<a name="sui_system_staking_pool_check_balance_invariants"></a>

## Function `check_balance_invariants`



<pre><code><b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_check_balance_invariants">check_balance_invariants</a>(pool: &<a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakingPool">sui_system::staking_pool::StakingPool</a>, epoch: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool_check_balance_invariants">check_balance_invariants</a>(pool: &<a href="../sui_system/staking_pool.md#sui_system_staking_pool_StakingPool">StakingPool</a>, epoch: u64) {
    <b>let</b> exchange_rate = <a href="../sui_system/staking_pool.md#sui_system_staking_pool_pool_token_exchange_rate_at_epoch">pool_token_exchange_rate_at_epoch</a>(pool, epoch);
    // check that the pool token balance and sui balance ratio matches the exchange rate stored.
    <b>let</b> expected = <a href="../sui_system/staking_pool.md#sui_system_staking_pool_get_token_amount">get_token_amount</a>(&exchange_rate, pool.<a href="../sui_system/staking_pool.md#sui_system_staking_pool_sui_balance">sui_balance</a>);
    <b>let</b> actual = pool.pool_token_balance;
    <b>assert</b>!(expected == actual, <a href="../sui_system/staking_pool.md#sui_system_staking_pool_ETokenBalancesDoNotMatchExchangeRate">ETokenBalancesDoNotMatchExchangeRate</a>)
}
</code></pre>



</details>
