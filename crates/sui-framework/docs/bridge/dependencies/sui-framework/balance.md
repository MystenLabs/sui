
<a name="0x2_balance"></a>

# Module `0x2::balance`



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


<pre><code><b>use</b> <a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context">0x2::tx_context</a>;
</code></pre>



<a name="0x2_balance_Supply"></a>

## Struct `Supply`



<pre><code><b>struct</b> <a href="../../dependencies/sui-framework/balance.md#0x2_balance_Supply">Supply</a>&lt;T&gt; <b>has</b> store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>value: u64</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x2_balance_Balance"></a>

## Struct `Balance`



<pre><code><b>struct</b> <a href="../../dependencies/sui-framework/balance.md#0x2_balance_Balance">Balance</a>&lt;T&gt; <b>has</b> store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>value: u64</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0x2_balance_ENotSystemAddress"></a>



<pre><code><b>const</b> <a href="../../dependencies/sui-framework/balance.md#0x2_balance_ENotSystemAddress">ENotSystemAddress</a>: u64 = 3;
</code></pre>



<a name="0x2_balance_ENonZero"></a>



<pre><code><b>const</b> <a href="../../dependencies/sui-framework/balance.md#0x2_balance_ENonZero">ENonZero</a>: u64 = 0;
</code></pre>



<a name="0x2_balance_ENotEnough"></a>



<pre><code><b>const</b> <a href="../../dependencies/sui-framework/balance.md#0x2_balance_ENotEnough">ENotEnough</a>: u64 = 2;
</code></pre>



<a name="0x2_balance_EOverflow"></a>



<pre><code><b>const</b> <a href="../../dependencies/sui-framework/balance.md#0x2_balance_EOverflow">EOverflow</a>: u64 = 1;
</code></pre>



<a name="0x2_balance_value"></a>

## Function `value`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/balance.md#0x2_balance_value">value</a>&lt;T&gt;(self: &<a href="../../dependencies/sui-framework/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;T&gt;): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/balance.md#0x2_balance_value">value</a>&lt;T&gt;(self: &<a href="../../dependencies/sui-framework/balance.md#0x2_balance_Balance">Balance</a>&lt;T&gt;): u64 {
    self.value
}
</code></pre>



</details>

<a name="0x2_balance_supply_value"></a>

## Function `supply_value`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/balance.md#0x2_balance_supply_value">supply_value</a>&lt;T&gt;(supply: &<a href="../../dependencies/sui-framework/balance.md#0x2_balance_Supply">balance::Supply</a>&lt;T&gt;): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/balance.md#0x2_balance_supply_value">supply_value</a>&lt;T&gt;(supply: &<a href="../../dependencies/sui-framework/balance.md#0x2_balance_Supply">Supply</a>&lt;T&gt;): u64 {
    supply.value
}
</code></pre>



</details>

<a name="0x2_balance_create_supply"></a>

## Function `create_supply`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/balance.md#0x2_balance_create_supply">create_supply</a>&lt;T: drop&gt;(_: T): <a href="../../dependencies/sui-framework/balance.md#0x2_balance_Supply">balance::Supply</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/balance.md#0x2_balance_create_supply">create_supply</a>&lt;T: drop&gt;(_: T): <a href="../../dependencies/sui-framework/balance.md#0x2_balance_Supply">Supply</a>&lt;T&gt; {
    <a href="../../dependencies/sui-framework/balance.md#0x2_balance_Supply">Supply</a> { value: 0 }
}
</code></pre>



</details>

<a name="0x2_balance_increase_supply"></a>

## Function `increase_supply`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/balance.md#0x2_balance_increase_supply">increase_supply</a>&lt;T&gt;(self: &<b>mut</b> <a href="../../dependencies/sui-framework/balance.md#0x2_balance_Supply">balance::Supply</a>&lt;T&gt;, value: u64): <a href="../../dependencies/sui-framework/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/balance.md#0x2_balance_increase_supply">increase_supply</a>&lt;T&gt;(self: &<b>mut</b> <a href="../../dependencies/sui-framework/balance.md#0x2_balance_Supply">Supply</a>&lt;T&gt;, value: u64): <a href="../../dependencies/sui-framework/balance.md#0x2_balance_Balance">Balance</a>&lt;T&gt; {
    <b>assert</b>!(<a href="../../dependencies/sui-framework/balance.md#0x2_balance_value">value</a> &lt; (18446744073709551615u64 - self.value), <a href="../../dependencies/sui-framework/balance.md#0x2_balance_EOverflow">EOverflow</a>);
    self.value = self.value + value;
    <a href="../../dependencies/sui-framework/balance.md#0x2_balance_Balance">Balance</a> { value }
}
</code></pre>



