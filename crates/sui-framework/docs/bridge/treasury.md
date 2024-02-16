
<a name="0xb_treasury"></a>

# Module `0xb::treasury`



-  [Struct `BridgeTreasury`](#0xb_treasury_BridgeTreasury)
-  [Constants](#@Constants_0)
-  [Function `token_id`](#0xb_treasury_token_id)
-  [Function `create`](#0xb_treasury_create)
-  [Function `burn`](#0xb_treasury_burn)
-  [Function `mint`](#0xb_treasury_mint)
-  [Function `create_treasury_if_not_exist`](#0xb_treasury_create_treasury_if_not_exist)


<pre><code><b>use</b> <a href="dependencies/move-stdlib/type_name.md#0x1_type_name">0x1::type_name</a>;
<b>use</b> <a href="dependencies/sui-framework/coin.md#0x2_coin">0x2::coin</a>;
<b>use</b> <a href="dependencies/sui-framework/object_bag.md#0x2_object_bag">0x2::object_bag</a>;
<b>use</b> <a href="dependencies/sui-framework/tx_context.md#0x2_tx_context">0x2::tx_context</a>;
<b>use</b> <a href="btc.md#0xb_btc">0xb::btc</a>;
<b>use</b> <a href="eth.md#0xb_eth">0xb::eth</a>;
<b>use</b> <a href="usdc.md#0xb_usdc">0xb::usdc</a>;
<b>use</b> <a href="usdt.md#0xb_usdt">0xb::usdt</a>;
</code></pre>



<a name="0xb_treasury_BridgeTreasury"></a>

## Struct `BridgeTreasury`



<pre><code><b>struct</b> <a href="treasury.md#0xb_treasury_BridgeTreasury">BridgeTreasury</a> <b>has</b> store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>treasuries: <a href="dependencies/sui-framework/object_bag.md#0x2_object_bag_ObjectBag">object_bag::ObjectBag</a></code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0xb_treasury_ENotSystemAddress"></a>



<pre><code><b>const</b> <a href="treasury.md#0xb_treasury_ENotSystemAddress">ENotSystemAddress</a>: u64 = 1;
</code></pre>



<a name="0xb_treasury_EUnsupportedTokenType"></a>



<pre><code><b>const</b> <a href="treasury.md#0xb_treasury_EUnsupportedTokenType">EUnsupportedTokenType</a>: u64 = 0;
</code></pre>



<a name="0xb_treasury_token_id"></a>

## Function `token_id`



<pre><code><b>public</b> <b>fun</b> <a href="treasury.md#0xb_treasury_token_id">token_id</a>&lt;T&gt;(): u8
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="treasury.md#0xb_treasury_token_id">token_id</a>&lt;T&gt;(): u8 {
    <b>let</b> coin_type = <a href="dependencies/move-stdlib/type_name.md#0x1_type_name_get">type_name::get</a>&lt;T&gt;();
    <b>if</b> (coin_type == <a href="dependencies/move-stdlib/type_name.md#0x1_type_name_get">type_name::get</a>&lt;BTC&gt;()) {
        1
    } <b>else</b> <b>if</b> (coin_type == <a href="dependencies/move-stdlib/type_name.md#0x1_type_name_get">type_name::get</a>&lt;ETH&gt;()) {
        2
    } <b>else</b> <b>if</b> (coin_type == <a href="dependencies/move-stdlib/type_name.md#0x1_type_name_get">type_name::get</a>&lt;USDC&gt;()) {
        3
    } <b>else</b> <b>if</b> (coin_type == <a href="dependencies/move-stdlib/type_name.md#0x1_type_name_get">type_name::get</a>&lt;USDT&gt;()) {
        4
    } <b>else</b> {
        <b>abort</b> <a href="treasury.md#0xb_treasury_EUnsupportedTokenType">EUnsupportedTokenType</a>
    }
}
</code></pre>



</details>

<a name="0xb_treasury_create"></a>

## Function `create`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="treasury.md#0xb_treasury_create">create</a>(ctx: &<b>mut</b> <a href="dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="treasury.md#0xb_treasury_BridgeTreasury">treasury::BridgeTreasury</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="treasury.md#0xb_treasury_create">create</a>(ctx: &<b>mut</b> TxContext): <a href="treasury.md#0xb_treasury_BridgeTreasury">BridgeTreasury</a> {
    <b>assert</b>!(<a href="dependencies/sui-framework/tx_context.md#0x2_tx_context_sender">tx_context::sender</a>(ctx) == @0x0, <a href="treasury.md#0xb_treasury_ENotSystemAddress">ENotSystemAddress</a>);
    <a href="treasury.md#0xb_treasury_BridgeTreasury">BridgeTreasury</a> {
        treasuries: <a href="dependencies/sui-framework/object_bag.md#0x2_object_bag_new">object_bag::new</a>(ctx)
    }
}
</code></pre>



</details>

<a name="0xb_treasury_burn"></a>

## Function `burn`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="treasury.md#0xb_treasury_burn">burn</a>&lt;T&gt;(self: &<b>mut</b> <a href="treasury.md#0xb_treasury_BridgeTreasury">treasury::BridgeTreasury</a>, token: <a href="dependencies/sui-framework/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;, ctx: &<b>mut</b> <a href="dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="treasury.md#0xb_treasury_burn">burn</a>&lt;T&gt;(self: &<b>mut</b> <a href="treasury.md#0xb_treasury_BridgeTreasury">BridgeTreasury</a>, token: Coin&lt;T&gt;, ctx: &<b>mut</b> TxContext) {
    <a href="treasury.md#0xb_treasury_create_treasury_if_not_exist">create_treasury_if_not_exist</a>&lt;T&gt;(self, ctx);
    <b>let</b> <a href="treasury.md#0xb_treasury">treasury</a> = <a href="dependencies/sui-framework/object_bag.md#0x2_object_bag_borrow_mut">object_bag::borrow_mut</a>(&<b>mut</b> self.treasuries, <a href="dependencies/move-stdlib/type_name.md#0x1_type_name_get">type_name::get</a>&lt;T&gt;());
    <a href="dependencies/sui-framework/coin.md#0x2_coin_burn">coin::burn</a>(<a href="treasury.md#0xb_treasury">treasury</a>, token);
}
</code></pre>



</details>

<a name="0xb_treasury_mint"></a>

## Function `mint`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="treasury.md#0xb_treasury_mint">mint</a>&lt;T&gt;(self: &<b>mut</b> <a href="treasury.md#0xb_treasury_BridgeTreasury">treasury::BridgeTreasury</a>, amount: u64, ctx: &<b>mut</b> <a href="dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="dependencies/sui-framework/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="treasury.md#0xb_treasury_mint">mint</a>&lt;T&gt;(self: &<b>mut</b> <a href="treasury.md#0xb_treasury_BridgeTreasury">BridgeTreasury</a>, amount: u64, ctx: &<b>mut</b> TxContext): Coin&lt;T&gt; {
    <a href="treasury.md#0xb_treasury_create_treasury_if_not_exist">create_treasury_if_not_exist</a>&lt;T&gt;(self, ctx);
    <b>let</b> <a href="treasury.md#0xb_treasury">treasury</a> = <a href="dependencies/sui-framework/object_bag.md#0x2_object_bag_borrow_mut">object_bag::borrow_mut</a>(&<b>mut</b> self.treasuries, <a href="dependencies/move-stdlib/type_name.md#0x1_type_name_get">type_name::get</a>&lt;T&gt;());
    <a href="dependencies/sui-framework/coin.md#0x2_coin_mint">coin::mint</a>(<a href="treasury.md#0xb_treasury">treasury</a>, amount, ctx)
}
</code></pre>



</details>

<a name="0xb_treasury_create_treasury_if_not_exist"></a>

## Function `create_treasury_if_not_exist`



<pre><code><b>fun</b> <a href="treasury.md#0xb_treasury_create_treasury_if_not_exist">create_treasury_if_not_exist</a>&lt;T&gt;(self: &<b>mut</b> <a href="treasury.md#0xb_treasury_BridgeTreasury">treasury::BridgeTreasury</a>, ctx: &<b>mut</b> <a href="dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="treasury.md#0xb_treasury_create_treasury_if_not_exist">create_treasury_if_not_exist</a>&lt;T&gt;(self: &<b>mut</b> <a href="treasury.md#0xb_treasury_BridgeTreasury">BridgeTreasury</a>, ctx: &<b>mut</b> TxContext) {
    <b>let</b> type = <a href="dependencies/move-stdlib/type_name.md#0x1_type_name_get">type_name::get</a>&lt;T&gt;();
    <b>if</b> (!<a href="dependencies/sui-framework/object_bag.md#0x2_object_bag_contains">object_bag::contains</a>(&self.treasuries, type)) {
        // Lazily create currency <b>if</b> not exists
        <b>if</b> (type == <a href="dependencies/move-stdlib/type_name.md#0x1_type_name_get">type_name::get</a>&lt;BTC&gt;()) {
            <a href="dependencies/sui-framework/object_bag.md#0x2_object_bag_add">object_bag::add</a>(&<b>mut</b> self.treasuries, type, <a href="btc.md#0xb_btc_create">btc::create</a>(ctx));
        } <b>else</b> <b>if</b> (type == <a href="dependencies/move-stdlib/type_name.md#0x1_type_name_get">type_name::get</a>&lt;ETH&gt;()) {
            <a href="dependencies/sui-framework/object_bag.md#0x2_object_bag_add">object_bag::add</a>(&<b>mut</b> self.treasuries, type, <a href="eth.md#0xb_eth_create">eth::create</a>(ctx));
        } <b>else</b> <b>if</b> (type == <a href="dependencies/move-stdlib/type_name.md#0x1_type_name_get">type_name::get</a>&lt;USDC&gt;()) {
            <a href="dependencies/sui-framework/object_bag.md#0x2_object_bag_add">object_bag::add</a>(&<b>mut</b> self.treasuries, type, <a href="usdc.md#0xb_usdc_create">usdc::create</a>(ctx));
        } <b>else</b> <b>if</b> (type == <a href="dependencies/move-stdlib/type_name.md#0x1_type_name_get">type_name::get</a>&lt;USDT&gt;()) {
            <a href="dependencies/sui-framework/object_bag.md#0x2_object_bag_add">object_bag::add</a>(&<b>mut</b> self.treasuries, type, <a href="usdt.md#0xb_usdt_create">usdt::create</a>(ctx));
        } <b>else</b> {
            <b>abort</b> <a href="treasury.md#0xb_treasury_EUnsupportedTokenType">EUnsupportedTokenType</a>
        };
    };
}
</code></pre>



</details>
