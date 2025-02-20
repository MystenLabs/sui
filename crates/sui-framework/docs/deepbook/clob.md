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
-  [Function `match_bid_with_quote_quantity`](#deepbook_clob_match_bid_with_quote_quantity)
-  [Function `match_bid`](#deepbook_clob_match_bid)
-  [Function `match_ask`](#deepbook_clob_match_ask)
-  [Function `place_market_order`](#deepbook_clob_place_market_order)
-  [Function `inject_limit_order`](#deepbook_clob_inject_limit_order)
-  [Function `place_limit_order`](#deepbook_clob_place_limit_order)
-  [Function `order_is_bid`](#deepbook_clob_order_is_bid)
-  [Function `emit_order_canceled`](#deepbook_clob_emit_order_canceled)
-  [Function `emit_order_filled`](#deepbook_clob_emit_order_filled)
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



<a name="deepbook_clob_EInsufficientBaseCoin"></a>



<pre><code><b>const</b> <a href="../deepbook/clob.md#deepbook_clob_EInsufficientBaseCoin">EInsufficientBaseCoin</a>: u64 = 7;
</code></pre>



<a name="deepbook_clob_EInsufficientQuoteCoin"></a>



<pre><code><b>const</b> <a href="../deepbook/clob.md#deepbook_clob_EInsufficientQuoteCoin">EInsufficientQuoteCoin</a>: u64 = 8;
</code></pre>



<a name="deepbook_clob_EInvalidExpireTimestamp"></a>



<pre><code><b>const</b> <a href="../deepbook/clob.md#deepbook_clob_EInvalidExpireTimestamp">EInvalidExpireTimestamp</a>: u64 = 19;
</code></pre>



<a name="deepbook_clob_EInvalidOrderId"></a>



<pre><code><b>const</b> <a href="../deepbook/clob.md#deepbook_clob_EInvalidOrderId">EInvalidOrderId</a>: u64 = 3;
</code></pre>



<a name="deepbook_clob_EInvalidPrice"></a>



<pre><code><b>const</b> <a href="../deepbook/clob.md#deepbook_clob_EInvalidPrice">EInvalidPrice</a>: u64 = 5;
</code></pre>



<a name="deepbook_clob_EInvalidQuantity"></a>



<pre><code><b>const</b> <a href="../deepbook/clob.md#deepbook_clob_EInvalidQuantity">EInvalidQuantity</a>: u64 = 6;
</code></pre>



<a name="deepbook_clob_EInvalidRestriction"></a>



<pre><code><b>const</b> <a href="../deepbook/clob.md#deepbook_clob_EInvalidRestriction">EInvalidRestriction</a>: u64 = 14;
</code></pre>



<a name="deepbook_clob_EInvalidTickPrice"></a>



<pre><code><b>const</b> <a href="../deepbook/clob.md#deepbook_clob_EInvalidTickPrice">EInvalidTickPrice</a>: u64 = 11;
</code></pre>



<a name="deepbook_clob_EInvalidUser"></a>



<pre><code><b>const</b> <a href="../deepbook/clob.md#deepbook_clob_EInvalidUser">EInvalidUser</a>: u64 = 12;
</code></pre>



<a name="deepbook_clob_EOrderCannotBeFullyFilled"></a>



<pre><code><b>const</b> <a href="../deepbook/clob.md#deepbook_clob_EOrderCannotBeFullyFilled">EOrderCannotBeFullyFilled</a>: u64 = 9;
</code></pre>



<a name="deepbook_clob_EOrderCannotBeFullyPassive"></a>



<pre><code><b>const</b> <a href="../deepbook/clob.md#deepbook_clob_EOrderCannotBeFullyPassive">EOrderCannotBeFullyPassive</a>: u64 = 10;
</code></pre>



<a name="deepbook_clob_EUnauthorizedCancel"></a>



<pre><code><b>const</b> <a href="../deepbook/clob.md#deepbook_clob_EUnauthorizedCancel">EUnauthorizedCancel</a>: u64 = 4;
</code></pre>



<a name="deepbook_clob_FILL_OR_KILL"></a>



<pre><code><b>const</b> <a href="../deepbook/clob.md#deepbook_clob_FILL_OR_KILL">FILL_OR_KILL</a>: u8 = 2;
</code></pre>



<a name="deepbook_clob_FLOAT_SCALING"></a>



<pre><code><b>const</b> <a href="../deepbook/clob.md#deepbook_clob_FLOAT_SCALING">FLOAT_SCALING</a>: u64 = 1000000000;
</code></pre>



<a name="deepbook_clob_IMMEDIATE_OR_CANCEL"></a>



<pre><code><b>const</b> <a href="../deepbook/clob.md#deepbook_clob_IMMEDIATE_OR_CANCEL">IMMEDIATE_OR_CANCEL</a>: u8 = 1;
</code></pre>



<a name="deepbook_clob_MAX_PRICE"></a>



<pre><code><b>const</b> <a href="../deepbook/clob.md#deepbook_clob_MAX_PRICE">MAX_PRICE</a>: u64 = 9223372036854775808;
</code></pre>



<a name="deepbook_clob_MIN_ASK_ORDER_ID"></a>



<pre><code><b>const</b> <a href="../deepbook/clob.md#deepbook_clob_MIN_ASK_ORDER_ID">MIN_ASK_ORDER_ID</a>: u64 = 9223372036854775808;
</code></pre>



<a name="deepbook_clob_MIN_PRICE"></a>



<pre><code><b>const</b> <a href="../deepbook/clob.md#deepbook_clob_MIN_PRICE">MIN_PRICE</a>: u64 = 0;
</code></pre>



<a name="deepbook_clob_NO_RESTRICTION"></a>



<pre><code><b>const</b> <a href="../deepbook/clob.md#deepbook_clob_NO_RESTRICTION">NO_RESTRICTION</a>: u8 = 0;
</code></pre>



<a name="deepbook_clob_POST_OR_ABORT"></a>



<pre><code><b>const</b> <a href="../deepbook/clob.md#deepbook_clob_POST_OR_ABORT">POST_OR_ABORT</a>: u8 = 3;
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



<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob.md#deepbook_clob_deposit_base">deposit_base</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<b>mut</b> <a href="../deepbook/clob.md#deepbook_clob_Pool">deepbook::clob::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, coin: <a href="../sui/coin.md#sui_coin_Coin">sui::coin::Coin</a>&lt;BaseAsset&gt;, account_cap: &<a href="../deepbook/custodian.md#deepbook_custodian_AccountCap">deepbook::custodian::AccountCap</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob.md#deepbook_clob_deposit_base">deposit_base</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool: &<b>mut</b> <a href="../deepbook/clob.md#deepbook_clob_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    coin: Coin&lt;BaseAsset&gt;,
    account_cap: &AccountCap
) {
    <b>assert</b>!(coin::value(&coin) != 0, <a href="../deepbook/clob.md#deepbook_clob_EInsufficientBaseCoin">EInsufficientBaseCoin</a>);
    <a href="../deepbook/custodian.md#deepbook_custodian_increase_user_available_balance">custodian::increase_user_available_balance</a>(
        &<b>mut</b> pool.base_custodian,
        object::id(account_cap),
        coin::into_balance(coin)
    )
}
</code></pre>



</details>

<a name="deepbook_clob_deposit_quote"></a>

## Function `deposit_quote`



<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob.md#deepbook_clob_deposit_quote">deposit_quote</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<b>mut</b> <a href="../deepbook/clob.md#deepbook_clob_Pool">deepbook::clob::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, coin: <a href="../sui/coin.md#sui_coin_Coin">sui::coin::Coin</a>&lt;QuoteAsset&gt;, account_cap: &<a href="../deepbook/custodian.md#deepbook_custodian_AccountCap">deepbook::custodian::AccountCap</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob.md#deepbook_clob_deposit_quote">deposit_quote</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool: &<b>mut</b> <a href="../deepbook/clob.md#deepbook_clob_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    coin: Coin&lt;QuoteAsset&gt;,
    account_cap: &AccountCap
) {
    <b>assert</b>!(coin::value(&coin) != 0, <a href="../deepbook/clob.md#deepbook_clob_EInsufficientQuoteCoin">EInsufficientQuoteCoin</a>);
    <a href="../deepbook/custodian.md#deepbook_custodian_increase_user_available_balance">custodian::increase_user_available_balance</a>(
        &<b>mut</b> pool.quote_custodian,
        object::id(account_cap),
        coin::into_balance(coin)
    )
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



<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob.md#deepbook_clob_swap_exact_base_for_quote">swap_exact_base_for_quote</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<b>mut</b> <a href="../deepbook/clob.md#deepbook_clob_Pool">deepbook::clob::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, quantity: u64, base_coin: <a href="../sui/coin.md#sui_coin_Coin">sui::coin::Coin</a>&lt;BaseAsset&gt;, quote_coin: <a href="../sui/coin.md#sui_coin_Coin">sui::coin::Coin</a>&lt;QuoteAsset&gt;, clock: &<a href="../sui/clock.md#sui_clock_Clock">sui::clock::Clock</a>, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): (<a href="../sui/coin.md#sui_coin_Coin">sui::coin::Coin</a>&lt;BaseAsset&gt;, <a href="../sui/coin.md#sui_coin_Coin">sui::coin::Coin</a>&lt;QuoteAsset&gt;, u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob.md#deepbook_clob_swap_exact_base_for_quote">swap_exact_base_for_quote</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool: &<b>mut</b> <a href="../deepbook/clob.md#deepbook_clob_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    quantity: u64,
    base_coin: Coin&lt;BaseAsset&gt;,
    quote_coin: Coin&lt;QuoteAsset&gt;,
    clock: &Clock,
    ctx: &<b>mut</b> TxContext,
): (Coin&lt;BaseAsset&gt;, Coin&lt;QuoteAsset&gt;, u64) {
    <b>assert</b>!(quantity &gt; 0, <a href="../deepbook/clob.md#deepbook_clob_EInvalidQuantity">EInvalidQuantity</a>);
    <b>assert</b>!(coin::value(&base_coin) &gt;= quantity, <a href="../deepbook/clob.md#deepbook_clob_EInsufficientBaseCoin">EInsufficientBaseCoin</a>);
    <b>let</b> original_val = coin::value(&quote_coin);
    <b>let</b> (ret_base_coin, ret_quote_coin) = <a href="../deepbook/clob.md#deepbook_clob_place_market_order">place_market_order</a>(
        pool,
        quantity,
        <b>false</b>,
        base_coin,
        quote_coin,
        clock,
        ctx
    );
    <b>let</b> ret_val = coin::value(&ret_quote_coin);
    (ret_base_coin, ret_quote_coin, ret_val - original_val)
}
</code></pre>



</details>

<a name="deepbook_clob_swap_exact_quote_for_base"></a>

## Function `swap_exact_quote_for_base`



<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob.md#deepbook_clob_swap_exact_quote_for_base">swap_exact_quote_for_base</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<b>mut</b> <a href="../deepbook/clob.md#deepbook_clob_Pool">deepbook::clob::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, quantity: u64, clock: &<a href="../sui/clock.md#sui_clock_Clock">sui::clock::Clock</a>, quote_coin: <a href="../sui/coin.md#sui_coin_Coin">sui::coin::Coin</a>&lt;QuoteAsset&gt;, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): (<a href="../sui/coin.md#sui_coin_Coin">sui::coin::Coin</a>&lt;BaseAsset&gt;, <a href="../sui/coin.md#sui_coin_Coin">sui::coin::Coin</a>&lt;QuoteAsset&gt;, u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob.md#deepbook_clob_swap_exact_quote_for_base">swap_exact_quote_for_base</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool: &<b>mut</b> <a href="../deepbook/clob.md#deepbook_clob_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    quantity: u64,
    clock: &Clock,
    quote_coin: Coin&lt;QuoteAsset&gt;,
    ctx: &<b>mut</b> TxContext,
): (Coin&lt;BaseAsset&gt;, Coin&lt;QuoteAsset&gt;, u64) {
    <b>assert</b>!(quantity &gt; 0, <a href="../deepbook/clob.md#deepbook_clob_EInvalidQuantity">EInvalidQuantity</a>);
    <b>assert</b>!(coin::value(&quote_coin) &gt;= quantity, <a href="../deepbook/clob.md#deepbook_clob_EInsufficientQuoteCoin">EInsufficientQuoteCoin</a>);
    <b>let</b> (base_asset_balance, quote_asset_balance) = <a href="../deepbook/clob.md#deepbook_clob_match_bid_with_quote_quantity">match_bid_with_quote_quantity</a>(
        pool,
        quantity,
        <a href="../deepbook/clob.md#deepbook_clob_MAX_PRICE">MAX_PRICE</a>,
        clock::timestamp_ms(clock),
        coin::into_balance(quote_coin)
    );
    <b>let</b> val = balance::value(&base_asset_balance);
    (coin::from_balance(base_asset_balance, ctx), coin::from_balance(quote_asset_balance, ctx), val)
}
</code></pre>



</details>

<a name="deepbook_clob_match_bid_with_quote_quantity"></a>

## Function `match_bid_with_quote_quantity`



<pre><code><b>fun</b> <a href="../deepbook/clob.md#deepbook_clob_match_bid_with_quote_quantity">match_bid_with_quote_quantity</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<b>mut</b> <a href="../deepbook/clob.md#deepbook_clob_Pool">deepbook::clob::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, quantity: u64, price_limit: u64, current_timestamp: u64, quote_balance: <a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;QuoteAsset&gt;): (<a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;BaseAsset&gt;, <a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;QuoteAsset&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../deepbook/clob.md#deepbook_clob_match_bid_with_quote_quantity">match_bid_with_quote_quantity</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool: &<b>mut</b> <a href="../deepbook/clob.md#deepbook_clob_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    quantity: u64,
    price_limit: u64,
    current_timestamp: u64,
    quote_balance: Balance&lt;QuoteAsset&gt;,
): (Balance&lt;BaseAsset&gt;, Balance&lt;QuoteAsset&gt;) {
    // Base balance received by taker, taking into account of taker commission.
    // Need to individually keep track of the remaining base quantity to be filled to avoid infinite <b>loop</b>.
    <b>let</b> pool_id = *object::uid_as_inner(&pool.id);
    <b>let</b> <b>mut</b> taker_quote_quantity_remaining = quantity;
    <b>let</b> <b>mut</b> base_balance_filled = balance::zero&lt;BaseAsset&gt;();
    <b>let</b> <b>mut</b> quote_balance_left = quote_balance;
    <b>let</b> all_open_orders = &<b>mut</b> pool.asks;
    <b>if</b> (<a href="../deepbook/critbit.md#deepbook_critbit_is_empty">critbit::is_empty</a>(all_open_orders)) {
        <b>return</b> (base_balance_filled, quote_balance_left)
    };
    <b>let</b> (<b>mut</b> tick_price, <b>mut</b> tick_index) = min_leaf(all_open_orders);
    <b>let</b> <b>mut</b> terminate_loop = <b>false</b>;
    <b>while</b> (!is_empty&lt;<a href="../deepbook/clob.md#deepbook_clob_TickLevel">TickLevel</a>&gt;(all_open_orders) && tick_price &lt;= price_limit) {
        <b>let</b> tick_level = borrow_mut_leaf_by_index(all_open_orders, tick_index);
        <b>let</b> <b>mut</b> order_id = *option::borrow(linked_table::front(&tick_level.open_orders));
        <b>while</b> (!linked_table::is_empty(&tick_level.open_orders)) {
            <b>let</b> maker_order = linked_table::borrow(&tick_level.open_orders, order_id);
            <b>let</b> <b>mut</b> maker_base_quantity = maker_order.quantity;
            <b>let</b> <b>mut</b> skip_order = <b>false</b>;
            <b>if</b> (maker_order.expire_timestamp &lt;= current_timestamp) {
                skip_order = <b>true</b>;
                <a href="../deepbook/custodian.md#deepbook_custodian_unlock_balance">custodian::unlock_balance</a>(&<b>mut</b> pool.base_custodian, maker_order.owner, maker_order.quantity);
                <a href="../deepbook/clob.md#deepbook_clob_emit_order_canceled">emit_order_canceled</a>&lt;BaseAsset, QuoteAsset&gt;(pool_id, maker_order);
            } <b>else</b> {
                // Calculate how much quote asset (maker_quote_quantity) is required, including the commission, to fill the maker order.
                <b>let</b> maker_quote_quantity_without_commission = clob_math::mul(
                    maker_base_quantity,
                    maker_order.price
                );
                <b>let</b> (is_round_down, <b>mut</b> taker_commission)  = clob_math::unsafe_mul_round(
                    maker_quote_quantity_without_commission,
                    pool.taker_fee_rate
                );
                <b>if</b> (is_round_down)  taker_commission = taker_commission + 1;
                <b>let</b> maker_quote_quantity = maker_quote_quantity_without_commission + taker_commission;
                // Total base quantity filled.
                <b>let</b> <b>mut</b> filled_base_quantity: u64;
                // Total quote quantity filled, excluding commission and rebate.
                <b>let</b> <b>mut</b> filled_quote_quantity: u64;
                // Total quote quantity paid by taker.
                // filled_quote_quantity_without_commission * (<a href="../deepbook/clob.md#deepbook_clob_FLOAT_SCALING">FLOAT_SCALING</a> + taker_fee_rate) = filled_quote_quantity
                <b>let</b> <b>mut</b> filled_quote_quantity_without_commission: u64;
                <b>if</b> (taker_quote_quantity_remaining &gt; maker_quote_quantity) {
                    filled_quote_quantity = maker_quote_quantity;
                    filled_quote_quantity_without_commission = maker_quote_quantity_without_commission;
                    filled_base_quantity = maker_base_quantity;
                } <b>else</b> {
                    terminate_loop = <b>true</b>;
                    // <b>if</b> not enough quote quantity to pay <b>for</b> taker commission, then no quantity will be filled
                    filled_quote_quantity_without_commission = clob_math::unsafe_div(
                        taker_quote_quantity_remaining,
                        <a href="../deepbook/clob.md#deepbook_clob_FLOAT_SCALING">FLOAT_SCALING</a> + pool.taker_fee_rate
                    );
                    // filled_base_quantity = 0 is permitted since filled_quote_quantity_without_commission can be 0
                    filled_base_quantity = clob_math::unsafe_div(
                        filled_quote_quantity_without_commission,
                        maker_order.price
                    );
                    <b>let</b> filled_base_lot = filled_base_quantity / pool.lot_size;
                    filled_base_quantity = filled_base_lot * pool.lot_size;
                    // filled_quote_quantity_without_commission = 0 is permitted here since filled_base_quantity could be 0
                    filled_quote_quantity_without_commission = clob_math::unsafe_mul(
                        filled_base_quantity,
                        maker_order.price
                    );
                    // <b>if</b> taker_commission = 0 due to underflow, round it up to 1
                    <b>let</b> (round_down, <b>mut</b> taker_commission) = clob_math::unsafe_mul_round(
                        filled_quote_quantity_without_commission,
                        pool.taker_fee_rate
                    );
                    <b>if</b> (round_down) {
                        taker_commission = taker_commission + 1;
                    };
                    filled_quote_quantity = filled_quote_quantity_without_commission + taker_commission;
                };
                // <b>if</b> maker_rebate = 0 due to underflow, maker will not receive a rebate
                <b>let</b> maker_rebate = clob_math::unsafe_mul(
                    filled_quote_quantity_without_commission,
                    pool.maker_rebate_rate
                );
                maker_base_quantity = maker_base_quantity - filled_base_quantity;
                // maker in ask side, decrease maker's locked base asset, increase maker's available quote asset
                taker_quote_quantity_remaining = taker_quote_quantity_remaining - filled_quote_quantity;
                <b>let</b> locked_base_balance = <a href="../deepbook/custodian.md#deepbook_custodian_decrease_user_locked_balance">custodian::decrease_user_locked_balance</a>&lt;BaseAsset&gt;(
                    &<b>mut</b> pool.base_custodian,
                    maker_order.owner,
                    filled_base_quantity
                );
                <b>let</b> <b>mut</b> quote_balance_filled = balance::split(
                    &<b>mut</b> quote_balance_left,
                    filled_quote_quantity,
                );
                // Send quote asset including rebate to maker.
                <a href="../deepbook/custodian.md#deepbook_custodian_increase_user_available_balance">custodian::increase_user_available_balance</a>&lt;QuoteAsset&gt;(
                    &<b>mut</b> pool.quote_custodian,
                    maker_order.owner,
                    balance::split(
                        &<b>mut</b> quote_balance_filled,
                        maker_rebate + filled_quote_quantity_without_commission,
                    ),
                );
                // Send remaining of commission - rebate to the protocol.
                // commission - rebate = filled_quote_quantity_without_commission - filled_quote_quantity - maker_rebate
                balance::join(&<b>mut</b> pool.quote_asset_trading_fees, quote_balance_filled);
                balance::join(&<b>mut</b> base_balance_filled, locked_base_balance);
                <a href="../deepbook/clob.md#deepbook_clob_emit_order_filled">emit_order_filled</a>&lt;BaseAsset, QuoteAsset&gt;(
                    *object::uid_as_inner(&pool.id),
                    maker_order,
                    filled_base_quantity,
                    // taker_commission = filled_quote_quantity - filled_quote_quantity_without_commission
                    // This guarantees that the subtraction will not underflow
                    filled_quote_quantity - filled_quote_quantity_without_commission,
                    maker_rebate
                )
            };
            <b>if</b> (skip_order || maker_base_quantity == 0) {
                // Remove the maker order.
                <b>let</b> old_order_id = order_id;
                <b>let</b> maybe_order_id = linked_table::next(&tick_level.open_orders, order_id);
                <b>if</b> (!option::is_none(maybe_order_id)) {
                    order_id = *option::borrow(maybe_order_id);
                };
                <b>let</b> usr_open_order_ids = table::borrow_mut(&<b>mut</b> pool.usr_open_orders, maker_order.owner);
                linked_table::remove(usr_open_order_ids, old_order_id);
                linked_table::remove(&<b>mut</b> tick_level.open_orders, old_order_id);
            } <b>else</b> {
                // Update the maker order.
                <b>let</b> maker_order_mut = linked_table::borrow_mut(
                    &<b>mut</b> tick_level.open_orders,
                    order_id);
                maker_order_mut.quantity = maker_base_quantity;
            };
            <b>if</b> (terminate_loop) {
                <b>break</b>
            };
        };
        <b>if</b> (linked_table::is_empty(&tick_level.open_orders)) {
            (tick_price, _) = next_leaf(all_open_orders, tick_price);
            <a href="../deepbook/clob.md#deepbook_clob_destroy_empty_level">destroy_empty_level</a>(remove_leaf_by_index(all_open_orders, tick_index));
            (_, tick_index) = find_leaf(all_open_orders, tick_price);
        };
        <b>if</b> (terminate_loop) {
            <b>break</b>
        };
    };
    <b>return</b> (base_balance_filled, quote_balance_left)
}
</code></pre>



</details>

<a name="deepbook_clob_match_bid"></a>

## Function `match_bid`



<pre><code><b>fun</b> <a href="../deepbook/clob.md#deepbook_clob_match_bid">match_bid</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<b>mut</b> <a href="../deepbook/clob.md#deepbook_clob_Pool">deepbook::clob::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, quantity: u64, price_limit: u64, current_timestamp: u64, quote_balance: <a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;QuoteAsset&gt;): (<a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;BaseAsset&gt;, <a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;QuoteAsset&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../deepbook/clob.md#deepbook_clob_match_bid">match_bid</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool: &<b>mut</b> <a href="../deepbook/clob.md#deepbook_clob_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    quantity: u64,
    price_limit: u64,
    current_timestamp: u64,
    quote_balance: Balance&lt;QuoteAsset&gt;,
): (Balance&lt;BaseAsset&gt;, Balance&lt;QuoteAsset&gt;) {
    <b>let</b> pool_id = *object::uid_as_inner(&pool.id);
    // Base balance received by taker.
    // Need to individually keep track of the remaining base quantity to be filled to avoid infinite <b>loop</b>.
    <b>let</b> <b>mut</b> taker_base_quantity_remaining = quantity;
    <b>let</b> <b>mut</b> base_balance_filled = balance::zero&lt;BaseAsset&gt;();
    <b>let</b> <b>mut</b> quote_balance_left = quote_balance;
    <b>let</b> all_open_orders = &<b>mut</b> pool.asks;
    <b>if</b> (<a href="../deepbook/critbit.md#deepbook_critbit_is_empty">critbit::is_empty</a>(all_open_orders)) {
        <b>return</b> (base_balance_filled, quote_balance_left)
    };
    <b>let</b> (<b>mut</b> tick_price, <b>mut</b> tick_index) = min_leaf(all_open_orders);
    <b>while</b> (!is_empty&lt;<a href="../deepbook/clob.md#deepbook_clob_TickLevel">TickLevel</a>&gt;(all_open_orders) && tick_price &lt;= price_limit) {
        <b>let</b> tick_level = borrow_mut_leaf_by_index(all_open_orders, tick_index);
        <b>let</b> <b>mut</b> order_id = *option::borrow(linked_table::front(&tick_level.open_orders));
        <b>while</b> (!linked_table::is_empty(&tick_level.open_orders)) {
            <b>let</b> maker_order = linked_table::borrow(&tick_level.open_orders, order_id);
            <b>let</b> <b>mut</b> maker_base_quantity = maker_order.quantity;
            <b>let</b> <b>mut</b> skip_order = <b>false</b>;
            <b>if</b> (maker_order.expire_timestamp &lt;= current_timestamp) {
                skip_order = <b>true</b>;
                <a href="../deepbook/custodian.md#deepbook_custodian_unlock_balance">custodian::unlock_balance</a>(&<b>mut</b> pool.base_custodian, maker_order.owner, maker_order.quantity);
                <a href="../deepbook/clob.md#deepbook_clob_emit_order_canceled">emit_order_canceled</a>&lt;BaseAsset, QuoteAsset&gt;(pool_id, maker_order);
            } <b>else</b> {
                <b>let</b> filled_base_quantity =
                    <b>if</b> (taker_base_quantity_remaining &gt; maker_base_quantity) { maker_base_quantity }
                    <b>else</b> { taker_base_quantity_remaining };
                <b>let</b> filled_quote_quantity = clob_math::mul(filled_base_quantity, maker_order.price);
                // <b>if</b> maker_rebate = 0 due to underflow, maker will not receive a rebate
                <b>let</b> maker_rebate = clob_math::unsafe_mul(filled_quote_quantity, pool.maker_rebate_rate);
                // <b>if</b> taker_commission = 0 due to underflow, round it up to 1
                <b>let</b> (is_round_down, <b>mut</b> taker_commission) = clob_math::unsafe_mul_round(
                    filled_quote_quantity,
                    pool.taker_fee_rate
                );
                <b>if</b> (is_round_down) taker_commission = taker_commission + 1;
                maker_base_quantity = maker_base_quantity - filled_base_quantity;
                // maker in ask side, decrease maker's locked base asset, increase maker's available quote asset
                taker_base_quantity_remaining = taker_base_quantity_remaining - filled_base_quantity;
                <b>let</b> locked_base_balance = <a href="../deepbook/custodian.md#deepbook_custodian_decrease_user_locked_balance">custodian::decrease_user_locked_balance</a>&lt;BaseAsset&gt;(
                    &<b>mut</b> pool.base_custodian,
                    maker_order.owner,
                    filled_base_quantity
                );
                <b>let</b> <b>mut</b> taker_commission_balance = balance::split(
                    &<b>mut</b> quote_balance_left,
                    taker_commission,
                );
                <a href="../deepbook/custodian.md#deepbook_custodian_increase_user_available_balance">custodian::increase_user_available_balance</a>&lt;QuoteAsset&gt;(
                    &<b>mut</b> pool.quote_custodian,
                    maker_order.owner,
                    balance::split(
                        &<b>mut</b> taker_commission_balance,
                        maker_rebate,
                    ),
                );
                balance::join(&<b>mut</b> pool.quote_asset_trading_fees, taker_commission_balance);
                balance::join(&<b>mut</b> base_balance_filled, locked_base_balance);
                <a href="../deepbook/custodian.md#deepbook_custodian_increase_user_available_balance">custodian::increase_user_available_balance</a>&lt;QuoteAsset&gt;(
                    &<b>mut</b> pool.quote_custodian,
                    maker_order.owner,
                    balance::split(
                        &<b>mut</b> quote_balance_left,
                        filled_quote_quantity,
                    ),
                );
                <a href="../deepbook/clob.md#deepbook_clob_emit_order_filled">emit_order_filled</a>&lt;BaseAsset, QuoteAsset&gt;(
                    *object::uid_as_inner(&pool.id),
                    maker_order,
                    filled_base_quantity,
                    taker_commission,
                    maker_rebate
                );
            };
            <b>if</b> (skip_order || maker_base_quantity == 0) {
                // Remove the maker order.
                <b>let</b> old_order_id = order_id;
                <b>let</b> maybe_order_id = linked_table::next(&tick_level.open_orders, order_id);
                <b>if</b> (!option::is_none(maybe_order_id)) {
                    order_id = *option::borrow(maybe_order_id);
                };
                <b>let</b> usr_open_order_ids = table::borrow_mut(&<b>mut</b> pool.usr_open_orders, maker_order.owner);
                linked_table::remove(usr_open_order_ids, old_order_id);
                linked_table::remove(&<b>mut</b> tick_level.open_orders, old_order_id);
            } <b>else</b> {
                // Update the maker order.
                <b>let</b> maker_order_mut = linked_table::borrow_mut(
                    &<b>mut</b> tick_level.open_orders,
                    order_id);
                maker_order_mut.quantity = maker_base_quantity;
            };
            <b>if</b> (taker_base_quantity_remaining == 0) {
                <b>break</b>
            };
        };
        <b>if</b> (linked_table::is_empty(&tick_level.open_orders)) {
            (tick_price, _) = next_leaf(all_open_orders, tick_price);
            <a href="../deepbook/clob.md#deepbook_clob_destroy_empty_level">destroy_empty_level</a>(remove_leaf_by_index(all_open_orders, tick_index));
            (_, tick_index) = find_leaf(all_open_orders, tick_price);
        };
        <b>if</b> (taker_base_quantity_remaining == 0) {
            <b>break</b>
        };
    };
    <b>return</b> (base_balance_filled, quote_balance_left)
}
</code></pre>



</details>

<a name="deepbook_clob_match_ask"></a>

## Function `match_ask`



<pre><code><b>fun</b> <a href="../deepbook/clob.md#deepbook_clob_match_ask">match_ask</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<b>mut</b> <a href="../deepbook/clob.md#deepbook_clob_Pool">deepbook::clob::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, price_limit: u64, current_timestamp: u64, base_balance: <a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;BaseAsset&gt;): (<a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;BaseAsset&gt;, <a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;QuoteAsset&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../deepbook/clob.md#deepbook_clob_match_ask">match_ask</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool: &<b>mut</b> <a href="../deepbook/clob.md#deepbook_clob_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    price_limit: u64,
    current_timestamp: u64,
    base_balance: Balance&lt;BaseAsset&gt;,
): (Balance&lt;BaseAsset&gt;, Balance&lt;QuoteAsset&gt;) {
    <b>let</b> pool_id = *object::uid_as_inner(&pool.id);
    <b>let</b> <b>mut</b> base_balance_left = base_balance;
    // Base balance received by taker, taking into account of taker commission.
    <b>let</b> <b>mut</b> quote_balance_filled = balance::zero&lt;QuoteAsset&gt;();
    <b>let</b> all_open_orders = &<b>mut</b> pool.bids;
    <b>if</b> (<a href="../deepbook/critbit.md#deepbook_critbit_is_empty">critbit::is_empty</a>(all_open_orders)) {
        <b>return</b> (base_balance_left, quote_balance_filled)
    };
    <b>let</b> (<b>mut</b> tick_price, <b>mut</b> tick_index) = max_leaf(all_open_orders);
    <b>while</b> (!is_empty&lt;<a href="../deepbook/clob.md#deepbook_clob_TickLevel">TickLevel</a>&gt;(all_open_orders) && tick_price &gt;= price_limit) {
        <b>let</b> tick_level = borrow_mut_leaf_by_index(all_open_orders, tick_index);
        <b>let</b> <b>mut</b> order_id = *option::borrow(linked_table::front(&tick_level.open_orders));
        <b>while</b> (!linked_table::is_empty(&tick_level.open_orders)) {
            <b>let</b> maker_order = linked_table::borrow(&tick_level.open_orders, order_id);
            <b>let</b> <b>mut</b> maker_base_quantity = maker_order.quantity;
            <b>let</b> <b>mut</b> skip_order = <b>false</b>;
            <b>if</b> (maker_order.expire_timestamp &lt;= current_timestamp) {
                skip_order = <b>true</b>;
                <b>let</b> maker_quote_quantity = clob_math::mul(maker_order.quantity, maker_order.price);
                <a href="../deepbook/custodian.md#deepbook_custodian_unlock_balance">custodian::unlock_balance</a>(&<b>mut</b> pool.quote_custodian, maker_order.owner, maker_quote_quantity);
                <a href="../deepbook/clob.md#deepbook_clob_emit_order_canceled">emit_order_canceled</a>&lt;BaseAsset, QuoteAsset&gt;(pool_id, maker_order);
            } <b>else</b> {
                <b>let</b> taker_base_quantity_remaining = balance::value(&base_balance_left);
                <b>let</b> filled_base_quantity =
                    <b>if</b> (taker_base_quantity_remaining &gt;= maker_base_quantity) { maker_base_quantity }
                    <b>else</b> { taker_base_quantity_remaining };
                <b>let</b> filled_quote_quantity = clob_math::mul(filled_base_quantity, maker_order.price);
                // <b>if</b> maker_rebate = 0 due to underflow, maker will not receive a rebate
                <b>let</b> maker_rebate = clob_math::unsafe_mul(filled_quote_quantity, pool.maker_rebate_rate);
                // <b>if</b> taker_commission = 0 due to underflow, round it up to 1
                <b>let</b> (is_round_down, <b>mut</b> taker_commission) = clob_math::unsafe_mul_round(
                    filled_quote_quantity,
                    pool.taker_fee_rate
                );
                <b>if</b> (is_round_down) taker_commission = taker_commission + 1;
                maker_base_quantity = maker_base_quantity - filled_base_quantity;
                // maker in bid side, decrease maker's locked quote asset, increase maker's available base asset
                <b>let</b> <b>mut</b> locked_quote_balance = <a href="../deepbook/custodian.md#deepbook_custodian_decrease_user_locked_balance">custodian::decrease_user_locked_balance</a>&lt;QuoteAsset&gt;(
                    &<b>mut</b> pool.quote_custodian,
                    maker_order.owner,
                    filled_quote_quantity
                );
                <b>let</b> <b>mut</b> taker_commission_balance = balance::split(
                    &<b>mut</b> locked_quote_balance,
                    taker_commission,
                );
                <a href="../deepbook/custodian.md#deepbook_custodian_increase_user_available_balance">custodian::increase_user_available_balance</a>&lt;QuoteAsset&gt;(
                    &<b>mut</b> pool.quote_custodian,
                    maker_order.owner,
                    balance::split(
                        &<b>mut</b> taker_commission_balance,
                        maker_rebate,
                    ),
                );
                balance::join(&<b>mut</b> pool.quote_asset_trading_fees, taker_commission_balance);
                balance::join(&<b>mut</b> quote_balance_filled, locked_quote_balance);
                <a href="../deepbook/custodian.md#deepbook_custodian_increase_user_available_balance">custodian::increase_user_available_balance</a>&lt;BaseAsset&gt;(
                    &<b>mut</b> pool.base_custodian,
                    maker_order.owner,
                    balance::split(
                        &<b>mut</b> base_balance_left,
                        filled_base_quantity,
                    ),
                );
                <a href="../deepbook/clob.md#deepbook_clob_emit_order_filled">emit_order_filled</a>&lt;BaseAsset, QuoteAsset&gt;(
                    *object::uid_as_inner(&pool.id),
                    maker_order,
                    filled_base_quantity,
                    taker_commission,
                    maker_rebate
                );
            };
            <b>if</b> (skip_order || maker_base_quantity == 0) {
                // Remove the maker order.
                <b>let</b> old_order_id = order_id;
                <b>let</b> maybe_order_id = linked_table::next(&tick_level.open_orders, order_id);
                <b>if</b> (!option::is_none(maybe_order_id)) {
                    order_id = *option::borrow(maybe_order_id);
                };
                <b>let</b> usr_open_order_ids = table::borrow_mut(&<b>mut</b> pool.usr_open_orders, maker_order.owner);
                linked_table::remove(usr_open_order_ids, old_order_id);
                linked_table::remove(&<b>mut</b> tick_level.open_orders, old_order_id);
            } <b>else</b> {
                // Update the maker order.
                <b>let</b> maker_order_mut = linked_table::borrow_mut(
                    &<b>mut</b> tick_level.open_orders,
                    order_id);
                maker_order_mut.quantity = maker_base_quantity;
            };
            <b>if</b> (balance::value(&base_balance_left) == 0) {
                <b>break</b>
            };
        };
        <b>if</b> (linked_table::is_empty(&tick_level.open_orders)) {
            (tick_price, _) = previous_leaf(all_open_orders, tick_price);
            <a href="../deepbook/clob.md#deepbook_clob_destroy_empty_level">destroy_empty_level</a>(remove_leaf_by_index(all_open_orders, tick_index));
            (_, tick_index) = find_leaf(all_open_orders, tick_price);
        };
        <b>if</b> (balance::value(&base_balance_left) == 0) {
            <b>break</b>
        };
    };
    <b>return</b> (base_balance_left, quote_balance_filled)
}
</code></pre>



</details>

<a name="deepbook_clob_place_market_order"></a>

## Function `place_market_order`

Place a market order to the order book.


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob.md#deepbook_clob_place_market_order">place_market_order</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<b>mut</b> <a href="../deepbook/clob.md#deepbook_clob_Pool">deepbook::clob::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, quantity: u64, is_bid: bool, base_coin: <a href="../sui/coin.md#sui_coin_Coin">sui::coin::Coin</a>&lt;BaseAsset&gt;, quote_coin: <a href="../sui/coin.md#sui_coin_Coin">sui::coin::Coin</a>&lt;QuoteAsset&gt;, clock: &<a href="../sui/clock.md#sui_clock_Clock">sui::clock::Clock</a>, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): (<a href="../sui/coin.md#sui_coin_Coin">sui::coin::Coin</a>&lt;BaseAsset&gt;, <a href="../sui/coin.md#sui_coin_Coin">sui::coin::Coin</a>&lt;QuoteAsset&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob.md#deepbook_clob_place_market_order">place_market_order</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool: &<b>mut</b> <a href="../deepbook/clob.md#deepbook_clob_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    quantity: u64,
    is_bid: bool,
    <b>mut</b> base_coin: Coin&lt;BaseAsset&gt;,
    <b>mut</b> quote_coin: Coin&lt;QuoteAsset&gt;,
    clock: &Clock,
    ctx: &<b>mut</b> TxContext,
): (Coin&lt;BaseAsset&gt;, Coin&lt;QuoteAsset&gt;) {
    // If market bid order, match against the open ask orders. Otherwise, match against the open bid orders.
    // Take market bid order <b>for</b> example.
    // We first retrieve the PriceLevel with the lowest price by calling min_leaf on the asks Critbit Tree.
    // We then match the market order by iterating through open orders on that price level in ascending order of the order id.
    // Open orders that are being filled are removed from the order book.
    // We stop the iteration until all quantities are filled.
    // If the total quantity of open orders at the lowest price level is not large enough to fully fill the market order,
    // we <b>move</b> on to the next price level by calling next_leaf on the asks Critbit Tree and repeat the same procedure.
    // Continue iterating over the price levels in ascending order until the market order is completely filled.
    // If the market order cannot be completely filled even after consuming all the open ask orders,
    // the unfilled quantity will be cancelled.
    // Market ask order follows similar procedure.
    // The difference is that market ask order is matched against the open bid orders.
    // We start with the bid PriceLeve with the highest price by calling max_leaf on the bids Critbit Tree.
    // The inner <b>loop</b> <b>for</b> iterating over the open orders in ascending orders of order id is the same <b>as</b> above.
    // Then iterate over the price levels in descending order until the market order is completely filled.
    <b>assert</b>!(quantity % pool.lot_size == 0, <a href="../deepbook/clob.md#deepbook_clob_EInvalidQuantity">EInvalidQuantity</a>);
    <b>assert</b>!(quantity != 0, <a href="../deepbook/clob.md#deepbook_clob_EInvalidQuantity">EInvalidQuantity</a>);
    <b>if</b> (is_bid) {
        <b>let</b> (base_balance_filled, quote_balance_left) = <a href="../deepbook/clob.md#deepbook_clob_match_bid">match_bid</a>(
            pool,
            quantity,
            <a href="../deepbook/clob.md#deepbook_clob_MAX_PRICE">MAX_PRICE</a>,
            clock::timestamp_ms(clock),
            coin::into_balance(quote_coin),
        );
        join(
            &<b>mut</b> base_coin,
            coin::from_balance(base_balance_filled, ctx),
        );
        quote_coin = coin::from_balance(quote_balance_left, ctx);
    } <b>else</b> {
        <b>assert</b>!(quantity &lt;= coin::value(&base_coin), <a href="../deepbook/clob.md#deepbook_clob_EInsufficientBaseCoin">EInsufficientBaseCoin</a>);
        <b>let</b> (base_balance_left, quote_balance_filled) = <a href="../deepbook/clob.md#deepbook_clob_match_ask">match_ask</a>(
            pool,
            <a href="../deepbook/clob.md#deepbook_clob_MIN_PRICE">MIN_PRICE</a>,
            clock::timestamp_ms(clock),
            coin::into_balance(base_coin),
        );
        base_coin = coin::from_balance(base_balance_left, ctx);
        join(
            &<b>mut</b> quote_coin,
            coin::from_balance(quote_balance_filled, ctx),
        );
    };
    (base_coin, quote_coin)
}
</code></pre>



</details>

<a name="deepbook_clob_inject_limit_order"></a>

## Function `inject_limit_order`

Injects a maker order to the order book.
Returns the order id.


<pre><code><b>fun</b> <a href="../deepbook/clob.md#deepbook_clob_inject_limit_order">inject_limit_order</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<b>mut</b> <a href="../deepbook/clob.md#deepbook_clob_Pool">deepbook::clob::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, price: u64, quantity: u64, is_bid: bool, expire_timestamp: u64, account_cap: &<a href="../deepbook/custodian.md#deepbook_custodian_AccountCap">deepbook::custodian::AccountCap</a>, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../deepbook/clob.md#deepbook_clob_inject_limit_order">inject_limit_order</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool: &<b>mut</b> <a href="../deepbook/clob.md#deepbook_clob_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    price: u64,
    quantity: u64,
    is_bid: bool,
    expire_timestamp: u64,
    account_cap: &AccountCap,
    ctx: &<b>mut</b> TxContext
): u64 {
    <b>let</b> user = object::id(account_cap);
    <b>let</b> order_id: u64;
    <b>let</b> open_orders: &<b>mut</b> CritbitTree&lt;<a href="../deepbook/clob.md#deepbook_clob_TickLevel">TickLevel</a>&gt;;
    <b>if</b> (is_bid) {
        <b>let</b> quote_quantity = clob_math::mul(quantity, price);
        <a href="../deepbook/custodian.md#deepbook_custodian_lock_balance">custodian::lock_balance</a>&lt;QuoteAsset&gt;(&<b>mut</b> pool.quote_custodian, account_cap, quote_quantity);
        order_id = pool.next_bid_order_id;
        pool.next_bid_order_id = pool.next_bid_order_id + 1;
        open_orders = &<b>mut</b> pool.bids;
    } <b>else</b> {
        <a href="../deepbook/custodian.md#deepbook_custodian_lock_balance">custodian::lock_balance</a>&lt;BaseAsset&gt;(&<b>mut</b> pool.base_custodian, account_cap, quantity);
        order_id = pool.next_ask_order_id;
        pool.next_ask_order_id = pool.next_ask_order_id + 1;
        open_orders = &<b>mut</b> pool.asks;
    };
    <b>let</b> order = <a href="../deepbook/clob.md#deepbook_clob_Order">Order</a> {
        order_id,
        price,
        quantity,
        is_bid,
        owner: user,
        expire_timestamp,
    };
    <b>let</b> (tick_exists, <b>mut</b> tick_index) = find_leaf(open_orders, price);
    <b>if</b> (!tick_exists) {
        tick_index = insert_leaf(
            open_orders,
            price,
            <a href="../deepbook/clob.md#deepbook_clob_TickLevel">TickLevel</a> {
                price,
                open_orders: linked_table::new(ctx),
            });
    };
    <b>let</b> tick_level = borrow_mut_leaf_by_index(open_orders, tick_index);
    linked_table::push_back(&<b>mut</b> tick_level.open_orders, order_id, order);
    event::emit(<a href="../deepbook/clob.md#deepbook_clob_OrderPlacedV2">OrderPlacedV2</a>&lt;BaseAsset, QuoteAsset&gt; {
        pool_id: *object::uid_as_inner(&pool.id),
        order_id,
        is_bid,
        owner: user,
        base_asset_quantity_placed: quantity,
        price,
        expire_timestamp
    });
    <b>if</b> (!contains(&pool.usr_open_orders, user)) {
        add(&<b>mut</b> pool.usr_open_orders, user, linked_table::new(ctx));
    };
    linked_table::push_back(borrow_mut(&<b>mut</b> pool.usr_open_orders, user), order_id, price);
    <b>return</b> order_id
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


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob.md#deepbook_clob_place_limit_order">place_limit_order</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<b>mut</b> <a href="../deepbook/clob.md#deepbook_clob_Pool">deepbook::clob::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, price: u64, quantity: u64, is_bid: bool, expire_timestamp: u64, restriction: u8, clock: &<a href="../sui/clock.md#sui_clock_Clock">sui::clock::Clock</a>, account_cap: &<a href="../deepbook/custodian.md#deepbook_custodian_AccountCap">deepbook::custodian::AccountCap</a>, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): (u64, u64, bool, u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob.md#deepbook_clob_place_limit_order">place_limit_order</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool: &<b>mut</b> <a href="../deepbook/clob.md#deepbook_clob_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    price: u64,
    quantity: u64,
    is_bid: bool,
    expire_timestamp: u64, // Expiration timestamp in ms in absolute value inclusive.
    restriction: u8,
    clock: &Clock,
    account_cap: &AccountCap,
    ctx: &<b>mut</b> TxContext
): (u64, u64, bool, u64) {
    // If limit bid order, check whether the price is lower than the lowest ask order by checking the min_leaf of asks Critbit Tree.
    // If so, assign the sequence id of the order to be next_bid_order_id and increment next_bid_order_id by 1.
    // Inject the new order to the bids Critbit Tree according to the price and order id.
    // Otherwise, find the price level from the asks Critbit Tree that is no greater than the input price.
    // Match the bid order against the asks Critbit Tree in the same way <b>as</b> a market order but up until the price level found in the previous step.
    // If the bid order is not completely filled, inject the remaining quantity to the bids Critbit Tree according to the input price and order id.
    // If limit ask order, vice versa.
    <b>assert</b>!(quantity &gt; 0, <a href="../deepbook/clob.md#deepbook_clob_EInvalidQuantity">EInvalidQuantity</a>);
    <b>assert</b>!(price &gt; 0, <a href="../deepbook/clob.md#deepbook_clob_EInvalidPrice">EInvalidPrice</a>);
    <b>assert</b>!(price % pool.tick_size == 0, <a href="../deepbook/clob.md#deepbook_clob_EInvalidPrice">EInvalidPrice</a>);
    <b>assert</b>!(quantity % pool.lot_size == 0, <a href="../deepbook/clob.md#deepbook_clob_EInvalidQuantity">EInvalidQuantity</a>);
    <b>assert</b>!(expire_timestamp &gt; clock::timestamp_ms(clock), <a href="../deepbook/clob.md#deepbook_clob_EInvalidExpireTimestamp">EInvalidExpireTimestamp</a>);
    <b>let</b> user = object::id(account_cap);
    <b>let</b> base_quantity_filled;
    <b>let</b> quote_quantity_filled;
    <b>if</b> (is_bid) {
        <b>let</b> quote_quantity_original = <a href="../deepbook/custodian.md#deepbook_custodian_account_available_balance">custodian::account_available_balance</a>&lt;QuoteAsset&gt;(
            &pool.quote_custodian,
            user,
        );
        <b>let</b> quote_balance = <a href="../deepbook/custodian.md#deepbook_custodian_decrease_user_available_balance">custodian::decrease_user_available_balance</a>&lt;QuoteAsset&gt;(
            &<b>mut</b> pool.quote_custodian,
            account_cap,
            quote_quantity_original,
        );
        <b>let</b> (base_balance_filled, quote_balance_left) = <a href="../deepbook/clob.md#deepbook_clob_match_bid">match_bid</a>(
            pool,
            quantity,
            price,
            clock::timestamp_ms(clock),
            quote_balance,
        );
        base_quantity_filled = balance::value(&base_balance_filled);
        quote_quantity_filled = quote_quantity_original - balance::value(&quote_balance_left);
        <a href="../deepbook/custodian.md#deepbook_custodian_increase_user_available_balance">custodian::increase_user_available_balance</a>&lt;BaseAsset&gt;(
            &<b>mut</b> pool.base_custodian,
            user,
            base_balance_filled,
        );
        <a href="../deepbook/custodian.md#deepbook_custodian_increase_user_available_balance">custodian::increase_user_available_balance</a>&lt;QuoteAsset&gt;(
            &<b>mut</b> pool.quote_custodian,
            user,
            quote_balance_left,
        );
    } <b>else</b> {
        <b>let</b> base_balance = <a href="../deepbook/custodian.md#deepbook_custodian_decrease_user_available_balance">custodian::decrease_user_available_balance</a>&lt;BaseAsset&gt;(
            &<b>mut</b> pool.base_custodian,
            account_cap,
            quantity,
        );
        <b>let</b> (base_balance_left, quote_balance_filled) = <a href="../deepbook/clob.md#deepbook_clob_match_ask">match_ask</a>(
            pool,
            price,
            clock::timestamp_ms(clock),
            base_balance,
        );
        base_quantity_filled = quantity - balance::value(&base_balance_left);
        quote_quantity_filled = balance::value(&quote_balance_filled);
        <a href="../deepbook/custodian.md#deepbook_custodian_increase_user_available_balance">custodian::increase_user_available_balance</a>&lt;BaseAsset&gt;(
            &<b>mut</b> pool.base_custodian,
            user,
            base_balance_left,
        );
        <a href="../deepbook/custodian.md#deepbook_custodian_increase_user_available_balance">custodian::increase_user_available_balance</a>&lt;QuoteAsset&gt;(
            &<b>mut</b> pool.quote_custodian,
            user,
            quote_balance_filled,
        );
    };
    <b>let</b> order_id;
    <b>if</b> (restriction == <a href="../deepbook/clob.md#deepbook_clob_IMMEDIATE_OR_CANCEL">IMMEDIATE_OR_CANCEL</a>) {
        <b>return</b> (base_quantity_filled, quote_quantity_filled, <b>false</b>, 0)
    };
    <b>if</b> (restriction == <a href="../deepbook/clob.md#deepbook_clob_FILL_OR_KILL">FILL_OR_KILL</a>) {
        <b>assert</b>!(base_quantity_filled == quantity, <a href="../deepbook/clob.md#deepbook_clob_EOrderCannotBeFullyFilled">EOrderCannotBeFullyFilled</a>);
        <b>return</b> (base_quantity_filled, quote_quantity_filled, <b>false</b>, 0)
    };
    <b>if</b> (restriction == <a href="../deepbook/clob.md#deepbook_clob_POST_OR_ABORT">POST_OR_ABORT</a>) {
        <b>assert</b>!(base_quantity_filled == 0, <a href="../deepbook/clob.md#deepbook_clob_EOrderCannotBeFullyPassive">EOrderCannotBeFullyPassive</a>);
        order_id = <a href="../deepbook/clob.md#deepbook_clob_inject_limit_order">inject_limit_order</a>(pool, price, quantity, is_bid, expire_timestamp, account_cap, ctx);
        <b>return</b> (base_quantity_filled, quote_quantity_filled, <b>true</b>, order_id)
    } <b>else</b> {
        <b>assert</b>!(restriction == <a href="../deepbook/clob.md#deepbook_clob_NO_RESTRICTION">NO_RESTRICTION</a>, <a href="../deepbook/clob.md#deepbook_clob_EInvalidRestriction">EInvalidRestriction</a>);
        <b>if</b> (quantity &gt; base_quantity_filled) {
            order_id = <a href="../deepbook/clob.md#deepbook_clob_inject_limit_order">inject_limit_order</a>(
                pool,
                price,
                quantity - base_quantity_filled,
                is_bid,
                expire_timestamp,
                account_cap,
                ctx
            );
            <b>return</b> (base_quantity_filled, quote_quantity_filled, <b>true</b>, order_id)
        };
        <b>return</b> (base_quantity_filled, quote_quantity_filled, <b>false</b>, 0)
    }
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

<a name="deepbook_clob_emit_order_filled"></a>

## Function `emit_order_filled`



<pre><code><b>fun</b> <a href="../deepbook/clob.md#deepbook_clob_emit_order_filled">emit_order_filled</a>&lt;BaseAsset, QuoteAsset&gt;(pool_id: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a>, order: &<a href="../deepbook/clob.md#deepbook_clob_Order">deepbook::clob::Order</a>, base_asset_quantity_filled: u64, taker_commission: u64, maker_rebates: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../deepbook/clob.md#deepbook_clob_emit_order_filled">emit_order_filled</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool_id: ID,
    order: &<a href="../deepbook/clob.md#deepbook_clob_Order">Order</a>,
    base_asset_quantity_filled: u64,
    taker_commission: u64,
    maker_rebates: u64
) {
    event::emit(<a href="../deepbook/clob.md#deepbook_clob_OrderFilledV2">OrderFilledV2</a>&lt;BaseAsset, QuoteAsset&gt; {
        pool_id,
        order_id: order.order_id,
        is_bid: order.is_bid,
        owner: order.owner,
        total_quantity: order.quantity,
        base_asset_quantity_filled,
        // order.quantity = base_asset_quantity_filled + base_asset_quantity_remaining
        // This guarantees that the subtraction will not underflow
        base_asset_quantity_remaining: order.quantity - base_asset_quantity_filled,
        price: order.price,
        taker_commission,
        maker_rebates
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