</details>

<a name="0x2_balance_decrease_supply"></a>

## Function `decrease_supply`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/balance.md#0x2_balance_decrease_supply">decrease_supply</a>&lt;T&gt;(self: &<b>mut</b> <a href="../../dependencies/sui-framework/balance.md#0x2_balance_Supply">balance::Supply</a>&lt;T&gt;, <a href="../../dependencies/sui-framework/balance.md#0x2_balance">balance</a>: <a href="../../dependencies/sui-framework/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;T&gt;): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/balance.md#0x2_balance_decrease_supply">decrease_supply</a>&lt;T&gt;(self: &<b>mut</b> <a href="../../dependencies/sui-framework/balance.md#0x2_balance_Supply">Supply</a>&lt;T&gt;, <a href="../../dependencies/sui-framework/balance.md#0x2_balance">balance</a>: <a href="../../dependencies/sui-framework/balance.md#0x2_balance_Balance">Balance</a>&lt;T&gt;): u64 {
    <b>let</b> <a href="../../dependencies/sui-framework/balance.md#0x2_balance_Balance">Balance</a> { value } = <a href="../../dependencies/sui-framework/balance.md#0x2_balance">balance</a>;
    <b>assert</b>!(self.value &gt;= value, <a href="../../dependencies/sui-framework/balance.md#0x2_balance_EOverflow">EOverflow</a>);
    self.value = self.value - value;
    value
}
</code></pre>



</details>

<a name="0x2_balance_zero"></a>

## Function `zero`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/balance.md#0x2_balance_zero">zero</a>&lt;T&gt;(): <a href="../../dependencies/sui-framework/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/balance.md#0x2_balance_zero">zero</a>&lt;T&gt;(): <a href="../../dependencies/sui-framework/balance.md#0x2_balance_Balance">Balance</a>&lt;T&gt; {
    <a href="../../dependencies/sui-framework/balance.md#0x2_balance_Balance">Balance</a> { value: 0 }
}
</code></pre>



</details>

<a name="0x2_balance_join"></a>

## Function `join`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/balance.md#0x2_balance_join">join</a>&lt;T&gt;(self: &<b>mut</b> <a href="../../dependencies/sui-framework/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;T&gt;, <a href="../../dependencies/sui-framework/balance.md#0x2_balance">balance</a>: <a href="../../dependencies/sui-framework/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;T&gt;): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/balance.md#0x2_balance_join">join</a>&lt;T&gt;(self: &<b>mut</b> <a href="../../dependencies/sui-framework/balance.md#0x2_balance_Balance">Balance</a>&lt;T&gt;, <a href="../../dependencies/sui-framework/balance.md#0x2_balance">balance</a>: <a href="../../dependencies/sui-framework/balance.md#0x2_balance_Balance">Balance</a>&lt;T&gt;): u64 {
    <b>let</b> <a href="../../dependencies/sui-framework/balance.md#0x2_balance_Balance">Balance</a> { value } = <a href="../../dependencies/sui-framework/balance.md#0x2_balance">balance</a>;
    self.value = self.value + value;
    self.value
}
</code></pre>



</details>

<a name="0x2_balance_split"></a>

## Function `split`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/balance.md#0x2_balance_split">split</a>&lt;T&gt;(self: &<b>mut</b> <a href="../../dependencies/sui-framework/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;T&gt;, value: u64): <a href="../../dependencies/sui-framework/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/balance.md#0x2_balance_split">split</a>&lt;T&gt;(self: &<b>mut</b> <a href="../../dependencies/sui-framework/balance.md#0x2_balance_Balance">Balance</a>&lt;T&gt;, value: u64): <a href="../../dependencies/sui-framework/balance.md#0x2_balance_Balance">Balance</a>&lt;T&gt; {
    <b>assert</b>!(self.value &gt;= value, <a href="../../dependencies/sui-framework/balance.md#0x2_balance_ENotEnough">ENotEnough</a>);
    self.value = self.value - value;
    <a href="../../dependencies/sui-framework/balance.md#0x2_balance_Balance">Balance</a> { value }
}
</code></pre>



</details>

<a name="0x2_balance_withdraw_all"></a>

