
<a name="0x2_pay"></a>

# Module `0x2::pay`

This module provides handy functionality for wallets and <code>sui::Coin</code> management.


-  [Constants](#@Constants_0)
-  [Function `join_vec`](#0x2_pay_join_vec)
-  [Function `join_vec_and_transfer`](#0x2_pay_join_vec_and_transfer)


<pre><code><b>use</b> <a href="coin.md#0x2_coin">0x2::coin</a>;
<b>use</b> <a href="transfer.md#0x2_transfer">0x2::transfer</a>;
</code></pre>



<a name="@Constants_0"></a>

## Constants


<a name="0x2_pay_ENoCoins"></a>

For when empty vector is supplied into join function.


<pre><code><b>const</b> <a href="pay.md#0x2_pay_ENoCoins">ENoCoins</a>: u64 = 0;
</code></pre>



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
    // safe because we've drained the <a href="">vector</a>
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
    <a href="transfer.md#0x2_transfer_public_transfer">transfer::public_transfer</a>(self, receiver)
}
</code></pre>



</details>
