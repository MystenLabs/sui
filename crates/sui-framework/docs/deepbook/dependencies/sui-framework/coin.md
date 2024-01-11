
<a name="0x2_coin"></a>

# Module `0x2::coin`



-  [Resource `Coin`](#0x2_coin_Coin)
-  [Resource `CoinMetadata`](#0x2_coin_CoinMetadata)
-  [Resource `TreasuryCap`](#0x2_coin_TreasuryCap)
-  [Struct `CurrencyCreated`](#0x2_coin_CurrencyCreated)
-  [Constants](#@Constants_0)
-  [Function `total_supply`](#0x2_coin_total_supply)
-  [Function `treasury_into_supply`](#0x2_coin_treasury_into_supply)
-  [Function `supply_immut`](#0x2_coin_supply_immut)
-  [Function `supply_mut`](#0x2_coin_supply_mut)
-  [Function `value`](#0x2_coin_value)
-  [Function `balance`](#0x2_coin_balance)
-  [Function `balance_mut`](#0x2_coin_balance_mut)
-  [Function `from_balance`](#0x2_coin_from_balance)
-  [Function `into_balance`](#0x2_coin_into_balance)
-  [Function `take`](#0x2_coin_take)
-  [Function `put`](#0x2_coin_put)
-  [Function `join`](#0x2_coin_join)
-  [Function `split`](#0x2_coin_split)
-  [Function `divide_into_n`](#0x2_coin_divide_into_n)
-  [Function `zero`](#0x2_coin_zero)
-  [Function `destroy_zero`](#0x2_coin_destroy_zero)
-  [Function `create_currency`](#0x2_coin_create_currency)
-  [Function `mint`](#0x2_coin_mint)
-  [Function `mint_balance`](#0x2_coin_mint_balance)
-  [Function `burn`](#0x2_coin_burn)
-  [Function `mint_and_transfer`](#0x2_coin_mint_and_transfer)
-  [Function `update_name`](#0x2_coin_update_name)
-  [Function `update_symbol`](#0x2_coin_update_symbol)
-  [Function `update_description`](#0x2_coin_update_description)
-  [Function `update_icon_url`](#0x2_coin_update_icon_url)
-  [Function `get_decimals`](#0x2_coin_get_decimals)
-  [Function `get_name`](#0x2_coin_get_name)
-  [Function `get_symbol`](#0x2_coin_get_symbol)
-  [Function `get_description`](#0x2_coin_get_description)
-  [Function `get_icon_url`](#0x2_coin_get_icon_url)
-  [Function `supply`](#0x2_coin_supply)


<pre><code><b>use</b> <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii">0x1::ascii</a>;
<b>use</b> <a href="../../dependencies/move-stdlib/option.md#0x1_option">0x1::option</a>;
<b>use</b> <a href="../../dependencies/move-stdlib/string.md#0x1_string">0x1::string</a>;
<b>use</b> <a href="../../dependencies/sui-framework/balance.md#0x2_balance">0x2::balance</a>;
<b>use</b> <a href="../../dependencies/sui-framework/object.md#0x2_object">0x2::object</a>;
<b>use</b> <a href="../../dependencies/sui-framework/transfer.md#0x2_transfer">0x2::transfer</a>;
<b>use</b> <a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context">0x2::tx_context</a>;
<b>use</b> <a href="../../dependencies/sui-framework/types.md#0x2_types">0x2::types</a>;
<b>use</b> <a href="../../dependencies/sui-framework/url.md#0x2_url">0x2::url</a>;
</code></pre>



<a name="0x2_coin_Coin"></a>

## Resource `Coin`



<pre><code><b>struct</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_Coin">Coin</a>&lt;T&gt; <b>has</b> store, key
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>id: <a href="../../dependencies/sui-framework/object.md#0x2_object_UID">object::UID</a></code>
</dt>
<dd>

</dd>
<dt>
<code><a href="../../dependencies/sui-framework/balance.md#0x2_balance">balance</a>: <a href="../../dependencies/sui-framework/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;T&gt;</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x2_coin_CoinMetadata"></a>

## Resource `CoinMetadata`



<pre><code><b>struct</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_CoinMetadata">CoinMetadata</a>&lt;T&gt; <b>has</b> store, key
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>id: <a href="../../dependencies/sui-framework/object.md#0x2_object_UID">object::UID</a></code>
</dt>
<dd>

</dd>
<dt>
<code>decimals: u8</code>
</dt>
<dd>

</dd>
<dt>
<code>name: <a href="../../dependencies/move-stdlib/string.md#0x1_string_String">string::String</a></code>
</dt>
<dd>

</dd>
<dt>
<code>symbol: <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_String">ascii::String</a></code>
</dt>
<dd>

</dd>
<dt>
<code>description: <a href="../../dependencies/move-stdlib/string.md#0x1_string_String">string::String</a></code>
</dt>
<dd>

</dd>
<dt>
<code>icon_url: <a href="../../dependencies/move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;<a href="../../dependencies/sui-framework/url.md#0x2_url_Url">url::Url</a>&gt;</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x2_coin_TreasuryCap"></a>

## Resource `TreasuryCap`



<pre><code><b>struct</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_TreasuryCap">TreasuryCap</a>&lt;T&gt; <b>has</b> store, key
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>id: <a href="../../dependencies/sui-framework/object.md#0x2_object_UID">object::UID</a></code>
</dt>
<dd>

</dd>
<dt>
<code>total_supply: <a href="../../dependencies/sui-framework/balance.md#0x2_balance_Supply">balance::Supply</a>&lt;T&gt;</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x2_coin_CurrencyCreated"></a>

## Struct `CurrencyCreated`



<pre><code><b>struct</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_CurrencyCreated">CurrencyCreated</a>&lt;T&gt; <b>has</b> <b>copy</b>, drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>decimals: u8</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0x2_coin_ENotEnough"></a>



<pre><code><b>const</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_ENotEnough">ENotEnough</a>: u64 = 2;
</code></pre>



<a name="0x2_coin_EBadWitness"></a>



<pre><code><b>const</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_EBadWitness">EBadWitness</a>: u64 = 0;
</code></pre>



<a name="0x2_coin_EInvalidArg"></a>



<pre><code><b>const</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_EInvalidArg">EInvalidArg</a>: u64 = 1;
</code></pre>



<a name="0x2_coin_total_supply"></a>

## Function `total_supply`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_total_supply">total_supply</a>&lt;T&gt;(cap: &<a href="../../dependencies/sui-framework/coin.md#0x2_coin_TreasuryCap">coin::TreasuryCap</a>&lt;T&gt;): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_total_supply">total_supply</a>&lt;T&gt;(cap: &<a href="../../dependencies/sui-framework/coin.md#0x2_coin_TreasuryCap">TreasuryCap</a>&lt;T&gt;): u64 {
    <a href="../../dependencies/sui-framework/balance.md#0x2_balance_supply_value">balance::supply_value</a>(&cap.total_supply)
}
</code></pre>



</details>

<a name="0x2_coin_treasury_into_supply"></a>

## Function `treasury_into_supply`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_treasury_into_supply">treasury_into_supply</a>&lt;T&gt;(treasury: <a href="../../dependencies/sui-framework/coin.md#0x2_coin_TreasuryCap">coin::TreasuryCap</a>&lt;T&gt;): <a href="../../dependencies/sui-framework/balance.md#0x2_balance_Supply">balance::Supply</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_treasury_into_supply">treasury_into_supply</a>&lt;T&gt;(treasury: <a href="../../dependencies/sui-framework/coin.md#0x2_coin_TreasuryCap">TreasuryCap</a>&lt;T&gt;): Supply&lt;T&gt; {
    <b>let</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_TreasuryCap">TreasuryCap</a> { id, total_supply } = treasury;
    <a href="../../dependencies/sui-framework/object.md#0x2_object_delete">object::delete</a>(id);
    total_supply
}
</code></pre>



</details>

<a name="0x2_coin_supply_immut"></a>

## Function `supply_immut`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_supply_immut">supply_immut</a>&lt;T&gt;(treasury: &<a href="../../dependencies/sui-framework/coin.md#0x2_coin_TreasuryCap">coin::TreasuryCap</a>&lt;T&gt;): &<a href="../../dependencies/sui-framework/balance.md#0x2_balance_Supply">balance::Supply</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_supply_immut">supply_immut</a>&lt;T&gt;(treasury: &<a href="../../dependencies/sui-framework/coin.md#0x2_coin_TreasuryCap">TreasuryCap</a>&lt;T&gt;): &Supply&lt;T&gt; {
    &treasury.total_supply
}
</code></pre>



</details>

<a name="0x2_coin_supply_mut"></a>

## Function `supply_mut`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_supply_mut">supply_mut</a>&lt;T&gt;(treasury: &<b>mut</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_TreasuryCap">coin::TreasuryCap</a>&lt;T&gt;): &<b>mut</b> <a href="../../dependencies/sui-framework/balance.md#0x2_balance_Supply">balance::Supply</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_supply_mut">supply_mut</a>&lt;T&gt;(treasury: &<b>mut</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_TreasuryCap">TreasuryCap</a>&lt;T&gt;): &<b>mut</b> Supply&lt;T&gt; {
    &<b>mut</b> treasury.total_supply
}
</code></pre>



</details>

<a name="0x2_coin_value"></a>

## Function `value`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_value">value</a>&lt;T&gt;(self: &<a href="../../dependencies/sui-framework/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_value">value</a>&lt;T&gt;(self: &<a href="../../dependencies/sui-framework/coin.md#0x2_coin_Coin">Coin</a>&lt;T&gt;): u64 {
    <a href="../../dependencies/sui-framework/balance.md#0x2_balance_value">balance::value</a>(&self.<a href="../../dependencies/sui-framework/balance.md#0x2_balance">balance</a>)
}
</code></pre>



</details>

<a name="0x2_coin_balance"></a>

## Function `balance`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/balance.md#0x2_balance">balance</a>&lt;T&gt;(<a href="../../dependencies/sui-framework/coin.md#0x2_coin">coin</a>: &<a href="../../dependencies/sui-framework/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;): &<a href="../../dependencies/sui-framework/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/balance.md#0x2_balance">balance</a>&lt;T&gt;(<a href="../../dependencies/sui-framework/coin.md#0x2_coin">coin</a>: &<a href="../../dependencies/sui-framework/coin.md#0x2_coin_Coin">Coin</a>&lt;T&gt;): &Balance&lt;T&gt; {
    &<a href="../../dependencies/sui-framework/coin.md#0x2_coin">coin</a>.<a href="../../dependencies/sui-framework/balance.md#0x2_balance">balance</a>
}
</code></pre>



</details>

<a name="0x2_coin_balance_mut"></a>

## Function `balance_mut`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_balance_mut">balance_mut</a>&lt;T&gt;(<a href="../../dependencies/sui-framework/coin.md#0x2_coin">coin</a>: &<b>mut</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;): &<b>mut</b> <a href="../../dependencies/sui-framework/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_balance_mut">balance_mut</a>&lt;T&gt;(<a href="../../dependencies/sui-framework/coin.md#0x2_coin">coin</a>: &<b>mut</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_Coin">Coin</a>&lt;T&gt;): &<b>mut</b> Balance&lt;T&gt; {
    &<b>mut</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin">coin</a>.<a href="../../dependencies/sui-framework/balance.md#0x2_balance">balance</a>
}
</code></pre>



</details>

<a name="0x2_coin_from_balance"></a>

## Function `from_balance`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_from_balance">from_balance</a>&lt;T&gt;(<a href="../../dependencies/sui-framework/balance.md#0x2_balance">balance</a>: <a href="../../dependencies/sui-framework/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;T&gt;, ctx: &<b>mut</b> <a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="../../dependencies/sui-framework/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_from_balance">from_balance</a>&lt;T&gt;(<a href="../../dependencies/sui-framework/balance.md#0x2_balance">balance</a>: Balance&lt;T&gt;, ctx: &<b>mut</b> TxContext): <a href="../../dependencies/sui-framework/coin.md#0x2_coin_Coin">Coin</a>&lt;T&gt; {
    <a href="../../dependencies/sui-framework/coin.md#0x2_coin_Coin">Coin</a> { id: <a href="../../dependencies/sui-framework/object.md#0x2_object_new">object::new</a>(ctx), <a href="../../dependencies/sui-framework/balance.md#0x2_balance">balance</a> }
}
</code></pre>



</details>

<a name="0x2_coin_into_balance"></a>

## Function `into_balance`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_into_balance">into_balance</a>&lt;T&gt;(<a href="../../dependencies/sui-framework/coin.md#0x2_coin">coin</a>: <a href="../../dependencies/sui-framework/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;): <a href="../../dependencies/sui-framework/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_into_balance">into_balance</a>&lt;T&gt;(<a href="../../dependencies/sui-framework/coin.md#0x2_coin">coin</a>: <a href="../../dependencies/sui-framework/coin.md#0x2_coin_Coin">Coin</a>&lt;T&gt;): Balance&lt;T&gt; {
    <b>let</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_Coin">Coin</a> { id, <a href="../../dependencies/sui-framework/balance.md#0x2_balance">balance</a> } = <a href="../../dependencies/sui-framework/coin.md#0x2_coin">coin</a>;
    <a href="../../dependencies/sui-framework/object.md#0x2_object_delete">object::delete</a>(id);
    <a href="../../dependencies/sui-framework/balance.md#0x2_balance">balance</a>
}
</code></pre>



</details>

<a name="0x2_coin_take"></a>

## Function `take`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_take">take</a>&lt;T&gt;(<a href="../../dependencies/sui-framework/balance.md#0x2_balance">balance</a>: &<b>mut</b> <a href="../../dependencies/sui-framework/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;T&gt;, value: u64, ctx: &<b>mut</b> <a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="../../dependencies/sui-framework/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_take">take</a>&lt;T&gt;(
    <a href="../../dependencies/sui-framework/balance.md#0x2_balance">balance</a>: &<b>mut</b> Balance&lt;T&gt;, value: u64, ctx: &<b>mut</b> TxContext,
): <a href="../../dependencies/sui-framework/coin.md#0x2_coin_Coin">Coin</a>&lt;T&gt; {
    <a href="../../dependencies/sui-framework/coin.md#0x2_coin_Coin">Coin</a> {
        id: <a href="../../dependencies/sui-framework/object.md#0x2_object_new">object::new</a>(ctx),
        <a href="../../dependencies/sui-framework/balance.md#0x2_balance">balance</a>: <a href="../../dependencies/sui-framework/balance.md#0x2_balance_split">balance::split</a>(<a href="../../dependencies/sui-framework/balance.md#0x2_balance">balance</a>, value)
    }
}
</code></pre>



</details>

<a name="0x2_coin_put"></a>

## Function `put`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_put">put</a>&lt;T&gt;(<a href="../../dependencies/sui-framework/balance.md#0x2_balance">balance</a>: &<b>mut</b> <a href="../../dependencies/sui-framework/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;T&gt;, <a href="../../dependencies/sui-framework/coin.md#0x2_coin">coin</a>: <a href="../../dependencies/sui-framework/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_put">put</a>&lt;T&gt;(<a href="../../dependencies/sui-framework/balance.md#0x2_balance">balance</a>: &<b>mut</b> Balance&lt;T&gt;, <a href="../../dependencies/sui-framework/coin.md#0x2_coin">coin</a>: <a href="../../dependencies/sui-framework/coin.md#0x2_coin_Coin">Coin</a>&lt;T&gt;) {
    <a href="../../dependencies/sui-framework/balance.md#0x2_balance_join">balance::join</a>(<a href="../../dependencies/sui-framework/balance.md#0x2_balance">balance</a>, <a href="../../dependencies/sui-framework/coin.md#0x2_coin_into_balance">into_balance</a>(<a href="../../dependencies/sui-framework/coin.md#0x2_coin">coin</a>));
}
</code></pre>



</details>

<a name="0x2_coin_join"></a>

## Function `join`



<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_join">join</a>&lt;T&gt;(self: &<b>mut</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;, c: <a href="../../dependencies/sui-framework/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_join">join</a>&lt;T&gt;(self: &<b>mut</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_Coin">Coin</a>&lt;T&gt;, c: <a href="../../dependencies/sui-framework/coin.md#0x2_coin_Coin">Coin</a>&lt;T&gt;) {
    <b>let</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_Coin">Coin</a> { id, <a href="../../dependencies/sui-framework/balance.md#0x2_balance">balance</a> } = c;
    <a href="../../dependencies/sui-framework/object.md#0x2_object_delete">object::delete</a>(id);
    <a href="../../dependencies/sui-framework/balance.md#0x2_balance_join">balance::join</a>(&<b>mut</b> self.<a href="../../dependencies/sui-framework/balance.md#0x2_balance">balance</a>, <a href="../../dependencies/sui-framework/balance.md#0x2_balance">balance</a>);
}
</code></pre>



</details>

<a name="0x2_coin_split"></a>

## Function `split`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_split">split</a>&lt;T&gt;(self: &<b>mut</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;, split_amount: u64, ctx: &<b>mut</b> <a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="../../dependencies/sui-framework/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_split">split</a>&lt;T&gt;(
    self: &<b>mut</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_Coin">Coin</a>&lt;T&gt;, split_amount: u64, ctx: &<b>mut</b> TxContext
): <a href="../../dependencies/sui-framework/coin.md#0x2_coin_Coin">Coin</a>&lt;T&gt; {
    <a href="../../dependencies/sui-framework/coin.md#0x2_coin_take">take</a>(&<b>mut</b> self.<a href="../../dependencies/sui-framework/balance.md#0x2_balance">balance</a>, split_amount, ctx)
}
</code></pre>



</details>

<a name="0x2_coin_divide_into_n"></a>

## Function `divide_into_n`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_divide_into_n">divide_into_n</a>&lt;T&gt;(self: &<b>mut</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;, n: u64, ctx: &<b>mut</b> <a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="../../dependencies/sui-framework/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_divide_into_n">divide_into_n</a>&lt;T&gt;(
    self: &<b>mut</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_Coin">Coin</a>&lt;T&gt;, n: u64, ctx: &<b>mut</b> TxContext
): <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="../../dependencies/sui-framework/coin.md#0x2_coin_Coin">Coin</a>&lt;T&gt;&gt; {
    <b>assert</b>!(n &gt; 0, <a href="../../dependencies/sui-framework/coin.md#0x2_coin_EInvalidArg">EInvalidArg</a>);
    <b>assert</b>!(n &lt;= <a href="../../dependencies/sui-framework/coin.md#0x2_coin_value">value</a>(self), <a href="../../dependencies/sui-framework/coin.md#0x2_coin_ENotEnough">ENotEnough</a>);

    <b>let</b> vec = <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_empty">vector::empty</a>&lt;<a href="../../dependencies/sui-framework/coin.md#0x2_coin_Coin">Coin</a>&lt;T&gt;&gt;();
    <b>let</b> i = 0;
    <b>let</b> split_amount = <a href="../../dependencies/sui-framework/coin.md#0x2_coin_value">value</a>(self) / n;
    <b>while</b> (i &lt; n - 1) {
        <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_push_back">vector::push_back</a>(&<b>mut</b> vec, <a href="../../dependencies/sui-framework/coin.md#0x2_coin_split">split</a>(self, split_amount, ctx));
        i = i + 1;
    };
    vec
}
</code></pre>



</details>

<a name="0x2_coin_zero"></a>

## Function `zero`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_zero">zero</a>&lt;T&gt;(ctx: &<b>mut</b> <a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="../../dependencies/sui-framework/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_zero">zero</a>&lt;T&gt;(ctx: &<b>mut</b> TxContext): <a href="../../dependencies/sui-framework/coin.md#0x2_coin_Coin">Coin</a>&lt;T&gt; {
    <a href="../../dependencies/sui-framework/coin.md#0x2_coin_Coin">Coin</a> { id: <a href="../../dependencies/sui-framework/object.md#0x2_object_new">object::new</a>(ctx), <a href="../../dependencies/sui-framework/balance.md#0x2_balance">balance</a>: <a href="../../dependencies/sui-framework/balance.md#0x2_balance_zero">balance::zero</a>() }
}
</code></pre>



</details>

<a name="0x2_coin_destroy_zero"></a>

## Function `destroy_zero`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_destroy_zero">destroy_zero</a>&lt;T&gt;(c: <a href="../../dependencies/sui-framework/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_destroy_zero">destroy_zero</a>&lt;T&gt;(c: <a href="../../dependencies/sui-framework/coin.md#0x2_coin_Coin">Coin</a>&lt;T&gt;) {
    <b>let</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_Coin">Coin</a> { id, <a href="../../dependencies/sui-framework/balance.md#0x2_balance">balance</a> } = c;
    <a href="../../dependencies/sui-framework/object.md#0x2_object_delete">object::delete</a>(id);
    <a href="../../dependencies/sui-framework/balance.md#0x2_balance_destroy_zero">balance::destroy_zero</a>(<a href="../../dependencies/sui-framework/balance.md#0x2_balance">balance</a>)
}
</code></pre>



</details>

<a name="0x2_coin_create_currency"></a>

## Function `create_currency`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_create_currency">create_currency</a>&lt;T: drop&gt;(witness: T, decimals: u8, symbol: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, name: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, description: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, icon_url: <a href="../../dependencies/move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;<a href="../../dependencies/sui-framework/url.md#0x2_url_Url">url::Url</a>&gt;, ctx: &<b>mut</b> <a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): (<a href="../../dependencies/sui-framework/coin.md#0x2_coin_TreasuryCap">coin::TreasuryCap</a>&lt;T&gt;, <a href="../../dependencies/sui-framework/coin.md#0x2_coin_CoinMetadata">coin::CoinMetadata</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_create_currency">create_currency</a>&lt;T: drop&gt;(
    witness: T,
    decimals: u8,
    symbol: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    name: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    description: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    icon_url: Option&lt;Url&gt;,
    ctx: &<b>mut</b> TxContext
): (<a href="../../dependencies/sui-framework/coin.md#0x2_coin_TreasuryCap">TreasuryCap</a>&lt;T&gt;, <a href="../../dependencies/sui-framework/coin.md#0x2_coin_CoinMetadata">CoinMetadata</a>&lt;T&gt;) {
    // Make sure there's only one instance of the type T
    <b>assert</b>!(sui::types::is_one_time_witness(&witness), <a href="../../dependencies/sui-framework/coin.md#0x2_coin_EBadWitness">EBadWitness</a>);

    (
        <a href="../../dependencies/sui-framework/coin.md#0x2_coin_TreasuryCap">TreasuryCap</a> {
            id: <a href="../../dependencies/sui-framework/object.md#0x2_object_new">object::new</a>(ctx),
            total_supply: <a href="../../dependencies/sui-framework/balance.md#0x2_balance_create_supply">balance::create_supply</a>(witness)
        },
        <a href="../../dependencies/sui-framework/coin.md#0x2_coin_CoinMetadata">CoinMetadata</a> {
            id: <a href="../../dependencies/sui-framework/object.md#0x2_object_new">object::new</a>(ctx),
            decimals,
            name: <a href="../../dependencies/move-stdlib/string.md#0x1_string_utf8">string::utf8</a>(name),
            symbol: <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_string">ascii::string</a>(symbol),
            description: <a href="../../dependencies/move-stdlib/string.md#0x1_string_utf8">string::utf8</a>(description),
            icon_url
        }
    )
}
</code></pre>



</details>

<a name="0x2_coin_mint"></a>

## Function `mint`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_mint">mint</a>&lt;T&gt;(cap: &<b>mut</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_TreasuryCap">coin::TreasuryCap</a>&lt;T&gt;, value: u64, ctx: &<b>mut</b> <a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="../../dependencies/sui-framework/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_mint">mint</a>&lt;T&gt;(
    cap: &<b>mut</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_TreasuryCap">TreasuryCap</a>&lt;T&gt;, value: u64, ctx: &<b>mut</b> TxContext,
): <a href="../../dependencies/sui-framework/coin.md#0x2_coin_Coin">Coin</a>&lt;T&gt; {
    <a href="../../dependencies/sui-framework/coin.md#0x2_coin_Coin">Coin</a> {
        id: <a href="../../dependencies/sui-framework/object.md#0x2_object_new">object::new</a>(ctx),
        <a href="../../dependencies/sui-framework/balance.md#0x2_balance">balance</a>: <a href="../../dependencies/sui-framework/balance.md#0x2_balance_increase_supply">balance::increase_supply</a>(&<b>mut</b> cap.total_supply, value)
    }
}
</code></pre>



</details>

<a name="0x2_coin_mint_balance"></a>

## Function `mint_balance`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_mint_balance">mint_balance</a>&lt;T&gt;(cap: &<b>mut</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_TreasuryCap">coin::TreasuryCap</a>&lt;T&gt;, value: u64): <a href="../../dependencies/sui-framework/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_mint_balance">mint_balance</a>&lt;T&gt;(
    cap: &<b>mut</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_TreasuryCap">TreasuryCap</a>&lt;T&gt;, value: u64
): Balance&lt;T&gt; {
    <a href="../../dependencies/sui-framework/balance.md#0x2_balance_increase_supply">balance::increase_supply</a>(&<b>mut</b> cap.total_supply, value)
}
</code></pre>



</details>

<a name="0x2_coin_burn"></a>

## Function `burn`



<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_burn">burn</a>&lt;T&gt;(cap: &<b>mut</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_TreasuryCap">coin::TreasuryCap</a>&lt;T&gt;, c: <a href="../../dependencies/sui-framework/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_burn">burn</a>&lt;T&gt;(cap: &<b>mut</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_TreasuryCap">TreasuryCap</a>&lt;T&gt;, c: <a href="../../dependencies/sui-framework/coin.md#0x2_coin_Coin">Coin</a>&lt;T&gt;): u64 {
    <b>let</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_Coin">Coin</a> { id, <a href="../../dependencies/sui-framework/balance.md#0x2_balance">balance</a> } = c;
    <a href="../../dependencies/sui-framework/object.md#0x2_object_delete">object::delete</a>(id);
    <a href="../../dependencies/sui-framework/balance.md#0x2_balance_decrease_supply">balance::decrease_supply</a>(&<b>mut</b> cap.total_supply, <a href="../../dependencies/sui-framework/balance.md#0x2_balance">balance</a>)
}
</code></pre>



</details>

<a name="0x2_coin_mint_and_transfer"></a>

## Function `mint_and_transfer`



<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_mint_and_transfer">mint_and_transfer</a>&lt;T&gt;(c: &<b>mut</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_TreasuryCap">coin::TreasuryCap</a>&lt;T&gt;, amount: u64, recipient: <b>address</b>, ctx: &<b>mut</b> <a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_mint_and_transfer">mint_and_transfer</a>&lt;T&gt;(
    c: &<b>mut</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_TreasuryCap">TreasuryCap</a>&lt;T&gt;, amount: u64, recipient: <b>address</b>, ctx: &<b>mut</b> TxContext
) {
    <a href="../../dependencies/sui-framework/transfer.md#0x2_transfer_public_transfer">transfer::public_transfer</a>(<a href="../../dependencies/sui-framework/coin.md#0x2_coin_mint">mint</a>(c, amount, ctx), recipient)
}
</code></pre>



</details>

<a name="0x2_coin_update_name"></a>

## Function `update_name`



<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_update_name">update_name</a>&lt;T&gt;(_treasury: &<a href="../../dependencies/sui-framework/coin.md#0x2_coin_TreasuryCap">coin::TreasuryCap</a>&lt;T&gt;, metadata: &<b>mut</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_CoinMetadata">coin::CoinMetadata</a>&lt;T&gt;, name: <a href="../../dependencies/move-stdlib/string.md#0x1_string_String">string::String</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_update_name">update_name</a>&lt;T&gt;(
    _treasury: &<a href="../../dependencies/sui-framework/coin.md#0x2_coin_TreasuryCap">TreasuryCap</a>&lt;T&gt;, metadata: &<b>mut</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_CoinMetadata">CoinMetadata</a>&lt;T&gt;, name: <a href="../../dependencies/move-stdlib/string.md#0x1_string_String">string::String</a>
) {
    metadata.name = name;
}
</code></pre>



</details>

<a name="0x2_coin_update_symbol"></a>

## Function `update_symbol`



<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_update_symbol">update_symbol</a>&lt;T&gt;(_treasury: &<a href="../../dependencies/sui-framework/coin.md#0x2_coin_TreasuryCap">coin::TreasuryCap</a>&lt;T&gt;, metadata: &<b>mut</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_CoinMetadata">coin::CoinMetadata</a>&lt;T&gt;, symbol: <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_String">ascii::String</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_update_symbol">update_symbol</a>&lt;T&gt;(
    _treasury: &<a href="../../dependencies/sui-framework/coin.md#0x2_coin_TreasuryCap">TreasuryCap</a>&lt;T&gt;, metadata: &<b>mut</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_CoinMetadata">CoinMetadata</a>&lt;T&gt;, symbol: <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_String">ascii::String</a>
) {
    metadata.symbol = symbol;
}
</code></pre>



</details>

<a name="0x2_coin_update_description"></a>

## Function `update_description`



<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_update_description">update_description</a>&lt;T&gt;(_treasury: &<a href="../../dependencies/sui-framework/coin.md#0x2_coin_TreasuryCap">coin::TreasuryCap</a>&lt;T&gt;, metadata: &<b>mut</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_CoinMetadata">coin::CoinMetadata</a>&lt;T&gt;, description: <a href="../../dependencies/move-stdlib/string.md#0x1_string_String">string::String</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_update_description">update_description</a>&lt;T&gt;(
    _treasury: &<a href="../../dependencies/sui-framework/coin.md#0x2_coin_TreasuryCap">TreasuryCap</a>&lt;T&gt;, metadata: &<b>mut</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_CoinMetadata">CoinMetadata</a>&lt;T&gt;, description: <a href="../../dependencies/move-stdlib/string.md#0x1_string_String">string::String</a>
) {
    metadata.description = description;
}
</code></pre>



</details>

<a name="0x2_coin_update_icon_url"></a>

## Function `update_icon_url`



<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_update_icon_url">update_icon_url</a>&lt;T&gt;(_treasury: &<a href="../../dependencies/sui-framework/coin.md#0x2_coin_TreasuryCap">coin::TreasuryCap</a>&lt;T&gt;, metadata: &<b>mut</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_CoinMetadata">coin::CoinMetadata</a>&lt;T&gt;, <a href="../../dependencies/sui-framework/url.md#0x2_url">url</a>: <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_String">ascii::String</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_update_icon_url">update_icon_url</a>&lt;T&gt;(
    _treasury: &<a href="../../dependencies/sui-framework/coin.md#0x2_coin_TreasuryCap">TreasuryCap</a>&lt;T&gt;, metadata: &<b>mut</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_CoinMetadata">CoinMetadata</a>&lt;T&gt;, <a href="../../dependencies/sui-framework/url.md#0x2_url">url</a>: <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_String">ascii::String</a>
) {
    metadata.icon_url = <a href="../../dependencies/move-stdlib/option.md#0x1_option_some">option::some</a>(<a href="../../dependencies/sui-framework/url.md#0x2_url_new_unsafe">url::new_unsafe</a>(<a href="../../dependencies/sui-framework/url.md#0x2_url">url</a>));
}
</code></pre>



</details>

<a name="0x2_coin_get_decimals"></a>

## Function `get_decimals`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_get_decimals">get_decimals</a>&lt;T&gt;(metadata: &<a href="../../dependencies/sui-framework/coin.md#0x2_coin_CoinMetadata">coin::CoinMetadata</a>&lt;T&gt;): u8
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_get_decimals">get_decimals</a>&lt;T&gt;(metadata: &<a href="../../dependencies/sui-framework/coin.md#0x2_coin_CoinMetadata">CoinMetadata</a>&lt;T&gt;): u8 {
    metadata.decimals
}
</code></pre>



</details>

<a name="0x2_coin_get_name"></a>

## Function `get_name`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_get_name">get_name</a>&lt;T&gt;(metadata: &<a href="../../dependencies/sui-framework/coin.md#0x2_coin_CoinMetadata">coin::CoinMetadata</a>&lt;T&gt;): <a href="../../dependencies/move-stdlib/string.md#0x1_string_String">string::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_get_name">get_name</a>&lt;T&gt;(metadata: &<a href="../../dependencies/sui-framework/coin.md#0x2_coin_CoinMetadata">CoinMetadata</a>&lt;T&gt;): <a href="../../dependencies/move-stdlib/string.md#0x1_string_String">string::String</a> {
    metadata.name
}
</code></pre>



</details>

<a name="0x2_coin_get_symbol"></a>

## Function `get_symbol`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_get_symbol">get_symbol</a>&lt;T&gt;(metadata: &<a href="../../dependencies/sui-framework/coin.md#0x2_coin_CoinMetadata">coin::CoinMetadata</a>&lt;T&gt;): <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_String">ascii::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_get_symbol">get_symbol</a>&lt;T&gt;(metadata: &<a href="../../dependencies/sui-framework/coin.md#0x2_coin_CoinMetadata">CoinMetadata</a>&lt;T&gt;): <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_String">ascii::String</a> {
    metadata.symbol
}
</code></pre>



</details>

<a name="0x2_coin_get_description"></a>

## Function `get_description`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_get_description">get_description</a>&lt;T&gt;(metadata: &<a href="../../dependencies/sui-framework/coin.md#0x2_coin_CoinMetadata">coin::CoinMetadata</a>&lt;T&gt;): <a href="../../dependencies/move-stdlib/string.md#0x1_string_String">string::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_get_description">get_description</a>&lt;T&gt;(metadata: &<a href="../../dependencies/sui-framework/coin.md#0x2_coin_CoinMetadata">CoinMetadata</a>&lt;T&gt;): <a href="../../dependencies/move-stdlib/string.md#0x1_string_String">string::String</a> {
    metadata.description
}
</code></pre>



</details>

<a name="0x2_coin_get_icon_url"></a>

## Function `get_icon_url`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_get_icon_url">get_icon_url</a>&lt;T&gt;(metadata: &<a href="../../dependencies/sui-framework/coin.md#0x2_coin_CoinMetadata">coin::CoinMetadata</a>&lt;T&gt;): <a href="../../dependencies/move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;<a href="../../dependencies/sui-framework/url.md#0x2_url_Url">url::Url</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_get_icon_url">get_icon_url</a>&lt;T&gt;(metadata: &<a href="../../dependencies/sui-framework/coin.md#0x2_coin_CoinMetadata">CoinMetadata</a>&lt;T&gt;): Option&lt;Url&gt; {
    metadata.icon_url
}
</code></pre>



</details>

<a name="0x2_coin_supply"></a>

## Function `supply`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_supply">supply</a>&lt;T&gt;(treasury: &<b>mut</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_TreasuryCap">coin::TreasuryCap</a>&lt;T&gt;): &<a href="../../dependencies/sui-framework/balance.md#0x2_balance_Supply">balance::Supply</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_supply">supply</a>&lt;T&gt;(treasury: &<b>mut</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin_TreasuryCap">TreasuryCap</a>&lt;T&gt;): &Supply&lt;T&gt; {
    &treasury.total_supply
}
</code></pre>



</details>
