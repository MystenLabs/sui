
<a name="0x2_coin"></a>

# Module `0x2::coin`

Defines the <code><a href="coin.md#0x2_coin_Coin">Coin</a></code> type - platform wide representation of fungible
tokens and coins. <code><a href="coin.md#0x2_coin_Coin">Coin</a></code> can be described as a secure wrapper around
<code>Balance</code> type.


-  [Resource `Coin`](#0x2_coin_Coin)
-  [Resource `TreasuryCap`](#0x2_coin_TreasuryCap)
-  [Struct `CurrencyCreated`](#0x2_coin_CurrencyCreated)
-  [Constants](#@Constants_0)
-  [Function `total_supply`](#0x2_coin_total_supply)
-  [Function `treasury_into_supply`](#0x2_coin_treasury_into_supply)
-  [Function `supply`](#0x2_coin_supply)
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
-  [Function `burn_`](#0x2_coin_burn_)


<pre><code><b>use</b> <a href="balance.md#0x2_balance">0x2::balance</a>;
<b>use</b> <a href="event.md#0x2_event">0x2::event</a>;
<b>use</b> <a href="object.md#0x2_object">0x2::object</a>;
<b>use</b> <a href="transfer.md#0x2_transfer">0x2::transfer</a>;
<b>use</b> <a href="tx_context.md#0x2_tx_context">0x2::tx_context</a>;
<b>use</b> <a href="types.md#0x2_types">0x2::types</a>;
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

Emitted when new currency is created through the <code>create_currency</code> call.
Contains currency metadata for off-chain discovery. Type parameter <code>T</code>
matches the one in <code><a href="coin.md#0x2_coin_Coin">Coin</a>&lt;T&gt;</code>


<pre><code><b>struct</b> <a href="coin.md#0x2_coin_CurrencyCreated">CurrencyCreated</a>&lt;T&gt; <b>has</b> <b>copy</b>, drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>decimals: u8</code>
</dt>
<dd>
 Number of decimal places the coin uses.
 A coin with <code>value </code> N and <code>decimals</code> D should be shown as N / 10^D
 E.g., a coin with <code>value</code> 7002 and decimals 3 should be displayed as 7.002
 This is metadata for display usage only.
</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0x2_coin_ENotEnough"></a>

For when trying to split a coin more times than its balance allows.


<pre><code><b>const</b> <a href="coin.md#0x2_coin_ENotEnough">ENotEnough</a>: u64 = 2;
</code></pre>



<a name="0x2_coin_EBadWitness"></a>

For when a type passed to create_supply is not a one-time witness.


<pre><code><b>const</b> <a href="coin.md#0x2_coin_EBadWitness">EBadWitness</a>: u64 = 0;
</code></pre>



<a name="0x2_coin_EInvalidArg"></a>

For when invalid arguments are passed to a function.


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

<a name="0x2_coin_supply"></a>

## Function `supply`

Get immutable reference to the treasury's <code>Supply</code>.


<pre><code><b>public</b> <b>fun</b> <a href="coin.md#0x2_coin_supply">supply</a>&lt;T&gt;(treasury: &<b>mut</b> <a href="coin.md#0x2_coin_TreasuryCap">coin::TreasuryCap</a>&lt;T&gt;): &<a href="balance.md#0x2_balance_Supply">balance::Supply</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="coin.md#0x2_coin_supply">supply</a>&lt;T&gt;(treasury: &<b>mut</b> <a href="coin.md#0x2_coin_TreasuryCap">TreasuryCap</a>&lt;T&gt;): &Supply&lt;T&gt; {
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
    <b>while</b> (i &lt; n - 1) {
        <a href="_push_back">vector::push_back</a>(&<b>mut</b> vec, <a href="coin.md#0x2_coin_split">split</a>(self, split_amount, ctx));
        i = i + 1;
    };
    vec
}
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


<pre><code><b>public</b> <b>fun</b> <a href="coin.md#0x2_coin_create_currency">create_currency</a>&lt;T: drop&gt;(witness: T, decimals: u8, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="coin.md#0x2_coin_TreasuryCap">coin::TreasuryCap</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="coin.md#0x2_coin_create_currency">create_currency</a>&lt;T: drop&gt;(
    witness: T,
    decimals: u8,
    ctx: &<b>mut</b> TxContext
): <a href="coin.md#0x2_coin_TreasuryCap">TreasuryCap</a>&lt;T&gt; {
    // Make sure there's only one instance of the type T
    <b>assert</b>!(sui::types::is_one_time_witness(&witness), <a href="coin.md#0x2_coin_EBadWitness">EBadWitness</a>);

    // Emit Currency metadata <b>as</b> an <a href="event.md#0x2_event">event</a>.
    <a href="event.md#0x2_event_emit">event::emit</a>(<a href="coin.md#0x2_coin_CurrencyCreated">CurrencyCreated</a>&lt;T&gt; {
        decimals
    });

    <a href="coin.md#0x2_coin_TreasuryCap">TreasuryCap</a> {
        id: <a href="object.md#0x2_object_new">object::new</a>(ctx),
        total_supply: <a href="balance.md#0x2_balance_create_supply">balance::create_supply</a>(witness)
    }
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

<a name="0x2_coin_burn"></a>

## Function `burn`

Destroy the coin <code>c</code> and decrease the total supply in <code>cap</code>
accordingly.


<pre><code><b>public</b> <b>fun</b> <a href="coin.md#0x2_coin_burn">burn</a>&lt;T&gt;(cap: &<b>mut</b> <a href="coin.md#0x2_coin_TreasuryCap">coin::TreasuryCap</a>&lt;T&gt;, c: <a href="coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="coin.md#0x2_coin_burn">burn</a>&lt;T&gt;(cap: &<b>mut</b> <a href="coin.md#0x2_coin_TreasuryCap">TreasuryCap</a>&lt;T&gt;, c: <a href="coin.md#0x2_coin_Coin">Coin</a>&lt;T&gt;): u64 {
    <b>let</b> <a href="coin.md#0x2_coin_Coin">Coin</a> { id, <a href="balance.md#0x2_balance">balance</a> } = c;
    <a href="object.md#0x2_object_delete">object::delete</a>(id);
    <a href="balance.md#0x2_balance_decrease_supply">balance::decrease_supply</a>(&<b>mut</b> cap.total_supply, <a href="balance.md#0x2_balance">balance</a>)
}
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
    <a href="transfer.md#0x2_transfer_transfer">transfer::transfer</a>(<a href="coin.md#0x2_coin_mint">mint</a>(c, amount, ctx), recipient)
}
</code></pre>



</details>

<a name="0x2_coin_burn_"></a>

## Function `burn_`

Burn a Coin and reduce the total_supply. Invokes <code><a href="coin.md#0x2_coin_burn">burn</a>()</code>.


<pre><code><b>public</b> entry <b>fun</b> <a href="coin.md#0x2_coin_burn_">burn_</a>&lt;T&gt;(c: &<b>mut</b> <a href="coin.md#0x2_coin_TreasuryCap">coin::TreasuryCap</a>&lt;T&gt;, <a href="coin.md#0x2_coin">coin</a>: <a href="coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="coin.md#0x2_coin_burn_">burn_</a>&lt;T&gt;(c: &<b>mut</b> <a href="coin.md#0x2_coin_TreasuryCap">TreasuryCap</a>&lt;T&gt;, <a href="coin.md#0x2_coin">coin</a>: <a href="coin.md#0x2_coin_Coin">Coin</a>&lt;T&gt;) {
    <a href="coin.md#0x2_coin_burn">burn</a>(c, <a href="coin.md#0x2_coin">coin</a>);
}
</code></pre>



</details>
