
<a name="0xdee9_clob"></a>

# Module `0xdee9::clob`

[DEPRECATED]
This module is deprecated and is no longer functional. Use the <code><a href="clob_v2.md#0xdee9_clob_v2">clob_v2</a></code>
module instead.

Legacy type definitions and public functions are kept due to package upgrade
constraints.


-  [Struct `PoolCreated`](#0xdee9_clob_PoolCreated)
-  [Struct `OrderPlacedV2`](#0xdee9_clob_OrderPlacedV2)
-  [Struct `OrderCanceled`](#0xdee9_clob_OrderCanceled)
-  [Struct `OrderFilledV2`](#0xdee9_clob_OrderFilledV2)
-  [Struct `Order`](#0xdee9_clob_Order)
-  [Struct `TickLevel`](#0xdee9_clob_TickLevel)
-  [Resource `Pool`](#0xdee9_clob_Pool)
-  [Struct `OrderPlaced`](#0xdee9_clob_OrderPlaced)
-  [Struct `OrderFilled`](#0xdee9_clob_OrderFilled)
-  [Constants](#@Constants_0)
-  [Function `create_account`](#0xdee9_clob_create_account)
-  [Function `create_pool`](#0xdee9_clob_create_pool)
-  [Function `deposit_base`](#0xdee9_clob_deposit_base)
-  [Function `deposit_quote`](#0xdee9_clob_deposit_quote)
-  [Function `withdraw_base`](#0xdee9_clob_withdraw_base)
-  [Function `withdraw_quote`](#0xdee9_clob_withdraw_quote)
-  [Function `swap_exact_base_for_quote`](#0xdee9_clob_swap_exact_base_for_quote)
-  [Function `swap_exact_quote_for_base`](#0xdee9_clob_swap_exact_quote_for_base)
-  [Function `place_market_order`](#0xdee9_clob_place_market_order)
-  [Function `place_limit_order`](#0xdee9_clob_place_limit_order)
-  [Function `cancel_order`](#0xdee9_clob_cancel_order)
-  [Function `cancel_all_orders`](#0xdee9_clob_cancel_all_orders)
-  [Function `batch_cancel_order`](#0xdee9_clob_batch_cancel_order)
-  [Function `list_open_orders`](#0xdee9_clob_list_open_orders)
-  [Function `account_balance`](#0xdee9_clob_account_balance)
-  [Function `get_market_price`](#0xdee9_clob_get_market_price)
-  [Function `get_level2_book_status_bid_side`](#0xdee9_clob_get_level2_book_status_bid_side)
-  [Function `get_level2_book_status_ask_side`](#0xdee9_clob_get_level2_book_status_ask_side)
-  [Function `get_order_status`](#0xdee9_clob_get_order_status)


<pre><code><b>use</b> <a href="">0x1::type_name</a>;
<b>use</b> <a href="../../../.././build/Sui/docs/balance.md#0x2_balance">0x2::balance</a>;
<b>use</b> <a href="../../../.././build/Sui/docs/clock.md#0x2_clock">0x2::clock</a>;
<b>use</b> <a href="../../../.././build/Sui/docs/coin.md#0x2_coin">0x2::coin</a>;
<b>use</b> <a href="../../../.././build/Sui/docs/linked_table.md#0x2_linked_table">0x2::linked_table</a>;
<b>use</b> <a href="../../../.././build/Sui/docs/object.md#0x2_object">0x2::object</a>;
<b>use</b> <a href="../../../.././build/Sui/docs/sui.md#0x2_sui">0x2::sui</a>;
<b>use</b> <a href="../../../.././build/Sui/docs/table.md#0x2_table">0x2::table</a>;
<b>use</b> <a href="../../../.././build/Sui/docs/tx_context.md#0x2_tx_context">0x2::tx_context</a>;
<b>use</b> <a href="critbit.md#0xdee9_critbit">0xdee9::critbit</a>;
<b>use</b> <a href="custodian.md#0xdee9_custodian">0xdee9::custodian</a>;
</code></pre>



<a name="0xdee9_clob_PoolCreated"></a>

## Struct `PoolCreated`



<pre><code><b>struct</b> <a href="clob.md#0xdee9_clob_PoolCreated">PoolCreated</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>pool_id: <a href="../../../.././build/Sui/docs/object.md#0x2_object_ID">object::ID</a></code>
</dt>
<dd>

</dd>
<dt>
<code>base_asset: <a href="_TypeName">type_name::TypeName</a></code>
</dt>
<dd>

</dd>
<dt>
<code>quote_asset: <a href="_TypeName">type_name::TypeName</a></code>
</dt>
<dd>

</dd>
<dt>
<code>taker_fee_rate: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>maker_rebate_rate: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>tick_size: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>lot_size: u64</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0xdee9_clob_OrderPlacedV2"></a>

## Struct `OrderPlacedV2`



<pre><code><b>struct</b> <a href="clob.md#0xdee9_clob_OrderPlacedV2">OrderPlacedV2</a>&lt;BaseAsset, QuoteAsset&gt; <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>pool_id: <a href="../../../.././build/Sui/docs/object.md#0x2_object_ID">object::ID</a></code>
</dt>
<dd>

</dd>
<dt>
<code>order_id: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>is_bid: bool</code>
</dt>
<dd>

</dd>
<dt>
<code>owner: <a href="../../../.././build/Sui/docs/object.md#0x2_object_ID">object::ID</a></code>
</dt>
<dd>

</dd>
<dt>
<code>base_asset_quantity_placed: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>price: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>expire_timestamp: u64</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0xdee9_clob_OrderCanceled"></a>

## Struct `OrderCanceled`



<pre><code><b>struct</b> <a href="clob.md#0xdee9_clob_OrderCanceled">OrderCanceled</a>&lt;BaseAsset, QuoteAsset&gt; <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>pool_id: <a href="../../../.././build/Sui/docs/object.md#0x2_object_ID">object::ID</a></code>
</dt>
<dd>

</dd>
<dt>
<code>order_id: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>is_bid: bool</code>
</dt>
<dd>

</dd>
<dt>
<code>owner: <a href="../../../.././build/Sui/docs/object.md#0x2_object_ID">object::ID</a></code>
</dt>
<dd>

</dd>
<dt>
<code>base_asset_quantity_canceled: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>price: u64</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0xdee9_clob_OrderFilledV2"></a>

## Struct `OrderFilledV2`



<pre><code><b>struct</b> <a href="clob.md#0xdee9_clob_OrderFilledV2">OrderFilledV2</a>&lt;BaseAsset, QuoteAsset&gt; <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>pool_id: <a href="../../../.././build/Sui/docs/object.md#0x2_object_ID">object::ID</a></code>
</dt>
<dd>

</dd>
<dt>
<code>order_id: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>is_bid: bool</code>
</dt>
<dd>

</dd>
<dt>
<code>owner: <a href="../../../.././build/Sui/docs/object.md#0x2_object_ID">object::ID</a></code>
</dt>
<dd>

</dd>
<dt>
<code>total_quantity: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>base_asset_quantity_filled: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>base_asset_quantity_remaining: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>price: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>taker_commission: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>maker_rebates: u64</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0xdee9_clob_Order"></a>

## Struct `Order`



<pre><code><b>struct</b> <a href="clob.md#0xdee9_clob_Order">Order</a> <b>has</b> drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>order_id: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>price: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>quantity: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>is_bid: bool</code>
</dt>
<dd>

</dd>
<dt>
<code>owner: <a href="../../../.././build/Sui/docs/object.md#0x2_object_ID">object::ID</a></code>
</dt>
<dd>

</dd>
<dt>
<code>expire_timestamp: u64</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0xdee9_clob_TickLevel"></a>

## Struct `TickLevel`



<pre><code><b>struct</b> <a href="clob.md#0xdee9_clob_TickLevel">TickLevel</a> <b>has</b> store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>price: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>open_orders: <a href="../../../.././build/Sui/docs/linked_table.md#0x2_linked_table_LinkedTable">linked_table::LinkedTable</a>&lt;u64, <a href="clob.md#0xdee9_clob_Order">clob::Order</a>&gt;</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0xdee9_clob_Pool"></a>

## Resource `Pool`



<pre><code><b>struct</b> <a href="clob.md#0xdee9_clob_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt; <b>has</b> key
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>id: <a href="../../../.././build/Sui/docs/object.md#0x2_object_UID">object::UID</a></code>
</dt>
<dd>

</dd>
<dt>
<code>bids: <a href="critbit.md#0xdee9_critbit_CritbitTree">critbit::CritbitTree</a>&lt;<a href="clob.md#0xdee9_clob_TickLevel">clob::TickLevel</a>&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>asks: <a href="critbit.md#0xdee9_critbit_CritbitTree">critbit::CritbitTree</a>&lt;<a href="clob.md#0xdee9_clob_TickLevel">clob::TickLevel</a>&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>next_bid_order_id: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>next_ask_order_id: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>usr_open_orders: <a href="../../../.././build/Sui/docs/table.md#0x2_table_Table">table::Table</a>&lt;<a href="../../../.././build/Sui/docs/object.md#0x2_object_ID">object::ID</a>, <a href="../../../.././build/Sui/docs/linked_table.md#0x2_linked_table_LinkedTable">linked_table::LinkedTable</a>&lt;u64, u64&gt;&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>taker_fee_rate: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>maker_rebate_rate: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>tick_size: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>lot_size: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>base_custodian: <a href="custodian.md#0xdee9_custodian_Custodian">custodian::Custodian</a>&lt;BaseAsset&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>quote_custodian: <a href="custodian.md#0xdee9_custodian_Custodian">custodian::Custodian</a>&lt;QuoteAsset&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>creation_fee: <a href="../../../.././build/Sui/docs/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="../../../.././build/Sui/docs/sui.md#0x2_sui_SUI">sui::SUI</a>&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>base_asset_trading_fees: <a href="../../../.././build/Sui/docs/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;BaseAsset&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>quote_asset_trading_fees: <a href="../../../.././build/Sui/docs/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;QuoteAsset&gt;</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0xdee9_clob_OrderPlaced"></a>

## Struct `OrderPlaced`

DEPRECATED


<pre><code><b>struct</b> <a href="clob.md#0xdee9_clob_OrderPlaced">OrderPlaced</a>&lt;BaseAsset, QuoteAsset&gt; <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>pool_id: <a href="../../../.././build/Sui/docs/object.md#0x2_object_ID">object::ID</a></code>
</dt>
<dd>

</dd>
<dt>
<code>order_id: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>is_bid: bool</code>
</dt>
<dd>

</dd>
<dt>
<code>owner: <a href="../../../.././build/Sui/docs/object.md#0x2_object_ID">object::ID</a></code>
</dt>
<dd>

</dd>
<dt>
<code>base_asset_quantity_placed: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>price: u64</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0xdee9_clob_OrderFilled"></a>

## Struct `OrderFilled`

DEPRECATED


<pre><code><b>struct</b> <a href="clob.md#0xdee9_clob_OrderFilled">OrderFilled</a>&lt;BaseAsset, QuoteAsset&gt; <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>pool_id: <a href="../../../.././build/Sui/docs/object.md#0x2_object_ID">object::ID</a></code>
</dt>
<dd>

</dd>
<dt>
<code>order_id: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>is_bid: bool</code>
</dt>
<dd>

</dd>
<dt>
<code>owner: <a href="../../../.././build/Sui/docs/object.md#0x2_object_ID">object::ID</a></code>
</dt>
<dd>

</dd>
<dt>
<code>total_quantity: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>base_asset_quantity_filled: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>base_asset_quantity_remaining: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>price: u64</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0xdee9_clob_EDeprecated"></a>



<pre><code><b>const</b> <a href="clob.md#0xdee9_clob_EDeprecated">EDeprecated</a>: u64 = 1337;
</code></pre>



<a name="0xdee9_clob_create_account"></a>

## Function `create_account`



<pre><code><b>public</b> <b>fun</b> <a href="clob.md#0xdee9_clob_create_account">create_account</a>(_ctx: &<b>mut</b> <a href="../../../.././build/Sui/docs/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="custodian.md#0xdee9_custodian_AccountCap">custodian::AccountCap</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="clob.md#0xdee9_clob_create_account">create_account</a>(_ctx: &<b>mut</b> TxContext): AccountCap {
    <b>abort</b> <a href="clob.md#0xdee9_clob_EDeprecated">EDeprecated</a>
}
</code></pre>



</details>

<a name="0xdee9_clob_create_pool"></a>

## Function `create_pool`



<pre><code><b>public</b> <b>fun</b> <a href="clob.md#0xdee9_clob_create_pool">create_pool</a>&lt;BaseAsset, QuoteAsset&gt;(_a: u64, _b: u64, _c: <a href="../../../.././build/Sui/docs/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;<a href="../../../.././build/Sui/docs/sui.md#0x2_sui_SUI">sui::SUI</a>&gt;, _ctx: &<b>mut</b> <a href="../../../.././build/Sui/docs/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="clob.md#0xdee9_clob_create_pool">create_pool</a>&lt;BaseAsset, QuoteAsset&gt;(_a: u64, _b: u64, _c: Coin&lt;SUI&gt;, _ctx: &<b>mut</b> TxContext) {
    <b>abort</b> <a href="clob.md#0xdee9_clob_EDeprecated">EDeprecated</a>
}
</code></pre>



</details>

<a name="0xdee9_clob_deposit_base"></a>

## Function `deposit_base`



<pre><code><b>public</b> <b>fun</b> <a href="clob.md#0xdee9_clob_deposit_base">deposit_base</a>&lt;BaseAsset, QuoteAsset&gt;(_pool: &<b>mut</b> <a href="clob.md#0xdee9_clob_Pool">clob::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, _coin: <a href="../../../.././build/Sui/docs/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;BaseAsset&gt;, _account_cap: &<a href="custodian.md#0xdee9_custodian_AccountCap">custodian::AccountCap</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="clob.md#0xdee9_clob_deposit_base">deposit_base</a>&lt;BaseAsset, QuoteAsset&gt;(
    _pool: &<b>mut</b> <a href="clob.md#0xdee9_clob_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    _coin: Coin&lt;BaseAsset&gt;,
    _account_cap: &AccountCap
) {
    <b>abort</b> <a href="clob.md#0xdee9_clob_EDeprecated">EDeprecated</a>
}
</code></pre>



</details>

<a name="0xdee9_clob_deposit_quote"></a>

## Function `deposit_quote`



<pre><code><b>public</b> <b>fun</b> <a href="clob.md#0xdee9_clob_deposit_quote">deposit_quote</a>&lt;BaseAsset, QuoteAsset&gt;(_pool: &<b>mut</b> <a href="clob.md#0xdee9_clob_Pool">clob::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, _coin: <a href="../../../.././build/Sui/docs/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;QuoteAsset&gt;, _account_cap: &<a href="custodian.md#0xdee9_custodian_AccountCap">custodian::AccountCap</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="clob.md#0xdee9_clob_deposit_quote">deposit_quote</a>&lt;BaseAsset, QuoteAsset&gt;(
    _pool: &<b>mut</b> <a href="clob.md#0xdee9_clob_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    _coin: Coin&lt;QuoteAsset&gt;,
    _account_cap: &AccountCap
) {
    <b>abort</b> <a href="clob.md#0xdee9_clob_EDeprecated">EDeprecated</a>
}
</code></pre>



</details>

<a name="0xdee9_clob_withdraw_base"></a>

## Function `withdraw_base`



<pre><code><b>public</b> <b>fun</b> <a href="clob.md#0xdee9_clob_withdraw_base">withdraw_base</a>&lt;BaseAsset, QuoteAsset&gt;(_pool: &<b>mut</b> <a href="clob.md#0xdee9_clob_Pool">clob::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, _quantity: u64, _account_cap: &<a href="custodian.md#0xdee9_custodian_AccountCap">custodian::AccountCap</a>, _ctx: &<b>mut</b> <a href="../../../.././build/Sui/docs/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="../../../.././build/Sui/docs/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;BaseAsset&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="clob.md#0xdee9_clob_withdraw_base">withdraw_base</a>&lt;BaseAsset, QuoteAsset&gt;(
    _pool: &<b>mut</b> <a href="clob.md#0xdee9_clob_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    _quantity: u64,
    _account_cap: &AccountCap,
    _ctx: &<b>mut</b> TxContext
): Coin&lt;BaseAsset&gt; {
    <b>abort</b> <a href="clob.md#0xdee9_clob_EDeprecated">EDeprecated</a>
}
</code></pre>



</details>

<a name="0xdee9_clob_withdraw_quote"></a>

## Function `withdraw_quote`



<pre><code><b>public</b> <b>fun</b> <a href="clob.md#0xdee9_clob_withdraw_quote">withdraw_quote</a>&lt;BaseAsset, QuoteAsset&gt;(_pool: &<b>mut</b> <a href="clob.md#0xdee9_clob_Pool">clob::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, _quantity: u64, _account_cap: &<a href="custodian.md#0xdee9_custodian_AccountCap">custodian::AccountCap</a>, _ctx: &<b>mut</b> <a href="../../../.././build/Sui/docs/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="../../../.././build/Sui/docs/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;QuoteAsset&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="clob.md#0xdee9_clob_withdraw_quote">withdraw_quote</a>&lt;BaseAsset, QuoteAsset&gt;(
    _pool: &<b>mut</b> <a href="clob.md#0xdee9_clob_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    _quantity: u64,
    _account_cap: &AccountCap,
    _ctx: &<b>mut</b> TxContext
): Coin&lt;QuoteAsset&gt; {
    <b>abort</b> <a href="clob.md#0xdee9_clob_EDeprecated">EDeprecated</a>
}
</code></pre>



</details>

<a name="0xdee9_clob_swap_exact_base_for_quote"></a>

## Function `swap_exact_base_for_quote`



<pre><code><b>public</b> <b>fun</b> <a href="clob.md#0xdee9_clob_swap_exact_base_for_quote">swap_exact_base_for_quote</a>&lt;BaseAsset, QuoteAsset&gt;(_pool: &<b>mut</b> <a href="clob.md#0xdee9_clob_Pool">clob::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, _quantity: u64, _base_coin: <a href="../../../.././build/Sui/docs/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;BaseAsset&gt;, _quote_coin: <a href="../../../.././build/Sui/docs/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;QuoteAsset&gt;, _clock: &<a href="../../../.././build/Sui/docs/clock.md#0x2_clock_Clock">clock::Clock</a>, _ctx: &<b>mut</b> <a href="../../../.././build/Sui/docs/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): (<a href="../../../.././build/Sui/docs/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;BaseAsset&gt;, <a href="../../../.././build/Sui/docs/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;QuoteAsset&gt;, u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="clob.md#0xdee9_clob_swap_exact_base_for_quote">swap_exact_base_for_quote</a>&lt;BaseAsset, QuoteAsset&gt;(
    _pool: &<b>mut</b> <a href="clob.md#0xdee9_clob_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    _quantity: u64,
    _base_coin: Coin&lt;BaseAsset&gt;,
    _quote_coin: Coin&lt;QuoteAsset&gt;,
    _clock: &Clock,
    _ctx: &<b>mut</b> TxContext,
): (Coin&lt;BaseAsset&gt;, Coin&lt;QuoteAsset&gt;, u64) {
    <b>abort</b> <a href="clob.md#0xdee9_clob_EDeprecated">EDeprecated</a>
}
</code></pre>



</details>

<a name="0xdee9_clob_swap_exact_quote_for_base"></a>

## Function `swap_exact_quote_for_base`



<pre><code><b>public</b> <b>fun</b> <a href="clob.md#0xdee9_clob_swap_exact_quote_for_base">swap_exact_quote_for_base</a>&lt;BaseAsset, QuoteAsset&gt;(_pool: &<b>mut</b> <a href="clob.md#0xdee9_clob_Pool">clob::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, _quantity: u64, _clock: &<a href="../../../.././build/Sui/docs/clock.md#0x2_clock_Clock">clock::Clock</a>, _quote_coin: <a href="../../../.././build/Sui/docs/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;QuoteAsset&gt;, _ctx: &<b>mut</b> <a href="../../../.././build/Sui/docs/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): (<a href="../../../.././build/Sui/docs/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;BaseAsset&gt;, <a href="../../../.././build/Sui/docs/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;QuoteAsset&gt;, u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="clob.md#0xdee9_clob_swap_exact_quote_for_base">swap_exact_quote_for_base</a>&lt;BaseAsset, QuoteAsset&gt;(
    _pool: &<b>mut</b> <a href="clob.md#0xdee9_clob_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    _quantity: u64,
    _clock: &Clock,
    _quote_coin: Coin&lt;QuoteAsset&gt;,
    _ctx: &<b>mut</b> TxContext,
): (Coin&lt;BaseAsset&gt;, Coin&lt;QuoteAsset&gt;, u64) {
    <b>abort</b> <a href="clob.md#0xdee9_clob_EDeprecated">EDeprecated</a>
}
</code></pre>



</details>

<a name="0xdee9_clob_place_market_order"></a>

## Function `place_market_order`



<pre><code><b>public</b> <b>fun</b> <a href="clob.md#0xdee9_clob_place_market_order">place_market_order</a>&lt;BaseAsset, QuoteAsset&gt;(_pool: &<b>mut</b> <a href="clob.md#0xdee9_clob_Pool">clob::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, _quantity: u64, _is_bid: bool, _base_coin: <a href="../../../.././build/Sui/docs/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;BaseAsset&gt;, _quote_coin: <a href="../../../.././build/Sui/docs/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;QuoteAsset&gt;, _clock: &<a href="../../../.././build/Sui/docs/clock.md#0x2_clock_Clock">clock::Clock</a>, _ctx: &<b>mut</b> <a href="../../../.././build/Sui/docs/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): (<a href="../../../.././build/Sui/docs/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;BaseAsset&gt;, <a href="../../../.././build/Sui/docs/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;QuoteAsset&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="clob.md#0xdee9_clob_place_market_order">place_market_order</a>&lt;BaseAsset, QuoteAsset&gt;(
    _pool: &<b>mut</b> <a href="clob.md#0xdee9_clob_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    _quantity: u64,
    _is_bid: bool,
    _base_coin: Coin&lt;BaseAsset&gt;,
    _quote_coin: Coin&lt;QuoteAsset&gt;,
    _clock: &Clock,
    _ctx: &<b>mut</b> TxContext,
): (Coin&lt;BaseAsset&gt;, Coin&lt;QuoteAsset&gt;) {
    <b>abort</b> <a href="clob.md#0xdee9_clob_EDeprecated">EDeprecated</a>
}
</code></pre>



</details>

<a name="0xdee9_clob_place_limit_order"></a>

## Function `place_limit_order`



<pre><code><b>public</b> <b>fun</b> <a href="clob.md#0xdee9_clob_place_limit_order">place_limit_order</a>&lt;BaseAsset, QuoteAsset&gt;(_pool: &<b>mut</b> <a href="clob.md#0xdee9_clob_Pool">clob::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, _price: u64, _quantity: u64, _is_bid: bool, _restriction: u8, _clock: &<a href="../../../.././build/Sui/docs/clock.md#0x2_clock_Clock">clock::Clock</a>, _account_cap: &<a href="custodian.md#0xdee9_custodian_AccountCap">custodian::AccountCap</a>, _ctx: &<b>mut</b> <a href="../../../.././build/Sui/docs/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): (u64, u64, bool, u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="clob.md#0xdee9_clob_place_limit_order">place_limit_order</a>&lt;BaseAsset, QuoteAsset&gt;(
    _pool: &<b>mut</b> <a href="clob.md#0xdee9_clob_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    _price: u64,
    _quantity: u64,
    _is_bid: bool,
    _restriction: u8,
    _clock: &Clock,
    _account_cap: &AccountCap,
    _ctx: &<b>mut</b> TxContext
): (u64, u64, bool, u64) {
    <b>abort</b> <a href="clob.md#0xdee9_clob_EDeprecated">EDeprecated</a>
}
</code></pre>



</details>

<a name="0xdee9_clob_cancel_order"></a>

## Function `cancel_order`



<pre><code><b>public</b> <b>fun</b> <a href="clob.md#0xdee9_clob_cancel_order">cancel_order</a>&lt;BaseAsset, QuoteAsset&gt;(_pool: &<b>mut</b> <a href="clob.md#0xdee9_clob_Pool">clob::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, _order_id: u64, _account_cap: &<a href="custodian.md#0xdee9_custodian_AccountCap">custodian::AccountCap</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="clob.md#0xdee9_clob_cancel_order">cancel_order</a>&lt;BaseAsset, QuoteAsset&gt;(
    _pool: &<b>mut</b> <a href="clob.md#0xdee9_clob_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    _order_id: u64,
    _account_cap: &AccountCap
) {
    <b>abort</b> <a href="clob.md#0xdee9_clob_EDeprecated">EDeprecated</a>
}
</code></pre>



</details>

<a name="0xdee9_clob_cancel_all_orders"></a>

## Function `cancel_all_orders`



<pre><code><b>public</b> <b>fun</b> <a href="clob.md#0xdee9_clob_cancel_all_orders">cancel_all_orders</a>&lt;BaseAsset, QuoteAsset&gt;(_pool: &<b>mut</b> <a href="clob.md#0xdee9_clob_Pool">clob::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, _account_cap: &<a href="custodian.md#0xdee9_custodian_AccountCap">custodian::AccountCap</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="clob.md#0xdee9_clob_cancel_all_orders">cancel_all_orders</a>&lt;BaseAsset, QuoteAsset&gt;(
    _pool: &<b>mut</b> <a href="clob.md#0xdee9_clob_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    _account_cap: &AccountCap
) {
    <b>abort</b> <a href="clob.md#0xdee9_clob_EDeprecated">EDeprecated</a>
}
</code></pre>



</details>

<a name="0xdee9_clob_batch_cancel_order"></a>

## Function `batch_cancel_order`



<pre><code><b>public</b> <b>fun</b> <a href="clob.md#0xdee9_clob_batch_cancel_order">batch_cancel_order</a>&lt;BaseAsset, QuoteAsset&gt;(_pool: &<b>mut</b> <a href="clob.md#0xdee9_clob_Pool">clob::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, _order_ids: <a href="">vector</a>&lt;u64&gt;, _account_cap: &<a href="custodian.md#0xdee9_custodian_AccountCap">custodian::AccountCap</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="clob.md#0xdee9_clob_batch_cancel_order">batch_cancel_order</a>&lt;BaseAsset, QuoteAsset&gt;(
    _pool: &<b>mut</b> <a href="clob.md#0xdee9_clob_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    _order_ids: <a href="">vector</a>&lt;u64&gt;,
    _account_cap: &AccountCap
) {
    <b>abort</b> <a href="clob.md#0xdee9_clob_EDeprecated">EDeprecated</a>
}
</code></pre>



</details>

<a name="0xdee9_clob_list_open_orders"></a>

## Function `list_open_orders`



<pre><code><b>public</b> <b>fun</b> <a href="clob.md#0xdee9_clob_list_open_orders">list_open_orders</a>&lt;BaseAsset, QuoteAsset&gt;(_pool: &<a href="clob.md#0xdee9_clob_Pool">clob::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, _account_cap: &<a href="custodian.md#0xdee9_custodian_AccountCap">custodian::AccountCap</a>): <a href="">vector</a>&lt;<a href="clob.md#0xdee9_clob_Order">clob::Order</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="clob.md#0xdee9_clob_list_open_orders">list_open_orders</a>&lt;BaseAsset, QuoteAsset&gt;(
    _pool: &<a href="clob.md#0xdee9_clob_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    _account_cap: &AccountCap
): <a href="">vector</a>&lt;<a href="clob.md#0xdee9_clob_Order">Order</a>&gt; {
    <b>abort</b> <a href="clob.md#0xdee9_clob_EDeprecated">EDeprecated</a>
}
</code></pre>



</details>

<a name="0xdee9_clob_account_balance"></a>

## Function `account_balance`



<pre><code><b>public</b> <b>fun</b> <a href="clob.md#0xdee9_clob_account_balance">account_balance</a>&lt;BaseAsset, QuoteAsset&gt;(_pool: &<a href="clob.md#0xdee9_clob_Pool">clob::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, _account_cap: &<a href="custodian.md#0xdee9_custodian_AccountCap">custodian::AccountCap</a>): (u64, u64, u64, u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="clob.md#0xdee9_clob_account_balance">account_balance</a>&lt;BaseAsset, QuoteAsset&gt;(
    _pool: &<a href="clob.md#0xdee9_clob_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    _account_cap: &AccountCap
): (u64, u64, u64, u64) {
    <b>abort</b> <a href="clob.md#0xdee9_clob_EDeprecated">EDeprecated</a>
}
</code></pre>



</details>

<a name="0xdee9_clob_get_market_price"></a>

## Function `get_market_price`



<pre><code><b>public</b> <b>fun</b> <a href="clob.md#0xdee9_clob_get_market_price">get_market_price</a>&lt;BaseAsset, QuoteAsset&gt;(_pool: &<a href="clob.md#0xdee9_clob_Pool">clob::Pool</a>&lt;BaseAsset, QuoteAsset&gt;): (u64, u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="clob.md#0xdee9_clob_get_market_price">get_market_price</a>&lt;BaseAsset, QuoteAsset&gt;(
    _pool: &<a href="clob.md#0xdee9_clob_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;
): (u64, u64){
    <b>abort</b> <a href="clob.md#0xdee9_clob_EDeprecated">EDeprecated</a>
}
</code></pre>



</details>

<a name="0xdee9_clob_get_level2_book_status_bid_side"></a>

## Function `get_level2_book_status_bid_side`



<pre><code><b>public</b> <b>fun</b> <a href="clob.md#0xdee9_clob_get_level2_book_status_bid_side">get_level2_book_status_bid_side</a>&lt;BaseAsset, QuoteAsset&gt;(_pool: &<a href="clob.md#0xdee9_clob_Pool">clob::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, _price_low: u64, _price_high: u64, _clock: &<a href="../../../.././build/Sui/docs/clock.md#0x2_clock_Clock">clock::Clock</a>): (<a href="">vector</a>&lt;u64&gt;, <a href="">vector</a>&lt;u64&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="clob.md#0xdee9_clob_get_level2_book_status_bid_side">get_level2_book_status_bid_side</a>&lt;BaseAsset, QuoteAsset&gt;(
    _pool: &<a href="clob.md#0xdee9_clob_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    _price_low: u64,
    _price_high: u64,
    _clock: &Clock
): (<a href="">vector</a>&lt;u64&gt;, <a href="">vector</a>&lt;u64&gt;) {
    <b>abort</b> <a href="clob.md#0xdee9_clob_EDeprecated">EDeprecated</a>
}
</code></pre>



</details>

<a name="0xdee9_clob_get_level2_book_status_ask_side"></a>

## Function `get_level2_book_status_ask_side`



<pre><code><b>public</b> <b>fun</b> <a href="clob.md#0xdee9_clob_get_level2_book_status_ask_side">get_level2_book_status_ask_side</a>&lt;BaseAsset, QuoteAsset&gt;(_pool: &<a href="clob.md#0xdee9_clob_Pool">clob::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, _price_low: u64, _price_high: u64, _clock: &<a href="../../../.././build/Sui/docs/clock.md#0x2_clock_Clock">clock::Clock</a>): (<a href="">vector</a>&lt;u64&gt;, <a href="">vector</a>&lt;u64&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="clob.md#0xdee9_clob_get_level2_book_status_ask_side">get_level2_book_status_ask_side</a>&lt;BaseAsset, QuoteAsset&gt;(
    _pool: &<a href="clob.md#0xdee9_clob_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    _price_low: u64,
    _price_high: u64,
    _clock: &Clock
): (<a href="">vector</a>&lt;u64&gt;, <a href="">vector</a>&lt;u64&gt;) {
    <b>abort</b> <a href="clob.md#0xdee9_clob_EDeprecated">EDeprecated</a>
}
</code></pre>



</details>

<a name="0xdee9_clob_get_order_status"></a>

## Function `get_order_status`



<pre><code><b>public</b> <b>fun</b> <a href="clob.md#0xdee9_clob_get_order_status">get_order_status</a>&lt;BaseAsset, QuoteAsset&gt;(_pool: &<a href="clob.md#0xdee9_clob_Pool">clob::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, _order_id: u64, _account_cap: &<a href="custodian.md#0xdee9_custodian_AccountCap">custodian::AccountCap</a>): &<a href="clob.md#0xdee9_clob_Order">clob::Order</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="clob.md#0xdee9_clob_get_order_status">get_order_status</a>&lt;BaseAsset, QuoteAsset&gt;(
    _pool: &<a href="clob.md#0xdee9_clob_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    _order_id: u64,
    _account_cap: &AccountCap
): &<a href="clob.md#0xdee9_clob_Order">Order</a> {
    <b>abort</b> <a href="clob.md#0xdee9_clob_EDeprecated">EDeprecated</a>
}
</code></pre>



</details>
