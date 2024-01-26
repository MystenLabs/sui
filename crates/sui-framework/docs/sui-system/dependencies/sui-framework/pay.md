
<a name="0x2_pay"></a>

# Module `0x2::pay`



-  [Constants](#@Constants_0)
-  [Function `keep`](#0x2_pay_keep)
-  [Function `split`](#0x2_pay_split)
-  [Function `split_vec`](#0x2_pay_split_vec)
-  [Function `split_and_transfer`](#0x2_pay_split_and_transfer)
-  [Function `divide_and_keep`](#0x2_pay_divide_and_keep)
-  [Function `join`](#0x2_pay_join)
-  [Function `join_vec`](#0x2_pay_join_vec)
-  [Function `join_vec_and_transfer`](#0x2_pay_join_vec_and_transfer)


<pre><code><b>use</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin">0x2::coin</a>;
<b>use</b> <a href="../../dependencies/sui-framework/transfer.md#0x2_transfer">0x2::transfer</a>;
<b>use</b> <a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context">0x2::tx_context</a>;
</code></pre>



<a name="@Constants_0"></a>

## Constants


<a name="0x2_pay_ENoCoins"></a>



<pre><code><b>const</b> <a href="../../dependencies/sui-framework/pay.md#0x2_pay_ENoCoins">ENoCoins</a>: u64 = 0;
</code></pre>



<a name="0x2_pay_keep"></a>

## Function `keep`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/pay.md#0x2_pay_keep">keep</a>&lt;T&gt;(c: <a href="../../dependencies/sui-framework/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;, ctx: &<a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/pay.md#0x2_pay_keep">keep</a>&lt;T&gt;(c: Coin&lt;T&gt;, ctx: &TxContext) {
    <a href="../../dependencies/sui-framework/transfer.md#0x2_transfer_public_transfer">transfer::public_transfer</a>(c, <a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_sender">tx_context::sender</a>(ctx))
}
</code></pre>



</details>

<a name="0x2_pay_split"></a>

## Function `split`



<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-framework/pay.md#0x2_pay_split">split</a>&lt;T&gt;(self: &<b>mut</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;, split_amount: u64, ctx: &<b>mut</b> <a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-framework/pay.md#0x2_pay_split">split</a>&lt;T&gt;(
    self: &<b>mut</b> Coin&lt;T&gt;, split_amount: u64, ctx: &<b>mut</b> TxContext
) {
    <a href="../../dependencies/sui-framework/pay.md#0x2_pay_keep">keep</a>(<a href="../../dependencies/sui-framework/coin.md#0x2_coin_split">coin::split</a>(self, split_amount, ctx), ctx)
}
</code></pre>



</details>

<a name="0x2_pay_split_vec"></a>

## Function `split_vec`



<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-framework/pay.md#0x2_pay_split_vec">split_vec</a>&lt;T&gt;(self: &<b>mut</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;, split_amounts: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u64&gt;, ctx: &<b>mut</b> <a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-framework/pay.md#0x2_pay_split_vec">split_vec</a>&lt;T&gt;(
    self: &<b>mut</b> Coin&lt;T&gt;, split_amounts: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u64&gt;, ctx: &<b>mut</b> TxContext
) {
    <b>let</b> (i, len) = (0, <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_length">vector::length</a>(&split_amounts));
    <b>while</b> (i &lt; len) {
        <a href="../../dependencies/sui-framework/pay.md#0x2_pay_split">split</a>(self, *<a href="../../dependencies/move-stdlib/vector.md#0x1_vector_borrow">vector::borrow</a>(&split_amounts, i), ctx);
        i = i + 1;
    };
}
</code></pre>



</details>

<a name="0x2_pay_split_and_transfer"></a>

## Function `split_and_transfer`



<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-framework/pay.md#0x2_pay_split_and_transfer">split_and_transfer</a>&lt;T&gt;(c: &<b>mut</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;, amount: u64, recipient: <b>address</b>, ctx: &<b>mut</b> <a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-framework/pay.md#0x2_pay_split_and_transfer">split_and_transfer</a>&lt;T&gt;(
    c: &<b>mut</b> Coin&lt;T&gt;, amount: u64, recipient: <b>address</b>, ctx: &<b>mut</b> TxContext
) {
    <a href="../../dependencies/sui-framework/transfer.md#0x2_transfer_public_transfer">transfer::public_transfer</a>(<a href="../../dependencies/sui-framework/coin.md#0x2_coin_split">coin::split</a>(c, amount, ctx), recipient)
}
</code></pre>



</details>

<a name="0x2_pay_divide_and_keep"></a>

## Function `divide_and_keep`



<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-framework/pay.md#0x2_pay_divide_and_keep">divide_and_keep</a>&lt;T&gt;(self: &<b>mut</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;, n: u64, ctx: &<b>mut</b> <a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-framework/pay.md#0x2_pay_divide_and_keep">divide_and_keep</a>&lt;T&gt;(
    self: &<b>mut</b> Coin&lt;T&gt;, n: u64, ctx: &<b>mut</b> TxContext
) {
    <b>let</b> vec: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;Coin&lt;T&gt;&gt; = <a href="../../dependencies/sui-framework/coin.md#0x2_coin_divide_into_n">coin::divide_into_n</a>(self, n, ctx);
    <b>let</b> (i, len) = (0, <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_length">vector::length</a>(&vec));
    <b>while</b> (i &lt; len) {
        <a href="../../dependencies/sui-framework/transfer.md#0x2_transfer_public_transfer">transfer::public_transfer</a>(<a href="../../dependencies/move-stdlib/vector.md#0x1_vector_pop_back">vector::pop_back</a>(&<b>mut</b> vec), <a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_sender">tx_context::sender</a>(ctx));
        i = i + 1;
    };
    <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_destroy_empty">vector::destroy_empty</a>(vec);
}
</code></pre>



</details>

<a name="0x2_pay_join"></a>

## Function `join`



<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-framework/pay.md#0x2_pay_join">join</a>&lt;T&gt;(self: &<b>mut</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;, <a href="../../dependencies/sui-framework/coin.md#0x2_coin">coin</a>: <a href="../../dependencies/sui-framework/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-framework/pay.md#0x2_pay_join">join</a>&lt;T&gt;(self: &<b>mut</b> Coin&lt;T&gt;, <a href="../../dependencies/sui-framework/coin.md#0x2_coin">coin</a>: Coin&lt;T&gt;) {
    <a href="../../dependencies/sui-framework/coin.md#0x2_coin_join">coin::join</a>(self, <a href="../../dependencies/sui-framework/coin.md#0x2_coin">coin</a>)
}
</code></pre>



</details>

<a name="0x2_pay_join_vec"></a>

## Function `join_vec`



<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-framework/pay.md#0x2_pay_join_vec">join_vec</a>&lt;T&gt;(self: &<b>mut</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;, coins: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="../../dependencies/sui-framework/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-framework/pay.md#0x2_pay_join_vec">join_vec</a>&lt;T&gt;(self: &<b>mut</b> Coin&lt;T&gt;, coins: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;Coin&lt;T&gt;&gt;) {
    <b>let</b> (i, len) = (0, <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_length">vector::length</a>(&coins));
    <b>while</b> (i &lt; len) {
        <b>let</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin">coin</a> = <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_pop_back">vector::pop_back</a>(&<b>mut</b> coins);
        <a href="../../dependencies/sui-framework/coin.md#0x2_coin_join">coin::join</a>(self, <a href="../../dependencies/sui-framework/coin.md#0x2_coin">coin</a>);
        i = i + 1
    };
    // safe because we've drained the <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>
    <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_destroy_empty">vector::destroy_empty</a>(coins)
}
</code></pre>



</details>

<a name="0x2_pay_join_vec_and_transfer"></a>

## Function `join_vec_and_transfer`



<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-framework/pay.md#0x2_pay_join_vec_and_transfer">join_vec_and_transfer</a>&lt;T&gt;(coins: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="../../dependencies/sui-framework/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;&gt;, receiver: <b>address</b>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-framework/pay.md#0x2_pay_join_vec_and_transfer">join_vec_and_transfer</a>&lt;T&gt;(coins: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;Coin&lt;T&gt;&gt;, receiver: <b>address</b>) {
    <b>assert</b>!(<a href="../../dependencies/move-stdlib/vector.md#0x1_vector_length">vector::length</a>(&coins) &gt; 0, <a href="../../dependencies/sui-framework/pay.md#0x2_pay_ENoCoins">ENoCoins</a>);

    <b>let</b> self = <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_pop_back">vector::pop_back</a>(&<b>mut</b> coins);
    <a href="../../dependencies/sui-framework/pay.md#0x2_pay_join_vec">join_vec</a>(&<b>mut</b> self, coins);
    <a href="../../dependencies/sui-framework/transfer.md#0x2_transfer_public_transfer">transfer::public_transfer</a>(self, receiver)
}
</code></pre>



</details>
