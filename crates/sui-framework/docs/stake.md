
<a name="0x2_stake"></a>

# Module `0x2::stake`



-  [Resource `Stake`](#0x2_stake_Stake)
-  [Constants](#@Constants_0)
-  [Function `create`](#0x2_stake_create)
-  [Function `withdraw_stake`](#0x2_stake_withdraw_stake)
-  [Function `burn`](#0x2_stake_burn)
-  [Function `value`](#0x2_stake_value)


<pre><code><b>use</b> <a href="">0x1::option</a>;
<b>use</b> <a href="balance.md#0x2_balance">0x2::balance</a>;
<b>use</b> <a href="epoch_time_lock.md#0x2_epoch_time_lock">0x2::epoch_time_lock</a>;
<b>use</b> <a href="locked_coin.md#0x2_locked_coin">0x2::locked_coin</a>;
<b>use</b> <a href="math.md#0x2_math">0x2::math</a>;
<b>use</b> <a href="object.md#0x2_object">0x2::object</a>;
<b>use</b> <a href="sui.md#0x2_sui">0x2::sui</a>;
<b>use</b> <a href="transfer.md#0x2_transfer">0x2::transfer</a>;
<b>use</b> <a href="tx_context.md#0x2_tx_context">0x2::tx_context</a>;
</code></pre>



<a name="0x2_stake_Stake"></a>

## Resource `Stake`

A custodial stake object holding the staked SUI coin.


<pre><code><b>struct</b> <a href="stake.md#0x2_stake_Stake">Stake</a> <b>has</b> key
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
<code><a href="balance.md#0x2_balance">balance</a>: <a href="balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="sui.md#0x2_sui_SUI">sui::SUI</a>&gt;</code>
</dt>
<dd>
 The staked SUI tokens.
</dd>
<dt>
<code>locked_until_epoch: <a href="_Option">option::Option</a>&lt;<a href="epoch_time_lock.md#0x2_epoch_time_lock_EpochTimeLock">epoch_time_lock::EpochTimeLock</a>&gt;</code>
</dt>
<dd>
 The epoch until which the staked coin is locked. If the stake
 comes from a Coin<SUI>, this field is None. If it comes from a LockedCoin<SUI>, this
 field will record the original lock expiration epoch, to be used when unstaking.
</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0x2_stake_BONDING_PERIOD"></a>

The number of epochs the withdrawn stake is locked for.
TODO: this is a placehodler number and may be changed.


<pre><code><b>const</b> <a href="stake.md#0x2_stake_BONDING_PERIOD">BONDING_PERIOD</a>: u64 = 1;
</code></pre>



<a name="0x2_stake_ENonzeroBalance"></a>

Error number for when a Stake with nonzero balance is burnt.


<pre><code><b>const</b> <a href="stake.md#0x2_stake_ENonzeroBalance">ENonzeroBalance</a>: u64 = 0;
</code></pre>



<a name="0x2_stake_create"></a>

## Function `create`

Create a stake object from a SUI balance. If the balance comes from a
<code>LockedCoin</code>, an EpochTimeLock is passed in to keep track of locking period.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="stake.md#0x2_stake_create">create</a>(<a href="balance.md#0x2_balance">balance</a>: <a href="balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="sui.md#0x2_sui_SUI">sui::SUI</a>&gt;, recipient: <b>address</b>, locked_until_epoch: <a href="_Option">option::Option</a>&lt;<a href="epoch_time_lock.md#0x2_epoch_time_lock_EpochTimeLock">epoch_time_lock::EpochTimeLock</a>&gt;, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="stake.md#0x2_stake_create">create</a>(
    <a href="balance.md#0x2_balance">balance</a>: Balance&lt;SUI&gt;,
    recipient: <b>address</b>,
    locked_until_epoch: Option&lt;EpochTimeLock&gt;,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> <a href="stake.md#0x2_stake">stake</a> = <a href="stake.md#0x2_stake_Stake">Stake</a> {
        id: <a href="object.md#0x2_object_new">object::new</a>(ctx),
        <a href="balance.md#0x2_balance">balance</a>,
        locked_until_epoch,
    };
    <a href="transfer.md#0x2_transfer_transfer">transfer::transfer</a>(<a href="stake.md#0x2_stake">stake</a>, recipient)
}
</code></pre>



</details>

<a name="0x2_stake_withdraw_stake"></a>

## Function `withdraw_stake`

Withdraw <code>amount</code> from the balance of <code><a href="stake.md#0x2_stake">stake</a></code>.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="stake.md#0x2_stake_withdraw_stake">withdraw_stake</a>(self: &<b>mut</b> <a href="stake.md#0x2_stake_Stake">stake::Stake</a>, amount: u64, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="stake.md#0x2_stake_withdraw_stake">withdraw_stake</a>(
    self: &<b>mut</b> <a href="stake.md#0x2_stake_Stake">Stake</a>,
    amount: u64,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> sender = <a href="tx_context.md#0x2_tx_context_sender">tx_context::sender</a>(ctx);
    <b>let</b> unlock_epoch = <a href="tx_context.md#0x2_tx_context_epoch">tx_context::epoch</a>(ctx) + <a href="stake.md#0x2_stake_BONDING_PERIOD">BONDING_PERIOD</a>;
    <b>let</b> <a href="balance.md#0x2_balance">balance</a> = <a href="balance.md#0x2_balance_split">balance::split</a>(&<b>mut</b> self.<a href="balance.md#0x2_balance">balance</a>, amount);

    <b>if</b> (<a href="_is_none">option::is_none</a>(&self.locked_until_epoch)) {
        // If the <a href="stake.md#0x2_stake">stake</a> didn't come from a locked <a href="coin.md#0x2_coin">coin</a>, we give back the <a href="stake.md#0x2_stake">stake</a> and
        // lock the <a href="coin.md#0x2_coin">coin</a> for `<a href="stake.md#0x2_stake_BONDING_PERIOD">BONDING_PERIOD</a>`.
        <a href="locked_coin.md#0x2_locked_coin_new_from_balance">locked_coin::new_from_balance</a>(<a href="balance.md#0x2_balance">balance</a>, <a href="epoch_time_lock.md#0x2_epoch_time_lock_new">epoch_time_lock::new</a>(unlock_epoch, ctx), sender, ctx);
    } <b>else</b> {
        // If the <a href="stake.md#0x2_stake">stake</a> did come from a locked <a href="coin.md#0x2_coin">coin</a>, we lock the <a href="coin.md#0x2_coin">coin</a> for
        // max(<a href="stake.md#0x2_stake_BONDING_PERIOD">BONDING_PERIOD</a>, remaining_lock_time).
        <b>let</b> original_unlock_epoch = <a href="epoch_time_lock.md#0x2_epoch_time_lock_epoch">epoch_time_lock::epoch</a>(<a href="_borrow">option::borrow</a>(&self.locked_until_epoch));
        <b>let</b> unlock_epoch = <a href="math.md#0x2_math_max">math::max</a>(original_unlock_epoch, unlock_epoch);
        <a href="locked_coin.md#0x2_locked_coin_new_from_balance">locked_coin::new_from_balance</a>(<a href="balance.md#0x2_balance">balance</a>, <a href="epoch_time_lock.md#0x2_epoch_time_lock_new">epoch_time_lock::new</a>(unlock_epoch, ctx), sender, ctx);
    };
}
</code></pre>



</details>

<a name="0x2_stake_burn"></a>

## Function `burn`

Burn the stake object. This can be done only when the stake has a zero balance.


<pre><code><b>public</b> entry <b>fun</b> <a href="stake.md#0x2_stake_burn">burn</a>(self: <a href="stake.md#0x2_stake_Stake">stake::Stake</a>, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="stake.md#0x2_stake_burn">burn</a>(self: <a href="stake.md#0x2_stake_Stake">Stake</a>, ctx: &<b>mut</b> TxContext) {
    <b>let</b> <a href="stake.md#0x2_stake_Stake">Stake</a> { id, <a href="balance.md#0x2_balance">balance</a>, locked_until_epoch } = self;
    <a href="object.md#0x2_object_delete">object::delete</a>(id);
    <a href="balance.md#0x2_balance_destroy_zero">balance::destroy_zero</a>(<a href="balance.md#0x2_balance">balance</a>);
    <b>if</b> (<a href="_is_some">option::is_some</a>(&locked_until_epoch)) {
        <a href="epoch_time_lock.md#0x2_epoch_time_lock_destroy">epoch_time_lock::destroy</a>(<a href="_extract">option::extract</a>(&<b>mut</b> locked_until_epoch), ctx);
    };
    <a href="_destroy_none">option::destroy_none</a>(locked_until_epoch);
}
</code></pre>



</details>

<a name="0x2_stake_value"></a>

## Function `value`



<pre><code><b>public</b> <b>fun</b> <a href="stake.md#0x2_stake_value">value</a>(self: &<a href="stake.md#0x2_stake_Stake">stake::Stake</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="stake.md#0x2_stake_value">value</a>(self: &<a href="stake.md#0x2_stake_Stake">Stake</a>): u64 {
    <a href="balance.md#0x2_balance_value">balance::value</a>(&self.<a href="balance.md#0x2_balance">balance</a>)
}
</code></pre>



</details>
