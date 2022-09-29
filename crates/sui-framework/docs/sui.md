
<a name="0x2_sui"></a>

# Module `0x2::sui`

Coin<SUI> is the token used to pay for gas in Sui.
It has 9 decimals, and the smallest unit (10^-9) is called "mist".


-  [Struct `SUI`](#0x2_sui_SUI)
-  [Function `new`](#0x2_sui_new)
-  [Function `transfer`](#0x2_sui_transfer)


<pre><code><b>use</b> <a href="balance.md#0x2_balance">0x2::balance</a>;
<b>use</b> <a href="coin.md#0x2_coin">0x2::coin</a>;
<b>use</b> <a href="transfer.md#0x2_transfer">0x2::transfer</a>;
<b>use</b> <a href="tx_context.md#0x2_tx_context">0x2::tx_context</a>;
</code></pre>



<a name="0x2_sui_SUI"></a>

## Struct `SUI`

Name of the coin


<pre><code><b>struct</b> <a href="sui.md#0x2_sui_SUI">SUI</a> <b>has</b> drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>dummy_field: bool</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x2_sui_new"></a>

## Function `new`

Register the <code><a href="sui.md#0x2_sui_SUI">SUI</a></code> Coin to acquire its <code>Supply</code>.
This should be called only once during genesis creation.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui.md#0x2_sui_new">new</a>(ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="balance.md#0x2_balance_Supply">balance::Supply</a>&lt;<a href="sui.md#0x2_sui_SUI">sui::SUI</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui.md#0x2_sui_new">new</a>(ctx: &<b>mut</b> TxContext): Supply&lt;<a href="sui.md#0x2_sui_SUI">SUI</a>&gt; {
    <a href="coin.md#0x2_coin_treasury_into_supply">coin::treasury_into_supply</a>(
        <a href="coin.md#0x2_coin_create_currency">coin::create_currency</a>(<a href="sui.md#0x2_sui_SUI">SUI</a> {}, 9, ctx)
    )
}
</code></pre>



</details>

<a name="0x2_sui_transfer"></a>

## Function `transfer`



<pre><code><b>public</b> <b>fun</b> <a href="transfer.md#0x2_transfer">transfer</a>(c: <a href="coin.md#0x2_coin_Coin">coin::Coin</a>&lt;<a href="sui.md#0x2_sui_SUI">sui::SUI</a>&gt;, recipient: <b>address</b>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="transfer.md#0x2_transfer">transfer</a>(c: <a href="coin.md#0x2_coin_Coin">coin::Coin</a>&lt;<a href="sui.md#0x2_sui_SUI">SUI</a>&gt;, recipient: <b>address</b>) {
    <a href="transfer.md#0x2_transfer_transfer">transfer::transfer</a>(c, recipient)
}
</code></pre>



</details>
