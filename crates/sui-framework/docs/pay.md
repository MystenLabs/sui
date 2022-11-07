
<a name="0x2_pay"></a>

# Module `0x2::pay`

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


<pre><code><b>use</b> <a href="coin.md#0x2_coin">0x2::coin</a>;
<b>use</b> <a href="transfer.md#0x2_transfer">0x2::transfer</a>;
<b>use</b> <a href="tx_context.md#0x2_tx_context">0x2::tx_context</a>;
</code></pre>



<a name="@Constants_0"></a>

## Constants


<a name="0x2_pay_ENoCoins"></a>

For when empty vector is supplied into join function.


<pre><code><b>const</b> <a href="pay.md#0x2_pay_ENoCoins">ENoCoins</a>: u64 = 0;
</code></pre>



<a name="0x2_pay_keep"></a>

## Function `keep`

Transfer <code>c</code> to the sender of the current transaction


<pre><code><b>public</b> <b>fun</b> <a href="pay.md#0x2_pay_keep">keep</a>&lt;T&gt;(c: <a href="coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;, ctx: &<a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="pay.md#0x2_pay_keep">keep</a>&lt;T&gt;(c: Coin&lt;T&gt;, ctx: &TxContext) {
    <a href="transfer.md#0x2_transfer_transfer">transfer::transfer</a>(c, <a href="tx_context.md#0x2_tx_context_sender">tx_context::sender</a>(ctx))
}
</code></pre>



</details>

<a name="0x2_pay_split"></a>

## Function `split`

Split coin <code>self</code> to two coins, one with balance <code>split_amount</code>,
and the remaining balance is left is <code>self</code>.


<pre><code><b>public</b> entry <b>fun</b> <a href="pay.md#0x2_pay_split">split</a>&lt;T&gt;(self: &<b>mut</b> <a href="coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;, split_amount: u64, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="pay.md#0x2_pay_split">split</a>&lt;T&gt;(
    self: &<b>mut</b> Coin&lt;T&gt;, split_amount: u64, ctx: &<b>mut</b> TxContext
) {
    <a href="pay.md#0x2_pay_keep">keep</a>(<a href="coin.md#0x2_coin_split">coin::split</a>(self, split_amount, ctx), ctx)
}
</code></pre>



</details>

<a name="0x2_pay_split_vec"></a>

## Function `split_vec`

Split coin <code>self</code> into multiple coins, each with balance specified
in <code>split_amounts</code>. Remaining balance is left in <code>self</code>.


<pre><code><b>public</b> entry <b>fun</b> <a href="pay.md#0x2_pay_split_vec">split_vec</a>&lt;T&gt;(self: &<b>mut</b> <a href="coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;, split_amounts: <a href="">vector</a>&lt;u64&gt;, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="pay.md#0x2_pay_split_vec">split_vec</a>&lt;T&gt;(
    self: &<b>mut</b> Coin&lt;T&gt;, split_amounts: <a href="">vector</a>&lt;u64&gt;, ctx: &<b>mut</b> TxContext
) {
    <b>let</b> (i, len) = (0, <a href="_length">vector::length</a>(&split_amounts));
    <b>while</b> (i &lt; len) {
        <a href="pay.md#0x2_pay_split">split</a>(self, *<a href="_borrow">vector::borrow</a>(&split_amounts, i), ctx);
        i = i + 1;
    };
}
</code></pre>



</details>

<a name="0x2_pay_split_and_transfer"></a>

## Function `split_and_transfer`

Send <code>amount</code> units of <code>c</code> to <code>recipient</code>
Aborts with <code>EVALUE</code> if <code>amount</code> is greater than or equal to <code>amount</code>


<pre><code><b>public</b> entry <b>fun</b> <a href="pay.md#0x2_pay_split_and_transfer">split_and_transfer</a>&lt;T&gt;(c: &<b>mut</b> <a href="coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;, amount: u64, recipient: <b>address</b>, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="pay.md#0x2_pay_split_and_transfer">split_and_transfer</a>&lt;T&gt;(
    c: &<b>mut</b> Coin&lt;T&gt;, amount: u64, recipient: <b>address</b>, ctx: &<b>mut</b> TxContext
) {
    <a href="transfer.md#0x2_transfer_transfer">transfer::transfer</a>(<a href="coin.md#0x2_coin_split">coin::split</a>(c, amount, ctx), recipient)
}
</code></pre>



</details>

<a name="0x2_pay_divide_and_keep"></a>

## Function `divide_and_keep`

Divide coin <code>self</code> into <code>n - 1</code> coins with equal balances. If the balance is
not evenly divisible by <code>n</code>, the remainder is left in <code>self</code>.


