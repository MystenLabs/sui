
<a name="0x2_locked_coin"></a>

# Module `0x2::locked_coin`



-  [Resource `LockedCoin`](#0x2_locked_coin_LockedCoin)
-  [Function `new_from_balance`](#0x2_locked_coin_new_from_balance)
-  [Function `into_balance`](#0x2_locked_coin_into_balance)
-  [Function `value`](#0x2_locked_coin_value)
-  [Function `lock_coin`](#0x2_locked_coin_lock_coin)
-  [Function `unlock_coin`](#0x2_locked_coin_unlock_coin)


<pre><code><b>use</b> <a href="balance.md#0x2_balance">0x2::balance</a>;
<b>use</b> <a href="coin.md#0x2_coin">0x2::coin</a>;
<b>use</b> <a href="epoch_time_lock.md#0x2_epoch_time_lock">0x2::epoch_time_lock</a>;
<b>use</b> <a href="object.md#0x2_object">0x2::object</a>;
<b>use</b> <a href="transfer.md#0x2_transfer">0x2::transfer</a>;
<b>use</b> <a href="tx_context.md#0x2_tx_context">0x2::tx_context</a>;
</code></pre>



<a name="0x2_locked_coin_LockedCoin"></a>

## Resource `LockedCoin`

A coin of type <code>T</code> locked until <code>locked_until_epoch</code>.


<pre><code><b>struct</b> <a href="locked_coin.md#0x2_locked_coin_LockedCoin">LockedCoin</a>&lt;T&gt; <b>has</b> key
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
<code><a href="balance.md#0x2_balance">balance</a>: <a href="balance.md#0x2_balance_Balance">balance::Balance</a>&lt;T&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>locked_until_epoch: <a href="epoch_time_lock.md#0x2_epoch_time_lock_EpochTimeLock">epoch_time_lock::EpochTimeLock</a></code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x2_locked_coin_new_from_balance"></a>

## Function `new_from_balance`

Create a LockedCoin from <code><a href="balance.md#0x2_balance">balance</a></code> and transfer it to <code>owner</code>.


<pre><code><b>public</b> <b>fun</b> <a href="locked_coin.md#0x2_locked_coin_new_from_balance">new_from_balance</a>&lt;T&gt;(<a href="balance.md#0x2_balance">balance</a>: <a href="balance.md#0x2_balance_Balance">balance::Balance</a>&lt;T&gt;, locked_until_epoch: <a href="epoch_time_lock.md#0x2_epoch_time_lock_EpochTimeLock">epoch_time_lock::EpochTimeLock</a>, owner: <b>address</b>, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="locked_coin.md#0x2_locked_coin_new_from_balance">new_from_balance</a>&lt;T&gt;(<a href="balance.md#0x2_balance">balance</a>: Balance&lt;T&gt;, locked_until_epoch: EpochTimeLock, owner: <b>address</b>, ctx: &<b>mut</b> TxContext) {
    <b>let</b> <a href="locked_coin.md#0x2_locked_coin">locked_coin</a> = <a href="locked_coin.md#0x2_locked_coin_LockedCoin">LockedCoin</a> {
        id: <a href="object.md#0x2_object_new">object::new</a>(ctx),
        <a href="balance.md#0x2_balance">balance</a>,
        locked_until_epoch
    };
    <a href="transfer.md#0x2_transfer_transfer">transfer::transfer</a>(<a href="locked_coin.md#0x2_locked_coin">locked_coin</a>, owner);
}
</code></pre>



</details>

<a name="0x2_locked_coin_into_balance"></a>

## Function `into_balance`

Destruct a LockedCoin wrapper and keep the balance.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="locked_coin.md#0x2_locked_coin_into_balance">into_balance</a>&lt;T&gt;(<a href="coin.md#0x2_coin">coin</a>: <a href="locked_coin.md#0x2_locked_coin_LockedCoin">locked_coin::LockedCoin</a>&lt;T&gt;): (<a href="balance.md#0x2_balance_Balance">balance::Balance</a>&lt;T&gt;, <a href="epoch_time_lock.md#0x2_epoch_time_lock_EpochTimeLock">epoch_time_lock::EpochTimeLock</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="locked_coin.md#0x2_locked_coin_into_balance">into_balance</a>&lt;T&gt;(<a href="coin.md#0x2_coin">coin</a>: <a href="locked_coin.md#0x2_locked_coin_LockedCoin">LockedCoin</a>&lt;T&gt;): (Balance&lt;T&gt;, EpochTimeLock) {
    <b>let</b> <a href="locked_coin.md#0x2_locked_coin_LockedCoin">LockedCoin</a> { id, locked_until_epoch, <a href="balance.md#0x2_balance">balance</a> } = <a href="coin.md#0x2_coin">coin</a>;
    <a href="object.md#0x2_object_delete">object::delete</a>(id);
    (<a href="balance.md#0x2_balance">balance</a>, locked_until_epoch)
}
</code></pre>



</details>

<a name="0x2_locked_coin_value"></a>

## Function `value`

Public getter for the locked coin's value


<pre><code><b>public</b> <b>fun</b> <a href="locked_coin.md#0x2_locked_coin_value">value</a>&lt;T&gt;(self: &<a href="locked_coin.md#0x2_locked_coin_LockedCoin">locked_coin::LockedCoin</a>&lt;T&gt;): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="locked_coin.md#0x2_locked_coin_value">value</a>&lt;T&gt;(self: &<a href="locked_coin.md#0x2_locked_coin_LockedCoin">LockedCoin</a>&lt;T&gt;): u64 {
    <a href="balance.md#0x2_balance_value">balance::value</a>(&self.<a href="balance.md#0x2_balance">balance</a>)
}
</code></pre>



</details>

<a name="0x2_locked_coin_lock_coin"></a>

## Function `lock_coin`

Lock a coin up until <code>locked_until_epoch</code>. The input Coin<T> is deleted and a LockedCoin<T>
is transferred to the <code>recipient</code>. This function aborts if the <code>locked_until_epoch</code> is less than
or equal to the current epoch.


<pre><code><b>public</b> entry <b>fun</b> <a href="locked_coin.md#0x2_locked_coin_lock_coin">lock_coin</a>&lt;T&gt;(<a href="coin.md#0x2_coin">coin</a>: <a href="coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;, recipient: <b>address</b>, locked_until_epoch: u64, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="locked_coin.md#0x2_locked_coin_lock_coin">lock_coin</a>&lt;T&gt;(
    <a href="coin.md#0x2_coin">coin</a>: Coin&lt;T&gt;, recipient: <b>address</b>, locked_until_epoch: u64, ctx: &<b>mut</b> TxContext
) {
    <b>let</b> <a href="balance.md#0x2_balance">balance</a> = <a href="coin.md#0x2_coin_into_balance">coin::into_balance</a>(<a href="coin.md#0x2_coin">coin</a>);
    <a href="locked_coin.md#0x2_locked_coin_new_from_balance">new_from_balance</a>(<a href="balance.md#0x2_balance">balance</a>, <a href="epoch_time_lock.md#0x2_epoch_time_lock_new">epoch_time_lock::new</a>(locked_until_epoch, ctx), recipient, ctx);
}
</code></pre>



</details>

<a name="0x2_locked_coin_unlock_coin"></a>

## Function `unlock_coin`

Unlock a locked coin. The function aborts if the current epoch is less than the <code>locked_until_epoch</code>
of the coin. If the check is successful, the locked coin is deleted and a Coin<T> is transferred back
to the sender.


<pre><code><b>public</b> entry <b>fun</b> <a href="locked_coin.md#0x2_locked_coin_unlock_coin">unlock_coin</a>&lt;T&gt;(<a href="locked_coin.md#0x2_locked_coin">locked_coin</a>: <a href="locked_coin.md#0x2_locked_coin_LockedCoin">locked_coin::LockedCoin</a>&lt;T&gt;, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="locked_coin.md#0x2_locked_coin_unlock_coin">unlock_coin</a>&lt;T&gt;(<a href="locked_coin.md#0x2_locked_coin">locked_coin</a>: <a href="locked_coin.md#0x2_locked_coin_LockedCoin">LockedCoin</a>&lt;T&gt;, ctx: &<b>mut</b> TxContext) {
    <b>let</b> <a href="locked_coin.md#0x2_locked_coin_LockedCoin">LockedCoin</a> { id, <a href="balance.md#0x2_balance">balance</a>, locked_until_epoch } = <a href="locked_coin.md#0x2_locked_coin">locked_coin</a>;
    <a href="object.md#0x2_object_delete">object::delete</a>(id);
    <a href="epoch_time_lock.md#0x2_epoch_time_lock_destroy">epoch_time_lock::destroy</a>(locked_until_epoch, ctx);
    <b>let</b> <a href="coin.md#0x2_coin">coin</a> = <a href="coin.md#0x2_coin_from_balance">coin::from_balance</a>(<a href="balance.md#0x2_balance">balance</a>, ctx);
    <a href="transfer.md#0x2_transfer_transfer">transfer::transfer</a>(<a href="coin.md#0x2_coin">coin</a>, <a href="tx_context.md#0x2_tx_context_sender">tx_context::sender</a>(ctx));
}
</code></pre>



</details>
