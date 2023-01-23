
<a name="0x2_staking_pool"></a>

# Module `0x2::staking_pool`



-  [Struct `StakingPool`](#0x2_staking_pool_StakingPool)
-  [Struct `PoolTokenExchangeRate`](#0x2_staking_pool_PoolTokenExchangeRate)
-  [Resource `InactiveStakingPool`](#0x2_staking_pool_InactiveStakingPool)
-  [Struct `DelegationToken`](#0x2_staking_pool_DelegationToken)
-  [Struct `PendingDelegationEntry`](#0x2_staking_pool_PendingDelegationEntry)
-  [Struct `PendingWithdrawEntry`](#0x2_staking_pool_PendingWithdrawEntry)
-  [Resource `Delegation`](#0x2_staking_pool_Delegation)
-  [Resource `StakedSui`](#0x2_staking_pool_StakedSui)
-  [Constants](#@Constants_0)
-  [Function `new`](#0x2_staking_pool_new)
-  [Function `request_add_delegation`](#0x2_staking_pool_request_add_delegation)
-  [Function `request_withdraw_delegation`](#0x2_staking_pool_request_withdraw_delegation)
-  [Function `cancel_delegation_request`](#0x2_staking_pool_cancel_delegation_request)
-  [Function `withdraw_from_principal`](#0x2_staking_pool_withdraw_from_principal)
-  [Function `destroy_delegation_and_return_pool_tokens`](#0x2_staking_pool_destroy_delegation_and_return_pool_tokens)
-  [Function `unwrap_staked_sui`](#0x2_staking_pool_unwrap_staked_sui)
-  [Function `deposit_rewards`](#0x2_staking_pool_deposit_rewards)
-  [Function `process_pending_delegation_withdraws`](#0x2_staking_pool_process_pending_delegation_withdraws)
-  [Function `process_pending_delegations`](#0x2_staking_pool_process_pending_delegations)
-  [Function `batch_withdraw_rewards_and_burn_pool_tokens`](#0x2_staking_pool_batch_withdraw_rewards_and_burn_pool_tokens)
-  [Function `withdraw_rewards_and_burn_pool_tokens`](#0x2_staking_pool_withdraw_rewards_and_burn_pool_tokens)
-  [Function `mint_delegation_tokens_to_delegator`](#0x2_staking_pool_mint_delegation_tokens_to_delegator)
-  [Function `deactivate_staking_pool`](#0x2_staking_pool_deactivate_staking_pool)
-  [Function `withdraw_from_inactive_pool`](#0x2_staking_pool_withdraw_from_inactive_pool)
-  [Function `destroy_empty_delegation`](#0x2_staking_pool_destroy_empty_delegation)
-  [Function `destroy_empty_staked_sui`](#0x2_staking_pool_destroy_empty_staked_sui)
-  [Function `sui_balance`](#0x2_staking_pool_sui_balance)
-  [Function `validator_address`](#0x2_staking_pool_validator_address)
-  [Function `staked_sui_amount`](#0x2_staking_pool_staked_sui_amount)
-  [Function `delegation_request_epoch`](#0x2_staking_pool_delegation_request_epoch)
-  [Function `delegation_token_amount`](#0x2_staking_pool_delegation_token_amount)
-  [Function `pool_token_exchange_rate`](#0x2_staking_pool_pool_token_exchange_rate)
-  [Function `new_pending_withdraw_entry`](#0x2_staking_pool_new_pending_withdraw_entry)
-  [Function `get_sui_amount`](#0x2_staking_pool_get_sui_amount)
-  [Function `get_token_amount`](#0x2_staking_pool_get_token_amount)


<pre><code><b>use</b> <a href="">0x1::option</a>;
<b>use</b> <a href="balance.md#0x2_balance">0x2::balance</a>;
<b>use</b> <a href="coin.md#0x2_coin">0x2::coin</a>;
<b>use</b> <a href="epoch_time_lock.md#0x2_epoch_time_lock">0x2::epoch_time_lock</a>;
<b>use</b> <a href="linked_table.md#0x2_linked_table">0x2::linked_table</a>;
<b>use</b> <a href="locked_coin.md#0x2_locked_coin">0x2::locked_coin</a>;
<b>use</b> <a href="math.md#0x2_math">0x2::math</a>;
<b>use</b> <a href="object.md#0x2_object">0x2::object</a>;
<b>use</b> <a href="sui.md#0x2_sui">0x2::sui</a>;
<b>use</b> <a href="table_vec.md#0x2_table_vec">0x2::table_vec</a>;
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
<code>sui_balance: u64</code>
</dt>
<dd>
 The total number of SUI tokens in this pool, including the SUI in the rewards_pool, as well as in all the principal
 in the <code><a href="staking_pool.md#0x2_staking_pool_Delegation">Delegation</a></code> object, updated at epoch boundaries.
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
 pool token balance in all the <code><a href="staking_pool.md#0x2_staking_pool_Delegation">Delegation</a></code> objects delegated to this pool. Updated at epoch boundaries.
</dd>
<dt>
<code>pending_delegations: <a href="linked_table.md#0x2_linked_table_LinkedTable">linked_table::LinkedTable</a>&lt;<a href="object.md#0x2_object_ID">object::ID</a>, <a href="staking_pool.md#0x2_staking_pool_PendingDelegationEntry">staking_pool::PendingDelegationEntry</a>&gt;</code>
</dt>
<dd>
 Delegations requested during the current epoch. We will activate these delegation at the end of current epoch
 and distribute staking pool tokens at the end-of-epoch exchange rate after the rewards for the current epoch
 have been deposited.
</dd>
<dt>
<code>pending_withdraws: <a href="table_vec.md#0x2_table_vec_TableVec">table_vec::TableVec</a>&lt;<a href="staking_pool.md#0x2_staking_pool_PendingWithdrawEntry">staking_pool::PendingWithdrawEntry</a>&gt;</code>
</dt>
<dd>
 Delegation withdraws requested during the current epoch. Similar to new delegation, the withdraws are processed
 at epoch boundaries. Rewards are withdrawn and distributed after the rewards for the current epoch have come in.
</dd>
</dl>


</details>

<a name="0x2_staking_pool_PoolTokenExchangeRate"></a>

## Struct `PoolTokenExchangeRate`

Struct representing the exchange rate of the delegation pool token to SUI.


<pre><code><b>struct</b> <a href="staking_pool.md#0x2_staking_pool_PoolTokenExchangeRate">PoolTokenExchangeRate</a> <b>has</b> <b>copy</b>, drop
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

<a name="0x2_staking_pool_PendingWithdrawEntry"></a>

## Struct `PendingWithdrawEntry`

Struct representing a pending delegation withdraw.


<pre><code><b>struct</b> <a href="staking_pool.md#0x2_staking_pool_PendingWithdrawEntry">PendingWithdrawEntry</a> <b>has</b> store
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
<code>principal_withdraw_amount: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>withdrawn_pool_tokens: <a href="balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="staking_pool.md#0x2_staking_pool_DelegationToken">staking_pool::DelegationToken</a>&gt;</code>
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
<code>staked_sui_id: <a href="object.md#0x2_object_ID">object::ID</a></code>
</dt>
<dd>
 The ID of the corresponding <code><a href="staking_pool.md#0x2_staking_pool_StakedSui">StakedSui</a></code> object.
</dd>
<dt>
<code>pool_tokens: <a href="balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="staking_pool.md#0x2_staking_pool_DelegationToken">staking_pool::DelegationToken</a>&gt;</code>
</dt>
<dd>
 The pool tokens representing the amount of rewards the delegator can get back when they withdraw
 from the pool.
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
<code>validator_address: <b>address</b></code>
</dt>
<dd>
 The validator we are staking with.
</dd>
<dt>
<code>pool_starting_epoch: u64</code>
</dt>
<dd>
 The epoch at which the staking pool started operating.
</dd>
<dt>
<code>delegation_request_epoch: u64</code>
</dt>
<dd>
 The epoch at which the delegation is requested.
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



<a name="0x2_staking_pool_EPENDING_DELEGATION_DOES_NOT_EXIST"></a>



<pre><code><b>const</b> <a href="staking_pool.md#0x2_staking_pool_EPENDING_DELEGATION_DOES_NOT_EXIST">EPENDING_DELEGATION_DOES_NOT_EXIST</a>: u64 = 8;
</code></pre>



<a name="0x2_staking_pool_ETOKEN_TIME_LOCK_IS_SOME"></a>



<pre><code><b>const</b> <a href="staking_pool.md#0x2_staking_pool_ETOKEN_TIME_LOCK_IS_SOME">ETOKEN_TIME_LOCK_IS_SOME</a>: u64 = 6;
</code></pre>



<a name="0x2_staking_pool_EWITHDRAW_AMOUNT_CANNOT_BE_ZERO"></a>



<pre><code><b>const</b> <a href="staking_pool.md#0x2_staking_pool_EWITHDRAW_AMOUNT_CANNOT_BE_ZERO">EWITHDRAW_AMOUNT_CANNOT_BE_ZERO</a>: u64 = 2;
</code></pre>



<a name="0x2_staking_pool_EWRONG_DELEGATION"></a>



<pre><code><b>const</b> <a href="staking_pool.md#0x2_staking_pool_EWRONG_DELEGATION">EWRONG_DELEGATION</a>: u64 = 7;
</code></pre>



<a name="0x2_staking_pool_EWRONG_POOL"></a>



<pre><code><b>const</b> <a href="staking_pool.md#0x2_staking_pool_EWRONG_POOL">EWRONG_POOL</a>: u64 = 1;
</code></pre>



<a name="0x2_staking_pool_new"></a>

## Function `new`

Create a new, empty staking pool.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_new">new</a>(validator_address: <b>address</b>, starting_epoch: u64, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="staking_pool.md#0x2_staking_pool_StakingPool">staking_pool::StakingPool</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_new">new</a>(validator_address: <b>address</b>, starting_epoch: u64, ctx: &<b>mut</b> TxContext) : <a href="staking_pool.md#0x2_staking_pool_StakingPool">StakingPool</a> {
    <a href="staking_pool.md#0x2_staking_pool_StakingPool">StakingPool</a> {
        validator_address,
        starting_epoch,
        sui_balance: 0,
        rewards_pool: <a href="balance.md#0x2_balance_zero">balance::zero</a>(),
        delegation_token_supply: <a href="balance.md#0x2_balance_create_supply">balance::create_supply</a>(<a href="staking_pool.md#0x2_staking_pool_DelegationToken">DelegationToken</a> {}),
        pending_delegations: <a href="linked_table.md#0x2_linked_table_new">linked_table::new</a>(ctx),
        pending_withdraws: <a href="table_vec.md#0x2_table_vec_empty">table_vec::empty</a>(ctx),
    }
}
</code></pre>



</details>

<a name="0x2_staking_pool_request_add_delegation"></a>

## Function `request_add_delegation`

Request to delegate to a staking pool. The delegation gets counted at the beginning of the next epoch,
when the delegation object containing the pool tokens is distributed to the delegator.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_request_add_delegation">request_add_delegation</a>(pool: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_StakingPool">staking_pool::StakingPool</a>, <a href="stake.md#0x2_stake">stake</a>: <a href="balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="sui.md#0x2_sui_SUI">sui::SUI</a>&gt;, sui_token_lock: <a href="_Option">option::Option</a>&lt;<a href="epoch_time_lock.md#0x2_epoch_time_lock_EpochTimeLock">epoch_time_lock::EpochTimeLock</a>&gt;, delegator: <b>address</b>, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_request_add_delegation">request_add_delegation</a>(
    pool: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_StakingPool">StakingPool</a>,
    <a href="stake.md#0x2_stake">stake</a>: Balance&lt;SUI&gt;,
    sui_token_lock: Option&lt;EpochTimeLock&gt;,
    delegator: <b>address</b>,
    ctx: &<b>mut</b> TxContext
) {
    <b>let</b> sui_amount = <a href="balance.md#0x2_balance_value">balance::value</a>(&<a href="stake.md#0x2_stake">stake</a>);
    <b>assert</b>!(sui_amount &gt; 0, 0);
    <b>let</b> staked_sui = <a href="staking_pool.md#0x2_staking_pool_StakedSui">StakedSui</a> {
        id: <a href="object.md#0x2_object_new">object::new</a>(ctx),
        validator_address: pool.validator_address,
        pool_starting_epoch: pool.starting_epoch,
        delegation_request_epoch: <a href="tx_context.md#0x2_tx_context_epoch">tx_context::epoch</a>(ctx),
        principal: <a href="stake.md#0x2_stake">stake</a>,
        sui_token_lock,
    };
    // insert delegation info into the pending_delegations <a href="table.md#0x2_table">table</a>.
    <a href="linked_table.md#0x2_linked_table_push_back">linked_table::push_back</a>(
        &<b>mut</b> pool.pending_delegations,
        <a href="object.md#0x2_object_id">object::id</a>(&staked_sui),
        <a href="staking_pool.md#0x2_staking_pool_PendingDelegationEntry">PendingDelegationEntry</a> { delegator, sui_amount }
    );
    <a href="transfer.md#0x2_transfer_transfer">transfer::transfer</a>(staked_sui, delegator);
}
</code></pre>



</details>

<a name="0x2_staking_pool_request_withdraw_delegation"></a>

## Function `request_withdraw_delegation`

Request to withdraw <code>principal_withdraw_amount</code> of stake plus rewards from a staking pool.
This amount of principal in SUI is withdrawn and transferred to the delegator. A proportional amount
of pool tokens will be later burnt.
The rewards portion will be withdrawn at the end of the epoch, after the rewards have come in so we
can use the new exchange rate to calculate the rewards.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_request_withdraw_delegation">request_withdraw_delegation</a>(pool: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_StakingPool">staking_pool::StakingPool</a>, delegation: <a href="staking_pool.md#0x2_staking_pool_Delegation">staking_pool::Delegation</a>, staked_sui: <a href="staking_pool.md#0x2_staking_pool_StakedSui">staking_pool::StakedSui</a>, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_request_withdraw_delegation">request_withdraw_delegation</a>(
    pool: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_StakingPool">StakingPool</a>,
    delegation: <a href="staking_pool.md#0x2_staking_pool_Delegation">Delegation</a>,
    staked_sui: <a href="staking_pool.md#0x2_staking_pool_StakedSui">StakedSui</a>,
    ctx: &<b>mut</b> TxContext
) : u64 {
    <b>let</b> (withdrawn_pool_tokens, principal_withdraw, time_lock) =
        <a href="staking_pool.md#0x2_staking_pool_withdraw_from_principal">withdraw_from_principal</a>(pool, delegation, staked_sui);

    <b>let</b> delegator = <a href="tx_context.md#0x2_tx_context_sender">tx_context::sender</a>(ctx);
    <b>let</b> principal_withdraw_amount = <a href="balance.md#0x2_balance_value">balance::value</a>(&principal_withdraw);
    <a href="table_vec.md#0x2_table_vec_push_back">table_vec::push_back</a>(&<b>mut</b> pool.pending_withdraws, <a href="staking_pool.md#0x2_staking_pool_PendingWithdrawEntry">PendingWithdrawEntry</a> {
        delegator, principal_withdraw_amount, withdrawn_pool_tokens });

    // TODO: implement withdraw bonding period here.
    <b>if</b> (<a href="_is_some">option::is_some</a>(&time_lock)) {
        <a href="locked_coin.md#0x2_locked_coin_new_from_balance">locked_coin::new_from_balance</a>(principal_withdraw, <a href="_destroy_some">option::destroy_some</a>(time_lock), delegator, ctx);
    } <b>else</b> {
        <a href="transfer.md#0x2_transfer_transfer">transfer::transfer</a>(<a href="coin.md#0x2_coin_from_balance">coin::from_balance</a>(principal_withdraw, ctx), delegator);
        <a href="_destroy_none">option::destroy_none</a>(time_lock);
    };
    principal_withdraw_amount
}
</code></pre>



</details>

<a name="0x2_staking_pool_cancel_delegation_request"></a>

## Function `cancel_delegation_request`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_cancel_delegation_request">cancel_delegation_request</a>(pool: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_StakingPool">staking_pool::StakingPool</a>, staked_sui: <a href="staking_pool.md#0x2_staking_pool_StakedSui">staking_pool::StakedSui</a>, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_cancel_delegation_request">cancel_delegation_request</a>(pool: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_StakingPool">StakingPool</a>, staked_sui: <a href="staking_pool.md#0x2_staking_pool_StakedSui">StakedSui</a>, ctx: &<b>mut</b> TxContext) {
    <b>let</b> delegator = <a href="tx_context.md#0x2_tx_context_sender">tx_context::sender</a>(ctx);
    <b>let</b> staked_sui_id = <a href="object.md#0x2_object_id">object::id</a>(&staked_sui);
    <b>assert</b>!(<a href="linked_table.md#0x2_linked_table_contains">linked_table::contains</a>(&<b>mut</b> pool.pending_delegations, staked_sui_id), <a href="staking_pool.md#0x2_staking_pool_EPENDING_DELEGATION_DOES_NOT_EXIST">EPENDING_DELEGATION_DOES_NOT_EXIST</a>);

    <a href="linked_table.md#0x2_linked_table_remove">linked_table::remove</a>(&<b>mut</b> pool.pending_delegations, staked_sui_id);

    <b>let</b> <a href="staking_pool.md#0x2_staking_pool_StakedSui">StakedSui</a> {
        id,
        validator_address,
        pool_starting_epoch,
        delegation_request_epoch: _,
        principal,
        sui_token_lock
    } = staked_sui;

    // sanity check that the <a href="staking_pool.md#0x2_staking_pool_StakedSui">StakedSui</a> is indeed from this pool. Should never fail.
    <b>assert</b>!(
        validator_address == pool.validator_address &&
        pool_starting_epoch == pool.starting_epoch,
        <a href="staking_pool.md#0x2_staking_pool_EWRONG_POOL">EWRONG_POOL</a>
    );
    <a href="object.md#0x2_object_delete">object::delete</a>(id);
    <b>if</b> (<a href="_is_some">option::is_some</a>(&sui_token_lock)) {
        <a href="locked_coin.md#0x2_locked_coin_new_from_balance">locked_coin::new_from_balance</a>(principal, <a href="_destroy_some">option::destroy_some</a>(sui_token_lock), delegator, ctx);
    } <b>else</b> {
        <a href="transfer.md#0x2_transfer_transfer">transfer::transfer</a>(<a href="coin.md#0x2_coin_from_balance">coin::from_balance</a>(principal, ctx), delegator);
        <a href="_destroy_none">option::destroy_none</a>(sui_token_lock);
    };
}
</code></pre>



</details>

<a name="0x2_staking_pool_withdraw_from_principal"></a>

## Function `withdraw_from_principal`

Withdraw the requested amount of the principal SUI stored in the StakedSui object, as
well as a proportional amount of pool tokens from the delegation object.
For example, suppose the delegation object contains 15 pool tokens and the principal SUI
amount is 21. Then if <code>principal_withdraw_amount</code> is 7, 5 pool tokens and 7 SUI tokens will
be withdrawn.
Returns values are withdrawn pool tokens, withdrawn principal portion of SUI, and its
time lock if applicable.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_withdraw_from_principal">withdraw_from_principal</a>(pool: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_StakingPool">staking_pool::StakingPool</a>, delegation: <a href="staking_pool.md#0x2_staking_pool_Delegation">staking_pool::Delegation</a>, staked_sui: <a href="staking_pool.md#0x2_staking_pool_StakedSui">staking_pool::StakedSui</a>): (<a href="balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="staking_pool.md#0x2_staking_pool_DelegationToken">staking_pool::DelegationToken</a>&gt;, <a href="balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="sui.md#0x2_sui_SUI">sui::SUI</a>&gt;, <a href="_Option">option::Option</a>&lt;<a href="epoch_time_lock.md#0x2_epoch_time_lock_EpochTimeLock">epoch_time_lock::EpochTimeLock</a>&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_withdraw_from_principal">withdraw_from_principal</a>(
    pool: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_StakingPool">StakingPool</a>,
    delegation: <a href="staking_pool.md#0x2_staking_pool_Delegation">Delegation</a>,
    staked_sui: <a href="staking_pool.md#0x2_staking_pool_StakedSui">StakedSui</a>,
) : (Balance&lt;<a href="staking_pool.md#0x2_staking_pool_DelegationToken">DelegationToken</a>&gt;, Balance&lt;SUI&gt;, Option&lt;EpochTimeLock&gt;) {
    // Check that the delegation and staked <a href="sui.md#0x2_sui">sui</a> objects match.
    <b>assert</b>!(<a href="object.md#0x2_object_id">object::id</a>(&staked_sui) == delegation.staked_sui_id, <a href="staking_pool.md#0x2_staking_pool_EWRONG_DELEGATION">EWRONG_DELEGATION</a>);

    // Check that the delegation information matches the pool.
    <b>assert</b>!(
        staked_sui.validator_address == pool.validator_address &&
        staked_sui.pool_starting_epoch == pool.starting_epoch,
        <a href="staking_pool.md#0x2_staking_pool_EWRONG_POOL">EWRONG_POOL</a>
    );

    <b>assert</b>!(delegation.principal_sui_amount == <a href="balance.md#0x2_balance_value">balance::value</a>(&staked_sui.principal), <a href="staking_pool.md#0x2_staking_pool_EINSUFFICIENT_SUI_TOKEN_BALANCE">EINSUFFICIENT_SUI_TOKEN_BALANCE</a>);

    <b>let</b> pool_tokens = <a href="staking_pool.md#0x2_staking_pool_destroy_delegation_and_return_pool_tokens">destroy_delegation_and_return_pool_tokens</a>(delegation);
    <b>let</b> (principal_withdraw, time_lock) = <a href="staking_pool.md#0x2_staking_pool_unwrap_staked_sui">unwrap_staked_sui</a>(staked_sui);

    (
        pool_tokens,
        principal_withdraw,
        time_lock
    )
}
</code></pre>



</details>

<a name="0x2_staking_pool_destroy_delegation_and_return_pool_tokens"></a>

## Function `destroy_delegation_and_return_pool_tokens`



<pre><code><b>fun</b> <a href="staking_pool.md#0x2_staking_pool_destroy_delegation_and_return_pool_tokens">destroy_delegation_and_return_pool_tokens</a>(delegation: <a href="staking_pool.md#0x2_staking_pool_Delegation">staking_pool::Delegation</a>): <a href="balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="staking_pool.md#0x2_staking_pool_DelegationToken">staking_pool::DelegationToken</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="staking_pool.md#0x2_staking_pool_destroy_delegation_and_return_pool_tokens">destroy_delegation_and_return_pool_tokens</a>(delegation: <a href="staking_pool.md#0x2_staking_pool_Delegation">Delegation</a>): Balance&lt;<a href="staking_pool.md#0x2_staking_pool_DelegationToken">DelegationToken</a>&gt; {
    <b>let</b> <a href="staking_pool.md#0x2_staking_pool_Delegation">Delegation</a> { id, staked_sui_id: _, pool_tokens, principal_sui_amount: _ } = delegation;
    <a href="object.md#0x2_object_delete">object::delete</a>(id);
    pool_tokens
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
        validator_address: _,
        pool_starting_epoch: _,
        delegation_request_epoch: _,
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

<a name="0x2_staking_pool_process_pending_delegation_withdraws"></a>

## Function `process_pending_delegation_withdraws`

Called at epoch boundaries to process pending delegation withdraws requested during the epoch.
For each pending withdraw entry, we withdraw the rewards from the pool at the new exchange rate and burn the pool
tokens.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_process_pending_delegation_withdraws">process_pending_delegation_withdraws</a>(pool: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_StakingPool">staking_pool::StakingPool</a>, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_process_pending_delegation_withdraws">process_pending_delegation_withdraws</a>(pool: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_StakingPool">StakingPool</a>, ctx: &<b>mut</b> TxContext) : u64 {
    <b>let</b> total_reward_withdraw = 0;

    <b>while</b> (!<a href="table_vec.md#0x2_table_vec_is_empty">table_vec::is_empty</a>(&pool.pending_withdraws)) {
        <b>let</b> <a href="staking_pool.md#0x2_staking_pool_PendingWithdrawEntry">PendingWithdrawEntry</a> {
            delegator, principal_withdraw_amount, withdrawn_pool_tokens
        } = <a href="table_vec.md#0x2_table_vec_pop_back">table_vec::pop_back</a>(&<b>mut</b> pool.pending_withdraws);
        <b>let</b> reward_withdraw = <a href="staking_pool.md#0x2_staking_pool_withdraw_rewards_and_burn_pool_tokens">withdraw_rewards_and_burn_pool_tokens</a>(pool, principal_withdraw_amount, withdrawn_pool_tokens);
        total_reward_withdraw = total_reward_withdraw + <a href="balance.md#0x2_balance_value">balance::value</a>(&reward_withdraw);
        <a href="transfer.md#0x2_transfer_transfer">transfer::transfer</a>(<a href="coin.md#0x2_coin_from_balance">coin::from_balance</a>(reward_withdraw, ctx), delegator);
    };
    total_reward_withdraw
}
</code></pre>



</details>

<a name="0x2_staking_pool_process_pending_delegations"></a>

## Function `process_pending_delegations`

Called at epoch boundaries to mint new pool tokens to new delegators at the new exchange rate.
New delegators include both entirely new delegations and delegations switched to this staking pool
during the previous epoch.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_process_pending_delegations">process_pending_delegations</a>(pool: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_StakingPool">staking_pool::StakingPool</a>, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_process_pending_delegations">process_pending_delegations</a>(pool: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_StakingPool">StakingPool</a>, ctx: &<b>mut</b> TxContext) {
    <b>while</b> (!<a href="linked_table.md#0x2_linked_table_is_empty">linked_table::is_empty</a>(&pool.pending_delegations)) {
        <b>let</b> (staked_sui_id, <a href="staking_pool.md#0x2_staking_pool_PendingDelegationEntry">PendingDelegationEntry</a> { delegator, sui_amount }) =
            <a href="linked_table.md#0x2_linked_table_pop_back">linked_table::pop_back</a>(&<b>mut</b> pool.pending_delegations);
        <a href="staking_pool.md#0x2_staking_pool_mint_delegation_tokens_to_delegator">mint_delegation_tokens_to_delegator</a>(pool, delegator, sui_amount, staked_sui_id, ctx);
        pool.sui_balance = pool.sui_balance + sui_amount;
    };
}
</code></pre>



</details>

<a name="0x2_staking_pool_batch_withdraw_rewards_and_burn_pool_tokens"></a>

## Function `batch_withdraw_rewards_and_burn_pool_tokens`

Called by validator_set at epoch boundaries for delegation switches.
This function goes through the provided vector of pending withdraw entries,
and for each entry, calls <code>withdraw_rewards_and_burn_pool_tokens</code> to withdraw
the rewards portion of the delegation and burn the pool tokens. We then aggregate
the delegator addresses and their rewards into vectors, as well as calculate
the total amount of rewards SUI withdrawn. These three return values are then
used in <code><a href="validator_set.md#0x2_validator_set">validator_set</a></code>'s delegation switching code to deposit the rewards part
into the new validator's staking pool.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_batch_withdraw_rewards_and_burn_pool_tokens">batch_withdraw_rewards_and_burn_pool_tokens</a>(pool: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_StakingPool">staking_pool::StakingPool</a>, entries: <a href="table_vec.md#0x2_table_vec_TableVec">table_vec::TableVec</a>&lt;<a href="staking_pool.md#0x2_staking_pool_PendingWithdrawEntry">staking_pool::PendingWithdrawEntry</a>&gt;): (<a href="">vector</a>&lt;<b>address</b>&gt;, <a href="">vector</a>&lt;<a href="balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="sui.md#0x2_sui_SUI">sui::SUI</a>&gt;&gt;, u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_batch_withdraw_rewards_and_burn_pool_tokens">batch_withdraw_rewards_and_burn_pool_tokens</a>(
    pool: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_StakingPool">StakingPool</a>,
    entries: TableVec&lt;<a href="staking_pool.md#0x2_staking_pool_PendingWithdrawEntry">PendingWithdrawEntry</a>&gt;,
) : (<a href="">vector</a>&lt;<b>address</b>&gt;, <a href="">vector</a>&lt;Balance&lt;SUI&gt;&gt;, u64) {
    <b>let</b> (delegators, rewards, total_rewards_withdraw_amount) = (<a href="_empty">vector::empty</a>(), <a href="_empty">vector::empty</a>(), 0);
    <b>while</b> (!<a href="table_vec.md#0x2_table_vec_is_empty">table_vec::is_empty</a>(&<b>mut</b> entries)) {
        <b>let</b> <a href="staking_pool.md#0x2_staking_pool_PendingWithdrawEntry">PendingWithdrawEntry</a> { delegator, principal_withdraw_amount, withdrawn_pool_tokens }
            = <a href="table_vec.md#0x2_table_vec_pop_back">table_vec::pop_back</a>(&<b>mut</b> entries);
        <b>let</b> reward = <a href="staking_pool.md#0x2_staking_pool_withdraw_rewards_and_burn_pool_tokens">withdraw_rewards_and_burn_pool_tokens</a>(pool, principal_withdraw_amount, withdrawn_pool_tokens);
        total_rewards_withdraw_amount = total_rewards_withdraw_amount + <a href="balance.md#0x2_balance_value">balance::value</a>(&reward);
        <a href="_push_back">vector::push_back</a>(&<b>mut</b> delegators, delegator);
        <a href="_push_back">vector::push_back</a>(&<b>mut</b> rewards, reward);
    };
    <a href="table_vec.md#0x2_table_vec_destroy_empty">table_vec::destroy_empty</a>(entries);
    (delegators, rewards, total_rewards_withdraw_amount)
}
</code></pre>



</details>

<a name="0x2_staking_pool_withdraw_rewards_and_burn_pool_tokens"></a>

## Function `withdraw_rewards_and_burn_pool_tokens`

This function does the following:
1. Calculates the total amount of SUI (including principal and rewards) that the provided pool tokens represent
at the current exchange rate.
2. Using the above number and the given <code>principal_withdraw_amount</code>, calculates the rewards portion of the
delegation we should withdraw.
3. Withdraws the rewards portion from the rewards pool at the current exchange rate. We only withdraw the rewards
portion because the principal portion was already taken out of the delegator's self custodied StakedSui at request
time in <code>request_withdraw_stake</code>.
4. Since SUI tokens are withdrawn, we need to burn the corresponding pool tokens to keep the exchange rate the same.
5. Updates the SUI balance amount of the pool.


<pre><code><b>fun</b> <a href="staking_pool.md#0x2_staking_pool_withdraw_rewards_and_burn_pool_tokens">withdraw_rewards_and_burn_pool_tokens</a>(pool: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_StakingPool">staking_pool::StakingPool</a>, principal_withdraw_amount: u64, withdrawn_pool_tokens: <a href="balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="staking_pool.md#0x2_staking_pool_DelegationToken">staking_pool::DelegationToken</a>&gt;): <a href="balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="sui.md#0x2_sui_SUI">sui::SUI</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="staking_pool.md#0x2_staking_pool_withdraw_rewards_and_burn_pool_tokens">withdraw_rewards_and_burn_pool_tokens</a>(
    pool: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_StakingPool">StakingPool</a>,
    principal_withdraw_amount: u64,
    withdrawn_pool_tokens: Balance&lt;<a href="staking_pool.md#0x2_staking_pool_DelegationToken">DelegationToken</a>&gt;,
) : Balance&lt;SUI&gt; {
    <b>let</b> pool_token_amount = <a href="balance.md#0x2_balance_value">balance::value</a>(&withdrawn_pool_tokens);
    <b>let</b> total_sui_withdraw_amount = <a href="staking_pool.md#0x2_staking_pool_get_sui_amount">get_sui_amount</a>(pool, pool_token_amount);
    <b>let</b> reward_withdraw_amount =
        <b>if</b> (total_sui_withdraw_amount &gt;= principal_withdraw_amount)
            total_sui_withdraw_amount - principal_withdraw_amount
        <b>else</b> 0;
    // This may happen when we are withdrawing everything from the pool and
    // the rewards pool <a href="balance.md#0x2_balance">balance</a> may be less than reward_withdraw_amount.
    // TODO: FIGURE OUT EXACTLY WHY THIS CAN HAPPEN.
    reward_withdraw_amount = <a href="math.md#0x2_math_min">math::min</a>(reward_withdraw_amount, <a href="balance.md#0x2_balance_value">balance::value</a>(&pool.rewards_pool));
    <a href="balance.md#0x2_balance_decrease_supply">balance::decrease_supply</a>(
        &<b>mut</b> pool.delegation_token_supply,
        withdrawn_pool_tokens
    );
    pool.sui_balance = pool.sui_balance - (principal_withdraw_amount + reward_withdraw_amount);
    <a href="balance.md#0x2_balance_split">balance::split</a>(&<b>mut</b> pool.rewards_pool, reward_withdraw_amount)
}
</code></pre>



</details>

<a name="0x2_staking_pool_mint_delegation_tokens_to_delegator"></a>

## Function `mint_delegation_tokens_to_delegator`

Given the <code>sui_amount</code>, mint the corresponding amount of pool tokens at the current exchange
rate, puts the pool tokens in a delegation object, and gives the delegation object to the delegator.


<pre><code><b>fun</b> <a href="staking_pool.md#0x2_staking_pool_mint_delegation_tokens_to_delegator">mint_delegation_tokens_to_delegator</a>(pool: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_StakingPool">staking_pool::StakingPool</a>, delegator: <b>address</b>, sui_amount: u64, staked_sui_id: <a href="object.md#0x2_object_ID">object::ID</a>, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="staking_pool.md#0x2_staking_pool_mint_delegation_tokens_to_delegator">mint_delegation_tokens_to_delegator</a>(
    pool: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_StakingPool">StakingPool</a>,
    delegator: <b>address</b>,
    sui_amount: u64,
    staked_sui_id: ID,
    ctx: &<b>mut</b> TxContext
) {
    <b>let</b> new_pool_token_amount = <a href="staking_pool.md#0x2_staking_pool_get_token_amount">get_token_amount</a>(pool, sui_amount);

    // Mint new pool tokens at the current exchange rate.
    <b>let</b> pool_tokens = <a href="balance.md#0x2_balance_increase_supply">balance::increase_supply</a>(&<b>mut</b> pool.delegation_token_supply, new_pool_token_amount);

    <b>let</b> delegation = <a href="staking_pool.md#0x2_staking_pool_Delegation">Delegation</a> {
        id: <a href="object.md#0x2_object_new">object::new</a>(ctx),
        staked_sui_id,
        pool_tokens,
        principal_sui_amount: sui_amount,
    };

    <a href="transfer.md#0x2_transfer_transfer">transfer::transfer</a>(delegation, delegator);
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

Withdraw delegation from an inactive pool. Since no epoch rewards will be added to an inactive pool,
the exchange rate between pool tokens and SUI tokens stay the same. Therefore, unlike withdrawing
from an active pool, we can handle both principal and rewards withdraws directly here.


<pre><code><b>public</b> entry <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_withdraw_from_inactive_pool">withdraw_from_inactive_pool</a>(inactive_pool: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_InactiveStakingPool">staking_pool::InactiveStakingPool</a>, staked_sui: <a href="staking_pool.md#0x2_staking_pool_StakedSui">staking_pool::StakedSui</a>, delegation: <a href="staking_pool.md#0x2_staking_pool_Delegation">staking_pool::Delegation</a>, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_withdraw_from_inactive_pool">withdraw_from_inactive_pool</a>(
    inactive_pool: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_InactiveStakingPool">InactiveStakingPool</a>,
    staked_sui: <a href="staking_pool.md#0x2_staking_pool_StakedSui">StakedSui</a>,
    delegation: <a href="staking_pool.md#0x2_staking_pool_Delegation">Delegation</a>,
    ctx: &<b>mut</b> TxContext
) {
    <b>let</b> pool = &<b>mut</b> inactive_pool.pool;
    <b>let</b> (withdrawn_pool_tokens, principal_withdraw, time_lock) =
        <a href="staking_pool.md#0x2_staking_pool_withdraw_from_principal">withdraw_from_principal</a>(pool, delegation, staked_sui);
    <b>let</b> principal_withdraw_amount = <a href="balance.md#0x2_balance_value">balance::value</a>(&principal_withdraw);
    <b>let</b> rewards_withdraw = <a href="staking_pool.md#0x2_staking_pool_withdraw_rewards_and_burn_pool_tokens">withdraw_rewards_and_burn_pool_tokens</a>(pool, principal_withdraw_amount, withdrawn_pool_tokens);
    <b>let</b> total_withdraw_amount = principal_withdraw_amount + <a href="balance.md#0x2_balance_value">balance::value</a>(&rewards_withdraw);
    pool.sui_balance = pool.sui_balance - total_withdraw_amount;

    <b>let</b> delegator = <a href="tx_context.md#0x2_tx_context_sender">tx_context::sender</a>(ctx);
    // TODO: implement withdraw bonding period here.
    <b>if</b> (<a href="_is_some">option::is_some</a>(&time_lock)) {
        <a href="locked_coin.md#0x2_locked_coin_new_from_balance">locked_coin::new_from_balance</a>(principal_withdraw, <a href="_destroy_some">option::destroy_some</a>(time_lock), delegator, ctx);
        <a href="transfer.md#0x2_transfer_transfer">transfer::transfer</a>(<a href="coin.md#0x2_coin_from_balance">coin::from_balance</a>(rewards_withdraw, ctx), delegator);
    } <b>else</b> {
        <a href="balance.md#0x2_balance_join">balance::join</a>(&<b>mut</b> principal_withdraw, rewards_withdraw);
        <a href="transfer.md#0x2_transfer_transfer">transfer::transfer</a>(<a href="coin.md#0x2_coin_from_balance">coin::from_balance</a>(principal_withdraw, ctx), delegator);
        <a href="_destroy_none">option::destroy_none</a>(time_lock);
    };
}
</code></pre>



</details>

<a name="0x2_staking_pool_destroy_empty_delegation"></a>

## Function `destroy_empty_delegation`

Destroy an empty delegation that no longer contains any SUI or pool tokens.


<pre><code><b>public</b> entry <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_destroy_empty_delegation">destroy_empty_delegation</a>(delegation: <a href="staking_pool.md#0x2_staking_pool_Delegation">staking_pool::Delegation</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_destroy_empty_delegation">destroy_empty_delegation</a>(delegation: <a href="staking_pool.md#0x2_staking_pool_Delegation">Delegation</a>) {
    <b>let</b> <a href="staking_pool.md#0x2_staking_pool_Delegation">Delegation</a> {
        id,
        staked_sui_id: _,
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


<pre><code><b>public</b> entry <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_destroy_empty_staked_sui">destroy_empty_staked_sui</a>(staked_sui: <a href="staking_pool.md#0x2_staking_pool_StakedSui">staking_pool::StakedSui</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_destroy_empty_staked_sui">destroy_empty_staked_sui</a>(staked_sui: <a href="staking_pool.md#0x2_staking_pool_StakedSui">StakedSui</a>) {
    <b>let</b> <a href="staking_pool.md#0x2_staking_pool_StakedSui">StakedSui</a> {
        id,
        validator_address: _,
        pool_starting_epoch: _,
        delegation_request_epoch: _,
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


<pre><code><b>public</b> <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_sui_balance">sui_balance</a>(pool: &<a href="staking_pool.md#0x2_staking_pool_StakingPool">StakingPool</a>) : u64 { pool.sui_balance }
</code></pre>



</details>

<a name="0x2_staking_pool_validator_address"></a>

## Function `validator_address`



<pre><code><b>public</b> <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_validator_address">validator_address</a>(staked_sui: &<a href="staking_pool.md#0x2_staking_pool_StakedSui">staking_pool::StakedSui</a>): <b>address</b>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_validator_address">validator_address</a>(staked_sui: &<a href="staking_pool.md#0x2_staking_pool_StakedSui">StakedSui</a>) : <b>address</b> { staked_sui.validator_address }
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

<a name="0x2_staking_pool_delegation_request_epoch"></a>

## Function `delegation_request_epoch`



<pre><code><b>public</b> <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_delegation_request_epoch">delegation_request_epoch</a>(staked_sui: &<a href="staking_pool.md#0x2_staking_pool_StakedSui">staking_pool::StakedSui</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_delegation_request_epoch">delegation_request_epoch</a>(staked_sui: &<a href="staking_pool.md#0x2_staking_pool_StakedSui">StakedSui</a>): u64 {
    staked_sui.delegation_request_epoch
}
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

<a name="0x2_staking_pool_pool_token_exchange_rate"></a>

## Function `pool_token_exchange_rate`



<pre><code><b>public</b> <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_pool_token_exchange_rate">pool_token_exchange_rate</a>(pool: &<a href="staking_pool.md#0x2_staking_pool_StakingPool">staking_pool::StakingPool</a>): <a href="staking_pool.md#0x2_staking_pool_PoolTokenExchangeRate">staking_pool::PoolTokenExchangeRate</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_pool_token_exchange_rate">pool_token_exchange_rate</a>(pool: &<a href="staking_pool.md#0x2_staking_pool_StakingPool">StakingPool</a>): <a href="staking_pool.md#0x2_staking_pool_PoolTokenExchangeRate">PoolTokenExchangeRate</a> {
    <a href="staking_pool.md#0x2_staking_pool_PoolTokenExchangeRate">PoolTokenExchangeRate</a> {
        sui_amount: pool.sui_balance,
        pool_token_amount: <a href="balance.md#0x2_balance_supply_value">balance::supply_value</a>(&pool.delegation_token_supply),
    }
}
</code></pre>



</details>

<a name="0x2_staking_pool_new_pending_withdraw_entry"></a>

## Function `new_pending_withdraw_entry`

Create a new pending withdraw entry.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_new_pending_withdraw_entry">new_pending_withdraw_entry</a>(delegator: <b>address</b>, principal_withdraw_amount: u64, withdrawn_pool_tokens: <a href="balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="staking_pool.md#0x2_staking_pool_DelegationToken">staking_pool::DelegationToken</a>&gt;): <a href="staking_pool.md#0x2_staking_pool_PendingWithdrawEntry">staking_pool::PendingWithdrawEntry</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="staking_pool.md#0x2_staking_pool_new_pending_withdraw_entry">new_pending_withdraw_entry</a>(
    delegator: <b>address</b>,
    principal_withdraw_amount: u64,
    withdrawn_pool_tokens: Balance&lt;<a href="staking_pool.md#0x2_staking_pool_DelegationToken">DelegationToken</a>&gt;,
) : <a href="staking_pool.md#0x2_staking_pool_PendingWithdrawEntry">PendingWithdrawEntry</a> {
    <a href="staking_pool.md#0x2_staking_pool_PendingWithdrawEntry">PendingWithdrawEntry</a> { delegator, principal_withdraw_amount, withdrawn_pool_tokens }
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
    <b>let</b> token_supply = <a href="balance.md#0x2_balance_supply_value">balance::supply_value</a>(&pool.delegation_token_supply);
    <b>if</b> (token_supply == 0) {
        <b>return</b> token_amount
    };
    <b>let</b> res = (pool.sui_balance <b>as</b> u128)
            * (token_amount <b>as</b> u128)
            / (token_supply <b>as</b> u128);
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
    <b>if</b> (pool.sui_balance == 0) {
        <b>return</b> sui_amount
    };
    <b>let</b> token_supply = <a href="balance.md#0x2_balance_supply_value">balance::supply_value</a>(&pool.delegation_token_supply);
    <b>let</b> res = (token_supply <b>as</b> u128)
            * (sui_amount <b>as</b> u128)
            / (pool.sui_balance <b>as</b> u128);
    (res <b>as</b> u64)
}
</code></pre>



</details>