<pre><code><b>public</b> entry <b>fun</b> <a href="pay.md#0x2_pay_divide_and_keep">divide_and_keep</a>&lt;T&gt;(self: &<b>mut</b> <a href="coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;, n: u64, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="pay.md#0x2_pay_divide_and_keep">divide_and_keep</a>&lt;T&gt;(
    self: &<b>mut</b> Coin&lt;T&gt;, n: u64, ctx: &<b>mut</b> TxContext
) {
    <b>let</b> vec: <a href="">vector</a>&lt;Coin&lt;T&gt;&gt; = <a href="coin.md#0x2_coin_divide_into_n">coin::divide_into_n</a>(self, n, ctx);
    <b>let</b> (i, len) = (0, <a href="_length">vector::length</a>(&vec));
    <b>while</b> (i &lt; len) {
        <a href="transfer.md#0x2_transfer_transfer">transfer::transfer</a>(<a href="_pop_back">vector::pop_back</a>(&<b>mut</b> vec), <a href="tx_context.md#0x2_tx_context_sender">tx_context::sender</a>(ctx));
        i = i + 1;
    };
    <a href="_destroy_empty">vector::destroy_empty</a>(vec);
}
</code></pre>



</details>

<a name="0x2_pay_join"></a>

## Function `join`

Join <code><a href="coin.md#0x2_coin">coin</a></code> into <code>self</code>. Re-exports <code><a href="coin.md#0x2_coin_join">coin::join</a></code> function.


<pre><code><b>public</b> entry <b>fun</b> <a href="pay.md#0x2_pay_join">join</a>&lt;T&gt;(self: &<b>mut</b> <a href="coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;, <a href="coin.md#0x2_coin">coin</a>: <a href="coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="pay.md#0x2_pay_join">join</a>&lt;T&gt;(self: &<b>mut</b> Coin&lt;T&gt;, <a href="coin.md#0x2_coin">coin</a>: Coin&lt;T&gt;) {
    <a href="coin.md#0x2_coin_join">coin::join</a>(self, <a href="coin.md#0x2_coin">coin</a>)
}
</code></pre>



</details>

<a name="0x2_pay_join_vec"></a>

## Function `join_vec`

Join everything in <code>coins</code> with <code>self</code>


<pre><code><b>public</b> entry <b>fun</b> <a href="pay.md#0x2_pay_join_vec">join_vec</a>&lt;T&gt;(self: &<b>mut</b> <a href="coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;, coins: <a href="">vector</a>&lt;<a href="coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="pay.md#0x2_pay_join_vec">join_vec</a>&lt;T&gt;(self: &<b>mut</b> Coin&lt;T&gt;, coins: <a href="">vector</a>&lt;Coin&lt;T&gt;&gt;) {
    <b>let</b> (i, len) = (0, <a href="_length">vector::length</a>(&coins));
    <b>while</b> (i &lt; len) {
        <b>let</b> <a href="coin.md#0x2_coin">coin</a> = <a href="_pop_back">vector::pop_back</a>(&<b>mut</b> coins);
        <a href="coin.md#0x2_coin_join">coin::join</a>(self, <a href="coin.md#0x2_coin">coin</a>);
        i = i + 1
    };
    // <a href="safe.md#0x2_safe">safe</a> because we've drained the <a href="">vector</a>
    <a href="_destroy_empty">vector::destroy_empty</a>(coins)
}
</code></pre>



</details>

<a name="0x2_pay_join_vec_and_transfer"></a>

## Function `join_vec_and_transfer`

Join a vector of <code>Coin</code> into a single object and transfer it to <code>receiver</code>.


<pre><code><b>public</b> entry <b>fun</b> <a href="pay.md#0x2_pay_join_vec_and_transfer">join_vec_and_transfer</a>&lt;T&gt;(coins: <a href="">vector</a>&lt;<a href="coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;&gt;, receiver: <b>address</b>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="pay.md#0x2_pay_join_vec_and_transfer">join_vec_and_transfer</a>&lt;T&gt;(coins: <a href="">vector</a>&lt;Coin&lt;T&gt;&gt;, receiver: <b>address</b>) {
    <b>assert</b>!(<a href="_length">vector::length</a>(&coins) &gt; 0, <a href="pay.md#0x2_pay_ENoCoins">ENoCoins</a>);

    <b>let</b> self = <a href="_pop_back">vector::pop_back</a>(&<b>mut</b> coins);
    <a href="pay.md#0x2_pay_join_vec">join_vec</a>(&<b>mut</b> self, coins);
    <a href="transfer.md#0x2_transfer_transfer">transfer::transfer</a>(self, receiver)
}
</code></pre>



</details>
