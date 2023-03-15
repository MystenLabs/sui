
<a name="0x2_wallet"></a>

# Module `0x2::wallet`



-  [Constants](#@Constants_0)
-  [Function `split`](#0x2_wallet_split)
-  [Function `split_vec`](#0x2_wallet_split_vec)
-  [Function `split_n_to_vec`](#0x2_wallet_split_n_to_vec)
-  [Function `split_n`](#0x2_wallet_split_n)
-  [Function `join_vec`](#0x2_wallet_join_vec)


<pre><code><b>use</b> <a href="">0x1::vector</a>;
<b>use</b> <a href="coin.md#0x2_coin">0x2::coin</a>;
<b>use</b> <a href="transfer.md#0x2_transfer">0x2::transfer</a>;
<b>use</b> <a href="tx_context.md#0x2_tx_context">0x2::tx_context</a>;
</code></pre>



<a name="@Constants_0"></a>

## Constants


<a name="0x2_wallet_ENotEnough"></a>

For when trying to split a coin more times than its balance allows.


<pre><code><b>const</b> <a href="wallet.md#0x2_wallet_ENotEnough">ENotEnough</a>: u64 = 0;
</code></pre>



<a name="0x2_wallet_EInvalidArg"></a>

For when invalid arguments are passed to a function.


<pre><code><b>const</b> <a href="wallet.md#0x2_wallet_EInvalidArg">EInvalidArg</a>: u64 = 1;
</code></pre>



<a name="0x2_wallet_split"></a>

## Function `split`

Split coin <code>self</code> to two coins, one with balance <code>split_amount</code>,
and the remaining balance is left is <code>self</code>.


<pre><code><b>public</b> <b>fun</b> <a href="wallet.md#0x2_wallet_split">split</a>&lt;T&gt;(self: &<b>mut</b> <a href="coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;, split_amount: u64, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="wallet.md#0x2_wallet_split">split</a>&lt;T&gt;(self: &<b>mut</b> Coin&lt;T&gt;, split_amount: u64, ctx: &<b>mut</b> TxContext) {
    <a href="transfer.md#0x2_transfer_transfer">transfer::transfer</a>(
        <a href="coin.md#0x2_coin_split">coin::split</a>(self, split_amount, ctx),
        <a href="tx_context.md#0x2_tx_context_sender">tx_context::sender</a>(ctx)
    )
}
</code></pre>



</details>

<a name="0x2_wallet_split_vec"></a>

## Function `split_vec`

Split coin <code>self</code> into multiple coins, each with balance specified
in <code>split_amounts</code>. Remaining balance is left in <code>self</code>.


<pre><code><b>public</b> <b>fun</b> <a href="wallet.md#0x2_wallet_split_vec">split_vec</a>&lt;T&gt;(self: &<b>mut</b> <a href="coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;, split_amounts: <a href="">vector</a>&lt;u64&gt;, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="wallet.md#0x2_wallet_split_vec">split_vec</a>&lt;T&gt;(self: &<b>mut</b> Coin&lt;T&gt;, split_amounts: <a href="">vector</a>&lt;u64&gt;, ctx: &<b>mut</b> TxContext) {
    <b>let</b> i = 0;
    <b>let</b> len = <a href="_length">vector::length</a>(&split_amounts);
    <b>while</b> (i &lt; len) {
        <a href="wallet.md#0x2_wallet_split">split</a>(self, *<a href="_borrow">vector::borrow</a>(&split_amounts, i), ctx);
        i = i + 1;
    };
}
</code></pre>



</details>

<a name="0x2_wallet_split_n_to_vec"></a>

## Function `split_n_to_vec`

Split coin <code>self</code> into <code>n</code> coins with equal balances. If the balance is
not evenly divisible by <code>n</code>, the remainder is left in <code>self</code>. Return
newly created coins.


<pre><code><b>public</b> <b>fun</b> <a href="wallet.md#0x2_wallet_split_n_to_vec">split_n_to_vec</a>&lt;T&gt;(self: &<b>mut</b> <a href="coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;, n: u64, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="">vector</a>&lt;<a href="coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="wallet.md#0x2_wallet_split_n_to_vec">split_n_to_vec</a>&lt;T&gt;(self: &<b>mut</b> Coin&lt;T&gt;, n: u64, ctx: &<b>mut</b> TxContext): <a href="">vector</a>&lt;Coin&lt;T&gt;&gt; {
    <b>assert</b>!(n &gt; 0, <a href="wallet.md#0x2_wallet_EInvalidArg">EInvalidArg</a>);
    <b>assert</b>!(n &lt;= <a href="coin.md#0x2_coin_value">coin::value</a>(self), <a href="wallet.md#0x2_wallet_ENotEnough">ENotEnough</a>);
    <b>let</b> vec = <a href="_empty">vector::empty</a>&lt;Coin&lt;T&gt;&gt;();
    <b>let</b> i = 0;
    <b>let</b> split_amount = <a href="coin.md#0x2_coin_value">coin::value</a>(self) / n;
    <b>while</b> (i &lt; n - 1) {
        <a href="_push_back">vector::push_back</a>(&<b>mut</b> vec, <a href="coin.md#0x2_coin_split">coin::split</a>(self, split_amount, ctx));
        i = i + 1;
    };
    vec
}
</code></pre>



</details>

<a name="0x2_wallet_split_n"></a>

## Function `split_n`

Split coin <code>self</code> into <code>n</code> coins with equal balances. If the balance is
not evenly divisible by <code>n</code>, the remainder is left in <code>self</code>.


<pre><code><b>public</b> <b>fun</b> <a href="wallet.md#0x2_wallet_split_n">split_n</a>&lt;T&gt;(self: &<b>mut</b> <a href="coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;, n: u64, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="wallet.md#0x2_wallet_split_n">split_n</a>&lt;T&gt;(self: &<b>mut</b> Coin&lt;T&gt;, n: u64, ctx: &<b>mut</b> TxContext) {
    <b>let</b> vec: <a href="">vector</a>&lt;Coin&lt;T&gt;&gt; = <a href="wallet.md#0x2_wallet_split_n_to_vec">split_n_to_vec</a>(self, n, ctx);
    <b>let</b> i = 0;
    <b>let</b> len = <a href="_length">vector::length</a>(&vec);
    <b>while</b> (i &lt; len) {
        <a href="transfer.md#0x2_transfer_transfer">transfer::transfer</a>(<a href="_pop_back">vector::pop_back</a>(&<b>mut</b> vec), <a href="tx_context.md#0x2_tx_context_sender">tx_context::sender</a>(ctx));
        i = i + 1;
    };
    <a href="_destroy_empty">vector::destroy_empty</a>(vec);
}
</code></pre>



</details>

<a name="0x2_wallet_join_vec"></a>

## Function `join_vec`

Join everything in <code>coins</code> with <code>self</code>


<pre><code><b>public</b> <b>fun</b> <a href="wallet.md#0x2_wallet_join_vec">join_vec</a>&lt;T&gt;(self: &<b>mut</b> <a href="coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;, coins: <a href="">vector</a>&lt;<a href="coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="wallet.md#0x2_wallet_join_vec">join_vec</a>&lt;T&gt;(self: &<b>mut</b> Coin&lt;T&gt;, coins: <a href="">vector</a>&lt;Coin&lt;T&gt;&gt;) {
    <b>let</b> i = 0;
    <b>let</b> len = <a href="_length">vector::length</a>(&coins);
    <b>while</b> (i &lt; len) {
        <b>let</b> <a href="coin.md#0x2_coin">coin</a> = <a href="_remove">vector::remove</a>(&<b>mut</b> coins, i);
        <a href="coin.md#0x2_coin_join">coin::join</a>(self, <a href="coin.md#0x2_coin">coin</a>);
        i = i + 1
    };
    // safe because we've drained the <a href="">vector</a>
    <a href="_destroy_empty">vector::destroy_empty</a>(coins)
}
</code></pre>



</details>
