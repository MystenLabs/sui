---
title: Module `0x2::pay`
---

This module provides handy functionality for wallets and <code>sui::Coin</code> management.


-  [Constants](#@Constants_0)
-  [Function `keep`](#0x2_pay_keep)
-  [Function `split`](#0x2_pay_split)
-  [Function `split_vec`](#0x2_pay_split_vec)
-  [Function `split_and_transfer`](#0x2_pay_split_and_transfer)
-  [Function `divide_and_keep`](#0x2_pay_divide_and_keep)
-  [Function `join`](#0x2_pay_join)
-  [Function `join_vec`](#0x2_pay_join_vec)
-  [Function `join_vec_and_transfer`](#0x2_pay_join_vec_and_transfer)


<pre><code><b>use</b> <a href="../sui-framework/coin.md#0x2_coin">0x2::coin</a>;
<b>use</b> <a href="../sui-framework/transfer.md#0x2_transfer">0x2::transfer</a>;
<b>use</b> <a href="../sui-framework/tx_context.md#0x2_tx_context">0x2::tx_context</a>;
</code></pre>



<a name="@Constants_0"></a>

## Constants


<a name="0x2_pay_ENoCoins"></a>

For when empty vector is supplied into join function.


<pre><code><b>const</b> <a href="../sui-framework/pay.md#0x2_pay_ENoCoins">ENoCoins</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 0;
</code></pre>



<a name="0x2_pay_keep"></a>

## Function `keep`

Transfer <code>c</code> to the sender of the current transaction


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/pay.md#0x2_pay_keep">keep</a>&lt;T&gt;(c: <a href="../sui-framework/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;, ctx: &<a href="../sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/pay.md#0x2_pay_keep">keep</a>&lt;T&gt;(c: Coin&lt;T&gt;, ctx: &TxContext) {
    <a href="../sui-framework/transfer.md#0x2_transfer_public_transfer">transfer::public_transfer</a>(c, ctx.sender())
}
</code></pre>



</details>

<a name="0x2_pay_split"></a>

## Function `split`

Split coin <code>self</code> to two coins, one with balance <code>split_amount</code>,
and the remaining balance is left is <code>self</code>.


<pre><code><b>public</b> entry <b>fun</b> <a href="../sui-framework/pay.md#0x2_pay_split">split</a>&lt;T&gt;(<a href="../sui-framework/coin.md#0x2_coin">coin</a>: &<b>mut</b> <a href="../sui-framework/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;, split_amount: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>, ctx: &<b>mut</b> <a href="../sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="../sui-framework/pay.md#0x2_pay_split">split</a>&lt;T&gt;(
    <a href="../sui-framework/coin.md#0x2_coin">coin</a>: &<b>mut</b> Coin&lt;T&gt;, split_amount: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>, ctx: &<b>mut</b> TxContext
) {
    <a href="../sui-framework/pay.md#0x2_pay_keep">keep</a>(<a href="../sui-framework/coin.md#0x2_coin">coin</a>.<a href="../sui-framework/pay.md#0x2_pay_split">split</a>(split_amount, ctx), ctx)
}
</code></pre>



</details>

<a name="0x2_pay_split_vec"></a>

## Function `split_vec`

Split coin <code>self</code> into multiple coins, each with balance specified
in <code>split_amounts</code>. Remaining balance is left in <code>self</code>.


<pre><code><b>public</b> entry <b>fun</b> <a href="../sui-framework/pay.md#0x2_pay_split_vec">split_vec</a>&lt;T&gt;(self: &<b>mut</b> <a href="../sui-framework/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;, split_amounts: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="../move-stdlib/u64.md#0x1_u64">u64</a>&gt;, ctx: &<b>mut</b> <a href="../sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="../sui-framework/pay.md#0x2_pay_split_vec">split_vec</a>&lt;T&gt;(
    self: &<b>mut</b> Coin&lt;T&gt;, split_amounts: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="../move-stdlib/u64.md#0x1_u64">u64</a>&gt;, ctx: &<b>mut</b> TxContext
) {
    <b>let</b> (<b>mut</b> i, len) = (0, split_amounts.length());
    <b>while</b> (i &lt; len) {
        <a href="../sui-framework/pay.md#0x2_pay_split">split</a>(self, split_amounts[i], ctx);
        i = i + 1;
    };
}
</code></pre>



</details>

<a name="0x2_pay_split_and_transfer"></a>

## Function `split_and_transfer`

Send <code>amount</code> units of <code>c</code> to <code>recipient</code>
Aborts with <code>EVALUE</code> if <code>amount</code> is greater than or equal to <code>amount</code>


<pre><code><b>public</b> entry <b>fun</b> <a href="../sui-framework/pay.md#0x2_pay_split_and_transfer">split_and_transfer</a>&lt;T&gt;(c: &<b>mut</b> <a href="../sui-framework/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;, amount: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>, recipient: <b>address</b>, ctx: &<b>mut</b> <a href="../sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="../sui-framework/pay.md#0x2_pay_split_and_transfer">split_and_transfer</a>&lt;T&gt;(
    c: &<b>mut</b> Coin&lt;T&gt;, amount: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>, recipient: <b>address</b>, ctx: &<b>mut</b> TxContext
) {
    <a href="../sui-framework/transfer.md#0x2_transfer_public_transfer">transfer::public_transfer</a>(c.<a href="../sui-framework/pay.md#0x2_pay_split">split</a>(amount, ctx), recipient)
}
</code></pre>



</details>

<a name="0x2_pay_divide_and_keep"></a>

## Function `divide_and_keep`

Divide coin <code>self</code> into <code>n - 1</code> coins with equal balances. If the balance is
not evenly divisible by <code>n</code>, the remainder is left in <code>self</code>.


<pre><code><b>public</b> entry <b>fun</b> <a href="../sui-framework/pay.md#0x2_pay_divide_and_keep">divide_and_keep</a>&lt;T&gt;(self: &<b>mut</b> <a href="../sui-framework/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;, n: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>, ctx: &<b>mut</b> <a href="../sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="../sui-framework/pay.md#0x2_pay_divide_and_keep">divide_and_keep</a>&lt;T&gt;(
    self: &<b>mut</b> Coin&lt;T&gt;, n: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>, ctx: &<b>mut</b> TxContext
) {
    <b>let</b> <b>mut</b> vec: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;Coin&lt;T&gt;&gt; = self.divide_into_n(n, ctx);
    <b>let</b> (<b>mut</b> i, len) = (0, vec.length());
    <b>while</b> (i &lt; len) {
        <a href="../sui-framework/transfer.md#0x2_transfer_public_transfer">transfer::public_transfer</a>(vec.pop_back(), ctx.sender());
        i = i + 1;
    };
    vec.destroy_empty();
}
</code></pre>



</details>

<a name="0x2_pay_join"></a>

## Function `join`

Join <code><a href="../sui-framework/coin.md#0x2_coin">coin</a></code> into <code>self</code>. Re-exports <code><a href="../sui-framework/coin.md#0x2_coin_join">coin::join</a></code> function.
Deprecated: you should call <code><a href="../sui-framework/coin.md#0x2_coin">coin</a>.<a href="../sui-framework/pay.md#0x2_pay_join">join</a>(other)</code> directly.


<pre><code><b>public</b> entry <b>fun</b> <a href="../sui-framework/pay.md#0x2_pay_join">join</a>&lt;T&gt;(self: &<b>mut</b> <a href="../sui-framework/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;, <a href="../sui-framework/coin.md#0x2_coin">coin</a>: <a href="../sui-framework/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="../sui-framework/pay.md#0x2_pay_join">join</a>&lt;T&gt;(self: &<b>mut</b> Coin&lt;T&gt;, <a href="../sui-framework/coin.md#0x2_coin">coin</a>: Coin&lt;T&gt;) {
    self.<a href="../sui-framework/pay.md#0x2_pay_join">join</a>(<a href="../sui-framework/coin.md#0x2_coin">coin</a>)
}
</code></pre>



</details>

<a name="0x2_pay_join_vec"></a>

## Function `join_vec`

Join everything in <code>coins</code> with <code>self</code>


<pre><code><b>public</b> entry <b>fun</b> <a href="../sui-framework/pay.md#0x2_pay_join_vec">join_vec</a>&lt;T&gt;(self: &<b>mut</b> <a href="../sui-framework/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;, coins: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="../sui-framework/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="../sui-framework/pay.md#0x2_pay_join_vec">join_vec</a>&lt;T&gt;(self: &<b>mut</b> Coin&lt;T&gt;, <b>mut</b> coins: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;Coin&lt;T&gt;&gt;) {
    <b>let</b> (<b>mut</b> i, len) = (0, coins.length());
    <b>while</b> (i &lt; len) {
        <b>let</b> <a href="../sui-framework/coin.md#0x2_coin">coin</a> = coins.pop_back();
        self.<a href="../sui-framework/pay.md#0x2_pay_join">join</a>(<a href="../sui-framework/coin.md#0x2_coin">coin</a>);
        i = i + 1
    };
    // safe because we've drained the <a href="../move-stdlib/vector.md#0x1_vector">vector</a>
    coins.destroy_empty()
}
</code></pre>



</details>

<a name="0x2_pay_join_vec_and_transfer"></a>

## Function `join_vec_and_transfer`

Join a vector of <code>Coin</code> into a single object and transfer it to <code>receiver</code>.


<pre><code><b>public</b> entry <b>fun</b> <a href="../sui-framework/pay.md#0x2_pay_join_vec_and_transfer">join_vec_and_transfer</a>&lt;T&gt;(coins: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="../sui-framework/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;&gt;, receiver: <b>address</b>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="../sui-framework/pay.md#0x2_pay_join_vec_and_transfer">join_vec_and_transfer</a>&lt;T&gt;(<b>mut</b> coins: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;Coin&lt;T&gt;&gt;, receiver: <b>address</b>) {
    <b>assert</b>!(coins.length() &gt; 0, <a href="../sui-framework/pay.md#0x2_pay_ENoCoins">ENoCoins</a>);

    <b>let</b> <b>mut</b> self = coins.pop_back();
    <a href="../sui-framework/pay.md#0x2_pay_join_vec">join_vec</a>(&<b>mut</b> self, coins);
    <a href="../sui-framework/transfer.md#0x2_transfer_public_transfer">transfer::public_transfer</a>(self, receiver)
}
</code></pre>



</details>
