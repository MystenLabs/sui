
<a name="0x2_coin"></a>

# Module `0x2::coin`

Defines the <code><a href="coin.md#0x2_coin_Coin">Coin</a></code> type - platform wide representation of fungible
tokens and coins. <code><a href="coin.md#0x2_coin_Coin">Coin</a></code> can be described as a secure wrapper around
<code>Balance</code> type.


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


<pre><code><b>use</b> <a href="">0x1::ascii</a>;
<b>use</b> <a href="">0x1::option</a>;
<b>use</b> <a href="">0x1::string</a>;
<b>use</b> <a href="balance.md#0x2_balance">0x2::balance</a>;
<b>use</b> <a href="object.md#0x2_object">0x2::object</a>;
<b>use</b> <a href="transfer.md#0x2_transfer">0x2::transfer</a>;
<b>use</b> <a href="tx_context.md#0x2_tx_context">0x2::tx_context</a>;
<b>use</b> <a href="types.md#0x2_types">0x2::types</a>;
<b>use</b> <a href="url.md#0x2_url">0x2::url</a>;
</code></pre>



<a name="0x2_coin_Coin"></a>

## Resource `Coin`

A coin of type <code>T</code> worth <code>value</code>. Transferable and storable


<pre><code><b>struct</b> <a href="coin.md#0x2_coin_Coin">Coin</a>&lt;T&gt; <b>has</b> store, key
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>id: <a href="object.md#0x2_object_UID">object::UID</a></code>
</dt>
<dd>

</dd>
<dt>
<code><a href="balance.md#0x2_balance">balance</a>: <a href="balance.md#0x2_balance_Balance">balance::Balance</a>&lt;T&gt;</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x2_coin_CoinMetadata"></a>

## Resource `CoinMetadata`

Each Coin type T created through <code>create_currency</code> function will have a
unique instance of CoinMetadata<T> that stores the metadata for this coin type.


<pre><code><b>struct</b> <a href="coin.md#0x2_coin_CoinMetadata">CoinMetadata</a>&lt;T&gt; <b>has</b> store, key
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>id: <a href="object.md#0x2_object_UID">object::UID</a></code>
</dt>
<dd>

</dd>
<dt>
<code>decimals: u8</code>
</dt>
<dd>
 Number of decimal places the coin uses.
 A coin with <code>value </code> N and <code>decimals</code> D should be shown as N / 10^D
 E.g., a coin with <code>value</code> 7002 and decimals 3 should be displayed as 7.002
 This is metadata for display usage only.
</dd>
<dt>
<code>name: <a href="_String">string::String</a></code>
</dt>
<dd>
 Name for the token
</dd>
<dt>
<code>symbol: <a href="_String">ascii::String</a></code>
</dt>
<dd>
 Symbol for the token
</dd>
<dt>
<code>description: <a href="_String">string::String</a></code>
</dt>
<dd>
 Description of the token
</dd>
<dt>
<code>icon_url: <a href="_Option">option::Option</a>&lt;<a href="url.md#0x2_url_Url">url::Url</a>&gt;</code>
</dt>
<dd>
 URL for the token logo
</dd>
</dl>


</details>

<a name="0x2_coin_TreasuryCap"></a>

## Resource `TreasuryCap`

Capability allowing the bearer to mint and burn
coins of type <code>T</code>. Transferable


<pre><code><b>struct</b> <a href="coin.md#0x2_coin_TreasuryCap">TreasuryCap</a>&lt;T&gt; <b>has</b> store, key
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>id: <a href="object.md#0x2_object_UID">object::UID</a></code>
</dt>
<dd>

</dd>
<dt>
<code>total_supply: <a href="balance.md#0x2_balance_Supply">balance::Supply</a>&lt;T&gt;</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x2_coin_CurrencyCreated"></a>

## Struct `CurrencyCreated`



<pre><code><b>struct</b> <a href="coin.md#0x2_coin_CurrencyCreated">CurrencyCreated</a>&lt;T&gt; <b>has</b> <b>copy</b>, drop
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

Trying to split a coin more times than its balance allows.


<pre><code><b>const</b> <a href="coin.md#0x2_coin_ENotEnough">ENotEnough</a>: u64 = 2;
</code></pre>



<a name="0x2_coin_EBadWitness"></a>

A type passed to create_supply is not a one-time witness.


<pre><code><b>const</b> <a href="coin.md#0x2_coin_EBadWitness">EBadWitness</a>: u64 = 0;
</code></pre>



<a name="0x2_coin_EInvalidArg"></a>

Invalid arguments are passed to a function.


<pre><code><b>const</b> <a href="coin.md#0x2_coin_EInvalidArg">EInvalidArg</a>: u64 = 1;
</code></pre>



<a name="0x2_coin_total_supply"></a>

## Function `total_supply`

Return the total number of <code>T</code>'s in circulation.


<pre><code><b>public</b> <b>fun</b> <a href="coin.md#0x2_coin_total_supply">total_supply</a>&lt;T&gt;(cap: &<a href="coin.md#0x2_coin_TreasuryCap">coin::TreasuryCap</a>&lt;T&gt;): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="coin.md#0x2_coin_total_supply">total_supply</a>&lt;T&gt;(cap: &<a href="coin.md#0x2_coin_TreasuryCap">TreasuryCap</a>&lt;T&gt;): u64 {
    <a href="balance.md#0x2_balance_supply_value">balance::supply_value</a>(&cap.total_supply)
}
</code></pre>



