---
title: Module `sui::sui`
---

Coin<SUI> is the token used to pay for gas in Sui.
It has 9 decimals, and the smallest unit (10^-9) is called "mist".


-  [Struct `SUI`](#sui_sui_SUI)
-  [Constants](#@Constants_0)
-  [Function `new`](#sui_sui_new)
-  [Function `transfer`](#sui_sui_transfer)


<pre><code><b>use</b> <a href="../std/address.md#std_address">std::address</a>;
<b>use</b> <a href="../std/ascii.md#std_ascii">std::ascii</a>;
<b>use</b> <a href="../std/bcs.md#std_bcs">std::bcs</a>;
<b>use</b> <a href="../std/option.md#std_option">std::option</a>;
<b>use</b> <a href="../std/string.md#std_string">std::string</a>;
<b>use</b> <a href="../std/type_name.md#std_type_name">std::type_name</a>;
<b>use</b> <a href="../std/vector.md#std_vector">std::vector</a>;
<b>use</b> <a href="../sui/address.md#sui_address">sui::address</a>;
<b>use</b> <a href="../sui/bag.md#sui_bag">sui::bag</a>;
<b>use</b> <a href="../sui/balance.md#sui_balance">sui::balance</a>;
<b>use</b> <a href="../sui/coin.md#sui_coin">sui::coin</a>;
<b>use</b> <a href="../sui/config.md#sui_config">sui::config</a>;
<b>use</b> <a href="../sui/deny_list.md#sui_deny_list">sui::deny_list</a>;
<b>use</b> <a href="../sui/dynamic_field.md#sui_dynamic_field">sui::dynamic_field</a>;
<b>use</b> <a href="../sui/dynamic_object_field.md#sui_dynamic_object_field">sui::dynamic_object_field</a>;
<b>use</b> <a href="../sui/event.md#sui_event">sui::event</a>;
<b>use</b> <a href="../sui/hex.md#sui_hex">sui::hex</a>;
<b>use</b> <a href="../sui/object.md#sui_object">sui::object</a>;
<b>use</b> <a href="../sui/table.md#sui_table">sui::table</a>;
<b>use</b> <a href="../sui/transfer.md#sui_transfer">sui::transfer</a>;
<b>use</b> <a href="../sui/tx_context.md#sui_tx_context">sui::tx_context</a>;
<b>use</b> <a href="../sui/types.md#sui_types">sui::types</a>;
<b>use</b> <a href="../sui/url.md#sui_url">sui::url</a>;
<b>use</b> <a href="../sui/vec_set.md#sui_vec_set">sui::vec_set</a>;
</code></pre>



<a name="sui_sui_SUI"></a>

## Struct `SUI`

Name of the coin


<pre><code><b>public</b> <b>struct</b> <a href="../sui/sui.md#sui_sui_SUI">SUI</a> <b>has</b> drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="sui_sui_EAlreadyMinted"></a>



<pre><code><b>const</b> <a href="../sui/sui.md#sui_sui_EAlreadyMinted">EAlreadyMinted</a>: u64 = 0;
</code></pre>



<a name="sui_sui_ENotSystemAddress"></a>

Sender is not @0x0 the system address.


<pre><code><b>const</b> <a href="../sui/sui.md#sui_sui_ENotSystemAddress">ENotSystemAddress</a>: u64 = 1;
</code></pre>



<a name="sui_sui_MIST_PER_SUI"></a>

The amount of Mist per Sui token based on the fact that mist is
10^-9 of a Sui token


<pre><code><b>const</b> <a href="../sui/sui.md#sui_sui_MIST_PER_SUI">MIST_PER_SUI</a>: u64 = 1000000000;
</code></pre>



<a name="sui_sui_TOTAL_SUPPLY_MIST"></a>

The total supply of Sui denominated in Mist (10 Billion * 10^9)


<pre><code><b>const</b> <a href="../sui/sui.md#sui_sui_TOTAL_SUPPLY_MIST">TOTAL_SUPPLY_MIST</a>: u64 = 10000000000000000000;
</code></pre>



<a name="sui_sui_TOTAL_SUPPLY_SUI"></a>

The total supply of Sui denominated in whole Sui tokens (10 Billion)


<pre><code><b>const</b> <a href="../sui/sui.md#sui_sui_TOTAL_SUPPLY_SUI">TOTAL_SUPPLY_SUI</a>: u64 = 10000000000;
</code></pre>



<a name="sui_sui_new"></a>

## Function `new`

Register the <code><a href="../sui/sui.md#sui_sui_SUI">SUI</a></code> Coin to acquire its <code>Supply</code>.
This should be called only once during genesis creation.


<pre><code><b>fun</b> <a href="../sui/sui.md#sui_sui_new">new</a>(ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;<a href="../sui/sui.md#sui_sui_SUI">sui::sui::SUI</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/sui.md#sui_sui_new">new</a>(ctx: &<b>mut</b> TxContext): Balance&lt;<a href="../sui/sui.md#sui_sui_SUI">SUI</a>&gt; {
    <b>assert</b>!(ctx.sender() == @0x0, <a href="../sui/sui.md#sui_sui_ENotSystemAddress">ENotSystemAddress</a>);
    <b>assert</b>!(ctx.epoch() == 0, <a href="../sui/sui.md#sui_sui_EAlreadyMinted">EAlreadyMinted</a>);
    <b>let</b> (treasury, metadata) = <a href="../sui/coin.md#sui_coin_create_currency">coin::create_currency</a>(
        <a href="../sui/sui.md#sui_sui_SUI">SUI</a> {},
        9,
        b"<a href="../sui/sui.md#sui_sui_SUI">SUI</a>",
        b"Sui",
        // TODO: add appropriate description and logo <a href="../sui/url.md#sui_url">url</a>
        b"",
        option::none(),
        ctx,
    );
    <a href="../sui/transfer.md#sui_transfer_public_freeze_object">transfer::public_freeze_object</a>(metadata);
    <b>let</b> <b>mut</b> supply = treasury.treasury_into_supply();
    <b>let</b> total_sui = supply.increase_supply(<a href="../sui/sui.md#sui_sui_TOTAL_SUPPLY_MIST">TOTAL_SUPPLY_MIST</a>);
    supply.destroy_supply();
    total_sui
}
</code></pre>



</details>

<a name="sui_sui_transfer"></a>

## Function `transfer`



<pre><code><b>public</b> <b>entry</b> <b>fun</b> <a href="../sui/transfer.md#sui_transfer">transfer</a>(c: <a href="../sui/coin.md#sui_coin_Coin">sui::coin::Coin</a>&lt;<a href="../sui/sui.md#sui_sui_SUI">sui::sui::SUI</a>&gt;, recipient: <b>address</b>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>entry</b> <b>fun</b> <a href="../sui/transfer.md#sui_transfer">transfer</a>(c: <a href="../sui/coin.md#sui_coin_Coin">coin::Coin</a>&lt;<a href="../sui/sui.md#sui_sui_SUI">SUI</a>&gt;, recipient: <b>address</b>) {
    <a href="../sui/transfer.md#sui_transfer_public_transfer">transfer::public_transfer</a>(c, recipient)
}
</code></pre>



</details>
