
<a name="0x2_staking_pool"></a>

# Module `0x2::staking_pool`



-  [Resource `StakingPool`](#0x2_staking_pool_StakingPool)
-  [Struct `PoolTokenExchangeRate`](#0x2_staking_pool_PoolTokenExchangeRate)
-  [Resource `StakedSui`](#0x2_staking_pool_StakedSui)
-  [Constants](#@Constants_0)
-  [Function `new`](#0x2_staking_pool_new)
-  [Function `request_add_stake`](#0x2_staking_pool_request_add_stake)
-  [Function `request_withdraw_stake`](#0x2_staking_pool_request_withdraw_stake)
-  [Function `withdraw_from_principal`](#0x2_staking_pool_withdraw_from_principal)
-  [Function `unwrap_staked_sui`](#0x2_staking_pool_unwrap_staked_sui)
-  [Function `deposit_rewards`](#0x2_staking_pool_deposit_rewards)
-  [Function `process_pending_stakes_and_withdraws`](#0x2_staking_pool_process_pending_stakes_and_withdraws)
-  [Function `process_pending_stake_withdraw`](#0x2_staking_pool_process_pending_stake_withdraw)
-  [Function `process_pending_stake`](#0x2_staking_pool_process_pending_stake)
-  [Function `withdraw_rewards_and_burn_pool_tokens`](#0x2_staking_pool_withdraw_rewards_and_burn_pool_tokens)
-  [Function `activate_staking_pool`](#0x2_staking_pool_activate_staking_pool)
-  [Function `request_withdraw_stake_preactive`](#0x2_staking_pool_request_withdraw_stake_preactive)
-  [Function `deactivate_staking_pool`](#0x2_staking_pool_deactivate_staking_pool)
-  [Function `sui_balance`](#0x2_staking_pool_sui_balance)
-  [Function `pool_id`](#0x2_staking_pool_pool_id)
-  [Function `staked_sui_amount`](#0x2_staking_pool_staked_sui_amount)
-  [Function `stake_activation_epoch`](#0x2_staking_pool_stake_activation_epoch)
-  [Function `is_preactive`](#0x2_staking_pool_is_preactive)
-  [Function `is_inactive`](#0x2_staking_pool_is_inactive)
-  [Function `split`](#0x2_staking_pool_split)
-  [Function `split_staked_sui`](#0x2_staking_pool_split_staked_sui)
-  [Function `join_staked_sui`](#0x2_staking_pool_join_staked_sui)
-  [Function `is_equal_staking_metadata`](#0x2_staking_pool_is_equal_staking_metadata)
-  [Function `pool_token_exchange_rate_at_epoch`](#0x2_staking_pool_pool_token_exchange_rate_at_epoch)
-  [Function `pending_stake_amount`](#0x2_staking_pool_pending_stake_amount)
-  [Function `pending_stake_withdraw_amount`](#0x2_staking_pool_pending_stake_withdraw_amount)
-  [Function `is_preactive_at_epoch`](#0x2_staking_pool_is_preactive_at_epoch)
-  [Function `get_sui_amount`](#0x2_staking_pool_get_sui_amount)
-  [Function `get_token_amount`](#0x2_staking_pool_get_token_amount)
-  [Function `initial_exchange_rate`](#0x2_staking_pool_initial_exchange_rate)
-  [Function `check_balance_invariants`](#0x2_staking_pool_check_balance_invariants)


<pre><code><b>use</b> <a href="">0x1::option</a>;
<b>use</b> <a href="balance.md#0x2_balance">0x2::balance</a>;
<b>use</b> <a href="coin.md#0x2_coin">0x2::coin</a>;
<b>use</b> <a href="epoch_time_lock.md#0x2_epoch_time_lock">0x2::epoch_time_lock</a>;
<b>use</b> <a href="locked_coin.md#0x2_locked_coin">0x2::locked_coin</a>;
<b>use</b> <a href="math.md#0x2_math">0x2::math</a>;
<b>use</b> <a href="object.md#0x2_object">0x2::object</a>;
<b>use</b> <a href="sui.md#0x2_sui">0x2::sui</a>;
<b>use</b> <a href="table.md#0x2_table">0x2::table</a>;
<b>use</b> <a href="transfer.md#0x2_transfer">0x2::transfer</a>;
<b>use</b> <a href="tx_context.md#0x2_tx_context">0x2::tx_context</a>;
</code></pre>



<a name="0x2_staking_pool_StakingPool"></a>

## Resource `StakingPool`

A staking pool embedded in each validator struct in the system state object.


<pre><code><b>struct</b> <a href="staking_pool.md#0x2_staking_pool_StakingPool">StakingPool</a> <b>has</b> store, key
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>id: <a href="object.md#0x2_object_UID">object::UID</a></code>
</dt>
<dd>

</dd>
<dt>
<code>activation_epoch: <a href="_Option">option::Option</a>&lt;u64&gt;</code>
</dt>
<dd>
 The epoch at which this pool became active.
 The value is <code>None</code> if the pool is pre-active and <code>Some(&lt;epoch_number&gt;)</code> if active or inactive.
</dd>
<dt>
<code>deactivation_epoch: <a href="_Option">option::Option</a>&lt;u64&gt;</code>
</dt>
<dd>
 The epoch at which this staking pool ceased to be active. <code>None</code> = {pre-active, active},
 <code>Some(&lt;epoch_number&gt;)</code> if in-active, and it was de-activated at epoch <code>&lt;epoch_number&gt;</code>.
</dd>
<dt>
<code>sui_balance: u64</code>
</dt>
<dd>
 The total number of SUI tokens in this pool, including the SUI in the rewards_pool, as well as in all the principal
 in the <code><a href="staking_pool.md#0x2_staking_pool_StakedSui">StakedSui</a></code> object, updated at epoch boundaries.
</dd>
<dt>
<code>rewards_pool: <a href="balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="sui.md#0x2_sui_SUI">sui::SUI</a>&gt;</code>
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
<code>exchange_rates: <a href="table.md#0x2_table_Table">table::Table</a>&lt;u64, <a href="staking_pool.md#0x2_staking_pool_PoolTokenExchangeRate">staking_pool::PoolTokenExchangeRate</a>&gt;</code>
</dt>
<dd>
 Exchange rate history of previous epochs. Key is the epoch number.
 The entries start from the <code>starting_epoch</code> of this pool and contain exchange rates at the beginning of each epoch,
 i.e., right after the rewards for the previous epoch have been deposited into the pool.
</dd>
<dt>
<code>pending_stake: u64</code>
</dt>
<dd>
 Pending stake amount for this epoch.
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
</dl>


</details>

<a name="0x2_staking_pool_PoolTokenExchangeRate"></a>

## Struct `PoolTokenExchangeRate`

Struct representing the exchange rate of the stake pool token to SUI.


<pre><code><b>struct</b> <a href="staking_pool.md#0x2_staking_pool_PoolTokenExchangeRate">PoolTokenExchangeRate</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>sui_amount: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>pool_token_amount: u64</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x2_staking_pool_StakedSui"></a>

## Resource `StakedSui`

A self-custodial object holding the staked SUI tokens.


<pre><code><b>struct</b> <a href="staking_pool.md#0x2_staking_pool_StakedSui">StakedSui</a> <b>has</b> key
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>id: <a href="object.md#0x2_object_UID">object::UID</a></code>
</dt>
<dd>

</dd>
<dt>
<code>pool_id: <a href="object.md#0x2_object_ID">object::ID</a></code>
</dt>
<dd>
 ID of the staking pool we are staking with.
</dd>
<dt>
<code>validator_address: <b>address</b></code>
</dt>
<dd>

</dd>
<dt>
<code>stake_activation_epoch: u64</code>
</dt>
<dd>
 The epoch at which the stake becomes active.
</dd>
<dt>
<code>principal: <a href="balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="sui.md#0x2_sui_SUI">sui::SUI</a>&gt;</code>
</dt>
<dd>
 The staked SUI tokens.
</dd>
<dt>
<code>sui_token_lock: <a href="_Option">option::Option</a>&lt;<a href="epoch_time_lock.md#0x2_epoch_time_lock_EpochTimeLock">epoch_time_lock::EpochTimeLock</a>&gt;</code>
</dt>
<dd>
 If the stake comes from a Coin<SUI>, this field is None. If it comes from a LockedCoin<SUI>, this
 field will record the original lock expiration epoch, to be used when unstaking.
</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0x2_staking_pool_EActivationOfInactivePool"></a>



<pre><code><b>const</b> <a href="staking_pool.md#0x2_staking_pool_EActivationOfInactivePool">EActivationOfInactivePool</a>: u64 = 16;
</code></pre>



<a name="0x2_staking_pool_EDeactivationOfInactivePool"></a>



<pre><code><b>const</b> <a href="staking_pool.md#0x2_staking_pool_EDeactivationOfInactivePool">EDeactivationOfInactivePool</a>: u64 = 11;
</code></pre>



<a name="0x2_staking_pool_EDelegationOfZeroSui"></a>



<pre><code><b>const</b> <a href="staking_pool.md#0x2_staking_pool_EDelegationOfZeroSui">EDelegationOfZeroSui</a>: u64 = 17;
</code></pre>



<a name="0x2_staking_pool_EDelegationToInactivePool"></a>



<pre><code><b>const</b> <a href="staking_pool.md#0x2_staking_pool_EDelegationToInactivePool">EDelegationToInactivePool</a>: u64 = 10;
</code></pre>



<a name="0x2_staking_pool_EDestroyNonzeroBalance"></a>



<pre><code><b>const</b> <a href="staking_pool.md#0x2_staking_pool_EDestroyNonzeroBalance">EDestroyNonzeroBalance</a>: u64 = 5;
</code></pre>



<a name="0x2_staking_pool_EIncompatibleStakedSui"></a>



<pre><code><b>const</b> <a href="staking_pool.md#0x2_staking_pool_EIncompatibleStakedSui">EIncompatibleStakedSui</a>: u64 = 12;
</code></pre>



<a name="0x2_staking_pool_EInsufficientPoolTokenBalance"></a>



<pre><code><b>const</b> <a href="staking_pool.md#0x2_staking_pool_EInsufficientPoolTokenBalance">EInsufficientPoolTokenBalance</a>: u64 = 0;
</code></pre>



<a name="0x2_staking_pool_EInsufficientRewardsPoolBalance"></a>



<pre><code><b>const</b> <a href="staking_pool.md#0x2_staking_pool_EInsufficientRewardsPoolBalance">EInsufficientRewardsPoolBalance</a>: u64 = 4;
</code></pre>



<a name="0x2_staking_pool_EInsufficientSuiTokenBalance"></a>



<pre><code><b>const</b> <a href="staking_pool.md#0x2_staking_pool_EInsufficientSuiTokenBalance">EInsufficientSuiTokenBalance</a>: u64 = 3;
</code></pre>



<a name="0x2_staking_pool_EPendingDelegationDoesNotExist"></a>



<pre><code><b>const</b> <a href="staking_pool.md#0x2_staking_pool_EPendingDelegationDoesNotExist">EPendingDelegationDoesNotExist</a>: u64 = 8;
</code></pre>



<a name="0x2_staking_pool_EPoolAlreadyActive"></a>



<pre><code><b>const</b> <a href="staking_pool.md#0x2_staking_pool_EPoolAlreadyActive">EPoolAlreadyActive</a>: u64 = 14;
</code></pre>



<a name="0x2_staking_pool_EPoolNotPreactive"></a>



<pre><code><b>const</b> <a href="staking_pool.md#0x2_staking_pool_EPoolNotPreactive">EPoolNotPreactive</a>: u64 = 15;
</code></pre>



<a name="0x2_staking_pool_ETokenBalancesDoNotMatchExchangeRate"></a>



<pre><code><b>const</b> <a href="staking_pool.md#0x2_staking_pool_ETokenBalancesDoNotMatchExchangeRate">ETokenBalancesDoNotMatchExchangeRate</a>: u64 = 9;
</code></pre>



<a name="0x2_staking_pool_ETokenTimeLockIsSome"></a>



<pre><code><b>const</b> <a href="staking_pool.md#0x2_staking_pool_ETokenTimeLockIsSome">ETokenTimeLockIsSome</a>: u64 = 6;
</code></pre>



<a name="0x2_staking_pool_EWithdrawAmountCannotBeZero"></a>



<pre><code><b>const</b> <a href="staking_pool.md#0x2_staking_pool_EWithdrawAmountCannotBeZero">EWithdrawAmountCannotBeZero</a>: u64 = 2;
</code></pre>



<a name="0x2_staking_pool_EWithdrawalInSameEpoch"></a>



<pre><code><b>const</b> <a href="staking_pool.md#0x2_staking_pool_EWithdrawalInSameEpoch">EWithdrawalInSameEpoch</a>: u64 = 13;
</code></pre>



<a name="0x2_staking_pool_EWrongDelegation"></a>



<pre><code><b>const</b> <a href="staking_pool.md#0x2_staking_pool_EWrongDelegation">EWrongDelegation</a>: u64 = 7;
</code></pre>



<a name="0x2_staking_pool_EWrongPool"></a>



<pre><code><b>const</b> <a href="staking_pool.md#0x2_staking_pool_EWrongPool">EWrongPool</a>: u64 = 1;
</code></pre>



<a name="0x2_staking_pool_new"></a>

## Function `new`

Create a new, empty staking pool.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_new">new</a>(ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="staking_pool.md#0x2_staking_pool_StakingPool">staking_pool::StakingPool</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_new">new</a>(ctx: &<b>mut</b> TxContext) : <a href="staking_pool.md#0x2_staking_pool_StakingPool">StakingPool</a> {
    <b>let</b> exchange_rates = <a href="table.md#0x2_table_new">table::new</a>(ctx);
    <a href="staking_pool.md#0x2_staking_pool_StakingPool">StakingPool</a> {
        id: <a href="object.md#0x2_object_new">object::new</a>(ctx),
        activation_epoch: <a href="_none">option::none</a>(),
        deactivation_epoch: <a href="_none">option::none</a>(),
        sui_balance: 0,
        rewards_pool: <a href="balance.md#0x2_balance_zero">balance::zero</a>(),
        pool_token_balance: 0,
        exchange_rates,
        pending_stake: 0,
        pending_total_sui_withdraw: 0,
        pending_pool_token_withdraw: 0,
    }
}
</code></pre>



</details>

<a name="0x2_staking_pool_request_add_stake"></a>

## Function `request_add_stake`

Request to stake to a staking pool. The stake starts counting at the beginning of the next epoch,


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_request_add_stake">request_add_stake</a>(pool: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_StakingPool">staking_pool::StakingPool</a>, stake: <a href="balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="sui.md#0x2_sui_SUI">sui::SUI</a>&gt;, sui_token_lock: <a href="_Option">option::Option</a>&lt;<a href="epoch_time_lock.md#0x2_epoch_time_lock_EpochTimeLock">epoch_time_lock::EpochTimeLock</a>&gt;, validator_address: <b>address</b>, staker: <b>address</b>, stake_activation_epoch: u64, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_request_add_stake">request_add_stake</a>(
    pool: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_StakingPool">StakingPool</a>,
    stake: Balance&lt;SUI&gt;,
    sui_token_lock: Option&lt;EpochTimeLock&gt;,
    validator_address: <b>address</b>,
    staker: <b>address</b>,
    stake_activation_epoch: u64,
    ctx: &<b>mut</b> TxContext
) {
    <b>let</b> sui_amount = <a href="balance.md#0x2_balance_value">balance::value</a>(&stake);
    <b>assert</b>!(!<a href="staking_pool.md#0x2_staking_pool_is_inactive">is_inactive</a>(pool), <a href="staking_pool.md#0x2_staking_pool_EDelegationToInactivePool">EDelegationToInactivePool</a>);
    <b>assert</b>!(sui_amount &gt; 0, <a href="staking_pool.md#0x2_staking_pool_EDelegationOfZeroSui">EDelegationOfZeroSui</a>);
    <b>let</b> staked_sui = <a href="staking_pool.md#0x2_staking_pool_StakedSui">StakedSui</a> {
        id: <a href="object.md#0x2_object_new">object::new</a>(ctx),
        pool_id: <a href="object.md#0x2_object_id">object::id</a>(pool),
        validator_address,
        stake_activation_epoch,
        principal: stake,
        sui_token_lock,
    };
    pool.pending_stake = pool.pending_stake + sui_amount;
    <a href="transfer.md#0x2_transfer_transfer">transfer::transfer</a>(staked_sui, staker);
}
</code></pre>



</details>

<a name="0x2_staking_pool_request_withdraw_stake"></a>

## Function `request_withdraw_stake`

Request to withdraw <code>principal_withdraw_amount</code> of stake plus rewards from a staking pool.
This amount of principal and corresponding rewards in SUI are withdrawn and transferred to the staker.
A proportional amount of pool tokens is burnt.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_request_withdraw_stake">request_withdraw_stake</a>(pool: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_StakingPool">staking_pool::StakingPool</a>, staked_sui: <a href="staking_pool.md#0x2_staking_pool_StakedSui">staking_pool::StakedSui</a>, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_request_withdraw_stake">request_withdraw_stake</a>(
    pool: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_StakingPool">StakingPool</a>,
    staked_sui: <a href="staking_pool.md#0x2_staking_pool_StakedSui">StakedSui</a>,
    ctx: &<b>mut</b> TxContext
) : u64 {
    <b>let</b> (pool_token_withdraw_amount, principal_withdraw, time_lock) =
        <a href="staking_pool.md#0x2_staking_pool_withdraw_from_principal">withdraw_from_principal</a>(pool, staked_sui);
    <b>let</b> staker = <a href="tx_context.md#0x2_tx_context_sender">tx_context::sender</a>(ctx);
    <b>let</b> principal_withdraw_amount = <a href="balance.md#0x2_balance_value">balance::value</a>(&principal_withdraw);

    <b>let</b> rewards_withdraw = <a href="staking_pool.md#0x2_staking_pool_withdraw_rewards_and_burn_pool_tokens">withdraw_rewards_and_burn_pool_tokens</a>(
        pool, principal_withdraw_amount, pool_token_withdraw_amount, <a href="tx_context.md#0x2_tx_context_epoch">tx_context::epoch</a>(ctx)
    );
    <b>let</b> total_sui_withdraw_amount = principal_withdraw_amount + <a href="balance.md#0x2_balance_value">balance::value</a>(&rewards_withdraw);

    pool.pending_total_sui_withdraw = pool.pending_total_sui_withdraw + total_sui_withdraw_amount;
    pool.pending_pool_token_withdraw = pool.pending_pool_token_withdraw + pool_token_withdraw_amount;

    // If the pool is inactive, we immediately process the withdrawal.
    <b>if</b> (<a href="staking_pool.md#0x2_staking_pool_is_inactive">is_inactive</a>(pool)) <a href="staking_pool.md#0x2_staking_pool_process_pending_stake_withdraw">process_pending_stake_withdraw</a>(pool);

    // TODO: implement withdraw bonding period here.
    <b>if</b> (<a href="_is_some">option::is_some</a>(&time_lock)) {
        <a href="locked_coin.md#0x2_locked_coin_new_from_balance">locked_coin::new_from_balance</a>(principal_withdraw, <a href="_destroy_some">option::destroy_some</a>(time_lock), staker, ctx);
        <b>if</b> (<a href="balance.md#0x2_balance_value">balance::value</a>(&rewards_withdraw) &gt; 0) {
            <a href="transfer.md#0x2_transfer_transfer">transfer::transfer</a>(<a href="coin.md#0x2_coin_from_balance">coin::from_balance</a>(rewards_withdraw, ctx), staker);
        } <b>else</b> {
            <a href="balance.md#0x2_balance_destroy_zero">balance::destroy_zero</a>(rewards_withdraw);
        }
    } <b>else</b> {
        <a href="balance.md#0x2_balance_join">balance::join</a>(&<b>mut</b> principal_withdraw, rewards_withdraw);
        <a href="transfer.md#0x2_transfer_transfer">transfer::transfer</a>(<a href="coin.md#0x2_coin_from_balance">coin::from_balance</a>(principal_withdraw, ctx), staker);
        <a href="_destroy_none">option::destroy_none</a>(time_lock);
    };
    total_sui_withdraw_amount

    // payment_amount
}
</code></pre>



</details>

<a name="0x2_staking_pool_withdraw_from_principal"></a>

## Function `withdraw_from_principal`

Withdraw the principal SUI stored in the StakedSui object, and calculate the corresponding amount of pool
tokens using exchange rate at stake epoch.
Returns values are amount of pool tokens withdrawn, withdrawn principal portion of SUI, and its
time lock if applicable.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_withdraw_from_principal">withdraw_from_principal</a>(pool: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_StakingPool">staking_pool::StakingPool</a>, staked_sui: <a href="staking_pool.md#0x2_staking_pool_StakedSui">staking_pool::StakedSui</a>): (u64, <a href="balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="sui.md#0x2_sui_SUI">sui::SUI</a>&gt;, <a href="_Option">option::Option</a>&lt;<a href="epoch_time_lock.md#0x2_epoch_time_lock_EpochTimeLock">epoch_time_lock::EpochTimeLock</a>&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_withdraw_from_principal">withdraw_from_principal</a>(
    pool: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_StakingPool">StakingPool</a>,
    staked_sui: <a href="staking_pool.md#0x2_staking_pool_StakedSui">StakedSui</a>,
) : (u64, Balance&lt;SUI&gt;, Option&lt;EpochTimeLock&gt;) {

    // Check that the stake information matches the pool.
    <b>assert</b>!(staked_sui.pool_id == <a href="object.md#0x2_object_id">object::id</a>(pool), <a href="staking_pool.md#0x2_staking_pool_EWrongPool">EWrongPool</a>);

    <b>let</b> exchange_rate_at_staking_epoch = <a href="staking_pool.md#0x2_staking_pool_pool_token_exchange_rate_at_epoch">pool_token_exchange_rate_at_epoch</a>(pool, staked_sui.stake_activation_epoch);
    <b>let</b> (principal_withdraw, time_lock) = <a href="staking_pool.md#0x2_staking_pool_unwrap_staked_sui">unwrap_staked_sui</a>(staked_sui);
    <b>let</b> pool_token_withdraw_amount = <a href="staking_pool.md#0x2_staking_pool_get_token_amount">get_token_amount</a>(&exchange_rate_at_staking_epoch, <a href="balance.md#0x2_balance_value">balance::value</a>(&principal_withdraw));

    (
        pool_token_withdraw_amount,
        principal_withdraw,
        time_lock
    )
}
</code></pre>



</details>

<a name="0x2_staking_pool_unwrap_staked_sui"></a>

## Function `unwrap_staked_sui`



<pre><code><b>fun</b> <a href="staking_pool.md#0x2_staking_pool_unwrap_staked_sui">unwrap_staked_sui</a>(staked_sui: <a href="staking_pool.md#0x2_staking_pool_StakedSui">staking_pool::StakedSui</a>): (<a href="balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="sui.md#0x2_sui_SUI">sui::SUI</a>&gt;, <a href="_Option">option::Option</a>&lt;<a href="epoch_time_lock.md#0x2_epoch_time_lock_EpochTimeLock">epoch_time_lock::EpochTimeLock</a>&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="staking_pool.md#0x2_staking_pool_unwrap_staked_sui">unwrap_staked_sui</a>(staked_sui: <a href="staking_pool.md#0x2_staking_pool_StakedSui">StakedSui</a>): (Balance&lt;SUI&gt;, Option&lt;EpochTimeLock&gt;) {
    <b>let</b> <a href="staking_pool.md#0x2_staking_pool_StakedSui">StakedSui</a> {
        id,
        pool_id: _,
        validator_address: _,
        stake_activation_epoch: _,
        principal,
        sui_token_lock
    } = staked_sui;
    <a href="object.md#0x2_object_delete">object::delete</a>(id);
    (principal, sui_token_lock)
}
</code></pre>



</details>

<a name="0x2_staking_pool_deposit_rewards"></a>

## Function `deposit_rewards`

Called at epoch advancement times to add rewards (in SUI) to the staking pool.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_deposit_rewards">deposit_rewards</a>(pool: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_StakingPool">staking_pool::StakingPool</a>, rewards: <a href="balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="sui.md#0x2_sui_SUI">sui::SUI</a>&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_deposit_rewards">deposit_rewards</a>(pool: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_StakingPool">StakingPool</a>, rewards: Balance&lt;SUI&gt;) {
    pool.sui_balance = pool.sui_balance + <a href="balance.md#0x2_balance_value">balance::value</a>(&rewards);
    <a href="balance.md#0x2_balance_join">balance::join</a>(&<b>mut</b> pool.rewards_pool, rewards);
}
</code></pre>



</details>

<a name="0x2_staking_pool_process_pending_stakes_and_withdraws"></a>

## Function `process_pending_stakes_and_withdraws`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_process_pending_stakes_and_withdraws">process_pending_stakes_and_withdraws</a>(pool: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_StakingPool">staking_pool::StakingPool</a>, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_process_pending_stakes_and_withdraws">process_pending_stakes_and_withdraws</a>(pool: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_StakingPool">StakingPool</a>, ctx: &<b>mut</b> TxContext) {
    <b>let</b> new_epoch = <a href="tx_context.md#0x2_tx_context_epoch">tx_context::epoch</a>(ctx) + 1;
    <a href="staking_pool.md#0x2_staking_pool_process_pending_stake_withdraw">process_pending_stake_withdraw</a>(pool);
    <a href="staking_pool.md#0x2_staking_pool_process_pending_stake">process_pending_stake</a>(pool);
    <a href="table.md#0x2_table_add">table::add</a>(
        &<b>mut</b> pool.exchange_rates,
        new_epoch,
        <a href="staking_pool.md#0x2_staking_pool_PoolTokenExchangeRate">PoolTokenExchangeRate</a> { sui_amount: pool.sui_balance, pool_token_amount: pool.pool_token_balance },
    );
    <a href="staking_pool.md#0x2_staking_pool_check_balance_invariants">check_balance_invariants</a>(pool, new_epoch);
}
</code></pre>



</details>

<a name="0x2_staking_pool_process_pending_stake_withdraw"></a>

## Function `process_pending_stake_withdraw`

Called at epoch boundaries to process pending stake withdraws requested during the epoch.
Also called immediately upon withdrawal if the pool is inactive.


<pre><code><b>fun</b> <a href="staking_pool.md#0x2_staking_pool_process_pending_stake_withdraw">process_pending_stake_withdraw</a>(pool: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_StakingPool">staking_pool::StakingPool</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="staking_pool.md#0x2_staking_pool_process_pending_stake_withdraw">process_pending_stake_withdraw</a>(pool: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_StakingPool">StakingPool</a>) {
    pool.sui_balance = pool.sui_balance - pool.pending_total_sui_withdraw;
    pool.pool_token_balance = pool.pool_token_balance - pool.pending_pool_token_withdraw;
    pool.pending_total_sui_withdraw = 0;
    pool.pending_pool_token_withdraw = 0;
}
</code></pre>



</details>

<a name="0x2_staking_pool_process_pending_stake"></a>

## Function `process_pending_stake`

Called at epoch boundaries to process the pending stake.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_process_pending_stake">process_pending_stake</a>(pool: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_StakingPool">staking_pool::StakingPool</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_process_pending_stake">process_pending_stake</a>(pool: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_StakingPool">StakingPool</a>) {
    // Use the most up <b>to</b> date exchange rate <b>with</b> the rewards deposited and withdraws effectuated.
    <b>let</b> latest_exchange_rate =
        <a href="staking_pool.md#0x2_staking_pool_PoolTokenExchangeRate">PoolTokenExchangeRate</a> { sui_amount: pool.sui_balance, pool_token_amount: pool.pool_token_balance };
    pool.sui_balance = pool.sui_balance + pool.pending_stake;
    pool.pool_token_balance = <a href="staking_pool.md#0x2_staking_pool_get_token_amount">get_token_amount</a>(&latest_exchange_rate, pool.sui_balance);
    pool.pending_stake = 0;
}
</code></pre>



</details>

<a name="0x2_staking_pool_withdraw_rewards_and_burn_pool_tokens"></a>

## Function `withdraw_rewards_and_burn_pool_tokens`

This function does the following:
1. Calculates the total amount of SUI (including principal and rewards) that the provided pool tokens represent
at the current exchange rate.
2. Using the above number and the given <code>principal_withdraw_amount</code>, calculates the rewards portion of the
stake we should withdraw.
3. Withdraws the rewards portion from the rewards pool at the current exchange rate. We only withdraw the rewards
portion because the principal portion was already taken out of the staker's self custodied StakedSui.
4. Since SUI tokens are withdrawn, we need to burn the corresponding pool tokens to keep the exchange rate the same.


<pre><code><b>fun</b> <a href="staking_pool.md#0x2_staking_pool_withdraw_rewards_and_burn_pool_tokens">withdraw_rewards_and_burn_pool_tokens</a>(pool: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_StakingPool">staking_pool::StakingPool</a>, principal_withdraw_amount: u64, pool_token_withdraw_amount: u64, epoch: u64): <a href="balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="sui.md#0x2_sui_SUI">sui::SUI</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="staking_pool.md#0x2_staking_pool_withdraw_rewards_and_burn_pool_tokens">withdraw_rewards_and_burn_pool_tokens</a>(
    pool: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_StakingPool">StakingPool</a>,
    principal_withdraw_amount: u64,
    pool_token_withdraw_amount: u64,
    epoch: u64,
) : Balance&lt;SUI&gt; {
    <b>let</b> exchange_rate = <a href="staking_pool.md#0x2_staking_pool_pool_token_exchange_rate_at_epoch">pool_token_exchange_rate_at_epoch</a>(pool, epoch);
    <b>let</b> total_sui_withdraw_amount = <a href="staking_pool.md#0x2_staking_pool_get_sui_amount">get_sui_amount</a>(&exchange_rate, pool_token_withdraw_amount);
    <b>let</b> reward_withdraw_amount =
        <b>if</b> (total_sui_withdraw_amount &gt;= principal_withdraw_amount)
            total_sui_withdraw_amount - principal_withdraw_amount
        <b>else</b> 0;
    // This may happen when we are withdrawing everything from the pool and
    // the rewards pool <a href="balance.md#0x2_balance">balance</a> may be less than reward_withdraw_amount.
    // TODO: FIGURE OUT EXACTLY WHY THIS CAN HAPPEN.
    reward_withdraw_amount = <a href="math.md#0x2_math_min">math::min</a>(reward_withdraw_amount, <a href="balance.md#0x2_balance_value">balance::value</a>(&pool.rewards_pool));
    <a href="balance.md#0x2_balance_split">balance::split</a>(&<b>mut</b> pool.rewards_pool, reward_withdraw_amount)
}
</code></pre>



</details>

<a name="0x2_staking_pool_activate_staking_pool"></a>

## Function `activate_staking_pool`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_activate_staking_pool">activate_staking_pool</a>(pool: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_StakingPool">staking_pool::StakingPool</a>, activation_epoch: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_activate_staking_pool">activate_staking_pool</a>(pool: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_StakingPool">StakingPool</a>, activation_epoch: u64) {
    // Add the initial exchange rate <b>to</b> the <a href="table.md#0x2_table">table</a>.
    <a href="table.md#0x2_table_add">table::add</a>(
        &<b>mut</b> pool.exchange_rates,
        activation_epoch,
        <a href="staking_pool.md#0x2_staking_pool_initial_exchange_rate">initial_exchange_rate</a>()
    );
    // Check that the pool is preactive and not inactive.
    <b>assert</b>!(<a href="staking_pool.md#0x2_staking_pool_is_preactive">is_preactive</a>(pool), <a href="staking_pool.md#0x2_staking_pool_EPoolAlreadyActive">EPoolAlreadyActive</a>);
    <b>assert</b>!(!<a href="staking_pool.md#0x2_staking_pool_is_inactive">is_inactive</a>(pool), <a href="staking_pool.md#0x2_staking_pool_EActivationOfInactivePool">EActivationOfInactivePool</a>);
    // Fill in the active epoch.
    <a href="_fill">option::fill</a>(&<b>mut</b> pool.activation_epoch, activation_epoch);
}
</code></pre>



</details>

<a name="0x2_staking_pool_request_withdraw_stake_preactive"></a>

## Function `request_withdraw_stake_preactive`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_request_withdraw_stake_preactive">request_withdraw_stake_preactive</a>(pool: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_StakingPool">staking_pool::StakingPool</a>, staked_sui: <a href="staking_pool.md#0x2_staking_pool_StakedSui">staking_pool::StakedSui</a>, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_request_withdraw_stake_preactive">request_withdraw_stake_preactive</a>(
    pool: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_StakingPool">StakingPool</a>,
    staked_sui: <a href="staking_pool.md#0x2_staking_pool_StakedSui">StakedSui</a>,
    ctx: &<b>mut</b> TxContext
) : u64 {
    // Check that the stake information matches the pool.
    <b>assert</b>!(staked_sui.pool_id == <a href="object.md#0x2_object_id">object::id</a>(pool), <a href="staking_pool.md#0x2_staking_pool_EWrongPool">EWrongPool</a>);

    <b>assert</b>!(<a href="staking_pool.md#0x2_staking_pool_is_preactive">is_preactive</a>(pool), <a href="staking_pool.md#0x2_staking_pool_EPoolNotPreactive">EPoolNotPreactive</a>);

    <b>let</b> staker = <a href="tx_context.md#0x2_tx_context_sender">tx_context::sender</a>(ctx);

    <b>let</b> (principal, time_lock) = <a href="staking_pool.md#0x2_staking_pool_unwrap_staked_sui">unwrap_staked_sui</a>(staked_sui);
    <b>let</b> withdraw_amount = <a href="balance.md#0x2_balance_value">balance::value</a>(&principal);
    pool.sui_balance = pool.sui_balance - withdraw_amount;
    pool.pool_token_balance = pool.pool_token_balance - withdraw_amount;

    // TODO: consider sharing code <b>with</b> `request_withdraw_stake`
    <b>if</b> (<a href="_is_some">option::is_some</a>(&time_lock)) {
        <a href="locked_coin.md#0x2_locked_coin_new_from_balance">locked_coin::new_from_balance</a>(principal, <a href="_destroy_some">option::destroy_some</a>(time_lock), staker, ctx);
    } <b>else</b> {
        <a href="transfer.md#0x2_transfer_transfer">transfer::transfer</a>(<a href="coin.md#0x2_coin_from_balance">coin::from_balance</a>(principal, ctx), staker);
        <a href="_destroy_none">option::destroy_none</a>(time_lock);
    };
    withdraw_amount
}
</code></pre>



</details>

<a name="0x2_staking_pool_deactivate_staking_pool"></a>

## Function `deactivate_staking_pool`

Deactivate a staking pool by setting the <code>deactivation_epoch</code>. After
this pool deactivation, the pool stops earning rewards. Only stake
withdraws can be made to the pool.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_deactivate_staking_pool">deactivate_staking_pool</a>(pool: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_StakingPool">staking_pool::StakingPool</a>, deactivation_epoch: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_deactivate_staking_pool">deactivate_staking_pool</a>(pool: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_StakingPool">StakingPool</a>, deactivation_epoch: u64) {
    // We can't deactivate an already deactivated pool.
    <b>assert</b>!(!<a href="staking_pool.md#0x2_staking_pool_is_inactive">is_inactive</a>(pool), <a href="staking_pool.md#0x2_staking_pool_EDeactivationOfInactivePool">EDeactivationOfInactivePool</a>);
    pool.deactivation_epoch = <a href="_some">option::some</a>(deactivation_epoch);
}
</code></pre>



</details>

<a name="0x2_staking_pool_sui_balance"></a>

## Function `sui_balance`



<pre><code><b>public</b> <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_sui_balance">sui_balance</a>(pool: &<a href="staking_pool.md#0x2_staking_pool_StakingPool">staking_pool::StakingPool</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_sui_balance">sui_balance</a>(pool: &<a href="staking_pool.md#0x2_staking_pool_StakingPool">StakingPool</a>): u64 { pool.sui_balance }
</code></pre>



</details>

<a name="0x2_staking_pool_pool_id"></a>

## Function `pool_id`



<pre><code><b>public</b> <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_pool_id">pool_id</a>(staked_sui: &<a href="staking_pool.md#0x2_staking_pool_StakedSui">staking_pool::StakedSui</a>): <a href="object.md#0x2_object_ID">object::ID</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_pool_id">pool_id</a>(staked_sui: &<a href="staking_pool.md#0x2_staking_pool_StakedSui">StakedSui</a>): ID { staked_sui.pool_id }
</code></pre>



</details>

<a name="0x2_staking_pool_staked_sui_amount"></a>

## Function `staked_sui_amount`



<pre><code><b>public</b> <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_staked_sui_amount">staked_sui_amount</a>(staked_sui: &<a href="staking_pool.md#0x2_staking_pool_StakedSui">staking_pool::StakedSui</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_staked_sui_amount">staked_sui_amount</a>(staked_sui: &<a href="staking_pool.md#0x2_staking_pool_StakedSui">StakedSui</a>): u64 { <a href="balance.md#0x2_balance_value">balance::value</a>(&staked_sui.principal) }
</code></pre>



</details>

<a name="0x2_staking_pool_stake_activation_epoch"></a>

## Function `stake_activation_epoch`



<pre><code><b>public</b> <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_stake_activation_epoch">stake_activation_epoch</a>(staked_sui: &<a href="staking_pool.md#0x2_staking_pool_StakedSui">staking_pool::StakedSui</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_stake_activation_epoch">stake_activation_epoch</a>(staked_sui: &<a href="staking_pool.md#0x2_staking_pool_StakedSui">StakedSui</a>): u64 {
    staked_sui.stake_activation_epoch
}
</code></pre>



</details>

<a name="0x2_staking_pool_is_preactive"></a>

## Function `is_preactive`

Returns true if the input staking pool is preactive.


<pre><code><b>public</b> <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_is_preactive">is_preactive</a>(pool: &<a href="staking_pool.md#0x2_staking_pool_StakingPool">staking_pool::StakingPool</a>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_is_preactive">is_preactive</a>(pool: &<a href="staking_pool.md#0x2_staking_pool_StakingPool">StakingPool</a>): bool{
    <a href="_is_none">option::is_none</a>(&pool.activation_epoch)
}
</code></pre>



</details>

<a name="0x2_staking_pool_is_inactive"></a>

## Function `is_inactive`

Returns true if the input staking pool is inactive.


<pre><code><b>public</b> <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_is_inactive">is_inactive</a>(pool: &<a href="staking_pool.md#0x2_staking_pool_StakingPool">staking_pool::StakingPool</a>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_is_inactive">is_inactive</a>(pool: &<a href="staking_pool.md#0x2_staking_pool_StakingPool">StakingPool</a>): bool {
    <a href="_is_some">option::is_some</a>(&pool.deactivation_epoch)
}
</code></pre>



</details>

<a name="0x2_staking_pool_split"></a>

## Function `split`

Split StakedSui <code>self</code> to two parts, one with principal <code>split_amount</code>,
and the remaining principal is left in <code>self</code>.
All the other parameters of the StakedSui like <code>stake_activation_epoch</code> or <code>pool_id</code> remain the same.


<pre><code><b>public</b> <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_split">split</a>(self: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_StakedSui">staking_pool::StakedSui</a>, split_amount: u64, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="staking_pool.md#0x2_staking_pool_StakedSui">staking_pool::StakedSui</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_split">split</a>(self: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_StakedSui">StakedSui</a>, split_amount: u64, ctx: &<b>mut</b> TxContext): <a href="staking_pool.md#0x2_staking_pool_StakedSui">StakedSui</a> {
    <a href="staking_pool.md#0x2_staking_pool_StakedSui">StakedSui</a> {
        id: <a href="object.md#0x2_object_new">object::new</a>(ctx),
        pool_id: self.pool_id,
        validator_address: self.validator_address,
        stake_activation_epoch: self.stake_activation_epoch,
        principal: <a href="balance.md#0x2_balance_split">balance::split</a>(&<b>mut</b> self.principal, split_amount),
        sui_token_lock: self.sui_token_lock,
    }
}
</code></pre>



</details>

<a name="0x2_staking_pool_split_staked_sui"></a>

## Function `split_staked_sui`

Split the given StakedSui to the two parts, one with principal <code>split_amount</code>,
transfer the newly split part to the sender address.


<pre><code><b>public</b> entry <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_split_staked_sui">split_staked_sui</a>(c: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_StakedSui">staking_pool::StakedSui</a>, split_amount: u64, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_split_staked_sui">split_staked_sui</a>(c: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_StakedSui">StakedSui</a>, split_amount: u64, ctx: &<b>mut</b> TxContext) {
    <a href="transfer.md#0x2_transfer_transfer">transfer::transfer</a>(<a href="staking_pool.md#0x2_staking_pool_split">split</a>(c, split_amount, ctx), <a href="tx_context.md#0x2_tx_context_sender">tx_context::sender</a>(ctx));
}
</code></pre>



</details>

<a name="0x2_staking_pool_join_staked_sui"></a>

## Function `join_staked_sui`

Consume the staked sui <code>other</code> and add its value to <code>self</code>.
Aborts if some of the staking parameters are incompatible (pool id, stake activation epoch, etc.)


<pre><code><b>public</b> entry <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_join_staked_sui">join_staked_sui</a>(self: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_StakedSui">staking_pool::StakedSui</a>, other: <a href="staking_pool.md#0x2_staking_pool_StakedSui">staking_pool::StakedSui</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_join_staked_sui">join_staked_sui</a>(self: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_StakedSui">StakedSui</a>, other: <a href="staking_pool.md#0x2_staking_pool_StakedSui">StakedSui</a>) {
    <b>assert</b>!(<a href="staking_pool.md#0x2_staking_pool_is_equal_staking_metadata">is_equal_staking_metadata</a>(self, &other), <a href="staking_pool.md#0x2_staking_pool_EIncompatibleStakedSui">EIncompatibleStakedSui</a>);
    <b>let</b> <a href="staking_pool.md#0x2_staking_pool_StakedSui">StakedSui</a> {
        id,
        pool_id: _,
        validator_address: _,
        stake_activation_epoch: _,
        principal,
        sui_token_lock
    } = other;

    <a href="object.md#0x2_object_delete">object::delete</a>(id);
    <b>if</b> (<a href="_is_some">option::is_some</a>(&sui_token_lock)) {
        <a href="epoch_time_lock.md#0x2_epoch_time_lock_destroy_unchecked">epoch_time_lock::destroy_unchecked</a>(<a href="_destroy_some">option::destroy_some</a>(sui_token_lock));
    } <b>else</b> {
        <a href="_destroy_none">option::destroy_none</a>(sui_token_lock);
    };
    <a href="balance.md#0x2_balance_join">balance::join</a>(&<b>mut</b> self.principal, principal);
}
</code></pre>



</details>

<a name="0x2_staking_pool_is_equal_staking_metadata"></a>

## Function `is_equal_staking_metadata`

Returns true if all the staking parameters of the staked sui except the principal are identical


<pre><code><b>public</b> <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_is_equal_staking_metadata">is_equal_staking_metadata</a>(self: &<a href="staking_pool.md#0x2_staking_pool_StakedSui">staking_pool::StakedSui</a>, other: &<a href="staking_pool.md#0x2_staking_pool_StakedSui">staking_pool::StakedSui</a>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_is_equal_staking_metadata">is_equal_staking_metadata</a>(self: &<a href="staking_pool.md#0x2_staking_pool_StakedSui">StakedSui</a>, other: &<a href="staking_pool.md#0x2_staking_pool_StakedSui">StakedSui</a>): bool {
    <b>if</b> ((self.pool_id != other.pool_id) ||
        (self.validator_address != other.validator_address) ||
        (self.stake_activation_epoch != other.stake_activation_epoch)) {
        <b>return</b> <b>false</b>
    };
    <b>if</b> (<a href="_is_none">option::is_none</a>(&self.sui_token_lock) && <a href="_is_none">option::is_none</a>(&other.sui_token_lock)) {
        <b>return</b> <b>true</b>
    };
    <b>if</b> (<a href="_is_some">option::is_some</a>(&self.sui_token_lock) && <a href="_is_some">option::is_some</a>(&other.sui_token_lock)) {
        <a href="epoch_time_lock.md#0x2_epoch_time_lock_epoch">epoch_time_lock::epoch</a>(<a href="_borrow">option::borrow</a>(&self.sui_token_lock)) ==
            <a href="epoch_time_lock.md#0x2_epoch_time_lock_epoch">epoch_time_lock::epoch</a>(<a href="_borrow">option::borrow</a>(&other.sui_token_lock))
    } <b>else</b>
        <b>false</b> // locked <a href="coin.md#0x2_coin">coin</a> in one and unlocked in another
}
</code></pre>



</details>

<a name="0x2_staking_pool_pool_token_exchange_rate_at_epoch"></a>

## Function `pool_token_exchange_rate_at_epoch`



<pre><code><b>public</b> <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_pool_token_exchange_rate_at_epoch">pool_token_exchange_rate_at_epoch</a>(pool: &<a href="staking_pool.md#0x2_staking_pool_StakingPool">staking_pool::StakingPool</a>, epoch: u64): <a href="staking_pool.md#0x2_staking_pool_PoolTokenExchangeRate">staking_pool::PoolTokenExchangeRate</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_pool_token_exchange_rate_at_epoch">pool_token_exchange_rate_at_epoch</a>(pool: &<a href="staking_pool.md#0x2_staking_pool_StakingPool">StakingPool</a>, epoch: u64): <a href="staking_pool.md#0x2_staking_pool_PoolTokenExchangeRate">PoolTokenExchangeRate</a> {
    // If the pool is preactive then the exchange rate is always 1:1.
    <b>if</b> (<a href="staking_pool.md#0x2_staking_pool_is_preactive_at_epoch">is_preactive_at_epoch</a>(pool, epoch)) {
        <b>return</b> <a href="staking_pool.md#0x2_staking_pool_initial_exchange_rate">initial_exchange_rate</a>()
    };
    <b>let</b> clamped_epoch = <a href="_get_with_default">option::get_with_default</a>(&pool.deactivation_epoch, epoch);
    <b>let</b> epoch = <a href="math.md#0x2_math_min">math::min</a>(clamped_epoch, epoch);
    <b>let</b> activation_epoch = *<a href="_borrow">option::borrow</a>(&pool.activation_epoch);

    // Find the latest epoch that's earlier than the given epoch <b>with</b> an entry in the <a href="table.md#0x2_table">table</a>
    <b>while</b> (epoch &gt;= activation_epoch) {
        <b>if</b> (<a href="table.md#0x2_table_contains">table::contains</a>(&pool.exchange_rates, epoch)) {
            <b>return</b> *<a href="table.md#0x2_table_borrow">table::borrow</a>(&pool.exchange_rates, epoch)
        };
        epoch = epoch - 1;
    };
    // This line really should be unreachable. Do we want an <b>assert</b> <b>false</b> here?
    <a href="staking_pool.md#0x2_staking_pool_initial_exchange_rate">initial_exchange_rate</a>()
}
</code></pre>



</details>

<a name="0x2_staking_pool_pending_stake_amount"></a>

## Function `pending_stake_amount`

Calculate the total value of the pending staking requests for this staking pool.


<pre><code><b>public</b> <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_pending_stake_amount">pending_stake_amount</a>(<a href="staking_pool.md#0x2_staking_pool">staking_pool</a>: &<a href="staking_pool.md#0x2_staking_pool_StakingPool">staking_pool::StakingPool</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_pending_stake_amount">pending_stake_amount</a>(<a href="staking_pool.md#0x2_staking_pool">staking_pool</a>: &<a href="staking_pool.md#0x2_staking_pool_StakingPool">StakingPool</a>): u64 {
    <a href="staking_pool.md#0x2_staking_pool">staking_pool</a>.pending_stake
}
</code></pre>



</details>

<a name="0x2_staking_pool_pending_stake_withdraw_amount"></a>

## Function `pending_stake_withdraw_amount`

Calculate the current the total withdrawal from the staking pool this epoch.


<pre><code><b>public</b> <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_pending_stake_withdraw_amount">pending_stake_withdraw_amount</a>(<a href="staking_pool.md#0x2_staking_pool">staking_pool</a>: &<a href="staking_pool.md#0x2_staking_pool_StakingPool">staking_pool::StakingPool</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_pending_stake_withdraw_amount">pending_stake_withdraw_amount</a>(<a href="staking_pool.md#0x2_staking_pool">staking_pool</a>: &<a href="staking_pool.md#0x2_staking_pool_StakingPool">StakingPool</a>): u64 {
    <a href="staking_pool.md#0x2_staking_pool">staking_pool</a>.pending_total_sui_withdraw
}
</code></pre>



</details>

<a name="0x2_staking_pool_is_preactive_at_epoch"></a>

## Function `is_preactive_at_epoch`

Returns true if the provided staking pool is preactive at the provided epoch.


<pre><code><b>fun</b> <a href="staking_pool.md#0x2_staking_pool_is_preactive_at_epoch">is_preactive_at_epoch</a>(pool: &<a href="staking_pool.md#0x2_staking_pool_StakingPool">staking_pool::StakingPool</a>, epoch: u64): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="staking_pool.md#0x2_staking_pool_is_preactive_at_epoch">is_preactive_at_epoch</a>(pool: &<a href="staking_pool.md#0x2_staking_pool_StakingPool">StakingPool</a>, epoch: u64): bool{
    // Either the pool is currently preactive or the pool's starting epoch is later than the provided epoch.
    <a href="staking_pool.md#0x2_staking_pool_is_preactive">is_preactive</a>(pool) || (*<a href="_borrow">option::borrow</a>(&pool.activation_epoch) &gt; epoch)
}
</code></pre>



</details>

<a name="0x2_staking_pool_get_sui_amount"></a>

## Function `get_sui_amount`



<pre><code><b>fun</b> <a href="staking_pool.md#0x2_staking_pool_get_sui_amount">get_sui_amount</a>(exchange_rate: &<a href="staking_pool.md#0x2_staking_pool_PoolTokenExchangeRate">staking_pool::PoolTokenExchangeRate</a>, token_amount: u64): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="staking_pool.md#0x2_staking_pool_get_sui_amount">get_sui_amount</a>(exchange_rate: &<a href="staking_pool.md#0x2_staking_pool_PoolTokenExchangeRate">PoolTokenExchangeRate</a>, token_amount: u64): u64 {
    // When either amount is 0, that means we have no stakes <b>with</b> this pool.
    // The other amount might be non-zero when there's dust left in the pool.
    <b>if</b> (exchange_rate.sui_amount == 0 || exchange_rate.pool_token_amount == 0) {
        <b>return</b> token_amount
    };
    <b>let</b> res = (exchange_rate.sui_amount <b>as</b> u128)
            * (token_amount <b>as</b> u128)
            / (exchange_rate.pool_token_amount <b>as</b> u128);
    (res <b>as</b> u64)
}
</code></pre>



</details>

<a name="0x2_staking_pool_get_token_amount"></a>

## Function `get_token_amount`



<pre><code><b>fun</b> <a href="staking_pool.md#0x2_staking_pool_get_token_amount">get_token_amount</a>(exchange_rate: &<a href="staking_pool.md#0x2_staking_pool_PoolTokenExchangeRate">staking_pool::PoolTokenExchangeRate</a>, sui_amount: u64): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="staking_pool.md#0x2_staking_pool_get_token_amount">get_token_amount</a>(exchange_rate: &<a href="staking_pool.md#0x2_staking_pool_PoolTokenExchangeRate">PoolTokenExchangeRate</a>, sui_amount: u64): u64 {
    // When either amount is 0, that means we have no stakes <b>with</b> this pool.
    // The other amount might be non-zero when there's dust left in the pool.
    <b>if</b> (exchange_rate.sui_amount == 0 || exchange_rate.pool_token_amount == 0) {
        <b>return</b> sui_amount
    };
    <b>let</b> res = (exchange_rate.pool_token_amount <b>as</b> u128)
            * (sui_amount <b>as</b> u128)
            / (exchange_rate.sui_amount <b>as</b> u128);
    (res <b>as</b> u64)
}
</code></pre>



</details>

<a name="0x2_staking_pool_initial_exchange_rate"></a>

## Function `initial_exchange_rate`



<pre><code><b>fun</b> <a href="staking_pool.md#0x2_staking_pool_initial_exchange_rate">initial_exchange_rate</a>(): <a href="staking_pool.md#0x2_staking_pool_PoolTokenExchangeRate">staking_pool::PoolTokenExchangeRate</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="staking_pool.md#0x2_staking_pool_initial_exchange_rate">initial_exchange_rate</a>(): <a href="staking_pool.md#0x2_staking_pool_PoolTokenExchangeRate">PoolTokenExchangeRate</a> {
    <a href="staking_pool.md#0x2_staking_pool_PoolTokenExchangeRate">PoolTokenExchangeRate</a> { sui_amount: 0, pool_token_amount: 0 }
}
</code></pre>



</details>

<a name="0x2_staking_pool_check_balance_invariants"></a>

## Function `check_balance_invariants`



<pre><code><b>fun</b> <a href="staking_pool.md#0x2_staking_pool_check_balance_invariants">check_balance_invariants</a>(pool: &<a href="staking_pool.md#0x2_staking_pool_StakingPool">staking_pool::StakingPool</a>, epoch: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="staking_pool.md#0x2_staking_pool_check_balance_invariants">check_balance_invariants</a>(pool: &<a href="staking_pool.md#0x2_staking_pool_StakingPool">StakingPool</a>, epoch: u64) {
    <b>let</b> exchange_rate = <a href="staking_pool.md#0x2_staking_pool_pool_token_exchange_rate_at_epoch">pool_token_exchange_rate_at_epoch</a>(pool, epoch);
    // check that the pool token <a href="balance.md#0x2_balance">balance</a> and <a href="sui.md#0x2_sui">sui</a> <a href="balance.md#0x2_balance">balance</a> ratio matches the exchange rate stored.
    <b>let</b> expected = <a href="staking_pool.md#0x2_staking_pool_get_token_amount">get_token_amount</a>(&exchange_rate, pool.sui_balance);
    <b>let</b> actual = pool.pool_token_balance;
    <b>assert</b>!(expected == actual, <a href="staking_pool.md#0x2_staking_pool_ETokenBalancesDoNotMatchExchangeRate">ETokenBalancesDoNotMatchExchangeRate</a>)
}
</code></pre>



</details>
