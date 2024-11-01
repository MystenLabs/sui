---
title: Module `0x2::balance`
---

A storable handler for Balances in general. Is used in the <code>Coin</code>
module to allow balance operations and can be used to implement
custom coins with <code><a href="../sui-framework/balance.md#0x2_balance_Supply">Supply</a></code> and <code><a href="../sui-framework/balance.md#0x2_balance_Balance">Balance</a></code>s.


-  [Struct `Supply`](#0x2_balance_Supply)
-  [Struct `Balance`](#0x2_balance_Balance)
-  [Constants](#@Constants_0)
-  [Function `value`](#0x2_balance_value)
-  [Function `supply_value`](#0x2_balance_supply_value)
-  [Function `create_supply`](#0x2_balance_create_supply)
-  [Function `increase_supply`](#0x2_balance_increase_supply)
-  [Function `decrease_supply`](#0x2_balance_decrease_supply)
-  [Function `zero`](#0x2_balance_zero)
-  [Function `join`](#0x2_balance_join)
-  [Function `split`](#0x2_balance_split)
-  [Function `withdraw_all`](#0x2_balance_withdraw_all)
-  [Function `destroy_zero`](#0x2_balance_destroy_zero)
-  [Function `create_staking_rewards`](#0x2_balance_create_staking_rewards)
-  [Function `destroy_storage_rebates`](#0x2_balance_destroy_storage_rebates)
-  [Function `destroy_supply`](#0x2_balance_destroy_supply)


<pre><code><b>use</b> <a href="../move-stdlib/ascii.md#0x1_ascii">0x1::ascii</a>;
<b>use</b> <a href="../move-stdlib/type_name.md#0x1_type_name">0x1::type_name</a>;
<b>use</b> <a href="../sui-framework/tx_context.md#0x2_tx_context">0x2::tx_context</a>;
</code></pre>



<a name="0x2_balance_Supply"></a>

## Struct `Supply`

A Supply of T. Used for minting and burning.
Wrapped into a <code>TreasuryCap</code> in the <code>Coin</code> module.


<pre><code><b>struct</b> <a href="../sui-framework/balance.md#0x2_balance_Supply">Supply</a>&lt;T&gt; <b>has</b> store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>value: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x2_balance_Balance"></a>

## Struct `Balance`

Storable balance - an inner struct of a Coin type.
Can be used to store coins which don't need the key ability.


<pre><code><b>struct</b> <a href="../sui-framework/balance.md#0x2_balance_Balance">Balance</a>&lt;T&gt; <b>has</b> store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>value: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0x2_balance_ENotSystemAddress"></a>

Sender is not @0x0 the system address.


<pre><code><b>const</b> <a href="../sui-framework/balance.md#0x2_balance_ENotSystemAddress">ENotSystemAddress</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 3;
</code></pre>



<a name="0x2_balance_ENonZero"></a>

For when trying to destroy a non-zero balance.


<pre><code><b>const</b> <a href="../sui-framework/balance.md#0x2_balance_ENonZero">ENonZero</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 0;
</code></pre>



<a name="0x2_balance_ENotEnough"></a>

For when trying to withdraw more than there is.


<pre><code><b>const</b> <a href="../sui-framework/balance.md#0x2_balance_ENotEnough">ENotEnough</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 2;
</code></pre>



<a name="0x2_balance_ENotSUI"></a>

System operation performed for a coin other than SUI


<pre><code><b>const</b> <a href="../sui-framework/balance.md#0x2_balance_ENotSUI">ENotSUI</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 4;
</code></pre>



<a name="0x2_balance_EOverflow"></a>

For when an overflow is happening on Supply operations.


<pre><code><b>const</b> <a href="../sui-framework/balance.md#0x2_balance_EOverflow">EOverflow</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 1;
</code></pre>



<a name="0x2_balance_SUI_TYPE_NAME"></a>



<pre><code><b>const</b> <a href="../sui-framework/balance.md#0x2_balance_SUI_TYPE_NAME">SUI_TYPE_NAME</a>: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt; = [48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 50, 58, 58, 115, 117, 105, 58, 58, 83, 85, 73];
</code></pre>



<a name="0x2_balance_value"></a>

## Function `value`

Get the amount stored in a <code><a href="../sui-framework/balance.md#0x2_balance_Balance">Balance</a></code>.


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/balance.md#0x2_balance_value">value</a>&lt;T&gt;(self: &<a href="../sui-framework/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;T&gt;): <a href="../move-stdlib/u64.md#0x1_u64">u64</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/balance.md#0x2_balance_value">value</a>&lt;T&gt;(self: &<a href="../sui-framework/balance.md#0x2_balance_Balance">Balance</a>&lt;T&gt;): <a href="../move-stdlib/u64.md#0x1_u64">u64</a> {
    self.value
}
</code></pre>



</details>

<a name="0x2_balance_supply_value"></a>

## Function `supply_value`

Get the <code><a href="../sui-framework/balance.md#0x2_balance_Supply">Supply</a></code> value.


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/balance.md#0x2_balance_supply_value">supply_value</a>&lt;T&gt;(supply: &<a href="../sui-framework/balance.md#0x2_balance_Supply">balance::Supply</a>&lt;T&gt;): <a href="../move-stdlib/u64.md#0x1_u64">u64</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/balance.md#0x2_balance_supply_value">supply_value</a>&lt;T&gt;(supply: &<a href="../sui-framework/balance.md#0x2_balance_Supply">Supply</a>&lt;T&gt;): <a href="../move-stdlib/u64.md#0x1_u64">u64</a> {
    supply.value
}
</code></pre>



</details>

<a name="0x2_balance_create_supply"></a>

## Function `create_supply`

Create a new supply for type T.


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/balance.md#0x2_balance_create_supply">create_supply</a>&lt;T: drop&gt;(_: T): <a href="../sui-framework/balance.md#0x2_balance_Supply">balance::Supply</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/balance.md#0x2_balance_create_supply">create_supply</a>&lt;T: drop&gt;(_: T): <a href="../sui-framework/balance.md#0x2_balance_Supply">Supply</a>&lt;T&gt; {
    <a href="../sui-framework/balance.md#0x2_balance_Supply">Supply</a> { value: 0 }
}
</code></pre>



</details>

<a name="0x2_balance_increase_supply"></a>

## Function `increase_supply`

Increase supply by <code>value</code> and create a new <code><a href="../sui-framework/balance.md#0x2_balance_Balance">Balance</a>&lt;T&gt;</code> with this value.


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/balance.md#0x2_balance_increase_supply">increase_supply</a>&lt;T&gt;(self: &<b>mut</b> <a href="../sui-framework/balance.md#0x2_balance_Supply">balance::Supply</a>&lt;T&gt;, value: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>): <a href="../sui-framework/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/balance.md#0x2_balance_increase_supply">increase_supply</a>&lt;T&gt;(self: &<b>mut</b> <a href="../sui-framework/balance.md#0x2_balance_Supply">Supply</a>&lt;T&gt;, value: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>): <a href="../sui-framework/balance.md#0x2_balance_Balance">Balance</a>&lt;T&gt; {
    <b>assert</b>!(<a href="../sui-framework/balance.md#0x2_balance_value">value</a> &lt; (18446744073709551615u64 - self.value), <a href="../sui-framework/balance.md#0x2_balance_EOverflow">EOverflow</a>);
    self.value = self.value + value;
    <a href="../sui-framework/balance.md#0x2_balance_Balance">Balance</a> { value }
}
</code></pre>



</details>

<a name="0x2_balance_decrease_supply"></a>

## Function `decrease_supply`

Burn a Balance<T> and decrease Supply<T>.


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/balance.md#0x2_balance_decrease_supply">decrease_supply</a>&lt;T&gt;(self: &<b>mut</b> <a href="../sui-framework/balance.md#0x2_balance_Supply">balance::Supply</a>&lt;T&gt;, <a href="../sui-framework/balance.md#0x2_balance">balance</a>: <a href="../sui-framework/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;T&gt;): <a href="../move-stdlib/u64.md#0x1_u64">u64</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/balance.md#0x2_balance_decrease_supply">decrease_supply</a>&lt;T&gt;(self: &<b>mut</b> <a href="../sui-framework/balance.md#0x2_balance_Supply">Supply</a>&lt;T&gt;, <a href="../sui-framework/balance.md#0x2_balance">balance</a>: <a href="../sui-framework/balance.md#0x2_balance_Balance">Balance</a>&lt;T&gt;): <a href="../move-stdlib/u64.md#0x1_u64">u64</a> {
    <b>let</b> <a href="../sui-framework/balance.md#0x2_balance_Balance">Balance</a> { value } = <a href="../sui-framework/balance.md#0x2_balance">balance</a>;
    <b>assert</b>!(self.value &gt;= value, <a href="../sui-framework/balance.md#0x2_balance_EOverflow">EOverflow</a>);
    self.value = self.value - value;
    value
}
</code></pre>



</details>

<a name="0x2_balance_zero"></a>

## Function `zero`

Create a zero <code><a href="../sui-framework/balance.md#0x2_balance_Balance">Balance</a></code> for type <code>T</code>.


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/balance.md#0x2_balance_zero">zero</a>&lt;T&gt;(): <a href="../sui-framework/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/balance.md#0x2_balance_zero">zero</a>&lt;T&gt;(): <a href="../sui-framework/balance.md#0x2_balance_Balance">Balance</a>&lt;T&gt; {
    <a href="../sui-framework/balance.md#0x2_balance_Balance">Balance</a> { value: 0 }
}
</code></pre>



</details>

<a name="0x2_balance_join"></a>

## Function `join`

Join two balances together.


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/balance.md#0x2_balance_join">join</a>&lt;T&gt;(self: &<b>mut</b> <a href="../sui-framework/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;T&gt;, <a href="../sui-framework/balance.md#0x2_balance">balance</a>: <a href="../sui-framework/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;T&gt;): <a href="../move-stdlib/u64.md#0x1_u64">u64</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/balance.md#0x2_balance_join">join</a>&lt;T&gt;(self: &<b>mut</b> <a href="../sui-framework/balance.md#0x2_balance_Balance">Balance</a>&lt;T&gt;, <a href="../sui-framework/balance.md#0x2_balance">balance</a>: <a href="../sui-framework/balance.md#0x2_balance_Balance">Balance</a>&lt;T&gt;): <a href="../move-stdlib/u64.md#0x1_u64">u64</a> {
    <b>let</b> <a href="../sui-framework/balance.md#0x2_balance_Balance">Balance</a> { value } = <a href="../sui-framework/balance.md#0x2_balance">balance</a>;
    self.value = self.value + value;
    self.value
}
</code></pre>



</details>

<a name="0x2_balance_split"></a>

## Function `split`

Split a <code><a href="../sui-framework/balance.md#0x2_balance_Balance">Balance</a></code> and take a sub balance from it.


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/balance.md#0x2_balance_split">split</a>&lt;T&gt;(self: &<b>mut</b> <a href="../sui-framework/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;T&gt;, value: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>): <a href="../sui-framework/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/balance.md#0x2_balance_split">split</a>&lt;T&gt;(self: &<b>mut</b> <a href="../sui-framework/balance.md#0x2_balance_Balance">Balance</a>&lt;T&gt;, value: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>): <a href="../sui-framework/balance.md#0x2_balance_Balance">Balance</a>&lt;T&gt; {
    <b>assert</b>!(self.value &gt;= value, <a href="../sui-framework/balance.md#0x2_balance_ENotEnough">ENotEnough</a>);
    self.value = self.value - value;
    <a href="../sui-framework/balance.md#0x2_balance_Balance">Balance</a> { value }
}
</code></pre>



</details>

<a name="0x2_balance_withdraw_all"></a>

## Function `withdraw_all`

Withdraw all balance. After this the remaining balance must be 0.


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/balance.md#0x2_balance_withdraw_all">withdraw_all</a>&lt;T&gt;(self: &<b>mut</b> <a href="../sui-framework/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;T&gt;): <a href="../sui-framework/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/balance.md#0x2_balance_withdraw_all">withdraw_all</a>&lt;T&gt;(self: &<b>mut</b> <a href="../sui-framework/balance.md#0x2_balance_Balance">Balance</a>&lt;T&gt;): <a href="../sui-framework/balance.md#0x2_balance_Balance">Balance</a>&lt;T&gt; {
    <b>let</b> value = self.value;
    <a href="../sui-framework/balance.md#0x2_balance_split">split</a>(self, value)
}
</code></pre>



</details>

<a name="0x2_balance_destroy_zero"></a>

## Function `destroy_zero`

Destroy a zero <code><a href="../sui-framework/balance.md#0x2_balance_Balance">Balance</a></code>.


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/balance.md#0x2_balance_destroy_zero">destroy_zero</a>&lt;T&gt;(<a href="../sui-framework/balance.md#0x2_balance">balance</a>: <a href="../sui-framework/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/balance.md#0x2_balance_destroy_zero">destroy_zero</a>&lt;T&gt;(<a href="../sui-framework/balance.md#0x2_balance">balance</a>: <a href="../sui-framework/balance.md#0x2_balance_Balance">Balance</a>&lt;T&gt;) {
    <b>assert</b>!(<a href="../sui-framework/balance.md#0x2_balance">balance</a>.value == 0, <a href="../sui-framework/balance.md#0x2_balance_ENonZero">ENonZero</a>);
    <b>let</b> <a href="../sui-framework/balance.md#0x2_balance_Balance">Balance</a> { value: _ } = <a href="../sui-framework/balance.md#0x2_balance">balance</a>;
}
</code></pre>



</details>

<a name="0x2_balance_create_staking_rewards"></a>

## Function `create_staking_rewards`

CAUTION: this function creates a <code><a href="../sui-framework/balance.md#0x2_balance_Balance">Balance</a></code> without increasing the supply.
It should only be called by the epoch change system txn to create staking rewards,
and nowhere else.


<pre><code><b>fun</b> <a href="../sui-framework/balance.md#0x2_balance_create_staking_rewards">create_staking_rewards</a>&lt;T&gt;(value: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>, ctx: &<a href="../sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="../sui-framework/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui-framework/balance.md#0x2_balance_create_staking_rewards">create_staking_rewards</a>&lt;T&gt;(value: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>, ctx: &TxContext): <a href="../sui-framework/balance.md#0x2_balance_Balance">Balance</a>&lt;T&gt; {
    <b>assert</b>!(ctx.sender() == @0x0, <a href="../sui-framework/balance.md#0x2_balance_ENotSystemAddress">ENotSystemAddress</a>);
    <b>assert</b>!(std::type_name::get&lt;T&gt;().into_string().into_bytes() == <a href="../sui-framework/balance.md#0x2_balance_SUI_TYPE_NAME">SUI_TYPE_NAME</a>, <a href="../sui-framework/balance.md#0x2_balance_ENotSUI">ENotSUI</a>);
    <a href="../sui-framework/balance.md#0x2_balance_Balance">Balance</a> { value }
}
</code></pre>



</details>

<a name="0x2_balance_destroy_storage_rebates"></a>

## Function `destroy_storage_rebates`

CAUTION: this function destroys a <code><a href="../sui-framework/balance.md#0x2_balance_Balance">Balance</a></code> without decreasing the supply.
It should only be called by the epoch change system txn to destroy storage rebates,
and nowhere else.


<pre><code><b>fun</b> <a href="../sui-framework/balance.md#0x2_balance_destroy_storage_rebates">destroy_storage_rebates</a>&lt;T&gt;(self: <a href="../sui-framework/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;T&gt;, ctx: &<a href="../sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui-framework/balance.md#0x2_balance_destroy_storage_rebates">destroy_storage_rebates</a>&lt;T&gt;(self: <a href="../sui-framework/balance.md#0x2_balance_Balance">Balance</a>&lt;T&gt;, ctx: &TxContext) {
    <b>assert</b>!(ctx.sender() == @0x0, <a href="../sui-framework/balance.md#0x2_balance_ENotSystemAddress">ENotSystemAddress</a>);
    <b>assert</b>!(std::type_name::get&lt;T&gt;().into_string().into_bytes() == <a href="../sui-framework/balance.md#0x2_balance_SUI_TYPE_NAME">SUI_TYPE_NAME</a>, <a href="../sui-framework/balance.md#0x2_balance_ENotSUI">ENotSUI</a>);
    <b>let</b> <a href="../sui-framework/balance.md#0x2_balance_Balance">Balance</a> { value: _ } = self;
}
</code></pre>



</details>

<a name="0x2_balance_destroy_supply"></a>

## Function `destroy_supply`

Destroy a <code><a href="../sui-framework/balance.md#0x2_balance_Supply">Supply</a></code> preventing any further minting and burning.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="../sui-framework/balance.md#0x2_balance_destroy_supply">destroy_supply</a>&lt;T&gt;(self: <a href="../sui-framework/balance.md#0x2_balance_Supply">balance::Supply</a>&lt;T&gt;): <a href="../move-stdlib/u64.md#0x1_u64">u64</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui-framework/balance.md#0x2_balance_destroy_supply">destroy_supply</a>&lt;T&gt;(self: <a href="../sui-framework/balance.md#0x2_balance_Supply">Supply</a>&lt;T&gt;): <a href="../move-stdlib/u64.md#0x1_u64">u64</a> {
    <b>let</b> <a href="../sui-framework/balance.md#0x2_balance_Supply">Supply</a> { value } = self;
    value
}
</code></pre>



</details>