</details>

<a name="0x2_coin_treasury_into_supply"></a>

## Function `treasury_into_supply`

Unwrap <code><a href="coin.md#0x2_coin_TreasuryCap">TreasuryCap</a></code> getting the <code>Supply</code>.

Operation is irreversible. Supply cannot be converted into a <code><a href="coin.md#0x2_coin_TreasuryCap">TreasuryCap</a></code> due
to different security guarantees (TreasuryCap can be created only once for a type)


<pre><code><b>public</b> <b>fun</b> <a href="coin.md#0x2_coin_treasury_into_supply">treasury_into_supply</a>&lt;T&gt;(treasury: <a href="coin.md#0x2_coin_TreasuryCap">coin::TreasuryCap</a>&lt;T&gt;): <a href="balance.md#0x2_balance_Supply">balance::Supply</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="coin.md#0x2_coin_treasury_into_supply">treasury_into_supply</a>&lt;T&gt;(treasury: <a href="coin.md#0x2_coin_TreasuryCap">TreasuryCap</a>&lt;T&gt;): Supply&lt;T&gt; {
    <b>let</b> <a href="coin.md#0x2_coin_TreasuryCap">TreasuryCap</a> { id, total_supply } = treasury;
    <a href="object.md#0x2_object_delete">object::delete</a>(id);
    total_supply
}
</code></pre>



</details>

<a name="0x2_coin_supply_immut"></a>

## Function `supply_immut`

Get immutable reference to the treasury's <code>Supply</code>.


<pre><code><b>public</b> <b>fun</b> <a href="coin.md#0x2_coin_supply_immut">supply_immut</a>&lt;T&gt;(treasury: &<a href="coin.md#0x2_coin_TreasuryCap">coin::TreasuryCap</a>&lt;T&gt;): &<a href="balance.md#0x2_balance_Supply">balance::Supply</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="coin.md#0x2_coin_supply_immut">supply_immut</a>&lt;T&gt;(treasury: &<a href="coin.md#0x2_coin_TreasuryCap">TreasuryCap</a>&lt;T&gt;): &Supply&lt;T&gt; {
    &treasury.total_supply
}
</code></pre>



</details>

<a name="0x2_coin_supply_mut"></a>

## Function `supply_mut`

Get mutable reference to the treasury's <code>Supply</code>.


<pre><code><b>public</b> <b>fun</b> <a href="coin.md#0x2_coin_supply_mut">supply_mut</a>&lt;T&gt;(treasury: &<b>mut</b> <a href="coin.md#0x2_coin_TreasuryCap">coin::TreasuryCap</a>&lt;T&gt;): &<b>mut</b> <a href="balance.md#0x2_balance_Supply">balance::Supply</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="coin.md#0x2_coin_supply_mut">supply_mut</a>&lt;T&gt;(treasury: &<b>mut</b> <a href="coin.md#0x2_coin_TreasuryCap">TreasuryCap</a>&lt;T&gt;): &<b>mut</b> Supply&lt;T&gt; {
    &<b>mut</b> treasury.total_supply
}
</code></pre>



</details>

<a name="0x2_coin_value"></a>

## Function `value`

Public getter for the coin's value


<pre><code><b>public</b> <b>fun</b> <a href="coin.md#0x2_coin_value">value</a>&lt;T&gt;(self: &<a href="coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="coin.md#0x2_coin_value">value</a>&lt;T&gt;(self: &<a href="coin.md#0x2_coin_Coin">Coin</a>&lt;T&gt;): u64 {
    <a href="balance.md#0x2_balance_value">balance::value</a>(&self.<a href="balance.md#0x2_balance">balance</a>)
}
</code></pre>



</details>

<a name="0x2_coin_balance"></a>

## Function `balance`

Get immutable reference to the balance of a coin.


<pre><code><b>public</b> <b>fun</b> <a href="balance.md#0x2_balance">balance</a>&lt;T&gt;(<a href="coin.md#0x2_coin">coin</a>: &<a href="coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;): &<a href="balance.md#0x2_balance_Balance">balance::Balance</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="balance.md#0x2_balance">balance</a>&lt;T&gt;(<a href="coin.md#0x2_coin">coin</a>: &<a href="coin.md#0x2_coin_Coin">Coin</a>&lt;T&gt;): &Balance&lt;T&gt; {
    &<a href="coin.md#0x2_coin">coin</a>.<a href="balance.md#0x2_balance">balance</a>
}
</code></pre>



</details>

<a name="0x2_coin_balance_mut"></a>

## Function `balance_mut`

Get a mutable reference to the balance of a coin.


