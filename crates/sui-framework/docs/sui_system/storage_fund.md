---
title: Module `sui_system::storage_fund`
---



-  [Struct `StorageFund`](#sui_system_storage_fund_StorageFund)
-  [Function `new`](#sui_system_storage_fund_new)
-  [Function `advance_epoch`](#sui_system_storage_fund_advance_epoch)
-  [Function `total_object_storage_rebates`](#sui_system_storage_fund_total_object_storage_rebates)
-  [Function `total_balance`](#sui_system_storage_fund_total_balance)


<pre><code><b>use</b> <a href="../std/address.md#std_address">std::address</a>;
<b>use</b> <a href="../std/ascii.md#std_ascii">std::ascii</a>;
<b>use</b> <a href="../std/bcs.md#std_bcs">std::bcs</a>;
<b>use</b> <a href="../std/option.md#std_option">std::option</a>;
<b>use</b> <a href="../std/string.md#std_string">std::string</a>;
<b>use</b> <a href="../std/type_name.md#std_type_name">std::type_name</a>;
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



<a name="sui_system_storage_fund_StorageFund"></a>

## Struct `StorageFund`

Struct representing the storage fund, containing two <code>Balance</code>s:
- <code><a href="../sui_system/storage_fund.md#sui_system_storage_fund_total_object_storage_rebates">total_object_storage_rebates</a></code> has the invariant that it's the sum of <code>storage_rebate</code> of
all objects currently stored on-chain. To maintain this invariant, the only inflow of this
balance is storage charges collected from transactions, and the only outflow is storage rebates
of transactions, including both the portion refunded to the transaction senders as well as
the non-refundable portion taken out and put into <code>non_refundable_balance</code>.
- <code>non_refundable_balance</code> contains any remaining inflow of the storage fund that should not
be taken out of the fund.


<pre><code><b>public</b> <b>struct</b> <a href="../sui_system/storage_fund.md#sui_system_storage_fund_StorageFund">StorageFund</a> <b>has</b> store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code><a href="../sui_system/storage_fund.md#sui_system_storage_fund_total_object_storage_rebates">total_object_storage_rebates</a>: <a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;<a href="../sui/sui.md#sui_sui_SUI">sui::sui::SUI</a>&gt;</code>
</dt>
<dd>
</dd>
<dt>
<code>non_refundable_balance: <a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;<a href="../sui/sui.md#sui_sui_SUI">sui::sui::SUI</a>&gt;</code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="sui_system_storage_fund_new"></a>

## Function `new`

Called by <code><a href="../sui_system/sui_system.md#sui_system_sui_system">sui_system</a></code> at genesis time.


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/storage_fund.md#sui_system_storage_fund_new">new</a>(initial_fund: <a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;<a href="../sui/sui.md#sui_sui_SUI">sui::sui::SUI</a>&gt;): <a href="../sui_system/storage_fund.md#sui_system_storage_fund_StorageFund">sui_system::storage_fund::StorageFund</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/storage_fund.md#sui_system_storage_fund_new">new</a>(initial_fund: Balance&lt;SUI&gt;) : <a href="../sui_system/storage_fund.md#sui_system_storage_fund_StorageFund">StorageFund</a> {
    <a href="../sui_system/storage_fund.md#sui_system_storage_fund_StorageFund">StorageFund</a> {
        // At the beginning there's no object in the storage yet
        <a href="../sui_system/storage_fund.md#sui_system_storage_fund_total_object_storage_rebates">total_object_storage_rebates</a>: balance::zero(),
        non_refundable_balance: initial_fund,
    }
}
</code></pre>



</details>

<a name="sui_system_storage_fund_advance_epoch"></a>

## Function `advance_epoch`

Called by <code><a href="../sui_system/sui_system.md#sui_system_sui_system">sui_system</a></code> at epoch change times to process the inflows and outflows of storage fund.


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/storage_fund.md#sui_system_storage_fund_advance_epoch">advance_epoch</a>(self: &<b>mut</b> <a href="../sui_system/storage_fund.md#sui_system_storage_fund_StorageFund">sui_system::storage_fund::StorageFund</a>, storage_charges: <a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;<a href="../sui/sui.md#sui_sui_SUI">sui::sui::SUI</a>&gt;, storage_fund_reinvestment: <a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;<a href="../sui/sui.md#sui_sui_SUI">sui::sui::SUI</a>&gt;, leftover_staking_rewards: <a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;<a href="../sui/sui.md#sui_sui_SUI">sui::sui::SUI</a>&gt;, storage_rebate_amount: u64, non_refundable_storage_fee_amount: u64): <a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;<a href="../sui/sui.md#sui_sui_SUI">sui::sui::SUI</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/storage_fund.md#sui_system_storage_fund_advance_epoch">advance_epoch</a>(
    self: &<b>mut</b> <a href="../sui_system/storage_fund.md#sui_system_storage_fund_StorageFund">StorageFund</a>,
    storage_charges: Balance&lt;SUI&gt;,
    storage_fund_reinvestment: Balance&lt;SUI&gt;,
    leftover_staking_rewards: Balance&lt;SUI&gt;,
    storage_rebate_amount: u64,
    non_refundable_storage_fee_amount: u64,
) : Balance&lt;SUI&gt; {
    // Both the reinvestment and leftover rewards are not to be refunded so they go to the non-refundable balance.
    self.non_refundable_balance.join(storage_fund_reinvestment);
    self.non_refundable_balance.join(leftover_staking_rewards);
    // The storage charges <b>for</b> the epoch come from the storage rebate of the <a href="../sui_system/storage_fund.md#sui_system_storage_fund_new">new</a> objects created
    // and the <a href="../sui_system/storage_fund.md#sui_system_storage_fund_new">new</a> storage rebates of the objects modified during the epoch so we put the charges
    // into `<a href="../sui_system/storage_fund.md#sui_system_storage_fund_total_object_storage_rebates">total_object_storage_rebates</a>`.
    self.<a href="../sui_system/storage_fund.md#sui_system_storage_fund_total_object_storage_rebates">total_object_storage_rebates</a>.join(storage_charges);
    // Split out the non-refundable portion of the storage rebate and put it into the non-refundable balance.
    <b>let</b> non_refundable_storage_fee = self.<a href="../sui_system/storage_fund.md#sui_system_storage_fund_total_object_storage_rebates">total_object_storage_rebates</a>.split(non_refundable_storage_fee_amount);
    self.non_refundable_balance.join(non_refundable_storage_fee);
    // `storage_rebates` include the already refunded rebates of deleted objects and old rebates of modified objects and
    // should be taken out of the `<a href="../sui_system/storage_fund.md#sui_system_storage_fund_total_object_storage_rebates">total_object_storage_rebates</a>`.
    <b>let</b> storage_rebate = self.<a href="../sui_system/storage_fund.md#sui_system_storage_fund_total_object_storage_rebates">total_object_storage_rebates</a>.split(storage_rebate_amount);
    // The storage rebate <b>has</b> already been returned to individual transaction senders' gas coins
    // so we <b>return</b> the balance to be burnt at the very end of epoch change.
    storage_rebate
}
</code></pre>



</details>

<a name="sui_system_storage_fund_total_object_storage_rebates"></a>

## Function `total_object_storage_rebates`



<pre><code><b>public</b> <b>fun</b> <a href="../sui_system/storage_fund.md#sui_system_storage_fund_total_object_storage_rebates">total_object_storage_rebates</a>(self: &<a href="../sui_system/storage_fund.md#sui_system_storage_fund_StorageFund">sui_system::storage_fund::StorageFund</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui_system/storage_fund.md#sui_system_storage_fund_total_object_storage_rebates">total_object_storage_rebates</a>(self: &<a href="../sui_system/storage_fund.md#sui_system_storage_fund_StorageFund">StorageFund</a>): u64 {
    self.<a href="../sui_system/storage_fund.md#sui_system_storage_fund_total_object_storage_rebates">total_object_storage_rebates</a>.value()
}
</code></pre>



</details>

<a name="sui_system_storage_fund_total_balance"></a>

## Function `total_balance`



<pre><code><b>public</b> <b>fun</b> <a href="../sui_system/storage_fund.md#sui_system_storage_fund_total_balance">total_balance</a>(self: &<a href="../sui_system/storage_fund.md#sui_system_storage_fund_StorageFund">sui_system::storage_fund::StorageFund</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui_system/storage_fund.md#sui_system_storage_fund_total_balance">total_balance</a>(self: &<a href="../sui_system/storage_fund.md#sui_system_storage_fund_StorageFund">StorageFund</a>): u64 {
    self.<a href="../sui_system/storage_fund.md#sui_system_storage_fund_total_object_storage_rebates">total_object_storage_rebates</a>.value() + self.non_refundable_balance.value()
}
</code></pre>



</details>
