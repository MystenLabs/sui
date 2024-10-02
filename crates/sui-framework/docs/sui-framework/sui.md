---
title: Module `0x2::sui`
---

Coin<SUI> is the token used to pay for gas in Sui.
It has 9 decimals, and the smallest unit (10^-9) is called "mist".


-  [Struct `SUI`](#0x2_sui_SUI)
-  [Constants](#@Constants_0)
-  [Function `new`](#0x2_sui_new)
-  [Function `transfer`](#0x2_sui_transfer)


<pre><code><b>use</b> <a href="../move-stdlib/option.md#0x1_option">0x1::option</a>;
<b>use</b> <a href="../sui-framework/balance.md#0x2_balance">0x2::balance</a>;
<b>use</b> <a href="../sui-framework/coin.md#0x2_coin">0x2::coin</a>;
<b>use</b> <a href="../sui-framework/transfer.md#0x2_transfer">0x2::transfer</a>;
<b>use</b> <a href="../sui-framework/tx_context.md#0x2_tx_context">0x2::tx_context</a>;
<b>use</b> <a href="../sui-framework/url.md#0x2_url">0x2::url</a>;
</code></pre>



<a name="0x2_sui_SUI"></a>

## Struct `SUI`

Name of the coin


<pre><code><b>struct</b> <a href="../sui-framework/sui.md#0x2_sui_SUI">SUI</a> <b>has</b> drop
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

Sender is not @0x0 the system address.


<pre><code><b>const</b> <a href="../sui-framework/sui.md#0x2_sui_ENotSystemAddress">ENotSystemAddress</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 1;
</code></pre>



<a name="0x2_sui_EAlreadyMinted"></a>



<pre><code><b>const</b> <a href="../sui-framework/sui.md#0x2_sui_EAlreadyMinted">EAlreadyMinted</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 0;
</code></pre>



<a name="0x2_sui_MIST_PER_SUI"></a>

The amount of Mist per Sui token based on the fact that mist is
10^-9 of a Sui token


<pre><code><b>const</b> <a href="../sui-framework/sui.md#0x2_sui_MIST_PER_SUI">MIST_PER_SUI</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 1000000000;
</code></pre>



<a name="0x2_sui_TOTAL_SUPPLY_MIST"></a>

The total supply of Sui denominated in Mist (10 Billion * 10^9)


<pre><code><b>const</b> <a href="../sui-framework/sui.md#0x2_sui_TOTAL_SUPPLY_MIST">TOTAL_SUPPLY_MIST</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 10000000000000000000;
</code></pre>



<a name="0x2_sui_TOTAL_SUPPLY_SUI"></a>

The total supply of Sui denominated in whole Sui tokens (10 Billion)


<pre><code><b>const</b> <a href="../sui-framework/sui.md#0x2_sui_TOTAL_SUPPLY_SUI">TOTAL_SUPPLY_SUI</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 10000000000;
</code></pre>



<a name="0x2_sui_new"></a>

## Function `new`

Register the <code><a href="../sui-framework/sui.md#0x2_sui_SUI">SUI</a></code> Coin to acquire its <code>Supply</code>.
This should be called only once during genesis creation.


<pre><code><b>fun</b> <a href="../sui-framework/sui.md#0x2_sui_new">new</a>(ctx: &<b>mut</b> <a href="../sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="../sui-framework/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="../sui-framework/sui.md#0x2_sui_SUI">sui::SUI</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui-framework/sui.md#0x2_sui_new">new</a>(ctx: &<b>mut</b> TxContext): Balance&lt;<a href="../sui-framework/sui.md#0x2_sui_SUI">SUI</a>&gt; {
    <b>assert</b>!(ctx.sender() == @0x0, <a href="../sui-framework/sui.md#0x2_sui_ENotSystemAddress">ENotSystemAddress</a>);
    <b>assert</b>!(ctx.epoch() == 0, <a href="../sui-framework/sui.md#0x2_sui_EAlreadyMinted">EAlreadyMinted</a>);

    <b>let</b> (treasury, metadata) = <a href="../sui-framework/coin.md#0x2_coin_create_currency">coin::create_currency</a>(
        <a href="../sui-framework/sui.md#0x2_sui_SUI">SUI</a> {},
        9,
        b"<a href="../sui-framework/sui.md#0x2_sui_SUI">SUI</a>",
        b"Sui",
        // TODO: add appropriate description and logo <a href="../sui-framework/url.md#0x2_url">url</a>
        b"",
        <a href="../move-stdlib/option.md#0x1_option_none">option::none</a>(),
        ctx,
    );
    <a href="../sui-framework/transfer.md#0x2_transfer_public_freeze_object">transfer::public_freeze_object</a>(metadata);
    <b>let</b> <b>mut</b> supply = treasury.treasury_into_supply();
    <b>let</b> total_sui = supply.increase_supply(<a href="../sui-framework/sui.md#0x2_sui_TOTAL_SUPPLY_MIST">TOTAL_SUPPLY_MIST</a>);
    supply.destroy_supply();
    total_sui
}
</code></pre>



</details>

<a name="0x2_sui_transfer"></a>

## Function `transfer`



<pre><code><b>public</b> entry <b>fun</b> <a href="../sui-framework/transfer.md#0x2_transfer">transfer</a>(c: <a href="../sui-framework/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;<a href="../sui-framework/sui.md#0x2_sui_SUI">sui::SUI</a>&gt;, recipient: <b>address</b>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="../sui-framework/transfer.md#0x2_transfer">transfer</a>(c: <a href="../sui-framework/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;<a href="../sui-framework/sui.md#0x2_sui_SUI">SUI</a>&gt;, recipient: <b>address</b>) {
    <a href="../sui-framework/transfer.md#0x2_transfer_public_transfer">transfer::public_transfer</a>(c, recipient)
}
</code></pre>



</details>
