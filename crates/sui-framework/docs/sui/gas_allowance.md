---
title: Module `sui::gas_allowance`
---

A shared SUI escrow that is available only to the gas payment mechanism.


-  [Struct `GasAllowance`](#sui_gas_allowance_GasAllowance)
-  [Struct `StorageRebate`](#sui_gas_allowance_StorageRebate)
-  [Constants](#@Constants_0)
-  [Function `new`](#sui_gas_allowance_new)
-  [Function `top_up`](#sui_gas_allowance_top_up)
-  [Function `withdraw`](#sui_gas_allowance_withdraw)
-  [Function `revoke`](#sui_gas_allowance_revoke)
-  [Function `claim_storage_rebate`](#sui_gas_allowance_claim_storage_rebate)
-  [Function `funder`](#sui_gas_allowance_funder)
-  [Function `spender`](#sui_gas_allowance_spender)
-  [Function `remaining`](#sui_gas_allowance_remaining)
-  [Function `revoked`](#sui_gas_allowance_revoked)
-  [Function `assert_funder`](#sui_gas_allowance_assert_funder)


<pre><code><b>use</b> <a href="../std/address.md#std_address">std::address</a>;
<b>use</b> <a href="../std/ascii.md#std_ascii">std::ascii</a>;
<b>use</b> <a href="../std/bcs.md#std_bcs">std::bcs</a>;
<b>use</b> <a href="../std/internal.md#std_internal">std::internal</a>;
<b>use</b> <a href="../std/option.md#std_option">std::option</a>;
<b>use</b> <a href="../std/string.md#std_string">std::string</a>;
<b>use</b> <a href="../std/type_name.md#std_type_name">std::type_name</a>;
<b>use</b> <a href="../std/u128.md#std_u128">std::u128</a>;
<b>use</b> <a href="../std/vector.md#std_vector">std::vector</a>;
<b>use</b> <a href="../sui/accumulator.md#sui_accumulator">sui::accumulator</a>;
<b>use</b> <a href="../sui/accumulator_settlement.md#sui_accumulator_settlement">sui::accumulator_settlement</a>;
<b>use</b> <a href="../sui/address.md#sui_address">sui::address</a>;
<b>use</b> <a href="../sui/bag.md#sui_bag">sui::bag</a>;
<b>use</b> <a href="../sui/balance.md#sui_balance">sui::balance</a>;
<b>use</b> <a href="../sui/bcs.md#sui_bcs">sui::bcs</a>;
<b>use</b> <a href="../sui/coin.md#sui_coin">sui::coin</a>;
<b>use</b> <a href="../sui/config.md#sui_config">sui::config</a>;
<b>use</b> <a href="../sui/deny_list.md#sui_deny_list">sui::deny_list</a>;
<b>use</b> <a href="../sui/dynamic_field.md#sui_dynamic_field">sui::dynamic_field</a>;
<b>use</b> <a href="../sui/dynamic_object_field.md#sui_dynamic_object_field">sui::dynamic_object_field</a>;
<b>use</b> <a href="../sui/event.md#sui_event">sui::event</a>;
<b>use</b> <a href="../sui/funds_accumulator.md#sui_funds_accumulator">sui::funds_accumulator</a>;
<b>use</b> <a href="../sui/hash.md#sui_hash">sui::hash</a>;
<b>use</b> <a href="../sui/hex.md#sui_hex">sui::hex</a>;
<b>use</b> <a href="../sui/object.md#sui_object">sui::object</a>;
<b>use</b> <a href="../sui/party.md#sui_party">sui::party</a>;
<b>use</b> <a href="../sui/protocol_config.md#sui_protocol_config">sui::protocol_config</a>;
<b>use</b> <a href="../sui/sui.md#sui_sui">sui::sui</a>;
<b>use</b> <a href="../sui/table.md#sui_table">sui::table</a>;
<b>use</b> <a href="../sui/transfer.md#sui_transfer">sui::transfer</a>;
<b>use</b> <a href="../sui/tx_context.md#sui_tx_context">sui::tx_context</a>;
<b>use</b> <a href="../sui/types.md#sui_types">sui::types</a>;
<b>use</b> <a href="../sui/url.md#sui_url">sui::url</a>;
<b>use</b> <a href="../sui/vec_map.md#sui_vec_map">sui::vec_map</a>;
<b>use</b> <a href="../sui/vec_set.md#sui_vec_set">sui::vec_set</a>;
</code></pre>



<a name="sui_gas_allowance_GasAllowance"></a>

## Struct `GasAllowance`

Shared escrow for gas payments. The gas payment mechanism is the only path that can spend its
balance on behalf of <code><a href="../sui/gas_allowance.md#sui_gas_allowance_spender">spender</a></code>.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/gas_allowance.md#sui_gas_allowance_GasAllowance">GasAllowance</a> <b>has</b> key
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
<code><a href="../sui/gas_allowance.md#sui_gas_allowance_funder">funder</a>: <b>address</b></code>
</dt>
<dd>
</dd>
<dt>
<code><a href="../sui/gas_allowance.md#sui_gas_allowance_spender">spender</a>: <b>address</b></code>
</dt>
<dd>
</dd>
<dt>
<code><a href="../sui/balance.md#sui_balance">balance</a>: <a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;<a href="../sui/sui.md#sui_sui_SUI">sui::sui::SUI</a>&gt;</code>
</dt>
<dd>
</dd>
<dt>
<code><a href="../sui/gas_allowance.md#sui_gas_allowance_revoked">revoked</a>: bool</code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="sui_gas_allowance_StorageRebate"></a>

## Struct `StorageRebate`

Native storage attribution. Object-owned markers protect <code>amount</code> of their parent's deposit.
Address-owned claims preserve deposits released by a different gas payer, including system
transactions, until their funder claims them. Neither kind can be constructed or publicly transferred.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/gas_allowance.md#sui_gas_allowance_StorageRebate">StorageRebate</a> <b>has</b> key
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
<code>recipient: <b>address</b></code>
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

<a name="@Constants_0"></a>

## Constants


<a name="sui_gas_allowance_ENotEnabled"></a>



<pre><code>#[error]
<b>const</b> <a href="../sui/gas_allowance.md#sui_gas_allowance_ENotEnabled">ENotEnabled</a>: vector&lt;u8&gt; = b"Gas allowances are not enabled";
</code></pre>



<a name="sui_gas_allowance_ENotFunder"></a>



<pre><code>#[error]
<b>const</b> <a href="../sui/gas_allowance.md#sui_gas_allowance_ENotFunder">ENotFunder</a>: vector&lt;u8&gt; = b"Only the gas <a href="../sui/allowance.md#sui_allowance">allowance</a> <a href="../sui/gas_allowance.md#sui_gas_allowance_funder">funder</a> may perform this operation";
</code></pre>



<a name="sui_gas_allowance_ERevoked"></a>



<pre><code>#[error]
<b>const</b> <a href="../sui/gas_allowance.md#sui_gas_allowance_ERevoked">ERevoked</a>: vector&lt;u8&gt; = b"Gas <a href="../sui/allowance.md#sui_allowance">allowance</a> <b>has</b> been <a href="../sui/gas_allowance.md#sui_gas_allowance_revoked">revoked</a>";
</code></pre>



<a name="sui_gas_allowance_new"></a>

## Function `new`

Creates and shares an allowance funded by <code>payment</code>.


<pre><code><b>entry</b> <b>fun</b> <a href="../sui/gas_allowance.md#sui_gas_allowance_new">new</a>(payment: <a href="../sui/coin.md#sui_coin_Coin">sui::coin::Coin</a>&lt;<a href="../sui/sui.md#sui_sui_SUI">sui::sui::SUI</a>&gt;, <a href="../sui/gas_allowance.md#sui_gas_allowance_spender">spender</a>: <b>address</b>, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>entry</b> <b>fun</b> <a href="../sui/gas_allowance.md#sui_gas_allowance_new">new</a>(payment: Coin&lt;SUI&gt;, <a href="../sui/gas_allowance.md#sui_gas_allowance_spender">spender</a>: <b>address</b>, ctx: &<b>mut</b> TxContext) {
    <b>assert</b>!(<a href="../sui/protocol_config.md#sui_protocol_config_is_feature_enabled">sui::protocol_config::is_feature_enabled</a>(b"enable_gas_allowances"), <a href="../sui/gas_allowance.md#sui_gas_allowance_ENotEnabled">ENotEnabled</a>);
    <a href="../sui/transfer.md#sui_transfer_share_object">transfer::share_object</a>(<a href="../sui/gas_allowance.md#sui_gas_allowance_GasAllowance">GasAllowance</a> {
        id: <a href="../sui/object.md#sui_object_new">object::new</a>(ctx),
        <a href="../sui/gas_allowance.md#sui_gas_allowance_funder">funder</a>: ctx.sender(),
        <a href="../sui/gas_allowance.md#sui_gas_allowance_spender">spender</a>,
        <a href="../sui/balance.md#sui_balance">balance</a>: <a href="../sui/coin.md#sui_coin_into_balance">coin::into_balance</a>(payment),
        <a href="../sui/gas_allowance.md#sui_gas_allowance_revoked">revoked</a>: <b>false</b>,
    });
}
</code></pre>



</details>

<a name="sui_gas_allowance_top_up"></a>

## Function `top_up`

Adds SUI to an active allowance. Only its funder can add funds.


<pre><code><b>entry</b> <b>fun</b> <a href="../sui/gas_allowance.md#sui_gas_allowance_top_up">top_up</a>(self: &<b>mut</b> <a href="../sui/gas_allowance.md#sui_gas_allowance_GasAllowance">sui::gas_allowance::GasAllowance</a>, payment: <a href="../sui/coin.md#sui_coin_Coin">sui::coin::Coin</a>&lt;<a href="../sui/sui.md#sui_sui_SUI">sui::sui::SUI</a>&gt;, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>entry</b> <b>fun</b> <a href="../sui/gas_allowance.md#sui_gas_allowance_top_up">top_up</a>(self: &<b>mut</b> <a href="../sui/gas_allowance.md#sui_gas_allowance_GasAllowance">GasAllowance</a>, payment: Coin&lt;SUI&gt;, ctx: &TxContext) {
    <a href="../sui/gas_allowance.md#sui_gas_allowance_assert_funder">assert_funder</a>(self, ctx);
    <b>assert</b>!(!self.<a href="../sui/gas_allowance.md#sui_gas_allowance_revoked">revoked</a>, <a href="../sui/gas_allowance.md#sui_gas_allowance_ERevoked">ERevoked</a>);
    self.<a href="../sui/balance.md#sui_balance">balance</a>.join(<a href="../sui/coin.md#sui_coin_into_balance">coin::into_balance</a>(payment));
}
</code></pre>



</details>

<a name="sui_gas_allowance_withdraw"></a>

## Function `withdraw`

Withdraws <code>amount</code> from an active allowance to its original funder.


<pre><code><b>entry</b> <b>fun</b> <a href="../sui/gas_allowance.md#sui_gas_allowance_withdraw">withdraw</a>(self: &<b>mut</b> <a href="../sui/gas_allowance.md#sui_gas_allowance_GasAllowance">sui::gas_allowance::GasAllowance</a>, amount: u64, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>entry</b> <b>fun</b> <a href="../sui/gas_allowance.md#sui_gas_allowance_withdraw">withdraw</a>(self: &<b>mut</b> <a href="../sui/gas_allowance.md#sui_gas_allowance_GasAllowance">GasAllowance</a>, amount: u64, ctx: &<b>mut</b> TxContext) {
    <a href="../sui/gas_allowance.md#sui_gas_allowance_assert_funder">assert_funder</a>(self, ctx);
    <b>assert</b>!(!self.<a href="../sui/gas_allowance.md#sui_gas_allowance_revoked">revoked</a>, <a href="../sui/gas_allowance.md#sui_gas_allowance_ERevoked">ERevoked</a>);
    <a href="../sui/transfer.md#sui_transfer_public_transfer">transfer::public_transfer</a>(self.<a href="../sui/balance.md#sui_balance">balance</a>.split(amount).into_coin(ctx), self.<a href="../sui/gas_allowance.md#sui_gas_allowance_funder">funder</a>);
}
</code></pre>



</details>

<a name="sui_gas_allowance_revoke"></a>

## Function `revoke`

Permanently disables an allowance and returns its remaining balance to its original funder.


<pre><code><b>entry</b> <b>fun</b> <a href="../sui/gas_allowance.md#sui_gas_allowance_revoke">revoke</a>(self: &<b>mut</b> <a href="../sui/gas_allowance.md#sui_gas_allowance_GasAllowance">sui::gas_allowance::GasAllowance</a>, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>entry</b> <b>fun</b> <a href="../sui/gas_allowance.md#sui_gas_allowance_revoke">revoke</a>(self: &<b>mut</b> <a href="../sui/gas_allowance.md#sui_gas_allowance_GasAllowance">GasAllowance</a>, ctx: &<b>mut</b> TxContext) {
    <a href="../sui/gas_allowance.md#sui_gas_allowance_assert_funder">assert_funder</a>(self, ctx);
    <b>assert</b>!(!self.<a href="../sui/gas_allowance.md#sui_gas_allowance_revoked">revoked</a>, <a href="../sui/gas_allowance.md#sui_gas_allowance_ERevoked">ERevoked</a>);
    self.<a href="../sui/gas_allowance.md#sui_gas_allowance_revoked">revoked</a> = <b>true</b>;
    <a href="../sui/transfer.md#sui_transfer_public_transfer">transfer::public_transfer</a>(self.<a href="../sui/balance.md#sui_balance">balance</a>.withdraw_all().into_coin(ctx), self.<a href="../sui/gas_allowance.md#sui_gas_allowance_funder">funder</a>);
}
</code></pre>



</details>

<a name="sui_gas_allowance_claim_storage_rebate"></a>

## Function `claim_storage_rebate`

Claims a protected storage deposit released by another payer or by system execution.
The claim must be owned by its recipient; object-owned markers cannot be transaction inputs.
Deleting the claim refunds its deposit through the ordinary gas-payment mechanism.


<pre><code><b>entry</b> <b>fun</b> <a href="../sui/gas_allowance.md#sui_gas_allowance_claim_storage_rebate">claim_storage_rebate</a>(claim: <a href="../sui/gas_allowance.md#sui_gas_allowance_StorageRebate">sui::gas_allowance::StorageRebate</a>, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>entry</b> <b>fun</b> <a href="../sui/gas_allowance.md#sui_gas_allowance_claim_storage_rebate">claim_storage_rebate</a>(claim: <a href="../sui/gas_allowance.md#sui_gas_allowance_StorageRebate">StorageRebate</a>, ctx: &TxContext) {
    <b>let</b> <a href="../sui/gas_allowance.md#sui_gas_allowance_StorageRebate">StorageRebate</a> { id, recipient, amount: _ } = claim;
    <b>assert</b>!(ctx.sender() == recipient, <a href="../sui/gas_allowance.md#sui_gas_allowance_ENotFunder">ENotFunder</a>);
    id.delete();
}
</code></pre>



</details>

<a name="sui_gas_allowance_funder"></a>

## Function `funder`

Returns the address that funded this allowance.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/gas_allowance.md#sui_gas_allowance_funder">funder</a>(self: &<a href="../sui/gas_allowance.md#sui_gas_allowance_GasAllowance">sui::gas_allowance::GasAllowance</a>): <b>address</b>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/gas_allowance.md#sui_gas_allowance_funder">funder</a>(self: &<a href="../sui/gas_allowance.md#sui_gas_allowance_GasAllowance">GasAllowance</a>): <b>address</b> {
    self.<a href="../sui/gas_allowance.md#sui_gas_allowance_funder">funder</a>
}
</code></pre>



</details>

<a name="sui_gas_allowance_spender"></a>

## Function `spender`

Returns the sole address authorized to use this allowance for gas.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/gas_allowance.md#sui_gas_allowance_spender">spender</a>(self: &<a href="../sui/gas_allowance.md#sui_gas_allowance_GasAllowance">sui::gas_allowance::GasAllowance</a>): <b>address</b>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/gas_allowance.md#sui_gas_allowance_spender">spender</a>(self: &<a href="../sui/gas_allowance.md#sui_gas_allowance_GasAllowance">GasAllowance</a>): <b>address</b> {
    self.<a href="../sui/gas_allowance.md#sui_gas_allowance_spender">spender</a>
}
</code></pre>



</details>

<a name="sui_gas_allowance_remaining"></a>

## Function `remaining`

Returns the escrowed SUI available to the gas payment mechanism.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/gas_allowance.md#sui_gas_allowance_remaining">remaining</a>(self: &<a href="../sui/gas_allowance.md#sui_gas_allowance_GasAllowance">sui::gas_allowance::GasAllowance</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/gas_allowance.md#sui_gas_allowance_remaining">remaining</a>(self: &<a href="../sui/gas_allowance.md#sui_gas_allowance_GasAllowance">GasAllowance</a>): u64 {
    self.<a href="../sui/balance.md#sui_balance">balance</a>.value()
}
</code></pre>



</details>

<a name="sui_gas_allowance_revoked"></a>

## Function `revoked`

Returns whether this allowance has been permanently revoked.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/gas_allowance.md#sui_gas_allowance_revoked">revoked</a>(self: &<a href="../sui/gas_allowance.md#sui_gas_allowance_GasAllowance">sui::gas_allowance::GasAllowance</a>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/gas_allowance.md#sui_gas_allowance_revoked">revoked</a>(self: &<a href="../sui/gas_allowance.md#sui_gas_allowance_GasAllowance">GasAllowance</a>): bool {
    self.<a href="../sui/gas_allowance.md#sui_gas_allowance_revoked">revoked</a>
}
</code></pre>



</details>

<a name="sui_gas_allowance_assert_funder"></a>

## Function `assert_funder`



<pre><code><b>fun</b> <a href="../sui/gas_allowance.md#sui_gas_allowance_assert_funder">assert_funder</a>(self: &<a href="../sui/gas_allowance.md#sui_gas_allowance_GasAllowance">sui::gas_allowance::GasAllowance</a>, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/gas_allowance.md#sui_gas_allowance_assert_funder">assert_funder</a>(self: &<a href="../sui/gas_allowance.md#sui_gas_allowance_GasAllowance">GasAllowance</a>, ctx: &TxContext) {
    <b>assert</b>!(ctx.sender() == self.<a href="../sui/gas_allowance.md#sui_gas_allowance_funder">funder</a>, <a href="../sui/gas_allowance.md#sui_gas_allowance_ENotFunder">ENotFunder</a>);
}
</code></pre>



</details>
