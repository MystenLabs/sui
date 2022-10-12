
<a name="0x2_staking_pool"></a>

# Module `0x2::staking_pool`



-  [Struct `StakingPool`](#0x2_staking_pool_StakingPool)
-  [Resource `InactiveStakingPool`](#0x2_staking_pool_InactiveStakingPool)
-  [Struct `DelegationToken`](#0x2_staking_pool_DelegationToken)
-  [Struct `PendingDelegationEntry`](#0x2_staking_pool_PendingDelegationEntry)
-  [Resource `Delegation`](#0x2_staking_pool_Delegation)
-  [Resource `StakedSui`](#0x2_staking_pool_StakedSui)
-  [Constants](#@Constants_0)
-  [Function `new`](#0x2_staking_pool_new)
-  [Function `advance_epoch`](#0x2_staking_pool_advance_epoch)
-  [Function `request_add_delegation`](#0x2_staking_pool_request_add_delegation)
-  [Function `mint_delegation_tokens_to_delegator`](#0x2_staking_pool_mint_delegation_tokens_to_delegator)
-  [Function `withdraw_stake`](#0x2_staking_pool_withdraw_stake)
-  [Function `withdraw_all_to_sui_tokens`](#0x2_staking_pool_withdraw_all_to_sui_tokens)
-  [Function `withdraw_to_sui_tokens`](#0x2_staking_pool_withdraw_to_sui_tokens)
-  [Function `deactivate_staking_pool`](#0x2_staking_pool_deactivate_staking_pool)
-  [Function `withdraw_from_inactive_pool`](#0x2_staking_pool_withdraw_from_inactive_pool)
-  [Function `destroy_empty_delegation`](#0x2_staking_pool_destroy_empty_delegation)
-  [Function `destroy_empty_staked_sui`](#0x2_staking_pool_destroy_empty_staked_sui)
-  [Function `sui_balance`](#0x2_staking_pool_sui_balance)
-  [Function `validator_address`](#0x2_staking_pool_validator_address)
-  [Function `staked_sui_amount`](#0x2_staking_pool_staked_sui_amount)
-  [Function `delegation_token_amount`](#0x2_staking_pool_delegation_token_amount)
-  [Function `withdraw_from_principal`](#0x2_staking_pool_withdraw_from_principal)
-  [Function `get_sui_amount`](#0x2_staking_pool_get_sui_amount)
-  [Function `get_token_amount`](#0x2_staking_pool_get_token_amount)


<pre><code><b>use</b> <a href="">0x1::option</a>;
<b>use</b> <a href="">0x1::vector</a>;
<b>use</b> <a href="balance.md#0x2_balance">0x2::balance</a>;
<b>use</b> <a href="coin.md#0x2_coin">0x2::coin</a>;
<b>use</b> <a href="epoch_time_lock.md#0x2_epoch_time_lock">0x2::epoch_time_lock</a>;
<b>use</b> <a href="locked_coin.md#0x2_locked_coin">0x2::locked_coin</a>;
<b>use</b> <a href="object.md#0x2_object">0x2::object</a>;
<b>use</b> <a href="sui.md#0x2_sui">0x2::sui</a>;
<b>use</b> <a href="transfer.md#0x2_transfer">0x2::transfer</a>;
<b>use</b> <a href="tx_context.md#0x2_tx_context">0x2::tx_context</a>;
</code></pre>



<a name="0x2_staking_pool_StakingPool"></a>

## Struct `StakingPool`

A staking pool embedded in each validator struct in the system state object.


<pre><code><b>struct</b> <a href="staking_pool.md#0x2_staking_pool_StakingPool">StakingPool</a> <b>has</b> store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>validator_address: <b>address</b></code>
</dt>
<dd>
 The sui address of the validator associated with this pool.
</dd>
<dt>
<code>starting_epoch: u64</code>
</dt>
<dd>
 The epoch at which this pool started operating. Should be the epoch at which the validator became active.
</dd>
<dt>
<code>epoch_starting_sui_balance: u64</code>
</dt>
<dd>
 The total number of SUI tokens in this pool at the beginning of the current epoch.
</dd>
<dt>
<code>epoch_starting_delegation_token_supply: u64</code>
</dt>
<dd>
 The total number of delegation tokens issued by this pool at the beginning of the current epoch.
</dd>
<dt>
<code>sui_balance: u64</code>
</dt>
<dd>
 The total number of SUI tokens in this pool, including the SUI in the rewards_pool, as well as in all the principal
 in the <code><a href="staking_pool.md#0x2_staking_pool_Delegation">Delegation</a></code> object.
</dd>
<dt>
<code>rewards_pool: <a href="balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="sui.md#0x2_sui_SUI">sui::SUI</a>&gt;</code>
</dt>
<dd>
 The epoch delegation rewards will be added here at the end of each epoch.
</dd>
<dt>
<code>delegation_token_supply: <a href="balance.md#0x2_balance_Supply">balance::Supply</a>&lt;<a href="staking_pool.md#0x2_staking_pool_DelegationToken">staking_pool::DelegationToken</a>&gt;</code>
</dt>
<dd>
 The number of delegation pool tokens we have issued so far. This number should equal the sum of
 pool token balance in all the <code><a href="staking_pool.md#0x2_staking_pool_Delegation">Delegation</a></code> objects delegated to this pool.
</dd>
<dt>
<code>pending_delegations: <a href="">vector</a>&lt;<a href="staking_pool.md#0x2_staking_pool_PendingDelegationEntry">staking_pool::PendingDelegationEntry</a>&gt;</code>
</dt>
<dd>
 Delegations requested during the current epoch. We will activate these delegation at the end of current epoch
 and distribute staking pool tokens at the end-of-epoch exchange rate after the rewards for the current epoch
 have been deposited.
</dd>
</dl>


</details>

<a name="0x2_staking_pool_InactiveStakingPool"></a>

## Resource `InactiveStakingPool`

An inactive staking pool associated with an inactive validator.
Only withdraws can be made from this pool.


<pre><code><b>struct</b> <a href="staking_pool.md#0x2_staking_pool_InactiveStakingPool">InactiveStakingPool</a> <b>has</b> key
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
<code>pool: <a href="staking_pool.md#0x2_staking_pool_StakingPool">staking_pool::StakingPool</a></code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x2_staking_pool_DelegationToken"></a>

## Struct `DelegationToken`

The staking pool token.


<pre><code><b>struct</b> <a href="staking_pool.md#0x2_staking_pool_DelegationToken">DelegationToken</a> <b>has</b> drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>dummy_field: bool</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x2_staking_pool_PendingDelegationEntry"></a>

## Struct `PendingDelegationEntry`

Struct representing a pending delegation.


<pre><code><b>struct</b> <a href="staking_pool.md#0x2_staking_pool_PendingDelegationEntry">PendingDelegationEntry</a> <b>has</b> drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>delegator: <b>address</b></code>
</dt>
<dd>

</dd>
<dt>
<code>sui_amount: u64</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x2_staking_pool_Delegation"></a>

## Resource `Delegation`

A self-custodial delegation object, serving as evidence that the delegator
has delegated to a staking pool.


<pre><code><b>struct</b> <a href="staking_pool.md#0x2_staking_pool_Delegation">Delegation</a> <b>has</b> key
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
<code>validator_address: <b>address</b></code>
</dt>
<dd>
 The sui address of the validator associated with the staking pool this object delgates to.
</dd>
<dt>
<code>pool_starting_epoch: u64</code>
</dt>
<dd>
 The epoch at which the staking pool started operating.
</dd>
<dt>
<code>pool_tokens: <a href="balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="staking_pool.md#0x2_staking_pool_DelegationToken">staking_pool::DelegationToken</a>&gt;</code>
</dt>
<dd>
 The pool tokens representing the amount of rewards the delegator can get back when they withdraw
 from the pool. If this field is <code>none</code>, that means the delegation hasn't been activated yet.
</dd>
<dt>
<code>principal_sui_amount: u64</code>
</dt>
<dd>
 Number of SUI token staked originally.
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


<a name="0x2_staking_pool_EDESTROY_NON_ZERO_BALANCE"></a>



<pre><code><b>const</b> <a href="staking_pool.md#0x2_staking_pool_EDESTROY_NON_ZERO_BALANCE">EDESTROY_NON_ZERO_BALANCE</a>: u64 = 5;
</code></pre>



<a name="0x2_staking_pool_EINSUFFICIENT_POOL_TOKEN_BALANCE"></a>



<pre><code><b>const</b> <a href="staking_pool.md#0x2_staking_pool_EINSUFFICIENT_POOL_TOKEN_BALANCE">EINSUFFICIENT_POOL_TOKEN_BALANCE</a>: u64 = 0;
</code></pre>



<a name="0x2_staking_pool_EINSUFFICIENT_REWARDS_POOL_BALANCE"></a>



<pre><code><b>const</b> <a href="staking_pool.md#0x2_staking_pool_EINSUFFICIENT_REWARDS_POOL_BALANCE">EINSUFFICIENT_REWARDS_POOL_BALANCE</a>: u64 = 4;
</code></pre>



<a name="0x2_staking_pool_EINSUFFICIENT_SUI_TOKEN_BALANCE"></a>



<pre><code><b>const</b> <a href="staking_pool.md#0x2_staking_pool_EINSUFFICIENT_SUI_TOKEN_BALANCE">EINSUFFICIENT_SUI_TOKEN_BALANCE</a>: u64 = 3;
</code></pre>



<a name="0x2_staking_pool_ETOKEN_TIME_LOCK_IS_SOME"></a>



<pre><code><b>const</b> <a href="staking_pool.md#0x2_staking_pool_ETOKEN_TIME_LOCK_IS_SOME">ETOKEN_TIME_LOCK_IS_SOME</a>: u64 = 6;
</code></pre>



<a name="0x2_staking_pool_EWITHDRAW_AMOUNT_CANNOT_BE_ZERO"></a>



<pre><code><b>const</b> <a href="staking_pool.md#0x2_staking_pool_EWITHDRAW_AMOUNT_CANNOT_BE_ZERO">EWITHDRAW_AMOUNT_CANNOT_BE_ZERO</a>: u64 = 2;
</code></pre>



<a name="0x2_staking_pool_EWRONG_POOL"></a>



<pre><code><b>const</b> <a href="staking_pool.md#0x2_staking_pool_EWRONG_POOL">EWRONG_POOL</a>: u64 = 1;
</code></pre>



<a name="0x2_staking_pool_new"></a>

## Function `new`

Create a new, empty staking pool.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_new">new</a>(validator_address: <b>address</b>, starting_epoch: u64): <a href="staking_pool.md#0x2_staking_pool_StakingPool">staking_pool::StakingPool</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_new">new</a>(validator_address: <b>address</b>, starting_epoch: u64) : <a href="staking_pool.md#0x2_staking_pool_StakingPool">StakingPool</a> {
    <a href="staking_pool.md#0x2_staking_pool_StakingPool">StakingPool</a> {
        validator_address,
        starting_epoch,
        epoch_starting_sui_balance: 0,
        epoch_starting_delegation_token_supply: 0,
        sui_balance: 0,
        rewards_pool: <a href="balance.md#0x2_balance_zero">balance::zero</a>(),
        delegation_token_supply: <a href="balance.md#0x2_balance_create_supply">balance::create_supply</a>(<a href="staking_pool.md#0x2_staking_pool_DelegationToken">DelegationToken</a> {}),
        pending_delegations: <a href="_empty">vector::empty</a>(),
    }
}
</code></pre>



</details>

<a name="0x2_staking_pool_advance_epoch"></a>

## Function `advance_epoch`

Called at epoch advancement times to add rewards (in SUI) to the staking pool, and distribute new delegation tokens.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_advance_epoch">advance_epoch</a>(pool: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_StakingPool">staking_pool::StakingPool</a>, rewards: <a href="balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="sui.md#0x2_sui_SUI">sui::SUI</a>&gt;, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_advance_epoch">advance_epoch</a>(pool: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_StakingPool">StakingPool</a>, rewards: Balance&lt;SUI&gt;, ctx: &<b>mut</b> TxContext) {
    pool.sui_balance = pool.sui_balance + <a href="balance.md#0x2_balance_value">balance::value</a>(&rewards);
    <a href="balance.md#0x2_balance_join">balance::join</a>(&<b>mut</b> pool.rewards_pool, rewards);

    // distribute pool tokens at new exchange rate.
    <b>while</b> (!<a href="_is_empty">vector::is_empty</a>(&pool.pending_delegations)) {
        <b>let</b> <a href="staking_pool.md#0x2_staking_pool_PendingDelegationEntry">PendingDelegationEntry</a> { delegator, sui_amount } = <a href="_pop_back">vector::pop_back</a>(&<b>mut</b> pool.pending_delegations);
        <a href="staking_pool.md#0x2_staking_pool_mint_delegation_tokens_to_delegator">mint_delegation_tokens_to_delegator</a>(pool, delegator, sui_amount, ctx);
        pool.sui_balance = pool.sui_balance + sui_amount
    };

    // Record the epoch starting balances.
    pool.epoch_starting_sui_balance = pool.sui_balance;
    pool.epoch_starting_delegation_token_supply = <a href="balance.md#0x2_balance_supply_value">balance::supply_value</a>(&pool.delegation_token_supply);
}
</code></pre>



</details>

<a name="0x2_staking_pool_request_add_delegation"></a>

## Function `request_add_delegation`

Request to delegate to a staking pool. The delegation gets counted at the beginning of the next epoch,
when the delegation object containing the pool tokens is distributed to the delegator.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_request_add_delegation">request_add_delegation</a>(pool: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_StakingPool">staking_pool::StakingPool</a>, <a href="stake.md#0x2_stake">stake</a>: <a href="balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="sui.md#0x2_sui_SUI">sui::SUI</a>&gt;, sui_token_lock: <a href="_Option">option::Option</a>&lt;<a href="epoch_time_lock.md#0x2_epoch_time_lock_EpochTimeLock">epoch_time_lock::EpochTimeLock</a>&gt;, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_request_add_delegation">request_add_delegation</a>(
    pool: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_StakingPool">StakingPool</a>,
    <a href="stake.md#0x2_stake">stake</a>: Balance&lt;SUI&gt;,
    sui_token_lock: Option&lt;EpochTimeLock&gt;,
    ctx: &<b>mut</b> TxContext
) {
    <b>let</b> sui_amount = <a href="balance.md#0x2_balance_value">balance::value</a>(&<a href="stake.md#0x2_stake">stake</a>);
    <b>assert</b>!(sui_amount &gt; 0, 0);
    <b>let</b> delegator = <a href="tx_context.md#0x2_tx_context_sender">tx_context::sender</a>(ctx);
    // insert delegation info into the pendng_delegations <a href="">vector</a>.
    <a href="_push_back">vector::push_back</a>(&<b>mut</b> pool.pending_delegations, <a href="staking_pool.md#0x2_staking_pool_PendingDelegationEntry">PendingDelegationEntry</a> { delegator, sui_amount });
    <b>let</b> staked_sui = <a href="staking_pool.md#0x2_staking_pool_StakedSui">StakedSui</a> {
        id: <a href="object.md#0x2_object_new">object::new</a>(ctx),
        principal: <a href="stake.md#0x2_stake">stake</a>,
        sui_token_lock,
    };
    <a href="transfer.md#0x2_transfer_transfer">transfer::transfer</a>(staked_sui, delegator);
}
</code></pre>



</details>

<a name="0x2_staking_pool_mint_delegation_tokens_to_delegator"></a>

## Function `mint_delegation_tokens_to_delegator`

Activate a delegation. New pool tokens are minted at the current exchange rate and put into the
<code>pool_tokens</code> field of the delegation object.
After activation, the delegation officially counts toward the staking power of the validator.
Aborts if the pool mismatches, the delegation is already activated, or the delegation cannot be activated yet.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_mint_delegation_tokens_to_delegator">mint_delegation_tokens_to_delegator</a>(pool: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_StakingPool">staking_pool::StakingPool</a>, delegator: <b>address</b>, sui_amount: u64, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_mint_delegation_tokens_to_delegator">mint_delegation_tokens_to_delegator</a>(
    pool: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_StakingPool">StakingPool</a>,
    delegator: <b>address</b>,
    sui_amount: u64,
    ctx: &<b>mut</b> TxContext
) {
    <b>let</b> new_pool_token_amount = <a href="staking_pool.md#0x2_staking_pool_get_token_amount">get_token_amount</a>(pool, sui_amount);

    // Mint new pool tokens at the current exchange rate.
    <b>let</b> pool_tokens = <a href="balance.md#0x2_balance_increase_supply">balance::increase_supply</a>(&<b>mut</b> pool.delegation_token_supply, new_pool_token_amount);

    <b>let</b> delegation = <a href="staking_pool.md#0x2_staking_pool_Delegation">Delegation</a> {
        id: <a href="object.md#0x2_object_new">object::new</a>(ctx),
        validator_address: pool.validator_address,
        pool_starting_epoch: pool.starting_epoch,
        pool_tokens,
        principal_sui_amount: sui_amount,
    };

    <a href="transfer.md#0x2_transfer_transfer">transfer::transfer</a>(delegation, delegator);
}
</code></pre>



</details>

<a name="0x2_staking_pool_withdraw_stake"></a>

## Function `withdraw_stake`

Withdraw <code>withdraw_pool_token_amount</code> worth of delegated stake from a staking pool. A proportional amount of principal and rewards
in SUI will be withdrawn and transferred to the delegator.
Returns the amount of SUI withdrawn.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_withdraw_stake">withdraw_stake</a>(pool: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_StakingPool">staking_pool::StakingPool</a>, delegation: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_Delegation">staking_pool::Delegation</a>, staked_sui: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_StakedSui">staking_pool::StakedSui</a>, withdraw_pool_token_amount: u64, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_withdraw_stake">withdraw_stake</a>(
    pool: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_StakingPool">StakingPool</a>,
    delegation: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_Delegation">Delegation</a>,
    staked_sui: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_StakedSui">StakedSui</a>,
    withdraw_pool_token_amount: u64,
    ctx: &<b>mut</b> TxContext
) : u64 {
    <b>let</b> (principal_withdraw, reward_withdraw, time_lock) =
        <a href="staking_pool.md#0x2_staking_pool_withdraw_to_sui_tokens">withdraw_to_sui_tokens</a>(pool, delegation, staked_sui, withdraw_pool_token_amount);
    <b>let</b> sui_withdraw_amount = <a href="balance.md#0x2_balance_value">balance::value</a>(&principal_withdraw) + <a href="balance.md#0x2_balance_value">balance::value</a>(&reward_withdraw);
    <b>let</b> delegator = <a href="tx_context.md#0x2_tx_context_sender">tx_context::sender</a>(ctx);

    // TODO: implement withdraw bonding period here.
    <b>if</b> (<a href="_is_some">option::is_some</a>(&time_lock)) {
        <a href="locked_coin.md#0x2_locked_coin_new_from_balance">locked_coin::new_from_balance</a>(principal_withdraw, <a href="_destroy_some">option::destroy_some</a>(time_lock), delegator, ctx);
        <a href="transfer.md#0x2_transfer_transfer">transfer::transfer</a>(<a href="coin.md#0x2_coin_from_balance">coin::from_balance</a>(reward_withdraw, ctx), delegator);
    } <b>else</b> {
        <a href="balance.md#0x2_balance_join">balance::join</a>(&<b>mut</b> principal_withdraw, reward_withdraw);
        <a href="transfer.md#0x2_transfer_transfer">transfer::transfer</a>(<a href="coin.md#0x2_coin_from_balance">coin::from_balance</a>(principal_withdraw, ctx), delegator);
        <a href="_destroy_none">option::destroy_none</a>(time_lock);
    };
    sui_withdraw_amount
}
</code></pre>



</details>

<a name="0x2_staking_pool_withdraw_all_to_sui_tokens"></a>

## Function `withdraw_all_to_sui_tokens`

Withdraw all the pool tokens in <code>delegation</code> object, with separate principal and rewards components, and
then destroy the delegation object.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_withdraw_all_to_sui_tokens">withdraw_all_to_sui_tokens</a>(pool: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_StakingPool">staking_pool::StakingPool</a>, delegation: <a href="staking_pool.md#0x2_staking_pool_Delegation">staking_pool::Delegation</a>, staked_sui: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_StakedSui">staking_pool::StakedSui</a>): (<a href="balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="sui.md#0x2_sui_SUI">sui::SUI</a>&gt;, <a href="balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="sui.md#0x2_sui_SUI">sui::SUI</a>&gt;, <a href="_Option">option::Option</a>&lt;<a href="epoch_time_lock.md#0x2_epoch_time_lock_EpochTimeLock">epoch_time_lock::EpochTimeLock</a>&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_withdraw_all_to_sui_tokens">withdraw_all_to_sui_tokens</a>(
    pool: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_StakingPool">StakingPool</a>,
    delegation: <a href="staking_pool.md#0x2_staking_pool_Delegation">Delegation</a>,
    staked_sui: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_StakedSui">StakedSui</a>,
) : (Balance&lt;SUI&gt;, Balance&lt;SUI&gt;, Option&lt;EpochTimeLock&gt;) {
    <b>let</b> withdraw_amount = <a href="balance.md#0x2_balance_value">balance::value</a>(&delegation.pool_tokens);
    <b>let</b> (principal_withdraw, reward_withdraw, time_lock) =
        <a href="staking_pool.md#0x2_staking_pool_withdraw_to_sui_tokens">withdraw_to_sui_tokens</a>(pool, &<b>mut</b> delegation, staked_sui, withdraw_amount);
    <a href="staking_pool.md#0x2_staking_pool_destroy_empty_delegation">destroy_empty_delegation</a>(delegation);
    (principal_withdraw, reward_withdraw, time_lock)
}
</code></pre>



</details>

<a name="0x2_staking_pool_withdraw_to_sui_tokens"></a>

## Function `withdraw_to_sui_tokens`



<pre><code><b>fun</b> <a href="staking_pool.md#0x2_staking_pool_withdraw_to_sui_tokens">withdraw_to_sui_tokens</a>(pool: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_StakingPool">staking_pool::StakingPool</a>, delegation: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_Delegation">staking_pool::Delegation</a>, staked_sui: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_StakedSui">staking_pool::StakedSui</a>, withdraw_pool_token_amount: u64): (<a href="balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="sui.md#0x2_sui_SUI">sui::SUI</a>&gt;, <a href="balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="sui.md#0x2_sui_SUI">sui::SUI</a>&gt;, <a href="_Option">option::Option</a>&lt;<a href="epoch_time_lock.md#0x2_epoch_time_lock_EpochTimeLock">epoch_time_lock::EpochTimeLock</a>&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="staking_pool.md#0x2_staking_pool_withdraw_to_sui_tokens">withdraw_to_sui_tokens</a>(
    pool: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_StakingPool">StakingPool</a>,
    delegation: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_Delegation">Delegation</a>,
    staked_sui: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_StakedSui">StakedSui</a>,
    withdraw_pool_token_amount: u64,
) : (Balance&lt;SUI&gt;, Balance&lt;SUI&gt;, Option&lt;EpochTimeLock&gt;) {
    <b>assert</b>!(
        delegation.validator_address == pool.validator_address &&
        delegation.pool_starting_epoch == pool.starting_epoch,
        <a href="staking_pool.md#0x2_staking_pool_EWRONG_POOL">EWRONG_POOL</a>
    );

    <b>assert</b>!(withdraw_pool_token_amount &gt; 0, <a href="staking_pool.md#0x2_staking_pool_EWITHDRAW_AMOUNT_CANNOT_BE_ZERO">EWITHDRAW_AMOUNT_CANNOT_BE_ZERO</a>);

    <b>let</b> pool_token_balance = <a href="balance.md#0x2_balance_value">balance::value</a>(&delegation.pool_tokens);
    <b>assert</b>!(pool_token_balance &gt;= withdraw_pool_token_amount, <a href="staking_pool.md#0x2_staking_pool_EINSUFFICIENT_POOL_TOKEN_BALANCE">EINSUFFICIENT_POOL_TOKEN_BALANCE</a>);

    // Calculate the amount of SUI tokens that should be withdrawn from the pool using the current exchange rate.
    <b>let</b> sui_withdraw_amount = <a href="staking_pool.md#0x2_staking_pool_get_sui_amount">get_sui_amount</a>(pool, withdraw_pool_token_amount);

    // decrement <a href="sui.md#0x2_sui">sui</a> <a href="balance.md#0x2_balance">balance</a> in the pool
    pool.sui_balance = pool.sui_balance - sui_withdraw_amount;

    // Calculate the amounts of SUI <b>to</b> be withdrawn from the principal component and the rewards component.
    // We already checked that pool_token_balance is greater than zero.
    <b>let</b> sui_withdraw_from_principal =
        (delegation.principal_sui_amount <b>as</b> u128) * (withdraw_pool_token_amount <b>as</b> u128) / (pool_token_balance <b>as</b> u128);
    <b>let</b> sui_withdraw_from_rewards = sui_withdraw_amount - (sui_withdraw_from_principal <b>as</b> u64);

    // burn the pool tokens
    <a href="balance.md#0x2_balance_decrease_supply">balance::decrease_supply</a>(
        &<b>mut</b> pool.delegation_token_supply,
        <a href="balance.md#0x2_balance_split">balance::split</a>(&<b>mut</b> delegation.pool_tokens, withdraw_pool_token_amount)
    );

    <b>let</b> (principal_withdraw, time_lock) = <a href="staking_pool.md#0x2_staking_pool_withdraw_from_principal">withdraw_from_principal</a>(delegation, staked_sui, (sui_withdraw_from_principal <b>as</b> u64));

    // withdraw the rewards component from rewards pool and <a href="transfer.md#0x2_transfer">transfer</a> it <b>to</b> the delegator.
    <b>assert</b>!(<a href="balance.md#0x2_balance_value">balance::value</a>(&pool.rewards_pool) &gt;= sui_withdraw_from_rewards, <a href="staking_pool.md#0x2_staking_pool_EINSUFFICIENT_REWARDS_POOL_BALANCE">EINSUFFICIENT_REWARDS_POOL_BALANCE</a>);
    <b>let</b> reward_withdraw = <a href="balance.md#0x2_balance_split">balance::split</a>(&<b>mut</b> pool.rewards_pool, sui_withdraw_from_rewards);

    (principal_withdraw, reward_withdraw, time_lock)
}
</code></pre>



</details>

<a name="0x2_staking_pool_deactivate_staking_pool"></a>

## Function `deactivate_staking_pool`

Deactivate a staking pool by wrapping it in an <code><a href="staking_pool.md#0x2_staking_pool_InactiveStakingPool">InactiveStakingPool</a></code> and sharing this newly created object.
After this pool deactivation, the pool stops earning rewards. Only delegation withdraws can be made to the pool.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_deactivate_staking_pool">deactivate_staking_pool</a>(pool: <a href="staking_pool.md#0x2_staking_pool_StakingPool">staking_pool::StakingPool</a>, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_deactivate_staking_pool">deactivate_staking_pool</a>(pool: <a href="staking_pool.md#0x2_staking_pool_StakingPool">StakingPool</a>, ctx: &<b>mut</b> TxContext) {
    <b>let</b> inactive_pool = <a href="staking_pool.md#0x2_staking_pool_InactiveStakingPool">InactiveStakingPool</a> { id: <a href="object.md#0x2_object_new">object::new</a>(ctx), pool};
    <a href="transfer.md#0x2_transfer_share_object">transfer::share_object</a>(inactive_pool);
}
</code></pre>



</details>

<a name="0x2_staking_pool_withdraw_from_inactive_pool"></a>

## Function `withdraw_from_inactive_pool`

Withdraw delegation from an inactive pool.


<pre><code><b>public</b> <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_withdraw_from_inactive_pool">withdraw_from_inactive_pool</a>(inactive_pool: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_InactiveStakingPool">staking_pool::InactiveStakingPool</a>, staked_sui: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_StakedSui">staking_pool::StakedSui</a>, delegation: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_Delegation">staking_pool::Delegation</a>, withdraw_amount: u64, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_withdraw_from_inactive_pool">withdraw_from_inactive_pool</a>(
    inactive_pool: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_InactiveStakingPool">InactiveStakingPool</a>,
    staked_sui: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_StakedSui">StakedSui</a>,
    delegation: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_Delegation">Delegation</a>,
    withdraw_amount: u64,
    ctx: &<b>mut</b> TxContext
) {
    <a href="staking_pool.md#0x2_staking_pool_withdraw_stake">withdraw_stake</a>(&<b>mut</b> inactive_pool.pool, delegation, staked_sui, withdraw_amount, ctx);
}
</code></pre>



</details>

<a name="0x2_staking_pool_destroy_empty_delegation"></a>

## Function `destroy_empty_delegation`

Destroy an empty delegation that no longer contains any SUI or pool tokens.


<pre><code><b>public</b> <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_destroy_empty_delegation">destroy_empty_delegation</a>(delegation: <a href="staking_pool.md#0x2_staking_pool_Delegation">staking_pool::Delegation</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_destroy_empty_delegation">destroy_empty_delegation</a>(delegation: <a href="staking_pool.md#0x2_staking_pool_Delegation">Delegation</a>) {
    <b>let</b> <a href="staking_pool.md#0x2_staking_pool_Delegation">Delegation</a> {
        id,
        validator_address: _,
        pool_starting_epoch: _,
        pool_tokens,
        principal_sui_amount,
    } = delegation;
    <a href="object.md#0x2_object_delete">object::delete</a>(id);
    <b>assert</b>!(<a href="balance.md#0x2_balance_value">balance::value</a>(&pool_tokens) == 0, <a href="staking_pool.md#0x2_staking_pool_EDESTROY_NON_ZERO_BALANCE">EDESTROY_NON_ZERO_BALANCE</a>);
    <b>assert</b>!(principal_sui_amount == 0, <a href="staking_pool.md#0x2_staking_pool_EDESTROY_NON_ZERO_BALANCE">EDESTROY_NON_ZERO_BALANCE</a>);
    <a href="balance.md#0x2_balance_destroy_zero">balance::destroy_zero</a>(pool_tokens);
}
</code></pre>



</details>

<a name="0x2_staking_pool_destroy_empty_staked_sui"></a>

## Function `destroy_empty_staked_sui`

Destroy an empty delegation that no longer contains any SUI or pool tokens.


<pre><code><b>public</b> <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_destroy_empty_staked_sui">destroy_empty_staked_sui</a>(staked_sui: <a href="staking_pool.md#0x2_staking_pool_StakedSui">staking_pool::StakedSui</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_destroy_empty_staked_sui">destroy_empty_staked_sui</a>(staked_sui: <a href="staking_pool.md#0x2_staking_pool_StakedSui">StakedSui</a>) {
    <b>let</b> <a href="staking_pool.md#0x2_staking_pool_StakedSui">StakedSui</a> {
        id,
        principal,
        sui_token_lock
    } = staked_sui;
    <a href="object.md#0x2_object_delete">object::delete</a>(id);
    <b>assert</b>!(<a href="balance.md#0x2_balance_value">balance::value</a>(&principal) == 0, <a href="staking_pool.md#0x2_staking_pool_EDESTROY_NON_ZERO_BALANCE">EDESTROY_NON_ZERO_BALANCE</a>);
    <a href="balance.md#0x2_balance_destroy_zero">balance::destroy_zero</a>(principal);
    <b>assert</b>!(<a href="_is_none">option::is_none</a>(&sui_token_lock), <a href="staking_pool.md#0x2_staking_pool_ETOKEN_TIME_LOCK_IS_SOME">ETOKEN_TIME_LOCK_IS_SOME</a>);
    <a href="_destroy_none">option::destroy_none</a>(sui_token_lock);
}
</code></pre>



</details>

<a name="0x2_staking_pool_sui_balance"></a>

## Function `sui_balance`



<pre><code><b>public</b> <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_sui_balance">sui_balance</a>(pool: &<a href="staking_pool.md#0x2_staking_pool_StakingPool">staking_pool::StakingPool</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_sui_balance">sui_balance</a>(pool: &<a href="staking_pool.md#0x2_staking_pool_StakingPool">StakingPool</a>) : u64 { pool.epoch_starting_sui_balance }
</code></pre>



</details>

<a name="0x2_staking_pool_validator_address"></a>

## Function `validator_address`



<pre><code><b>public</b> <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_validator_address">validator_address</a>(delegation: &<a href="staking_pool.md#0x2_staking_pool_Delegation">staking_pool::Delegation</a>): <b>address</b>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_validator_address">validator_address</a>(delegation: &<a href="staking_pool.md#0x2_staking_pool_Delegation">Delegation</a>) : <b>address</b> { delegation.validator_address }
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

<a name="0x2_staking_pool_delegation_token_amount"></a>

## Function `delegation_token_amount`



<pre><code><b>public</b> <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_delegation_token_amount">delegation_token_amount</a>(delegation: &<a href="staking_pool.md#0x2_staking_pool_Delegation">staking_pool::Delegation</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_delegation_token_amount">delegation_token_amount</a>(delegation: &<a href="staking_pool.md#0x2_staking_pool_Delegation">Delegation</a>): u64 { <a href="balance.md#0x2_balance_value">balance::value</a>(&delegation.pool_tokens) }
</code></pre>



</details>

<a name="0x2_staking_pool_withdraw_from_principal"></a>

## Function `withdraw_from_principal`

Withdraw <code>withdraw_amount</code> of SUI tokens from the delegation and give it back to the delegator
in the original state of the tokens.


<pre><code><b>fun</b> <a href="staking_pool.md#0x2_staking_pool_withdraw_from_principal">withdraw_from_principal</a>(delegation: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_Delegation">staking_pool::Delegation</a>, staked_sui: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_StakedSui">staking_pool::StakedSui</a>, withdraw_amount: u64): (<a href="balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="sui.md#0x2_sui_SUI">sui::SUI</a>&gt;, <a href="_Option">option::Option</a>&lt;<a href="epoch_time_lock.md#0x2_epoch_time_lock_EpochTimeLock">epoch_time_lock::EpochTimeLock</a>&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="staking_pool.md#0x2_staking_pool_withdraw_from_principal">withdraw_from_principal</a>(
    delegation: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_Delegation">Delegation</a>,
    staked_sui: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_StakedSui">StakedSui</a>,
    withdraw_amount: u64,
) : (Balance&lt;SUI&gt;, Option&lt;EpochTimeLock&gt;) {
    <b>assert</b>!(<a href="balance.md#0x2_balance_value">balance::value</a>(&staked_sui.principal) &gt;= withdraw_amount, <a href="staking_pool.md#0x2_staking_pool_EINSUFFICIENT_SUI_TOKEN_BALANCE">EINSUFFICIENT_SUI_TOKEN_BALANCE</a>);
    delegation.principal_sui_amount = delegation.principal_sui_amount - withdraw_amount;
    <b>let</b> principal_withdraw = <a href="balance.md#0x2_balance_split">balance::split</a>(&<b>mut</b> staked_sui.principal, withdraw_amount);
    <b>if</b> (<a href="_is_some">option::is_some</a>(&staked_sui.sui_token_lock)) {
        <b>let</b> time_lock =
            <b>if</b> (<a href="balance.md#0x2_balance_value">balance::value</a>(&staked_sui.principal) == 0) {<a href="_extract">option::extract</a>(&<b>mut</b> staked_sui.sui_token_lock)}
            <b>else</b> *<a href="_borrow">option::borrow</a>(&staked_sui.sui_token_lock);
        (principal_withdraw, <a href="_some">option::some</a>(time_lock))
    } <b>else</b> {
        (principal_withdraw, <a href="_none">option::none</a>())
    }
}
</code></pre>



</details>

<a name="0x2_staking_pool_get_sui_amount"></a>

## Function `get_sui_amount`



<pre><code><b>fun</b> <a href="staking_pool.md#0x2_staking_pool_get_sui_amount">get_sui_amount</a>(pool: &<a href="staking_pool.md#0x2_staking_pool_StakingPool">staking_pool::StakingPool</a>, token_amount: u64): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="staking_pool.md#0x2_staking_pool_get_sui_amount">get_sui_amount</a>(pool: &<a href="staking_pool.md#0x2_staking_pool_StakingPool">StakingPool</a>, token_amount: u64): u64 {
    <b>if</b> (pool.epoch_starting_delegation_token_supply == 0) {
        <b>return</b> token_amount
    };
    <b>let</b> res = (pool.epoch_starting_sui_balance <b>as</b> u128)
            * (token_amount <b>as</b> u128)
            / (pool.epoch_starting_delegation_token_supply <b>as</b> u128);
    (res <b>as</b> u64)
}
</code></pre>



</details>

<a name="0x2_staking_pool_get_token_amount"></a>

## Function `get_token_amount`



<pre><code><b>fun</b> <a href="staking_pool.md#0x2_staking_pool_get_token_amount">get_token_amount</a>(pool: &<a href="staking_pool.md#0x2_staking_pool_StakingPool">staking_pool::StakingPool</a>, sui_amount: u64): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="staking_pool.md#0x2_staking_pool_get_token_amount">get_token_amount</a>(pool: &<a href="staking_pool.md#0x2_staking_pool_StakingPool">StakingPool</a>, sui_amount: u64): u64 {
    <b>if</b> (pool.epoch_starting_sui_balance == 0) {
        <b>return</b> sui_amount
    };
    <b>let</b> res = (pool.epoch_starting_delegation_token_supply <b>as</b> u128)
            * (sui_amount <b>as</b> u128)
            / (pool.epoch_starting_sui_balance <b>as</b> u128);
    (res <b>as</b> u64)
}
</code></pre>



</details>
