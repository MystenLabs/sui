---
title: Module `deepbook::clob`
---



-  [Struct `PoolCreated`](#deepbook_clob_PoolCreated)
-  [Struct `OrderPlacedV2`](#deepbook_clob_OrderPlacedV2)
-  [Struct `OrderCanceled`](#deepbook_clob_OrderCanceled)
-  [Struct `OrderFilledV2`](#deepbook_clob_OrderFilledV2)
-  [Struct `Order`](#deepbook_clob_Order)
-  [Struct `TickLevel`](#deepbook_clob_TickLevel)
-  [Struct `Pool`](#deepbook_clob_Pool)
-  [Struct `OrderPlaced`](#deepbook_clob_OrderPlaced)
-  [Struct `OrderFilled`](#deepbook_clob_OrderFilled)
-  [Constants](#@Constants_0)
-  [Function `destroy_empty_level`](#deepbook_clob_destroy_empty_level)
-  [Function `create_account`](#deepbook_clob_create_account)
-  [Function `create_pool`](#deepbook_clob_create_pool)
-  [Function `deposit_base`](#deepbook_clob_deposit_base)
-  [Function `deposit_quote`](#deepbook_clob_deposit_quote)
-  [Function `withdraw_base`](#deepbook_clob_withdraw_base)
-  [Function `withdraw_quote`](#deepbook_clob_withdraw_quote)
-  [Function `swap_exact_base_for_quote`](#deepbook_clob_swap_exact_base_for_quote)
-  [Function `swap_exact_quote_for_base`](#deepbook_clob_swap_exact_quote_for_base)
-  [Function `place_market_order`](#deepbook_clob_place_market_order)
-  [Function `place_limit_order`](#deepbook_clob_place_limit_order)
-  [Function `order_is_bid`](#deepbook_clob_order_is_bid)
-  [Function `emit_order_canceled`](#deepbook_clob_emit_order_canceled)
-  [Function `cancel_order`](#deepbook_clob_cancel_order)
-  [Function `remove_order`](#deepbook_clob_remove_order)
-  [Function `cancel_all_orders`](#deepbook_clob_cancel_all_orders)
-  [Function `batch_cancel_order`](#deepbook_clob_batch_cancel_order)
-  [Function `list_open_orders`](#deepbook_clob_list_open_orders)
-  [Function `account_balance`](#deepbook_clob_account_balance)
-  [Function `get_market_price`](#deepbook_clob_get_market_price)
-  [Function `get_level2_book_status_bid_side`](#deepbook_clob_get_level2_book_status_bid_side)
-  [Function `get_level2_book_status_ask_side`](#deepbook_clob_get_level2_book_status_ask_side)
-  [Function `get_level2_book_status`](#deepbook_clob_get_level2_book_status)
-  [Function `get_order_status`](#deepbook_clob_get_order_status)


<pre><code><b>use</b> <a href="../deepbook/critbit.md#deepbook_critbit">deepbook::critbit</a>;
<b>use</b> <a href="../deepbook/custodian.md#deepbook_custodian">deepbook::custodian</a>;
<b>use</b> <a href="../deepbook/math.md#deepbook_math">deepbook::math</a>;
<b>use</b> <a href="../std/address.md#std_address">std::address</a>;
<b>use</b> <a href="../std/ascii.md#std_ascii">std::ascii</a>;
<b>use</b> <a href="../std/bcs.md#std_bcs">std::bcs</a>;
<b>use</b> <a href="../std/option.md#std_option">std::option</a>;
<b>use</b> <a href="../std/string.md#std_string">std::string</a>;
<b>use</b> <a href="../std/type_name.md#std_type_name">std::type_name</a>;
<b>use</b> <a href="../std/vector.md#std_vector">std::vector</a>;
<b>use</b> <a href="../sui/address.md#sui_address">sui::address</a>;
<b>use</b> <a href="../sui/bag.md#sui_bag">sui::bag</a>;
<b>use</b> <a href="../sui/balance.md#sui_balance">sui::balance</a>;
<b>use</b> <a href="../sui/clock.md#sui_clock">sui::clock</a>;
<b>use</b> <a href="../sui/coin.md#sui_coin">sui::coin</a>;
<b>use</b> <a href="../sui/config.md#sui_config">sui::config</a>;
<b>use</b> <a href="../sui/deny_list.md#sui_deny_list">sui::deny_list</a>;
<b>use</b> <a href="../sui/dynamic_field.md#sui_dynamic_field">sui::dynamic_field</a>;
<b>use</b> <a href="../sui/dynamic_object_field.md#sui_dynamic_object_field">sui::dynamic_object_field</a>;
<b>use</b> <a href="../sui/event.md#sui_event">sui::event</a>;
<b>use</b> <a href="../sui/hex.md#sui_hex">sui::hex</a>;
<b>use</b> <a href="../sui/linked_table.md#sui_linked_table">sui::linked_table</a>;
<b>use</b> <a href="../sui/object.md#sui_object">sui::object</a>;
<b>use</b> <a href="../sui/sui.md#sui_sui">sui::sui</a>;
<b>use</b> <a href="../sui/table.md#sui_table">sui::table</a>;
<b>use</b> <a href="../sui/transfer.md#sui_transfer">sui::transfer</a>;
<b>use</b> <a href="../sui/tx_context.md#sui_tx_context">sui::tx_context</a>;
<b>use</b> <a href="../sui/types.md#sui_types">sui::types</a>;
<b>use</b> <a href="../sui/url.md#sui_url">sui::url</a>;
<b>use</b> <a href="../sui/vec_set.md#sui_vec_set">sui::vec_set</a>;
</code></pre>



<a name="deepbook_clob_PoolCreated"></a>

## Struct `PoolCreated`

Emitted when a new pool is created


<pre><code><b>public</b> <b>struct</b> <a href="../deepbook/clob.md#deepbook_clob_PoolCreated">PoolCreated</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>pool_id: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a></code>
</dt>
<dd>
 object ID of the newly created pool
</dd>
<dt>
<code>base_asset: <a href="../std/type_name.md#std_type_name_TypeName">std::type_name::TypeName</a></code>
</dt>
<dd>
</dd>
<dt>
<code>quote_asset: <a href="../std/type_name.md#std_type_name_TypeName">std::type_name::TypeName</a></code>
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

<a name="deepbook_clob_OrderPlacedV2"></a>

## Struct `OrderPlacedV2`

Emitted when a maker order is injected into the order book.


<pre><code><b>public</b> <b>struct</b> <a href="../deepbook/clob.md#deepbook_clob_OrderPlacedV2">OrderPlacedV2</a>&lt;<b>phantom</b> BaseAsset, <b>phantom</b> QuoteAsset&gt; <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>pool_id: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a></code>
</dt>
<dd>
 object ID of the pool the order was placed on
</dd>
<dt>
<code>order_id: u64</code>
</dt>
<dd>
 ID of the order within the pool
</dd>
<dt>
<code>is_bid: bool</code>
</dt>
<dd>
</dd>
<dt>
<code>owner: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a></code>
</dt>
<dd>
 object ID of the <code>AccountCap</code> that placed the order
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

<a name="deepbook_clob_OrderCanceled"></a>

## Struct `OrderCanceled`

Emitted when a maker order is canceled.


<pre><code><b>public</b> <b>struct</b> <a href="../deepbook/clob.md#deepbook_clob_OrderCanceled">OrderCanceled</a>&lt;<b>phantom</b> BaseAsset, <b>phantom</b> QuoteAsset&gt; <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>pool_id: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a></code>
</dt>
<dd>
 object ID of the pool the order was placed on
</dd>
<dt>
<code>order_id: u64</code>
</dt>
<dd>
 ID of the order within the pool
</dd>
<dt>
<code>is_bid: bool</code>
</dt>
<dd>
</dd>
<dt>
<code>owner: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a></code>
</dt>
<dd>
 object ID of the <code>AccountCap</code> that placed the order
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

<a name="deepbook_clob_OrderFilledV2"></a>

## Struct `OrderFilledV2`

Emitted only when a maker order is filled.


<pre><code><b>public</b> <b>struct</b> <a href="../deepbook/clob.md#deepbook_clob_OrderFilledV2">OrderFilledV2</a>&lt;<b>phantom</b> BaseAsset, <b>phantom</b> QuoteAsset&gt; <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>pool_id: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a></code>
</dt>
<dd>
 object ID of the pool the order was placed on
</dd>
<dt>
<code>order_id: u64</code>
</dt>
<dd>
 ID of the order within the pool
</dd>
<dt>
<code>is_bid: bool</code>
</dt>
<dd>
</dd>
<dt>
<code>owner: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a></code>
</dt>
<dd>
 object ID of the <code>AccountCap</code> that placed the order
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

<a name="deepbook_clob_Order"></a>

## Struct `Order`



<pre><code><b>public</b> <b>struct</b> <a href="../deepbook/clob.md#deepbook_clob_Order">Order</a> <b>has</b> drop, store
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
<code>owner: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a></code>
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

<a name="deepbook_clob_TickLevel"></a>

## Struct `TickLevel`



<pre><code><b>public</b> <b>struct</b> <a href="../deepbook/clob.md#deepbook_clob_TickLevel">TickLevel</a> <b>has</b> store
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
<code>open_orders: <a href="../sui/linked_table.md#sui_linked_table_LinkedTable">sui::linked_table::LinkedTable</a>&lt;u64, <a href="../deepbook/clob.md#deepbook_clob_Order">deepbook::clob::Order</a>&gt;</code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="deepbook_clob_Pool"></a>

## Struct `Pool`



<pre><code><b>public</b> <b>struct</b> <a href="../deepbook/clob.md#deepbook_clob_Pool">Pool</a>&lt;<b>phantom</b> BaseAsset, <b>phantom</b> QuoteAsset&gt; <b>has</b> key
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>id: <a href="../sui/object.md#sui_object_UID">sui::object::UID</a></code>
</dt>
<dd>
</dd>
<dt>
<code>bids: <a href="../deepbook/critbit.md#deepbook_critbit_CritbitTree">deepbook::critbit::CritbitTree</a>&lt;<a href="../deepbook/clob.md#deepbook_clob_TickLevel">deepbook::clob::TickLevel</a>&gt;</code>
</dt>
<dd>
</dd>
<dt>
<code>asks: <a href="../deepbook/critbit.md#deepbook_critbit_CritbitTree">deepbook::critbit::CritbitTree</a>&lt;<a href="../deepbook/clob.md#deepbook_clob_TickLevel">deepbook::clob::TickLevel</a>&gt;</code>
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
<code>usr_open_orders: <a href="../sui/table.md#sui_table_Table">sui::table::Table</a>&lt;<a href="../sui/object.md#sui_object_ID">sui::object::ID</a>, <a href="../sui/linked_table.md#sui_linked_table_LinkedTable">sui::linked_table::LinkedTable</a>&lt;u64, u64&gt;&gt;</code>
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
<code>base_custodian: <a href="../deepbook/custodian.md#deepbook_custodian_Custodian">deepbook::custodian::Custodian</a>&lt;BaseAsset&gt;</code>
</dt>
<dd>
</dd>
<dt>
<code>quote_custodian: <a href="../deepbook/custodian.md#deepbook_custodian_Custodian">deepbook::custodian::Custodian</a>&lt;QuoteAsset&gt;</code>
</dt>
<dd>
</dd>
<dt>
<code>creation_fee: <a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;<a href="../sui/sui.md#sui_sui_SUI">sui::sui::SUI</a>&gt;</code>
</dt>
<dd>
</dd>
<dt>
<code>base_asset_trading_fees: <a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;BaseAsset&gt;</code>
</dt>
<dd>
</dd>
<dt>
<code>quote_asset_trading_fees: <a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;QuoteAsset&gt;</code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="deepbook_clob_OrderPlaced"></a>

## Struct `OrderPlaced`

Deprecated since v1.0.0, use <code><a href="../deepbook/clob.md#deepbook_clob_OrderPlacedV2">OrderPlacedV2</a></code> instead.


<pre><code><b>public</b> <b>struct</b> <a href="../deepbook/clob.md#deepbook_clob_OrderPlaced">OrderPlaced</a>&lt;<b>phantom</b> BaseAsset, <b>phantom</b> QuoteAsset&gt; <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>pool_id: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a></code>
</dt>
<dd>
 object ID of the pool the order was placed on
</dd>
<dt>
<code>order_id: u64</code>
</dt>
<dd>
 ID of the order within the pool
</dd>
<dt>
<code>is_bid: bool</code>
</dt>
<dd>
</dd>
<dt>
<code>owner: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a></code>
</dt>
<dd>
 object ID of the <code>AccountCap</code> that placed the order
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

<a name="deepbook_clob_OrderFilled"></a>

## Struct `OrderFilled`

Deprecated since v1.0.0, use <code><a href="../deepbook/clob.md#deepbook_clob_OrderFilledV2">OrderFilledV2</a></code> instead.


<pre><code><b>public</b> <b>struct</b> <a href="../deepbook/clob.md#deepbook_clob_OrderFilled">OrderFilled</a>&lt;<b>phantom</b> BaseAsset, <b>phantom</b> QuoteAsset&gt; <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>pool_id: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a></code>
</dt>
<dd>
 object ID of the pool the order was placed on
</dd>
<dt>
<code>order_id: u64</code>
</dt>
<dd>
 ID of the order within the pool
</dd>
<dt>
<code>is_bid: bool</code>
</dt>
<dd>
</dd>
<dt>
<code>owner: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a></code>
</dt>
<dd>
 object ID of the <code>AccountCap</code> that placed the order
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


<a name="deepbook_clob_DEPRECATED"></a>



<pre><code><b>const</b> <a href="../deepbook/clob.md#deepbook_clob_DEPRECATED">DEPRECATED</a>: u64 = 0;
</code></pre>



<a name="deepbook_clob_EInvalidOrderId"></a>



<pre><code><b>const</b> <a href="../deepbook/clob.md#deepbook_clob_EInvalidOrderId">EInvalidOrderId</a>: u64 = 3;
</code></pre>



<a name="deepbook_clob_EUnauthorizedCancel"></a>



<pre><code><b>const</b> <a href="../deepbook/clob.md#deepbook_clob_EUnauthorizedCancel">EUnauthorizedCancel</a>: u64 = 4;
</code></pre>



<a name="deepbook_clob_EInvalidQuantity"></a>



<pre><code><b>const</b> <a href="../deepbook/clob.md#deepbook_clob_EInvalidQuantity">EInvalidQuantity</a>: u64 = 6;
</code></pre>



<a name="deepbook_clob_EInvalidTickPrice"></a>



<pre><code><b>const</b> <a href="../deepbook/clob.md#deepbook_clob_EInvalidTickPrice">EInvalidTickPrice</a>: u64 = 11;
</code></pre>



<a name="deepbook_clob_EInvalidUser"></a>



<pre><code><b>const</b> <a href="../deepbook/clob.md#deepbook_clob_EInvalidUser">EInvalidUser</a>: u64 = 12;
</code></pre>



<a name="deepbook_clob_MIN_ASK_ORDER_ID"></a>



<pre><code><b>const</b> <a href="../deepbook/clob.md#deepbook_clob_MIN_ASK_ORDER_ID">MIN_ASK_ORDER_ID</a>: u64 = 9223372036854775808;
</code></pre>



<a name="deepbook_clob_destroy_empty_level"></a>

## Function `destroy_empty_level`



<pre><code><b>fun</b> <a href="../deepbook/clob.md#deepbook_clob_destroy_empty_level">destroy_empty_level</a>(level: <a href="../deepbook/clob.md#deepbook_clob_TickLevel">deepbook::clob::TickLevel</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../deepbook/clob.md#deepbook_clob_destroy_empty_level">destroy_empty_level</a>(level: <a href="../deepbook/clob.md#deepbook_clob_TickLevel">TickLevel</a>) {
    <b>let</b> <a href="../deepbook/clob.md#deepbook_clob_TickLevel">TickLevel</a> {
        price: _,
        open_orders: orders,
    } = level;
    linked_table::destroy_empty(orders);
}
</code></pre>



</details>

<a name="deepbook_clob_create_account"></a>

## Function `create_account`



<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob.md#deepbook_clob_create_account">create_account</a>(_ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <a href="../deepbook/custodian.md#deepbook_custodian_AccountCap">deepbook::custodian::AccountCap</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob.md#deepbook_clob_create_account">create_account</a>(_ctx: &<b>mut</b> TxContext): AccountCap {
    <b>abort</b> <a href="../deepbook/clob.md#deepbook_clob_DEPRECATED">DEPRECATED</a>
}
</code></pre>



</details>

<a name="deepbook_clob_create_pool"></a>

## Function `create_pool`



<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob.md#deepbook_clob_create_pool">create_pool</a>&lt;BaseAsset, QuoteAsset&gt;(_tick_size: u64, _lot_size: u64, _creation_fee: <a href="../sui/coin.md#sui_coin_Coin">sui::coin::Coin</a>&lt;<a href="../sui/sui.md#sui_sui_SUI">sui::sui::SUI</a>&gt;, _ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob.md#deepbook_clob_create_pool">create_pool</a>&lt;BaseAsset, QuoteAsset&gt;(
    _tick_size: u64,
    _lot_size: u64,
    _creation_fee: Coin&lt;SUI&gt;,
    _ctx: &<b>mut</b> TxContext,
) {
    <b>abort</b> <a href="../deepbook/clob.md#deepbook_clob_DEPRECATED">DEPRECATED</a>
}
</code></pre>



</details>

<a name="deepbook_clob_deposit_base"></a>

## Function `deposit_base`



<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob.md#deepbook_clob_deposit_base">deposit_base</a>&lt;BaseAsset, QuoteAsset&gt;(_pool: &<b>mut</b> <a href="../deepbook/clob.md#deepbook_clob_Pool">deepbook::clob::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, _coin: <a href="../sui/coin.md#sui_coin_Coin">sui::coin::Coin</a>&lt;BaseAsset&gt;, _account_cap: &<a href="../deepbook/custodian.md#deepbook_custodian_AccountCap">deepbook::custodian::AccountCap</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob.md#deepbook_clob_deposit_base">deposit_base</a>&lt;BaseAsset, QuoteAsset&gt;(
    _pool: &<b>mut</b> <a href="../deepbook/clob.md#deepbook_clob_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    _coin: Coin&lt;BaseAsset&gt;,
    _account_cap: &AccountCap
) {
    <b>abort</b> 1337
}
</code></pre>



</details>

<a name="deepbook_clob_deposit_quote"></a>

## Function `deposit_quote`



<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob.md#deepbook_clob_deposit_quote">deposit_quote</a>&lt;BaseAsset, QuoteAsset&gt;(_pool: &<b>mut</b> <a href="../deepbook/clob.md#deepbook_clob_Pool">deepbook::clob::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, _coin: <a href="../sui/coin.md#sui_coin_Coin">sui::coin::Coin</a>&lt;QuoteAsset&gt;, _account_cap: &<a href="../deepbook/custodian.md#deepbook_custodian_AccountCap">deepbook::custodian::AccountCap</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob.md#deepbook_clob_deposit_quote">deposit_quote</a>&lt;BaseAsset, QuoteAsset&gt;(
    _pool: &<b>mut</b> <a href="../deepbook/clob.md#deepbook_clob_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    _coin: Coin&lt;QuoteAsset&gt;,
    _account_cap: &AccountCap
) {
    <b>abort</b> 1337
}
</code></pre>



</details>

<a name="deepbook_clob_withdraw_base"></a>

## Function `withdraw_base`



<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob.md#deepbook_clob_withdraw_base">withdraw_base</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<b>mut</b> <a href="../deepbook/clob.md#deepbook_clob_Pool">deepbook::clob::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, quantity: u64, account_cap: &<a href="../deepbook/custodian.md#deepbook_custodian_AccountCap">deepbook::custodian::AccountCap</a>, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <a href="../sui/coin.md#sui_coin_Coin">sui::coin::Coin</a>&lt;BaseAsset&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob.md#deepbook_clob_withdraw_base">withdraw_base</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool: &<b>mut</b> <a href="../deepbook/clob.md#deepbook_clob_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    quantity: u64,
    account_cap: &AccountCap,
    ctx: &<b>mut</b> TxContext
): Coin&lt;BaseAsset&gt; {
    <b>assert</b>!(quantity &gt; 0, <a href="../deepbook/clob.md#deepbook_clob_EInvalidQuantity">EInvalidQuantity</a>);
    <a href="../deepbook/custodian.md#deepbook_custodian_withdraw_asset">custodian::withdraw_asset</a>(&<b>mut</b> pool.base_custodian, quantity, account_cap, ctx)
}
</code></pre>



</details>

<a name="deepbook_clob_withdraw_quote"></a>

## Function `withdraw_quote`



<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob.md#deepbook_clob_withdraw_quote">withdraw_quote</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<b>mut</b> <a href="../deepbook/clob.md#deepbook_clob_Pool">deepbook::clob::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, quantity: u64, account_cap: &<a href="../deepbook/custodian.md#deepbook_custodian_AccountCap">deepbook::custodian::AccountCap</a>, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <a href="../sui/coin.md#sui_coin_Coin">sui::coin::Coin</a>&lt;QuoteAsset&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob.md#deepbook_clob_withdraw_quote">withdraw_quote</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool: &<b>mut</b> <a href="../deepbook/clob.md#deepbook_clob_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    quantity: u64,
    account_cap: &AccountCap,
    ctx: &<b>mut</b> TxContext
): Coin&lt;QuoteAsset&gt; {
    <b>assert</b>!(quantity &gt; 0, <a href="../deepbook/clob.md#deepbook_clob_EInvalidQuantity">EInvalidQuantity</a>);
    <a href="../deepbook/custodian.md#deepbook_custodian_withdraw_asset">custodian::withdraw_asset</a>(&<b>mut</b> pool.quote_custodian, quantity, account_cap, ctx)
}
</code></pre>



</details>

<a name="deepbook_clob_swap_exact_base_for_quote"></a>

## Function `swap_exact_base_for_quote`



<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob.md#deepbook_clob_swap_exact_base_for_quote">swap_exact_base_for_quote</a>&lt;BaseAsset, QuoteAsset&gt;(_pool: &<b>mut</b> <a href="../deepbook/clob.md#deepbook_clob_Pool">deepbook::clob::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, _quantity: u64, _base_coin: <a href="../sui/coin.md#sui_coin_Coin">sui::coin::Coin</a>&lt;BaseAsset&gt;, _quote_coin: <a href="../sui/coin.md#sui_coin_Coin">sui::coin::Coin</a>&lt;QuoteAsset&gt;, _clock: &<a href="../sui/clock.md#sui_clock_Clock">sui::clock::Clock</a>, _ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): (<a href="../sui/coin.md#sui_coin_Coin">sui::coin::Coin</a>&lt;BaseAsset&gt;, <a href="../sui/coin.md#sui_coin_Coin">sui::coin::Coin</a>&lt;QuoteAsset&gt;, u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob.md#deepbook_clob_swap_exact_base_for_quote">swap_exact_base_for_quote</a>&lt;BaseAsset, QuoteAsset&gt;(
    _pool: &<b>mut</b> <a href="../deepbook/clob.md#deepbook_clob_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    _quantity: u64,
    _base_coin: Coin&lt;BaseAsset&gt;,
    _quote_coin: Coin&lt;QuoteAsset&gt;,
    _clock: &Clock,
    _ctx: &<b>mut</b> TxContext,
): (Coin&lt;BaseAsset&gt;, Coin&lt;QuoteAsset&gt;, u64) {
    <b>abort</b> 1337
}
</code></pre>



</details>

<a name="deepbook_clob_swap_exact_quote_for_base"></a>

## Function `swap_exact_quote_for_base`



<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob.md#deepbook_clob_swap_exact_quote_for_base">swap_exact_quote_for_base</a>&lt;BaseAsset, QuoteAsset&gt;(_pool: &<b>mut</b> <a href="../deepbook/clob.md#deepbook_clob_Pool">deepbook::clob::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, _quantity: u64, _clock: &<a href="../sui/clock.md#sui_clock_Clock">sui::clock::Clock</a>, _quote_coin: <a href="../sui/coin.md#sui_coin_Coin">sui::coin::Coin</a>&lt;QuoteAsset&gt;, _ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): (<a href="../sui/coin.md#sui_coin_Coin">sui::coin::Coin</a>&lt;BaseAsset&gt;, <a href="../sui/coin.md#sui_coin_Coin">sui::coin::Coin</a>&lt;QuoteAsset&gt;, u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob.md#deepbook_clob_swap_exact_quote_for_base">swap_exact_quote_for_base</a>&lt;BaseAsset, QuoteAsset&gt;(
    _pool: &<b>mut</b> <a href="../deepbook/clob.md#deepbook_clob_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    _quantity: u64,
    _clock: &Clock,
    _quote_coin: Coin&lt;QuoteAsset&gt;,
    _ctx: &<b>mut</b> TxContext,
): (Coin&lt;BaseAsset&gt;, Coin&lt;QuoteAsset&gt;, u64) {
    <b>abort</b> 1337
}
</code></pre>



</details>

<a name="deepbook_clob_place_market_order"></a>

## Function `place_market_order`

Place a market order to the order book.


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob.md#deepbook_clob_place_market_order">place_market_order</a>&lt;BaseAsset, QuoteAsset&gt;(_pool: &<b>mut</b> <a href="../deepbook/clob.md#deepbook_clob_Pool">deepbook::clob::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, _quantity: u64, _is_bid: bool, _base_coin: <a href="../sui/coin.md#sui_coin_Coin">sui::coin::Coin</a>&lt;BaseAsset&gt;, _quote_coin: <a href="../sui/coin.md#sui_coin_Coin">sui::coin::Coin</a>&lt;QuoteAsset&gt;, _clock: &<a href="../sui/clock.md#sui_clock_Clock">sui::clock::Clock</a>, _ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): (<a href="../sui/coin.md#sui_coin_Coin">sui::coin::Coin</a>&lt;BaseAsset&gt;, <a href="../sui/coin.md#sui_coin_Coin">sui::coin::Coin</a>&lt;QuoteAsset&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob.md#deepbook_clob_place_market_order">place_market_order</a>&lt;BaseAsset, QuoteAsset&gt;(
    _pool: &<b>mut</b> <a href="../deepbook/clob.md#deepbook_clob_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    _quantity: u64,
    _is_bid: bool,
    <b>mut</b> _base_coin: Coin&lt;BaseAsset&gt;,
    <b>mut</b> _quote_coin: Coin&lt;QuoteAsset&gt;,
    _clock: &Clock,
    _ctx: &<b>mut</b> TxContext,
): (Coin&lt;BaseAsset&gt;, Coin&lt;QuoteAsset&gt;) {
    <b>abort</b> 1337
}
</code></pre>



</details>

<a name="deepbook_clob_place_limit_order"></a>

## Function `place_limit_order`

Place a limit order to the order book.
Returns (base quantity filled, quote quantity filled, whether a maker order is being placed, order id of the maker order).
When the limit order is not successfully placed, we return false to indicate that and also returns a meaningless order_id 0.
When the limit order is successfully placed, we return true to indicate that and also the corresponding order_id.
So please check that boolean value first before using the order id.


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob.md#deepbook_clob_place_limit_order">place_limit_order</a>&lt;BaseAsset, QuoteAsset&gt;(_pool: &<b>mut</b> <a href="../deepbook/clob.md#deepbook_clob_Pool">deepbook::clob::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, _price: u64, _quantity: u64, _is_bid: bool, _expire_timestamp: u64, _restriction: u8, _clock: &<a href="../sui/clock.md#sui_clock_Clock">sui::clock::Clock</a>, _account_cap: &<a href="../deepbook/custodian.md#deepbook_custodian_AccountCap">deepbook::custodian::AccountCap</a>, _ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): (u64, u64, bool, u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob.md#deepbook_clob_place_limit_order">place_limit_order</a>&lt;BaseAsset, QuoteAsset&gt;(
    _pool: &<b>mut</b> <a href="../deepbook/clob.md#deepbook_clob_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    _price: u64,
    _quantity: u64,
    _is_bid: bool,
    _expire_timestamp: u64, // Expiration timestamp in ms in absolute value inclusive.
    _restriction: u8,
    _clock: &Clock,
    _account_cap: &AccountCap,
    _ctx: &<b>mut</b> TxContext
): (u64, u64, bool, u64) {
   <b>abort</b> 1337
}
</code></pre>



</details>

<a name="deepbook_clob_order_is_bid"></a>

## Function `order_is_bid`



<pre><code><b>fun</b> <a href="../deepbook/clob.md#deepbook_clob_order_is_bid">order_is_bid</a>(order_id: u64): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../deepbook/clob.md#deepbook_clob_order_is_bid">order_is_bid</a>(order_id: u64): bool {
    <b>return</b> order_id &lt; <a href="../deepbook/clob.md#deepbook_clob_MIN_ASK_ORDER_ID">MIN_ASK_ORDER_ID</a>
}
</code></pre>



</details>

<a name="deepbook_clob_emit_order_canceled"></a>

## Function `emit_order_canceled`



<pre><code><b>fun</b> <a href="../deepbook/clob.md#deepbook_clob_emit_order_canceled">emit_order_canceled</a>&lt;BaseAsset, QuoteAsset&gt;(pool_id: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a>, order: &<a href="../deepbook/clob.md#deepbook_clob_Order">deepbook::clob::Order</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../deepbook/clob.md#deepbook_clob_emit_order_canceled">emit_order_canceled</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool_id: ID,
    order: &<a href="../deepbook/clob.md#deepbook_clob_Order">Order</a>
) {
    event::emit(<a href="../deepbook/clob.md#deepbook_clob_OrderCanceled">OrderCanceled</a>&lt;BaseAsset, QuoteAsset&gt; {
        pool_id,
        order_id: order.order_id,
        is_bid: order.is_bid,
        owner: order.owner,
        base_asset_quantity_canceled: order.quantity,
        price: order.price
    })
}
</code></pre>



</details>

<a name="deepbook_clob_cancel_order"></a>

## Function `cancel_order`

Cancel and opening order.
Abort if order_id is invalid or if the order is not submitted by the transaction sender.


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob.md#deepbook_clob_cancel_order">cancel_order</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<b>mut</b> <a href="../deepbook/clob.md#deepbook_clob_Pool">deepbook::clob::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, order_id: u64, account_cap: &<a href="../deepbook/custodian.md#deepbook_custodian_AccountCap">deepbook::custodian::AccountCap</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob.md#deepbook_clob_cancel_order">cancel_order</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool: &<b>mut</b> <a href="../deepbook/clob.md#deepbook_clob_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    order_id: u64,
    account_cap: &AccountCap
) {
    // First check the highest bit of the order id to see whether it's bid or ask.
    // Then retrieve the price using the order id.
    // Using the price to retrieve the corresponding PriceLevel from the bids / asks Critbit Tree.
    // Retrieve and remove the order from open orders of the PriceLevel.
    <b>let</b> user = object::id(account_cap);
    <b>assert</b>!(contains(&pool.usr_open_orders, user), <a href="../deepbook/clob.md#deepbook_clob_EInvalidUser">EInvalidUser</a>);
    <b>let</b> usr_open_orders = borrow_mut(&<b>mut</b> pool.usr_open_orders, user);
    <b>assert</b>!(linked_table::contains(usr_open_orders, order_id), <a href="../deepbook/clob.md#deepbook_clob_EInvalidOrderId">EInvalidOrderId</a>);
    <b>let</b> tick_price = *linked_table::borrow(usr_open_orders, order_id);
    <b>let</b> is_bid = <a href="../deepbook/clob.md#deepbook_clob_order_is_bid">order_is_bid</a>(order_id);
    <b>let</b> (tick_exists, tick_index) = find_leaf(
        <b>if</b> (is_bid) { &pool.bids } <b>else</b> { &pool.asks },
        tick_price);
    <b>assert</b>!(tick_exists, <a href="../deepbook/clob.md#deepbook_clob_EInvalidOrderId">EInvalidOrderId</a>);
    <b>let</b> order = <a href="../deepbook/clob.md#deepbook_clob_remove_order">remove_order</a>(
        <b>if</b> (is_bid) { &<b>mut</b> pool.bids } <b>else</b> { &<b>mut</b> pool.asks },
        usr_open_orders,
        tick_index,
        order_id,
        user
    );
    <b>if</b> (is_bid) {
        <b>let</b> balance_locked = clob_math::mul(order.quantity, order.price);
        <a href="../deepbook/custodian.md#deepbook_custodian_unlock_balance">custodian::unlock_balance</a>(&<b>mut</b> pool.quote_custodian, user, balance_locked);
    } <b>else</b> {
        <a href="../deepbook/custodian.md#deepbook_custodian_unlock_balance">custodian::unlock_balance</a>(&<b>mut</b> pool.base_custodian, user, order.quantity);
    };
    <a href="../deepbook/clob.md#deepbook_clob_emit_order_canceled">emit_order_canceled</a>&lt;BaseAsset, QuoteAsset&gt;(*object::uid_as_inner(&pool.id), &order);
}
</code></pre>



</details>

<a name="deepbook_clob_remove_order"></a>

## Function `remove_order`



<pre><code><b>fun</b> <a href="../deepbook/clob.md#deepbook_clob_remove_order">remove_order</a>(open_orders: &<b>mut</b> <a href="../deepbook/critbit.md#deepbook_critbit_CritbitTree">deepbook::critbit::CritbitTree</a>&lt;<a href="../deepbook/clob.md#deepbook_clob_TickLevel">deepbook::clob::TickLevel</a>&gt;, usr_open_orders: &<b>mut</b> <a href="../sui/linked_table.md#sui_linked_table_LinkedTable">sui::linked_table::LinkedTable</a>&lt;u64, u64&gt;, tick_index: u64, order_id: u64, user: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a>): <a href="../deepbook/clob.md#deepbook_clob_Order">deepbook::clob::Order</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../deepbook/clob.md#deepbook_clob_remove_order">remove_order</a>(
    open_orders: &<b>mut</b> CritbitTree&lt;<a href="../deepbook/clob.md#deepbook_clob_TickLevel">TickLevel</a>&gt;,
    usr_open_orders: &<b>mut</b> LinkedTable&lt;u64, u64&gt;,
    tick_index: u64,
    order_id: u64,
    user: ID,
): <a href="../deepbook/clob.md#deepbook_clob_Order">Order</a> {
    linked_table::remove(usr_open_orders, order_id);
    <b>let</b> tick_level = borrow_leaf_by_index(open_orders, tick_index);
    <b>assert</b>!(linked_table::contains(&tick_level.open_orders, order_id), <a href="../deepbook/clob.md#deepbook_clob_EInvalidOrderId">EInvalidOrderId</a>);
    <b>let</b> mut_tick_level = borrow_mut_leaf_by_index(open_orders, tick_index);
    <b>let</b> order = linked_table::remove(&<b>mut</b> mut_tick_level.open_orders, order_id);
    <b>assert</b>!(order.owner == user, <a href="../deepbook/clob.md#deepbook_clob_EUnauthorizedCancel">EUnauthorizedCancel</a>);
    <b>if</b> (linked_table::is_empty(&mut_tick_level.open_orders)) {
        <a href="../deepbook/clob.md#deepbook_clob_destroy_empty_level">destroy_empty_level</a>(remove_leaf_by_index(open_orders, tick_index));
    };
    order
}
</code></pre>



</details>

<a name="deepbook_clob_cancel_all_orders"></a>

## Function `cancel_all_orders`



<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob.md#deepbook_clob_cancel_all_orders">cancel_all_orders</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<b>mut</b> <a href="../deepbook/clob.md#deepbook_clob_Pool">deepbook::clob::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, account_cap: &<a href="../deepbook/custodian.md#deepbook_custodian_AccountCap">deepbook::custodian::AccountCap</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob.md#deepbook_clob_cancel_all_orders">cancel_all_orders</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool: &<b>mut</b> <a href="../deepbook/clob.md#deepbook_clob_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    account_cap: &AccountCap
) {
    <b>let</b> pool_id = *object::uid_as_inner(&pool.id);
    <b>let</b> user = object::id(account_cap);
    <b>assert</b>!(contains(&pool.usr_open_orders, user), <a href="../deepbook/clob.md#deepbook_clob_EInvalidUser">EInvalidUser</a>);
    <b>let</b> usr_open_order_ids = table::borrow_mut(&<b>mut</b> pool.usr_open_orders, user);
    <b>while</b> (!linked_table::is_empty(usr_open_order_ids)) {
        <b>let</b> order_id = *option::borrow(linked_table::back(usr_open_order_ids));
        <b>let</b> order_price = *linked_table::borrow(usr_open_order_ids, order_id);
        <b>let</b> is_bid = <a href="../deepbook/clob.md#deepbook_clob_order_is_bid">order_is_bid</a>(order_id);
        <b>let</b> open_orders =
            <b>if</b> (is_bid) { &<b>mut</b> pool.bids }
            <b>else</b> { &<b>mut</b> pool.asks };
        <b>let</b> (_, tick_index) = <a href="../deepbook/critbit.md#deepbook_critbit_find_leaf">critbit::find_leaf</a>(open_orders, order_price);
        <b>let</b> order = <a href="../deepbook/clob.md#deepbook_clob_remove_order">remove_order</a>(
            open_orders,
            usr_open_order_ids,
            tick_index,
            order_id,
            user
        );
        <b>if</b> (is_bid) {
            <b>let</b> balance_locked = clob_math::mul(order.quantity, order.price);
            <a href="../deepbook/custodian.md#deepbook_custodian_unlock_balance">custodian::unlock_balance</a>(&<b>mut</b> pool.quote_custodian, user, balance_locked);
        } <b>else</b> {
            <a href="../deepbook/custodian.md#deepbook_custodian_unlock_balance">custodian::unlock_balance</a>(&<b>mut</b> pool.base_custodian, user, order.quantity);
        };
        <a href="../deepbook/clob.md#deepbook_clob_emit_order_canceled">emit_order_canceled</a>&lt;BaseAsset, QuoteAsset&gt;(pool_id, &order);
    };
}
</code></pre>



</details>

<a name="deepbook_clob_batch_cancel_order"></a>

## Function `batch_cancel_order`

Batch cancel limit orders to save gas cost.
Abort if any of the order_ids are not submitted by the sender.
Skip any order_id that is invalid.
Note that this function can reduce gas cost even further if caller has multiple orders at the same price level,
and if orders with the same price are grouped together in the vector.
For example, if we have the following order_id to price mapping, {0: 100., 1: 200., 2: 100., 3: 200.}.
Grouping order_ids like [0, 2, 1, 3] would make it the most gas efficient.


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob.md#deepbook_clob_batch_cancel_order">batch_cancel_order</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<b>mut</b> <a href="../deepbook/clob.md#deepbook_clob_Pool">deepbook::clob::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, order_ids: vector&lt;u64&gt;, account_cap: &<a href="../deepbook/custodian.md#deepbook_custodian_AccountCap">deepbook::custodian::AccountCap</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob.md#deepbook_clob_batch_cancel_order">batch_cancel_order</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool: &<b>mut</b> <a href="../deepbook/clob.md#deepbook_clob_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    order_ids: vector&lt;u64&gt;,
    account_cap: &AccountCap
) {
    <b>let</b> pool_id = *object::uid_as_inner(&pool.id);
    // First group the order ids according to price level,
    // so that we don't have to retrieve the PriceLevel multiple times <b>if</b> there are orders at the same price level.
    // Iterate over each price level, retrieve the corresponding PriceLevel.
    // Iterate over the order ids that need to be canceled at that price level,
    // retrieve and remove the order from open orders of the PriceLevel.
    <b>let</b> user = object::id(account_cap);
    <b>assert</b>!(contains(&pool.usr_open_orders, user), 0);
    <b>let</b> <b>mut</b> tick_index: u64 = 0;
    <b>let</b> <b>mut</b> tick_price: u64 = 0;
    <b>let</b> n_order = vector::length(&order_ids);
    <b>let</b> <b>mut</b> i_order = 0;
    <b>let</b> usr_open_orders = borrow_mut(&<b>mut</b> pool.usr_open_orders, user);
    <b>while</b> (i_order &lt; n_order) {
        <b>let</b> order_id = *vector::borrow(&order_ids, i_order);
        <b>assert</b>!(linked_table::contains(usr_open_orders, order_id), <a href="../deepbook/clob.md#deepbook_clob_EInvalidOrderId">EInvalidOrderId</a>);
        <b>let</b> new_tick_price = *linked_table::borrow(usr_open_orders, order_id);
        <b>let</b> is_bid = <a href="../deepbook/clob.md#deepbook_clob_order_is_bid">order_is_bid</a>(order_id);
        <b>if</b> (new_tick_price != tick_price) {
            tick_price = new_tick_price;
            <b>let</b> (tick_exists, new_tick_index) = find_leaf(
                <b>if</b> (is_bid) { &pool.bids } <b>else</b> { &pool.asks },
                tick_price
            );
            <b>assert</b>!(tick_exists, <a href="../deepbook/clob.md#deepbook_clob_EInvalidTickPrice">EInvalidTickPrice</a>);
            tick_index = new_tick_index;
        };
        <b>let</b> order = <a href="../deepbook/clob.md#deepbook_clob_remove_order">remove_order</a>(
            <b>if</b> (is_bid) { &<b>mut</b> pool.bids } <b>else</b> { &<b>mut</b> pool.asks },
            usr_open_orders,
            tick_index,
            order_id,
            user
        );
        <b>if</b> (is_bid) {
            <b>let</b> balance_locked = clob_math::mul(order.quantity, order.price);
            <a href="../deepbook/custodian.md#deepbook_custodian_unlock_balance">custodian::unlock_balance</a>(&<b>mut</b> pool.quote_custodian, user, balance_locked);
        } <b>else</b> {
            <a href="../deepbook/custodian.md#deepbook_custodian_unlock_balance">custodian::unlock_balance</a>(&<b>mut</b> pool.base_custodian, user, order.quantity);
        };
        <a href="../deepbook/clob.md#deepbook_clob_emit_order_canceled">emit_order_canceled</a>&lt;BaseAsset, QuoteAsset&gt;(pool_id, &order);
        i_order = i_order + 1;
    }
}
</code></pre>



</details>

<a name="deepbook_clob_list_open_orders"></a>

## Function `list_open_orders`



<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob.md#deepbook_clob_list_open_orders">list_open_orders</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<a href="../deepbook/clob.md#deepbook_clob_Pool">deepbook::clob::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, account_cap: &<a href="../deepbook/custodian.md#deepbook_custodian_AccountCap">deepbook::custodian::AccountCap</a>): vector&lt;<a href="../deepbook/clob.md#deepbook_clob_Order">deepbook::clob::Order</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob.md#deepbook_clob_list_open_orders">list_open_orders</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool: &<a href="../deepbook/clob.md#deepbook_clob_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    account_cap: &AccountCap
): vector&lt;<a href="../deepbook/clob.md#deepbook_clob_Order">Order</a>&gt; {
    <b>let</b> user = object::id(account_cap);
    <b>let</b> usr_open_order_ids = table::borrow(&pool.usr_open_orders, user);
    <b>let</b> <b>mut</b> open_orders = vector::empty&lt;<a href="../deepbook/clob.md#deepbook_clob_Order">Order</a>&gt;();
    <b>let</b> <b>mut</b> order_id = linked_table::front(usr_open_order_ids);
    <b>while</b> (!option::is_none(order_id)) {
        <b>let</b> order_price = *linked_table::borrow(usr_open_order_ids, *option::borrow(order_id));
        <b>let</b> tick_level =
            <b>if</b> (<a href="../deepbook/clob.md#deepbook_clob_order_is_bid">order_is_bid</a>(*option::borrow(order_id))) borrow_leaf_by_key(&pool.bids, order_price)
            <b>else</b> borrow_leaf_by_key(&pool.asks, order_price);
        <b>let</b> order = linked_table::borrow(&tick_level.open_orders, *option::borrow(order_id));
        vector::push_back(&<b>mut</b> open_orders, <a href="../deepbook/clob.md#deepbook_clob_Order">Order</a> {
            order_id: order.order_id,
            price: order.price,
            quantity: order.quantity,
            is_bid: order.is_bid,
            owner: order.owner,
            expire_timestamp: order.expire_timestamp
        });
        order_id = linked_table::next(usr_open_order_ids, *option::borrow(order_id));
    };
    open_orders
}
</code></pre>



</details>

<a name="deepbook_clob_account_balance"></a>

## Function `account_balance`

query user balance inside custodian


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob.md#deepbook_clob_account_balance">account_balance</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<a href="../deepbook/clob.md#deepbook_clob_Pool">deepbook::clob::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, account_cap: &<a href="../deepbook/custodian.md#deepbook_custodian_AccountCap">deepbook::custodian::AccountCap</a>): (u64, u64, u64, u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob.md#deepbook_clob_account_balance">account_balance</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool: &<a href="../deepbook/clob.md#deepbook_clob_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    account_cap: &AccountCap
): (u64, u64, u64, u64) {
    <b>let</b> user = object::id(account_cap);
    <b>let</b> (base_avail, base_locked) = <a href="../deepbook/custodian.md#deepbook_custodian_account_balance">custodian::account_balance</a>(&pool.base_custodian, user);
    <b>let</b> (quote_avail, quote_locked) = <a href="../deepbook/custodian.md#deepbook_custodian_account_balance">custodian::account_balance</a>(&pool.quote_custodian, user);
    (base_avail, base_locked, quote_avail, quote_locked)
}
</code></pre>



</details>

<a name="deepbook_clob_get_market_price"></a>

## Function `get_market_price`

Query the market price of order book
returns (best_bid_price, best_ask_price)


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob.md#deepbook_clob_get_market_price">get_market_price</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<a href="../deepbook/clob.md#deepbook_clob_Pool">deepbook::clob::Pool</a>&lt;BaseAsset, QuoteAsset&gt;): (u64, u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob.md#deepbook_clob_get_market_price">get_market_price</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool: &<a href="../deepbook/clob.md#deepbook_clob_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;
): (u64, u64){
    <b>let</b> (bid_price, _) = <a href="../deepbook/critbit.md#deepbook_critbit_max_leaf">critbit::max_leaf</a>(&pool.bids);
    <b>let</b> (ask_price, _) = <a href="../deepbook/critbit.md#deepbook_critbit_min_leaf">critbit::min_leaf</a>(&pool.asks);
    <b>return</b> (bid_price, ask_price)
}
</code></pre>



</details>

<a name="deepbook_clob_get_level2_book_status_bid_side"></a>

## Function `get_level2_book_status_bid_side`

Enter a price range and return the level2 order depth of all valid prices within this price range in bid side
returns two vectors of u64
The previous is a list of all valid prices
The latter is the corresponding depth list


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob.md#deepbook_clob_get_level2_book_status_bid_side">get_level2_book_status_bid_side</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<a href="../deepbook/clob.md#deepbook_clob_Pool">deepbook::clob::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, price_low: u64, price_high: u64, clock: &<a href="../sui/clock.md#sui_clock_Clock">sui::clock::Clock</a>): (vector&lt;u64&gt;, vector&lt;u64&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob.md#deepbook_clob_get_level2_book_status_bid_side">get_level2_book_status_bid_side</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool: &<a href="../deepbook/clob.md#deepbook_clob_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    <b>mut</b> price_low: u64,
    <b>mut</b> price_high: u64,
    clock: &Clock
): (vector&lt;u64&gt;, vector&lt;u64&gt;) {
    <b>let</b> (price_low_, _) = <a href="../deepbook/critbit.md#deepbook_critbit_min_leaf">critbit::min_leaf</a>(&pool.bids);
    <b>if</b> (price_low &lt; price_low_) price_low = price_low_;
    <b>let</b> (price_high_, _) = <a href="../deepbook/critbit.md#deepbook_critbit_max_leaf">critbit::max_leaf</a>(&pool.bids);
    <b>if</b> (price_high &gt; price_high_) price_high = price_high_;
    price_low = <a href="../deepbook/critbit.md#deepbook_critbit_find_closest_key">critbit::find_closest_key</a>(&pool.bids, price_low);
    price_high = <a href="../deepbook/critbit.md#deepbook_critbit_find_closest_key">critbit::find_closest_key</a>(&pool.bids, price_high);
    <b>let</b> <b>mut</b> price_vec = vector::empty&lt;u64&gt;();
    <b>let</b> <b>mut</b> depth_vec = vector::empty&lt;u64&gt;();
    <b>if</b> (price_low == 0) { <b>return</b> (price_vec, depth_vec) };
    <b>while</b> (price_low &lt;= price_high) {
        <b>let</b> depth = <a href="../deepbook/clob.md#deepbook_clob_get_level2_book_status">get_level2_book_status</a>(
            &pool.bids,
            price_low,
            clock::timestamp_ms(clock)
        );
        vector::push_back(&<b>mut</b> price_vec, price_low);
        vector::push_back(&<b>mut</b> depth_vec, depth);
        <b>let</b> (next_price, _) = <a href="../deepbook/critbit.md#deepbook_critbit_next_leaf">critbit::next_leaf</a>(&pool.bids, price_low);
        <b>if</b> (next_price == 0) { <b>break</b> }
        <b>else</b> { price_low = next_price };
    };
    (price_vec, depth_vec)
}
</code></pre>



</details>

<a name="deepbook_clob_get_level2_book_status_ask_side"></a>

## Function `get_level2_book_status_ask_side`

Enter a price range and return the level2 order depth of all valid prices within this price range in ask side
returns two vectors of u64
The previous is a list of all valid prices
The latter is the corresponding depth list


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob.md#deepbook_clob_get_level2_book_status_ask_side">get_level2_book_status_ask_side</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<a href="../deepbook/clob.md#deepbook_clob_Pool">deepbook::clob::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, price_low: u64, price_high: u64, clock: &<a href="../sui/clock.md#sui_clock_Clock">sui::clock::Clock</a>): (vector&lt;u64&gt;, vector&lt;u64&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob.md#deepbook_clob_get_level2_book_status_ask_side">get_level2_book_status_ask_side</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool: &<a href="../deepbook/clob.md#deepbook_clob_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    <b>mut</b> price_low: u64,
    <b>mut</b> price_high: u64,
    clock: &Clock
): (vector&lt;u64&gt;, vector&lt;u64&gt;) {
    <b>let</b> (price_low_, _) = <a href="../deepbook/critbit.md#deepbook_critbit_min_leaf">critbit::min_leaf</a>(&pool.asks);
    <b>if</b> (price_low &lt; price_low_) price_low = price_low_;
    <b>let</b> (price_high_, _) = <a href="../deepbook/critbit.md#deepbook_critbit_max_leaf">critbit::max_leaf</a>(&pool.asks);
    <b>if</b> (price_high &gt; price_high_) price_high = price_high_;
    price_low = <a href="../deepbook/critbit.md#deepbook_critbit_find_closest_key">critbit::find_closest_key</a>(&pool.asks, price_low);
    price_high = <a href="../deepbook/critbit.md#deepbook_critbit_find_closest_key">critbit::find_closest_key</a>(&pool.asks, price_high);
    <b>let</b> <b>mut</b> price_vec = vector::empty&lt;u64&gt;();
    <b>let</b> <b>mut</b> depth_vec = vector::empty&lt;u64&gt;();
    <b>if</b> (price_low == 0) { <b>return</b> (price_vec, depth_vec) };
    <b>while</b> (price_low &lt;= price_high) {
        <b>let</b> depth = <a href="../deepbook/clob.md#deepbook_clob_get_level2_book_status">get_level2_book_status</a>(
            &pool.asks,
            price_low,
            clock::timestamp_ms(clock)
        );
        vector::push_back(&<b>mut</b> price_vec, price_low);
        vector::push_back(&<b>mut</b> depth_vec, depth);
        <b>let</b> (next_price, _) = <a href="../deepbook/critbit.md#deepbook_critbit_next_leaf">critbit::next_leaf</a>(&pool.asks, price_low);
        <b>if</b> (next_price == 0) { <b>break</b> }
        <b>else</b> { price_low = next_price };
    };
    (price_vec, depth_vec)
}
</code></pre>



</details>

<a name="deepbook_clob_get_level2_book_status"></a>

## Function `get_level2_book_status`

internal func to retrieve single depth of a tick price


<pre><code><b>fun</b> <a href="../deepbook/clob.md#deepbook_clob_get_level2_book_status">get_level2_book_status</a>(open_orders: &<a href="../deepbook/critbit.md#deepbook_critbit_CritbitTree">deepbook::critbit::CritbitTree</a>&lt;<a href="../deepbook/clob.md#deepbook_clob_TickLevel">deepbook::clob::TickLevel</a>&gt;, price: u64, time_stamp: u64): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../deepbook/clob.md#deepbook_clob_get_level2_book_status">get_level2_book_status</a>(
    open_orders: &CritbitTree&lt;<a href="../deepbook/clob.md#deepbook_clob_TickLevel">TickLevel</a>&gt;,
    price: u64,
    time_stamp: u64
): u64 {
    <b>let</b> tick_level = <a href="../deepbook/critbit.md#deepbook_critbit_borrow_leaf_by_key">critbit::borrow_leaf_by_key</a>(open_orders, price);
    <b>let</b> tick_open_orders = &tick_level.open_orders;
    <b>let</b> <b>mut</b> depth = 0;
    <b>let</b> <b>mut</b> order_id = linked_table::front(tick_open_orders);
    <b>let</b> <b>mut</b> order: &<a href="../deepbook/clob.md#deepbook_clob_Order">Order</a>;
    <b>while</b> (!option::is_none(order_id)) {
        order = linked_table::borrow(tick_open_orders, *option::borrow(order_id));
        <b>if</b> (order.expire_timestamp &gt; time_stamp) depth = depth + order.quantity;
        order_id = linked_table::next(tick_open_orders, *option::borrow(order_id));
    };
    depth
}
</code></pre>



</details>

<a name="deepbook_clob_get_order_status"></a>

## Function `get_order_status`



<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob.md#deepbook_clob_get_order_status">get_order_status</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<a href="../deepbook/clob.md#deepbook_clob_Pool">deepbook::clob::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, order_id: u64, account_cap: &<a href="../deepbook/custodian.md#deepbook_custodian_AccountCap">deepbook::custodian::AccountCap</a>): &<a href="../deepbook/clob.md#deepbook_clob_Order">deepbook::clob::Order</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob.md#deepbook_clob_get_order_status">get_order_status</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool: &<a href="../deepbook/clob.md#deepbook_clob_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    order_id: u64,
    account_cap: &AccountCap
): &<a href="../deepbook/clob.md#deepbook_clob_Order">Order</a> {
    <b>let</b> user = object::id(account_cap);
    <b>assert</b>!(table::contains(&pool.usr_open_orders, user), <a href="../deepbook/clob.md#deepbook_clob_EInvalidUser">EInvalidUser</a>);
    <b>let</b> usr_open_order_ids = table::borrow(&pool.usr_open_orders, user);
    <b>assert</b>!(linked_table::contains(usr_open_order_ids, order_id), <a href="../deepbook/clob.md#deepbook_clob_EInvalidOrderId">EInvalidOrderId</a>);
    <b>let</b> order_price = *linked_table::borrow(usr_open_order_ids, order_id);
    <b>let</b> open_orders =
        <b>if</b> (order_id &lt; <a href="../deepbook/clob.md#deepbook_clob_MIN_ASK_ORDER_ID">MIN_ASK_ORDER_ID</a>) { &pool.bids }
        <b>else</b> { &pool.asks };
    <b>let</b> tick_level = <a href="../deepbook/critbit.md#deepbook_critbit_borrow_leaf_by_key">critbit::borrow_leaf_by_key</a>(open_orders, order_price);
    <b>let</b> tick_open_orders = &tick_level.open_orders;
    <b>let</b> order = linked_table::borrow(tick_open_orders, order_id);
    order
}
</code></pre>



</details>
