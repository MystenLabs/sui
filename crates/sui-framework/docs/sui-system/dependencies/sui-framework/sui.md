
<a name="0x2_sui"></a>

# Module `0x2::sui`



-  [Struct `SUI`](#0x2_sui_SUI)
-  [Constants](#@Constants_0)
-  [Function `new`](#0x2_sui_new)
-  [Function `transfer`](#0x2_sui_transfer)


<pre><code><b>use</b> <a href="../../dependencies/move-stdlib/option.md#0x1_option">0x1::option</a>;
<b>use</b> <a href="../../dependencies/sui-framework/balance.md#0x2_balance">0x2::balance</a>;
<b>use</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin">0x2::coin</a>;
<b>use</b> <a href="../../dependencies/sui-framework/transfer.md#0x2_transfer">0x2::transfer</a>;
<b>use</b> <a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context">0x2::tx_context</a>;
<b>use</b> <a href="../../dependencies/sui-framework/url.md#0x2_url">0x2::url</a>;
</code></pre>



<a name="0x2_sui_SUI"></a>

## Struct `SUI`



<pre><code><b>struct</b> <a href="../../dependencies/sui-framework/sui.md#0x2_sui_SUI">SUI</a> <b>has</b> drop
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

<a name="@Constants_0"></a>

## Constants


<a name="0x2_sui_ENotSystemAddress"></a>



<pre><code><b>const</b> <a href="../../dependencies/sui-framework/sui.md#0x2_sui_ENotSystemAddress">ENotSystemAddress</a>: u64 = 1;
</code></pre>



<a name="0x2_sui_EAlreadyMinted"></a>



<pre><code><b>const</b> <a href="../../dependencies/sui-framework/sui.md#0x2_sui_EAlreadyMinted">EAlreadyMinted</a>: u64 = 0;
</code></pre>



<a name="0x2_sui_MIST_PER_SUI"></a>



<pre><code><b>const</b> <a href="../../dependencies/sui-framework/sui.md#0x2_sui_MIST_PER_SUI">MIST_PER_SUI</a>: u64 = 1000000000;
</code></pre>



<a name="0x2_sui_TOTAL_SUPPLY_MIST"></a>



<pre><code><b>const</b> <a href="../../dependencies/sui-framework/sui.md#0x2_sui_TOTAL_SUPPLY_MIST">TOTAL_SUPPLY_MIST</a>: u64 = 10000000000000000000;
</code></pre>



<a name="0x2_sui_TOTAL_SUPPLY_SUI"></a>



<pre><code><b>const</b> <a href="../../dependencies/sui-framework/sui.md#0x2_sui_TOTAL_SUPPLY_SUI">TOTAL_SUPPLY_SUI</a>: u64 = 10000000000;
</code></pre>



<a name="0x2_sui_new"></a>

## Function `new`



<pre><code><b>fun</b> <a href="../../dependencies/sui-framework/sui.md#0x2_sui_new">new</a>(ctx: &<b>mut</b> <a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="../../dependencies/sui-framework/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="../../dependencies/sui-framework/sui.md#0x2_sui_SUI">sui::SUI</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../../dependencies/sui-framework/sui.md#0x2_sui_new">new</a>(ctx: &<b>mut</b> TxContext): Balance&lt;<a href="../../dependencies/sui-framework/sui.md#0x2_sui_SUI">SUI</a>&gt; {
    <b>assert</b>!(<a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_sender">tx_context::sender</a>(ctx) == @0x0, <a href="../../dependencies/sui-framework/sui.md#0x2_sui_ENotSystemAddress">ENotSystemAddress</a>);
    <b>assert</b>!(<a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_epoch">tx_context::epoch</a>(ctx) == 0, <a href="../../dependencies/sui-framework/sui.md#0x2_sui_EAlreadyMinted">EAlreadyMinted</a>);

    <b>let</b> (treasury, metadata) = <a href="../../dependencies/sui-framework/coin.md#0x2_coin_create_currency">coin::create_currency</a>(
        <a href="../../dependencies/sui-framework/sui.md#0x2_sui_SUI">SUI</a> {},
        9,
        b"<a href="../../dependencies/sui-framework/sui.md#0x2_sui_SUI">SUI</a>",
        b"Sui",
        // TODO: add appropriate description and logo <a href="../../dependencies/sui-framework/url.md#0x2_url">url</a>
        b"",
        <a href="../../dependencies/move-stdlib/option.md#0x1_option_none">option::none</a>(),
        ctx
    );
    <a href="../../dependencies/sui-framework/transfer.md#0x2_transfer_public_freeze_object">transfer::public_freeze_object</a>(metadata);
    <b>let</b> supply = <a href="../../dependencies/sui-framework/coin.md#0x2_coin_treasury_into_supply">coin::treasury_into_supply</a>(treasury);
    <b>let</b> total_sui = <a href="../../dependencies/sui-framework/balance.md#0x2_balance_increase_supply">balance::increase_supply</a>(&<b>mut</b> supply, <a href="../../dependencies/sui-framework/sui.md#0x2_sui_TOTAL_SUPPLY_MIST">TOTAL_SUPPLY_MIST</a>);
    <a href="../../dependencies/sui-framework/balance.md#0x2_balance_destroy_supply">balance::destroy_supply</a>(supply);
    total_sui
}
</code></pre>



</details>

<a name="0x2_sui_transfer"></a>

## Function `transfer`



<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-framework/transfer.md#0x2_transfer">transfer</a>(c: <a href="../../dependencies/sui-framework/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;<a href="../../dependencies/sui-framework/sui.md#0x2_sui_SUI">sui::SUI</a>&gt;, recipient: <b>address</b>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-framework/transfer.md#0x2_transfer">transfer</a>(c: <a href="../../dependencies/sui-framework/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;<a href="../../dependencies/sui-framework/sui.md#0x2_sui_SUI">SUI</a>&gt;, recipient: <b>address</b>) {
    <a href="../../dependencies/sui-framework/transfer.md#0x2_transfer_public_transfer">transfer::public_transfer</a>(c, recipient)
}
</code></pre>



</details>