<pre><code><b>public</b> <b>fun</b> <a href="coin.md#0x2_coin_balance_mut">balance_mut</a>&lt;T&gt;(<a href="coin.md#0x2_coin">coin</a>: &<b>mut</b> <a href="coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;): &<b>mut</b> <a href="balance.md#0x2_balance_Balance">balance::Balance</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="coin.md#0x2_coin_balance_mut">balance_mut</a>&lt;T&gt;(<a href="coin.md#0x2_coin">coin</a>: &<b>mut</b> <a href="coin.md#0x2_coin_Coin">Coin</a>&lt;T&gt;): &<b>mut</b> Balance&lt;T&gt; {
    &<b>mut</b> <a href="coin.md#0x2_coin">coin</a>.<a href="balance.md#0x2_balance">balance</a>
}
</code></pre>



</details>

<a name="0x2_coin_from_balance"></a>

## Function `from_balance`

Wrap a balance into a Coin to make it transferable.


<pre><code><b>public</b> <b>fun</b> <a href="coin.md#0x2_coin_from_balance">from_balance</a>&lt;T&gt;(<a href="balance.md#0x2_balance">balance</a>: <a href="balance.md#0x2_balance_Balance">balance::Balance</a>&lt;T&gt;, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="coin.md#0x2_coin_from_balance">from_balance</a>&lt;T&gt;(<a href="balance.md#0x2_balance">balance</a>: Balance&lt;T&gt;, ctx: &<b>mut</b> TxContext): <a href="coin.md#0x2_coin_Coin">Coin</a>&lt;T&gt; {
    <a href="coin.md#0x2_coin_Coin">Coin</a> { id: <a href="object.md#0x2_object_new">object::new</a>(ctx), <a href="balance.md#0x2_balance">balance</a> }
}
</code></pre>



</details>

<a name="0x2_coin_into_balance"></a>

## Function `into_balance`

Destruct a Coin wrapper and keep the balance.


<pre><code><b>public</b> <b>fun</b> <a href="coin.md#0x2_coin_into_balance">into_balance</a>&lt;T&gt;(<a href="coin.md#0x2_coin">coin</a>: <a href="coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;): <a href="balance.md#0x2_balance_Balance">balance::Balance</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="coin.md#0x2_coin_into_balance">into_balance</a>&lt;T&gt;(<a href="coin.md#0x2_coin">coin</a>: <a href="coin.md#0x2_coin_Coin">Coin</a>&lt;T&gt;): Balance&lt;T&gt; {
    <b>let</b> <a href="coin.md#0x2_coin_Coin">Coin</a> { id, <a href="balance.md#0x2_balance">balance</a> } = <a href="coin.md#0x2_coin">coin</a>;
    <a href="object.md#0x2_object_delete">object::delete</a>(id);
    <a href="balance.md#0x2_balance">balance</a>
}
</code></pre>



</details>

<a name="0x2_coin_take"></a>

## Function `take`

Take a <code><a href="coin.md#0x2_coin_Coin">Coin</a></code> worth of <code>value</code> from <code>Balance</code>.
Aborts if <code>value &gt; <a href="balance.md#0x2_balance">balance</a>.value</code>


<pre><code><b>public</b> <b>fun</b> <a href="coin.md#0x2_coin_take">take</a>&lt;T&gt;(<a href="balance.md#0x2_balance">balance</a>: &<b>mut</b> <a href="balance.md#0x2_balance_Balance">balance::Balance</a>&lt;T&gt;, value: u64, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="coin.md#0x2_coin_take">take</a>&lt;T&gt;(
    <a href="balance.md#0x2_balance">balance</a>: &<b>mut</b> Balance&lt;T&gt;, value: u64, ctx: &<b>mut</b> TxContext,
): <a href="coin.md#0x2_coin_Coin">Coin</a>&lt;T&gt; {
    <a href="coin.md#0x2_coin_Coin">Coin</a> {
        id: <a href="object.md#0x2_object_new">object::new</a>(ctx),
        <a href="balance.md#0x2_balance">balance</a>: <a href="balance.md#0x2_balance_split">balance::split</a>(<a href="balance.md#0x2_balance">balance</a>, value)
    }
}
</code></pre>



</details>

<details>
<summary>Specification</summary>



<pre><code><b>let</b> before_val = <a href="balance.md#0x2_balance">balance</a>.value;
<b>let</b> <b>post</b> after_val = <a href="balance.md#0x2_balance">balance</a>.value;
<b>ensures</b> after_val == before_val - value;
<b>aborts_if</b> value &gt; before_val;
<b>aborts_if</b> ctx.ids_created + 1 &gt; MAX_U64;
</code></pre>



</details>

<a name="0x2_coin_put"></a>

## Function `put`

Put a <code><a href="coin.md#0x2_coin_Coin">Coin</a>&lt;T&gt;</code> to the <code>Balance&lt;T&gt;</code>.


<pre><code><b>public</b> <b>fun</b> <a href="coin.md#0x2_coin_put">put</a>&lt;T&gt;(<a href="balance.md#0x2_balance">balance</a>: &<b>mut</b> <a href="balance.md#0x2_balance_Balance">balance::Balance</a>&lt;T&gt;, <a href="coin.md#0x2_coin">coin</a>: <a href="coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="coin.md#0x2_coin_put">put</a>&lt;T&gt;(<a href="balance.md#0x2_balance">balance</a>: &<b>mut</b> Balance&lt;T&gt;, <a href="coin.md#0x2_coin">coin</a>: <a href="coin.md#0x2_coin_Coin">Coin</a>&lt;T&gt;) {
    <a href="balance.md#0x2_balance_join">balance::join</a>(<a href="balance.md#0x2_balance">balance</a>, <a href="coin.md#0x2_coin_into_balance">into_balance</a>(<a href="coin.md#0x2_coin">coin</a>));
}
</code></pre>



</details>

<details>
<summary>Specification</summary>



<pre><code><b>let</b> before_val = <a href="balance.md#0x2_balance">balance</a>.value;
<b>let</b> <b>post</b> after_val = <a href="balance.md#0x2_balance">balance</a>.value;
<b>ensures</b> after_val == before_val + <a href="coin.md#0x2_coin">coin</a>.<a href="balance.md#0x2_balance">balance</a>.value;
<b>aborts_if</b> before_val + <a href="coin.md#0x2_coin">coin</a>.<a href="balance.md#0x2_balance">balance</a>.value &gt; MAX_U64;
</code></pre>



</details>

<a name="0x2_coin_join"></a>

## Function `join`

Consume the coin <code>c</code> and add its value to <code>self</code>.
Aborts if <code>c.value + self.value &gt; U64_MAX</code>


<pre><code><b>public</b> entry <b>fun</b> <a href="coin.md#0x2_coin_join">join</a>&lt;T&gt;(self: &<b>mut</b> <a href="coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;, c: <a href="coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="coin.md#0x2_coin_join">join</a>&lt;T&gt;(self: &<b>mut</b> <a href="coin.md#0x2_coin_Coin">Coin</a>&lt;T&gt;, c: <a href="coin.md#0x2_coin_Coin">Coin</a>&lt;T&gt;) {
    <b>let</b> <a href="coin.md#0x2_coin_Coin">Coin</a> { id, <a href="balance.md#0x2_balance">balance</a> } = c;
    <a href="object.md#0x2_object_delete">object::delete</a>(id);
    <a href="balance.md#0x2_balance_join">balance::join</a>(&<b>mut</b> self.<a href="balance.md#0x2_balance">balance</a>, <a href="balance.md#0x2_balance">balance</a>);
}
</code></pre>



</details>

<details>
<summary>Specification</summary>



<pre><code><b>let</b> before_val = self.<a href="balance.md#0x2_balance">balance</a>.value;
<b>let</b> <b>post</b> after_val = self.<a href="balance.md#0x2_balance">balance</a>.value;
<b>ensures</b> after_val == before_val + c.<a href="balance.md#0x2_balance">balance</a>.value;
<b>aborts_if</b> before_val + c.<a href="balance.md#0x2_balance">balance</a>.value &gt; MAX_U64;
</code></pre>



</details>

<a name="0x2_coin_split"></a>

## Function `split`

Split coin <code>self</code> to two coins, one with balance <code>split_amount</code>,
and the remaining balance is left is <code>self</code>.


<pre><code><b>public</b> <b>fun</b> <a href="coin.md#0x2_coin_split">split</a>&lt;T&gt;(self: &<b>mut</b> <a href="coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;, split_amount: u64, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="coin.md#0x2_coin_split">split</a>&lt;T&gt;(
    self: &<b>mut</b> <a href="coin.md#0x2_coin_Coin">Coin</a>&lt;T&gt;, split_amount: u64, ctx: &<b>mut</b> TxContext
): <a href="coin.md#0x2_coin_Coin">Coin</a>&lt;T&gt; {
    <a href="coin.md#0x2_coin_take">take</a>(&<b>mut</b> self.<a href="balance.md#0x2_balance">balance</a>, split_amount, ctx)
}
</code></pre>



</details>

<details>
<summary>Specification</summary>



<pre><code><b>let</b> before_val = self.<a href="balance.md#0x2_balance">balance</a>.value;
<b>let</b> <b>post</b> after_val = self.<a href="balance.md#0x2_balance">balance</a>.value;
<b>ensures</b> after_val == before_val - split_amount;
<b>aborts_if</b> split_amount &gt; before_val;
<b>aborts_if</b> ctx.ids_created + 1 &gt; MAX_U64;
</code></pre>



</details>

<a name="0x2_coin_divide_into_n"></a>

## Function `divide_into_n`

Split coin <code>self</code> into <code>n - 1</code> coins with equal balances. The remainder is left in
<code>self</code>. Return newly created coins.


<pre><code><b>public</b> <b>fun</b> <a href="coin.md#0x2_coin_divide_into_n">divide_into_n</a>&lt;T&gt;(self: &<b>mut</b> <a href="coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;, n: u64, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="">vector</a>&lt;<a href="coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="coin.md#0x2_coin_divide_into_n">divide_into_n</a>&lt;T&gt;(
    self: &<b>mut</b> <a href="coin.md#0x2_coin_Coin">Coin</a>&lt;T&gt;, n: u64, ctx: &<b>mut</b> TxContext
): <a href="">vector</a>&lt;<a href="coin.md#0x2_coin_Coin">Coin</a>&lt;T&gt;&gt; {
    <b>assert</b>!(n &gt; 0, <a href="coin.md#0x2_coin_EInvalidArg">EInvalidArg</a>);
    <b>assert</b>!(n &lt;= <a href="coin.md#0x2_coin_value">value</a>(self), <a href="coin.md#0x2_coin_ENotEnough">ENotEnough</a>);

    <b>let</b> vec = <a href="_empty">vector::empty</a>&lt;<a href="coin.md#0x2_coin_Coin">Coin</a>&lt;T&gt;&gt;();
    <b>let</b> i = 0;
    <b>let</b> split_amount = <a href="coin.md#0x2_coin_value">value</a>(self) / n;
    <b>while</b> ({
        <b>spec</b> {
            <b>invariant</b> i &lt;= n-1;
            <b>invariant</b> self.<a href="balance.md#0x2_balance">balance</a>.value == <b>old</b>(self).<a href="balance.md#0x2_balance">balance</a>.value - (i * split_amount);
            <b>invariant</b> ctx.ids_created == <b>old</b>(ctx).ids_created + i;
        };
        i &lt; n - 1
    }) {
        <a href="_push_back">vector::push_back</a>(&<b>mut</b> vec, <a href="coin.md#0x2_coin_split">split</a>(self, split_amount, ctx));
        i = i + 1;
    };
    vec
}
</code></pre>



</details>

<details>
<summary>Specification</summary>



<pre><code><b>let</b> before_val = self.<a href="balance.md#0x2_balance">balance</a>.value;
<b>let</b> <b>post</b> after_val = self.<a href="balance.md#0x2_balance">balance</a>.value;
<b>let</b> split_amount = before_val / n;
<b>ensures</b> after_val == before_val - ((n - 1) * split_amount);
<b>aborts_if</b> n == 0;
<b>aborts_if</b> self.<a href="balance.md#0x2_balance">balance</a>.<a href="coin.md#0x2_coin_value">value</a> &lt; n;
<b>aborts_if</b> ctx.ids_created + n - 1 &gt; MAX_U64;
</code></pre>



</details>

<a name="0x2_coin_zero"></a>

## Function `zero`

Make any Coin with a zero value. Useful for placeholding
bids/payments or preemptively making empty balances.


<pre><code><b>public</b> <b>fun</b> <a href="coin.md#0x2_coin_zero">zero</a>&lt;T&gt;(ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="coin.md#0x2_coin_zero">zero</a>&lt;T&gt;(ctx: &<b>mut</b> TxContext): <a href="coin.md#0x2_coin_Coin">Coin</a>&lt;T&gt; {
    <a href="coin.md#0x2_coin_Coin">Coin</a> { id: <a href="object.md#0x2_object_new">object::new</a>(ctx), <a href="balance.md#0x2_balance">balance</a>: <a href="balance.md#0x2_balance_zero">balance::zero</a>() }
}
</code></pre>



</details>

<a name="0x2_coin_destroy_zero"></a>

## Function `destroy_zero`

Destroy a coin with value zero


<pre><code><b>public</b> <b>fun</b> <a href="coin.md#0x2_coin_destroy_zero">destroy_zero</a>&lt;T&gt;(c: <a href="coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="coin.md#0x2_coin_destroy_zero">destroy_zero</a>&lt;T&gt;(c: <a href="coin.md#0x2_coin_Coin">Coin</a>&lt;T&gt;) {
    <b>let</b> <a href="coin.md#0x2_coin_Coin">Coin</a> { id, <a href="balance.md#0x2_balance">balance</a> } = c;
    <a href="object.md#0x2_object_delete">object::delete</a>(id);
    <a href="balance.md#0x2_balance_destroy_zero">balance::destroy_zero</a>(<a href="balance.md#0x2_balance">balance</a>)
}
</code></pre>



</details>

<a name="0x2_coin_create_currency"></a>

## Function `create_currency`

Create a new currency type <code>T</code> as and return the <code><a href="coin.md#0x2_coin_TreasuryCap">TreasuryCap</a></code> for
<code>T</code> to the caller. Can only be called with a <code>one-time-witness</code>
type, ensuring that there's only one <code><a href="coin.md#0x2_coin_TreasuryCap">TreasuryCap</a></code> per <code>T</code>.


<pre><code><b>public</b> <b>fun</b> <a href="coin.md#0x2_coin_create_currency">create_currency</a>&lt;T: drop&gt;(witness: T, decimals: u8, symbol: <a href="">vector</a>&lt;u8&gt;, name: <a href="">vector</a>&lt;u8&gt;, description: <a href="">vector</a>&lt;u8&gt;, icon_url: <a href="_Option">option::Option</a>&lt;<a href="url.md#0x2_url_Url">url::Url</a>&gt;, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): (<a href="coin.md#0x2_coin_TreasuryCap">coin::TreasuryCap</a>&lt;T&gt;, <a href="coin.md#0x2_coin_CoinMetadata">coin::CoinMetadata</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="coin.md#0x2_coin_create_currency">create_currency</a>&lt;T: drop&gt;(
    witness: T,
    decimals: u8,
    symbol: <a href="">vector</a>&lt;u8&gt;,
    name: <a href="">vector</a>&lt;u8&gt;,
    description: <a href="">vector</a>&lt;u8&gt;,
    icon_url: Option&lt;Url&gt;,
    ctx: &<b>mut</b> TxContext
): (<a href="coin.md#0x2_coin_TreasuryCap">TreasuryCap</a>&lt;T&gt;, <a href="coin.md#0x2_coin_CoinMetadata">CoinMetadata</a>&lt;T&gt;) {
    // Make sure there's only one instance of the type T
    <b>assert</b>!(sui::types::is_one_time_witness(&witness), <a href="coin.md#0x2_coin_EBadWitness">EBadWitness</a>);

    (
        <a href="coin.md#0x2_coin_TreasuryCap">TreasuryCap</a> {
            id: <a href="object.md#0x2_object_new">object::new</a>(ctx),
            total_supply: <a href="balance.md#0x2_balance_create_supply">balance::create_supply</a>(witness)
        },
        <a href="coin.md#0x2_coin_CoinMetadata">CoinMetadata</a> {
            id: <a href="object.md#0x2_object_new">object::new</a>(ctx),
            decimals,
            name: <a href="_utf8">string::utf8</a>(name),
            symbol: <a href="_string">ascii::string</a>(symbol),
            description: <a href="_utf8">string::utf8</a>(description),
            icon_url
        }
    )
}
</code></pre>



</details>

<a name="0x2_coin_mint"></a>

## Function `mint`

Create a coin worth <code>value</code>. and increase the total supply
in <code>cap</code> accordingly.


<pre><code><b>public</b> <b>fun</b> <a href="coin.md#0x2_coin_mint">mint</a>&lt;T&gt;(cap: &<b>mut</b> <a href="coin.md#0x2_coin_TreasuryCap">coin::TreasuryCap</a>&lt;T&gt;, value: u64, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="coin.md#0x2_coin_mint">mint</a>&lt;T&gt;(
    cap: &<b>mut</b> <a href="coin.md#0x2_coin_TreasuryCap">TreasuryCap</a>&lt;T&gt;, value: u64, ctx: &<b>mut</b> TxContext,
): <a href="coin.md#0x2_coin_Coin">Coin</a>&lt;T&gt; {
    <a href="coin.md#0x2_coin_Coin">Coin</a> {
        id: <a href="object.md#0x2_object_new">object::new</a>(ctx),
        <a href="balance.md#0x2_balance">balance</a>: <a href="balance.md#0x2_balance_increase_supply">balance::increase_supply</a>(&<b>mut</b> cap.total_supply, value)
    }
}
</code></pre>



</details>

<details>
<summary>Specification</summary>



<pre><code><b>include</b> <a href="coin.md#0x2_coin_MintBalance">MintBalance</a>&lt;T&gt;;
<b>aborts_if</b> ctx.ids_created + 1 &gt; MAX_U64;
</code></pre>



</details>

<a name="0x2_coin_mint_balance"></a>

## Function `mint_balance`

Mint some amount of T as a <code>Balance</code> and increase the total
supply in <code>cap</code> accordingly.
Aborts if <code>value</code> + <code>cap.total_supply</code> >= U64_MAX


<pre><code><b>public</b> <b>fun</b> <a href="coin.md#0x2_coin_mint_balance">mint_balance</a>&lt;T&gt;(cap: &<b>mut</b> <a href="coin.md#0x2_coin_TreasuryCap">coin::TreasuryCap</a>&lt;T&gt;, value: u64): <a href="balance.md#0x2_balance_Balance">balance::Balance</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="coin.md#0x2_coin_mint_balance">mint_balance</a>&lt;T&gt;(
    cap: &<b>mut</b> <a href="coin.md#0x2_coin_TreasuryCap">TreasuryCap</a>&lt;T&gt;, value: u64
): Balance&lt;T&gt; {
    <a href="balance.md#0x2_balance_increase_supply">balance::increase_supply</a>(&<b>mut</b> cap.total_supply, value)
}
</code></pre>



</details>

<details>
<summary>Specification</summary>



<pre><code><b>include</b> <a href="coin.md#0x2_coin_MintBalance">MintBalance</a>&lt;T&gt;;
</code></pre>



</details>

<a name="0x2_coin_burn"></a>

## Function `burn`

Destroy the coin <code>c</code> and decrease the total supply in <code>cap</code>
accordingly.


<pre><code><b>public</b> entry <b>fun</b> <a href="coin.md#0x2_coin_burn">burn</a>&lt;T&gt;(cap: &<b>mut</b> <a href="coin.md#0x2_coin_TreasuryCap">coin::TreasuryCap</a>&lt;T&gt;, c: <a href="coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="coin.md#0x2_coin_burn">burn</a>&lt;T&gt;(cap: &<b>mut</b> <a href="coin.md#0x2_coin_TreasuryCap">TreasuryCap</a>&lt;T&gt;, c: <a href="coin.md#0x2_coin_Coin">Coin</a>&lt;T&gt;): u64 {
    <b>let</b> <a href="coin.md#0x2_coin_Coin">Coin</a> { id, <a href="balance.md#0x2_balance">balance</a> } = c;
    <a href="object.md#0x2_object_delete">object::delete</a>(id);
    <a href="balance.md#0x2_balance_decrease_supply">balance::decrease_supply</a>(&<b>mut</b> cap.total_supply, <a href="balance.md#0x2_balance">balance</a>)
}
</code></pre>



</details>

<details>
<summary>Specification</summary>



<pre><code><b>include</b> <a href="coin.md#0x2_coin_Burn">Burn</a>&lt;T&gt;;
</code></pre>



</details>

<a name="0x2_coin_mint_and_transfer"></a>

## Function `mint_and_transfer`

Mint <code>amount</code> of <code><a href="coin.md#0x2_coin_Coin">Coin</a></code> and send it to <code>recipient</code>. Invokes <code><a href="coin.md#0x2_coin_mint">mint</a>()</code>.


<pre><code><b>public</b> entry <b>fun</b> <a href="coin.md#0x2_coin_mint_and_transfer">mint_and_transfer</a>&lt;T&gt;(c: &<b>mut</b> <a href="coin.md#0x2_coin_TreasuryCap">coin::TreasuryCap</a>&lt;T&gt;, amount: u64, recipient: <b>address</b>, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="coin.md#0x2_coin_mint_and_transfer">mint_and_transfer</a>&lt;T&gt;(
    c: &<b>mut</b> <a href="coin.md#0x2_coin_TreasuryCap">TreasuryCap</a>&lt;T&gt;, amount: u64, recipient: <b>address</b>, ctx: &<b>mut</b> TxContext
) {
    <a href="transfer.md#0x2_transfer_public_transfer">transfer::public_transfer</a>(<a href="coin.md#0x2_coin_mint">mint</a>(c, amount, ctx), recipient)
}
</code></pre>



</details>

<a name="0x2_coin_update_name"></a>

## Function `update_name`

Update name of the coin in <code><a href="coin.md#0x2_coin_CoinMetadata">CoinMetadata</a></code>


<pre><code><b>public</b> entry <b>fun</b> <a href="coin.md#0x2_coin_update_name">update_name</a>&lt;T&gt;(_treasury: &<a href="coin.md#0x2_coin_TreasuryCap">coin::TreasuryCap</a>&lt;T&gt;, metadata: &<b>mut</b> <a href="coin.md#0x2_coin_CoinMetadata">coin::CoinMetadata</a>&lt;T&gt;, name: <a href="_String">string::String</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="coin.md#0x2_coin_update_name">update_name</a>&lt;T&gt;(
    _treasury: &<a href="coin.md#0x2_coin_TreasuryCap">TreasuryCap</a>&lt;T&gt;, metadata: &<b>mut</b> <a href="coin.md#0x2_coin_CoinMetadata">CoinMetadata</a>&lt;T&gt;, name: <a href="_String">string::String</a>
) {
    metadata.name = name;
}
</code></pre>



</details>

<a name="0x2_coin_update_symbol"></a>

## Function `update_symbol`

Update the symbol of the coin in <code><a href="coin.md#0x2_coin_CoinMetadata">CoinMetadata</a></code>


<pre><code><b>public</b> entry <b>fun</b> <a href="coin.md#0x2_coin_update_symbol">update_symbol</a>&lt;T&gt;(_treasury: &<a href="coin.md#0x2_coin_TreasuryCap">coin::TreasuryCap</a>&lt;T&gt;, metadata: &<b>mut</b> <a href="coin.md#0x2_coin_CoinMetadata">coin::CoinMetadata</a>&lt;T&gt;, symbol: <a href="_String">ascii::String</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="coin.md#0x2_coin_update_symbol">update_symbol</a>&lt;T&gt;(
    _treasury: &<a href="coin.md#0x2_coin_TreasuryCap">TreasuryCap</a>&lt;T&gt;, metadata: &<b>mut</b> <a href="coin.md#0x2_coin_CoinMetadata">CoinMetadata</a>&lt;T&gt;, symbol: <a href="_String">ascii::String</a>
) {
    metadata.symbol = symbol;
}
</code></pre>



</details>

<a name="0x2_coin_update_description"></a>

## Function `update_description`

Update the description of the coin in <code><a href="coin.md#0x2_coin_CoinMetadata">CoinMetadata</a></code>


<pre><code><b>public</b> entry <b>fun</b> <a href="coin.md#0x2_coin_update_description">update_description</a>&lt;T&gt;(_treasury: &<a href="coin.md#0x2_coin_TreasuryCap">coin::TreasuryCap</a>&lt;T&gt;, metadata: &<b>mut</b> <a href="coin.md#0x2_coin_CoinMetadata">coin::CoinMetadata</a>&lt;T&gt;, description: <a href="_String">string::String</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="coin.md#0x2_coin_update_description">update_description</a>&lt;T&gt;(
    _treasury: &<a href="coin.md#0x2_coin_TreasuryCap">TreasuryCap</a>&lt;T&gt;, metadata: &<b>mut</b> <a href="coin.md#0x2_coin_CoinMetadata">CoinMetadata</a>&lt;T&gt;, description: <a href="_String">string::String</a>
) {
    metadata.description = description;
}
</code></pre>



</details>

<a name="0x2_coin_update_icon_url"></a>

## Function `update_icon_url`

Update the url of the coin in <code><a href="coin.md#0x2_coin_CoinMetadata">CoinMetadata</a></code>


<pre><code><b>public</b> entry <b>fun</b> <a href="coin.md#0x2_coin_update_icon_url">update_icon_url</a>&lt;T&gt;(_treasury: &<a href="coin.md#0x2_coin_TreasuryCap">coin::TreasuryCap</a>&lt;T&gt;, metadata: &<b>mut</b> <a href="coin.md#0x2_coin_CoinMetadata">coin::CoinMetadata</a>&lt;T&gt;, <a href="url.md#0x2_url">url</a>: <a href="_String">ascii::String</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="coin.md#0x2_coin_update_icon_url">update_icon_url</a>&lt;T&gt;(
    _treasury: &<a href="coin.md#0x2_coin_TreasuryCap">TreasuryCap</a>&lt;T&gt;, metadata: &<b>mut</b> <a href="coin.md#0x2_coin_CoinMetadata">CoinMetadata</a>&lt;T&gt;, <a href="url.md#0x2_url">url</a>: <a href="_String">ascii::String</a>
) {
    metadata.icon_url = <a href="_some">option::some</a>(<a href="url.md#0x2_url_new_unsafe">url::new_unsafe</a>(<a href="url.md#0x2_url">url</a>));
}
</code></pre>



</details>

<a name="0x2_coin_get_decimals"></a>

## Function `get_decimals`



<pre><code><b>public</b> <b>fun</b> <a href="coin.md#0x2_coin_get_decimals">get_decimals</a>&lt;T&gt;(metadata: &<a href="coin.md#0x2_coin_CoinMetadata">coin::CoinMetadata</a>&lt;T&gt;): u8
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="coin.md#0x2_coin_get_decimals">get_decimals</a>&lt;T&gt;(
    metadata: &<a href="coin.md#0x2_coin_CoinMetadata">CoinMetadata</a>&lt;T&gt;
): u8 {
    metadata.decimals
}
</code></pre>



</details>

<a name="0x2_coin_get_name"></a>

## Function `get_name`



<pre><code><b>public</b> <b>fun</b> <a href="coin.md#0x2_coin_get_name">get_name</a>&lt;T&gt;(metadata: &<a href="coin.md#0x2_coin_CoinMetadata">coin::CoinMetadata</a>&lt;T&gt;): <a href="_String">string::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="coin.md#0x2_coin_get_name">get_name</a>&lt;T&gt;(
    metadata: &<a href="coin.md#0x2_coin_CoinMetadata">CoinMetadata</a>&lt;T&gt;
): <a href="_String">string::String</a> {
    metadata.name
}
</code></pre>



</details>

<a name="0x2_coin_get_symbol"></a>

## Function `get_symbol`



<pre><code><b>public</b> <b>fun</b> <a href="coin.md#0x2_coin_get_symbol">get_symbol</a>&lt;T&gt;(metadata: &<a href="coin.md#0x2_coin_CoinMetadata">coin::CoinMetadata</a>&lt;T&gt;): <a href="_String">ascii::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="coin.md#0x2_coin_get_symbol">get_symbol</a>&lt;T&gt;(
    metadata: &<a href="coin.md#0x2_coin_CoinMetadata">CoinMetadata</a>&lt;T&gt;
): <a href="_String">ascii::String</a> {
    metadata.symbol
}
</code></pre>



</details>

<a name="0x2_coin_get_description"></a>

## Function `get_description`



<pre><code><b>public</b> <b>fun</b> <a href="coin.md#0x2_coin_get_description">get_description</a>&lt;T&gt;(metadata: &<a href="coin.md#0x2_coin_CoinMetadata">coin::CoinMetadata</a>&lt;T&gt;): <a href="_String">string::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="coin.md#0x2_coin_get_description">get_description</a>&lt;T&gt;(
    metadata: &<a href="coin.md#0x2_coin_CoinMetadata">CoinMetadata</a>&lt;T&gt;
): <a href="_String">string::String</a> {
    metadata.description
}
</code></pre>



</details>

<a name="0x2_coin_get_icon_url"></a>

## Function `get_icon_url`



<pre><code><b>public</b> <b>fun</b> <a href="coin.md#0x2_coin_get_icon_url">get_icon_url</a>&lt;T&gt;(metadata: &<a href="coin.md#0x2_coin_CoinMetadata">coin::CoinMetadata</a>&lt;T&gt;): <a href="_Option">option::Option</a>&lt;<a href="url.md#0x2_url_Url">url::Url</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="coin.md#0x2_coin_get_icon_url">get_icon_url</a>&lt;T&gt;(
    metadata: &<a href="coin.md#0x2_coin_CoinMetadata">CoinMetadata</a>&lt;T&gt;
): Option&lt;Url&gt; {
    metadata.icon_url
}
</code></pre>



</details>

<a name="0x2_coin_supply"></a>

## Function `supply`



<pre><code><b>public</b> <b>fun</b> <a href="coin.md#0x2_coin_supply">supply</a>&lt;T&gt;(treasury: &<b>mut</b> <a href="coin.md#0x2_coin_TreasuryCap">coin::TreasuryCap</a>&lt;T&gt;): &<a href="balance.md#0x2_balance_Supply">balance::Supply</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="coin.md#0x2_coin_supply">supply</a>&lt;T&gt;(treasury: &<b>mut</b> <a href="coin.md#0x2_coin_TreasuryCap">TreasuryCap</a>&lt;T&gt;): &Supply&lt;T&gt; {
    &treasury.total_supply
}
</code></pre>



</details>