## Function `withdraw_all`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/balance.md#0x2_balance_withdraw_all">withdraw_all</a>&lt;T&gt;(self: &<b>mut</b> <a href="../../dependencies/sui-framework/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;T&gt;): <a href="../../dependencies/sui-framework/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/balance.md#0x2_balance_withdraw_all">withdraw_all</a>&lt;T&gt;(self: &<b>mut</b> <a href="../../dependencies/sui-framework/balance.md#0x2_balance_Balance">Balance</a>&lt;T&gt;): <a href="../../dependencies/sui-framework/balance.md#0x2_balance_Balance">Balance</a>&lt;T&gt; {
    <b>let</b> value = self.value;
    <a href="../../dependencies/sui-framework/balance.md#0x2_balance_split">split</a>(self, value)
}
</code></pre>



</details>

<a name="0x2_balance_destroy_zero"></a>

## Function `destroy_zero`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/balance.md#0x2_balance_destroy_zero">destroy_zero</a>&lt;T&gt;(<a href="../../dependencies/sui-framework/balance.md#0x2_balance">balance</a>: <a href="../../dependencies/sui-framework/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/balance.md#0x2_balance_destroy_zero">destroy_zero</a>&lt;T&gt;(<a href="../../dependencies/sui-framework/balance.md#0x2_balance">balance</a>: <a href="../../dependencies/sui-framework/balance.md#0x2_balance_Balance">Balance</a>&lt;T&gt;) {
    <b>assert</b>!(<a href="../../dependencies/sui-framework/balance.md#0x2_balance">balance</a>.value == 0, <a href="../../dependencies/sui-framework/balance.md#0x2_balance_ENonZero">ENonZero</a>);
    <b>let</b> <a href="../../dependencies/sui-framework/balance.md#0x2_balance_Balance">Balance</a> { value: _ } = <a href="../../dependencies/sui-framework/balance.md#0x2_balance">balance</a>;
}
</code></pre>



</details>

<a name="0x2_balance_create_staking_rewards"></a>

## Function `create_staking_rewards`



<pre><code><b>fun</b> <a href="../../dependencies/sui-framework/balance.md#0x2_balance_create_staking_rewards">create_staking_rewards</a>&lt;T&gt;(value: u64, ctx: &<a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="../../dependencies/sui-framework/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../../dependencies/sui-framework/balance.md#0x2_balance_create_staking_rewards">create_staking_rewards</a>&lt;T&gt;(value: u64, ctx: &TxContext): <a href="../../dependencies/sui-framework/balance.md#0x2_balance_Balance">Balance</a>&lt;T&gt; {
    <b>assert</b>!(<a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_sender">tx_context::sender</a>(ctx) == @0x0, <a href="../../dependencies/sui-framework/balance.md#0x2_balance_ENotSystemAddress">ENotSystemAddress</a>);
    <a href="../../dependencies/sui-framework/balance.md#0x2_balance_Balance">Balance</a> { value }
}
</code></pre>



</details>

<a name="0x2_balance_destroy_storage_rebates"></a>

## Function `destroy_storage_rebates`



<pre><code><b>fun</b> <a href="../../dependencies/sui-framework/balance.md#0x2_balance_destroy_storage_rebates">destroy_storage_rebates</a>&lt;T&gt;(self: <a href="../../dependencies/sui-framework/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;T&gt;, ctx: &<a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../../dependencies/sui-framework/balance.md#0x2_balance_destroy_storage_rebates">destroy_storage_rebates</a>&lt;T&gt;(self: <a href="../../dependencies/sui-framework/balance.md#0x2_balance_Balance">Balance</a>&lt;T&gt;, ctx: &TxContext) {
    <b>assert</b>!(<a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_sender">tx_context::sender</a>(ctx) == @0x0, <a href="../../dependencies/sui-framework/balance.md#0x2_balance_ENotSystemAddress">ENotSystemAddress</a>);
    <b>let</b> <a href="../../dependencies/sui-framework/balance.md#0x2_balance_Balance">Balance</a> { value: _ } = self;
}
</code></pre>



</details>

<a name="0x2_balance_destroy_supply"></a>

## Function `destroy_supply`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="../../dependencies/sui-framework/balance.md#0x2_balance_destroy_supply">destroy_supply</a>&lt;T&gt;(self: <a href="../../dependencies/sui-framework/balance.md#0x2_balance_Supply">balance::Supply</a>&lt;T&gt;): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="../../dependencies/sui-framework/balance.md#0x2_balance_destroy_supply">destroy_supply</a>&lt;T&gt;(self: <a href="../../dependencies/sui-framework/balance.md#0x2_balance_Supply">Supply</a>&lt;T&gt;): u64 {
    <b>let</b> <a href="../../dependencies/sui-framework/balance.md#0x2_balance_Supply">Supply</a> { value } = self;
    value
}
</code></pre>



</details>
