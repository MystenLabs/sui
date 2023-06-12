
<a name="0xdee9_clob"></a>

# Module `0xdee9::clob`



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
-  [Function `destroy_empty_level`](#0xdee9_clob_destroy_empty_level)
-  [Function `create_account`](#0xdee9_clob_create_account)
-  [Function `create_pool_`](#0xdee9_clob_create_pool_)
-  [Function `create_pool`](#0xdee9_clob_create_pool)
-  [Function `deposit_base`](#0xdee9_clob_deposit_base)
-  [Function `deposit_quote`](#0xdee9_clob_deposit_quote)
-  [Function `withdraw_base`](#0xdee9_clob_withdraw_base)
-  [Function `withdraw_quote`](#0xdee9_clob_withdraw_quote)
-  [Function `swap_exact_base_for_quote`](#0xdee9_clob_swap_exact_base_for_quote)
-  [Function `swap_exact_quote_for_base`](#0xdee9_clob_swap_exact_quote_for_base)
-  [Function `match_bid_with_quote_quantity`](#0xdee9_clob_match_bid_with_quote_quantity)
-  [Function `match_bid`](#0xdee9_clob_match_bid)
-  [Function `match_ask`](#0xdee9_clob_match_ask)
-  [Function `place_market_order`](#0xdee9_clob_place_market_order)
-  [Function `inject_limit_order`](#0xdee9_clob_inject_limit_order)
-  [Function `place_limit_order`](#0xdee9_clob_place_limit_order)
-  [Function `order_is_bid`](#0xdee9_clob_order_is_bid)
-  [Function `emit_order_canceled`](#0xdee9_clob_emit_order_canceled)
-  [Function `emit_order_filled`](#0xdee9_clob_emit_order_filled)
-  [Function `cancel_order`](#0xdee9_clob_cancel_order)
-  [Function `remove_order`](#0xdee9_clob_remove_order)
-  [Function `cancel_all_orders`](#0xdee9_clob_cancel_all_orders)
-  [Function `batch_cancel_order`](#0xdee9_clob_batch_cancel_order)
-  [Function `list_open_orders`](#0xdee9_clob_list_open_orders)
-  [Function `account_balance`](#0xdee9_clob_account_balance)
-  [Function `get_market_price`](#0xdee9_clob_get_market_price)
-  [Function `get_level2_book_status_bid_side`](#0xdee9_clob_get_level2_book_status_bid_side)
-  [Function `get_level2_book_status_ask_side`](#0xdee9_clob_get_level2_book_status_ask_side)
-  [Function `get_level2_book_status`](#0xdee9_clob_get_level2_book_status)
-  [Function `get_order_status`](#0xdee9_clob_get_order_status)


<pre><code><b>use</b> <a href="">0x1::option</a>;
<b>use</b> <a href="">0x1::type_name</a>;
<b>use</b> <a href="../../../.././build/Sui/docs/balance.md#0x2_balance">0x2::balance</a>;
<b>use</b> <a href="../../../.././build/Sui/docs/clock.md#0x2_clock">0x2::clock</a>;
<b>use</b> <a href="../../../.././build/Sui/docs/coin.md#0x2_coin">0x2::coin</a>;
<b>use</b> <a href="../../../.././build/Sui/docs/event.md#0x2_event">0x2::event</a>;
<b>use</b> <a href="../../../.././build/Sui/docs/linked_table.md#0x2_linked_table">0x2::linked_table</a>;
<b>use</b> <a href="../../../.././build/Sui/docs/object.md#0x2_object">0x2::object</a>;
<b>use</b> <a href="../../../.././build/Sui/docs/sui.md#0x2_sui">0x2::sui</a>;
<b>use</b> <a href="../../../.././build/Sui/docs/table.md#0x2_table">0x2::table</a>;
<b>use</b> <a href="../../../.././build/Sui/docs/transfer.md#0x2_transfer">0x2::transfer</a>;
<b>use</b> <a href="../../../.././build/Sui/docs/tx_context.md#0x2_tx_context">0x2::tx_context</a>;
<b>use</b> <a href="critbit.md#0xdee9_critbit">0xdee9::critbit</a>;
<b>use</b> <a href="custodian.md#0xdee9_custodian">0xdee9::custodian</a>;
<b>use</b> <a href="math.md#0xdee9_math">0xdee9::math</a>;
</code></pre>



<a name="0xdee9_clob_PoolCreated"></a>

## Struct `PoolCreated`

Emitted when a new pool is created


<pre><code><b>struct</b> <a href="clob.md#0xdee9_clob_PoolCreated">PoolCreated</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>pool_id: <a href="../../../.././build/Sui/docs/object.md#0x2_object_ID">object::ID</a></code>
</dt>
<dd>
 object ID of the newly created pool
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

Emitted when a maker order is injected into the order book.


<pre><code><b>struct</b> <a href="clob.md#0xdee9_clob_OrderPlacedV2">OrderPlacedV2</a>&lt;BaseAsset, QuoteAsset&gt; <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>pool_id: <a href="../../../.././build/Sui/docs/object.md#0x2_object_ID">object::ID</a></code>
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
<code>owner: <a href="../../../.././build/Sui/docs/object.md#0x2_object_ID">object::ID</a></code>
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

<a name="0xdee9_clob_OrderCanceled"></a>

## Struct `OrderCanceled`

Emitted when a maker order is canceled.


<pre><code><b>struct</b> <a href="clob.md#0xdee9_clob_OrderCanceled">OrderCanceled</a>&lt;BaseAsset, QuoteAsset&gt; <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>pool_id: <a href="../../../.././build/Sui/docs/object.md#0x2_object_ID">object::ID</a></code>
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
<code>owner: <a href="../../../.././build/Sui/docs/object.md#0x2_object_ID">object::ID</a></code>
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

<a name="0xdee9_clob_OrderFilledV2"></a>

## Struct `OrderFilledV2`

Emitted only when a maker order is filled.


<pre><code><b>struct</b> <a href="clob.md#0xdee9_clob_OrderFilledV2">OrderFilledV2</a>&lt;BaseAsset, QuoteAsset&gt; <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>pool_id: <a href="../../../.././build/Sui/docs/object.md#0x2_object_ID">object::ID</a></code>
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
<code>owner: <a href="../../../.././build/Sui/docs/object.md#0x2_object_ID">object::ID</a></code>
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

Deprecated since v1.0.0, use <code><a href="clob.md#0xdee9_clob_OrderPlacedV2">OrderPlacedV2</a></code> instead.


<pre><code><b>struct</b> <a href="clob.md#0xdee9_clob_OrderPlaced">OrderPlaced</a>&lt;BaseAsset, QuoteAsset&gt; <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>pool_id: <a href="../../../.././build/Sui/docs/object.md#0x2_object_ID">object::ID</a></code>
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
<code>owner: <a href="../../../.././build/Sui/docs/object.md#0x2_object_ID">object::ID</a></code>
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

<a name="0xdee9_clob_OrderFilled"></a>

## Struct `OrderFilled`

Deprecated since v1.0.0, use <code><a href="clob.md#0xdee9_clob_OrderFilledV2">OrderFilledV2</a></code> instead.


<pre><code><b>struct</b> <a href="clob.md#0xdee9_clob_OrderFilled">OrderFilled</a>&lt;BaseAsset, QuoteAsset&gt; <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>pool_id: <a href="../../../.././build/Sui/docs/object.md#0x2_object_ID">object::ID</a></code>
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
<code>owner: <a href="../../../.././build/Sui/docs/object.md#0x2_object_ID">object::ID</a></code>
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


<a name="0xdee9_clob_FLOAT_SCALING"></a>



<pre><code><b>const</b> <a href="clob.md#0xdee9_clob_FLOAT_SCALING">FLOAT_SCALING</a>: u64 = 1000000000;
</code></pre>



<a name="0xdee9_clob_ENotImplemented"></a>



<pre><code><b>const</b> <a href="clob.md#0xdee9_clob_ENotImplemented">ENotImplemented</a>: u64 = 1;
</code></pre>



<a name="0xdee9_clob_DEPRECATED"></a>



<pre><code><b>const</b> <a href="clob.md#0xdee9_clob_DEPRECATED">DEPRECATED</a>: u64 = 0;
</code></pre>



<a name="0xdee9_clob_EInsufficientBaseCoin"></a>



<pre><code><b>const</b> <a href="clob.md#0xdee9_clob_EInsufficientBaseCoin">EInsufficientBaseCoin</a>: u64 = 7;
</code></pre>



<a name="0xdee9_clob_EInsufficientQuoteCoin"></a>



<pre><code><b>const</b> <a href="clob.md#0xdee9_clob_EInsufficientQuoteCoin">EInsufficientQuoteCoin</a>: u64 = 8;
</code></pre>



<a name="0xdee9_clob_EInvalidBaseBalance"></a>



<pre><code><b>const</b> <a href="clob.md#0xdee9_clob_EInvalidBaseBalance">EInvalidBaseBalance</a>: u64 = 17;
</code></pre>



<a name="0xdee9_clob_EInvalidExpireTimestamp"></a>



<pre><code><b>const</b> <a href="clob.md#0xdee9_clob_EInvalidExpireTimestamp">EInvalidExpireTimestamp</a>: u64 = 19;
</code></pre>



<a name="0xdee9_clob_EInvalidFee"></a>



<pre><code><b>const</b> <a href="clob.md#0xdee9_clob_EInvalidFee">EInvalidFee</a>: u64 = 18;
</code></pre>



<a name="0xdee9_clob_EInvalidFeeRateRebateRate"></a>



<pre><code><b>const</b> <a href="clob.md#0xdee9_clob_EInvalidFeeRateRebateRate">EInvalidFeeRateRebateRate</a>: u64 = 2;
</code></pre>



<a name="0xdee9_clob_EInvalidOrderId"></a>



<pre><code><b>const</b> <a href="clob.md#0xdee9_clob_EInvalidOrderId">EInvalidOrderId</a>: u64 = 3;
</code></pre>



<a name="0xdee9_clob_EInvalidPair"></a>



<pre><code><b>const</b> <a href="clob.md#0xdee9_clob_EInvalidPair">EInvalidPair</a>: u64 = 16;
</code></pre>



<a name="0xdee9_clob_EInvalidPrice"></a>



<pre><code><b>const</b> <a href="clob.md#0xdee9_clob_EInvalidPrice">EInvalidPrice</a>: u64 = 5;
</code></pre>



<a name="0xdee9_clob_EInvalidQuantity"></a>



<pre><code><b>const</b> <a href="clob.md#0xdee9_clob_EInvalidQuantity">EInvalidQuantity</a>: u64 = 6;
</code></pre>



<a name="0xdee9_clob_EInvalidRestriction"></a>



<pre><code><b>const</b> <a href="clob.md#0xdee9_clob_EInvalidRestriction">EInvalidRestriction</a>: u64 = 14;
</code></pre>



<a name="0xdee9_clob_EInvalidTickPrice"></a>



<pre><code><b>const</b> <a href="clob.md#0xdee9_clob_EInvalidTickPrice">EInvalidTickPrice</a>: u64 = 11;
</code></pre>



<a name="0xdee9_clob_EInvalidTickSizeLotSize"></a>



<pre><code><b>const</b> <a href="clob.md#0xdee9_clob_EInvalidTickSizeLotSize">EInvalidTickSizeLotSize</a>: u64 = 20;
</code></pre>



<a name="0xdee9_clob_EInvalidUser"></a>



<pre><code><b>const</b> <a href="clob.md#0xdee9_clob_EInvalidUser">EInvalidUser</a>: u64 = 12;
</code></pre>



<a name="0xdee9_clob_ELevelNotEmpty"></a>



<pre><code><b>const</b> <a href="clob.md#0xdee9_clob_ELevelNotEmpty">ELevelNotEmpty</a>: u64 = 15;
</code></pre>



<a name="0xdee9_clob_ENotEqual"></a>



<pre><code><b>const</b> <a href="clob.md#0xdee9_clob_ENotEqual">ENotEqual</a>: u64 = 13;
</code></pre>



<a name="0xdee9_clob_EOrderCannotBeFullyFilled"></a>



<pre><code><b>const</b> <a href="clob.md#0xdee9_clob_EOrderCannotBeFullyFilled">EOrderCannotBeFullyFilled</a>: u64 = 9;
</code></pre>



<a name="0xdee9_clob_EOrderCannotBeFullyPassive"></a>



<pre><code><b>const</b> <a href="clob.md#0xdee9_clob_EOrderCannotBeFullyPassive">EOrderCannotBeFullyPassive</a>: u64 = 10;
</code></pre>



<a name="0xdee9_clob_EUnauthorizedCancel"></a>



<pre><code><b>const</b> <a href="clob.md#0xdee9_clob_EUnauthorizedCancel">EUnauthorizedCancel</a>: u64 = 4;
</code></pre>



<a name="0xdee9_clob_FEE_AMOUNT_FOR_CREATE_POOL"></a>



<pre><code><b>const</b> <a href="clob.md#0xdee9_clob_FEE_AMOUNT_FOR_CREATE_POOL">FEE_AMOUNT_FOR_CREATE_POOL</a>: u64 = 100000000000;
</code></pre>



<a name="0xdee9_clob_FILL_OR_KILL"></a>



<pre><code><b>const</b> <a href="clob.md#0xdee9_clob_FILL_OR_KILL">FILL_OR_KILL</a>: u8 = 2;
</code></pre>



<a name="0xdee9_clob_IMMEDIATE_OR_CANCEL"></a>



<pre><code><b>const</b> <a href="clob.md#0xdee9_clob_IMMEDIATE_OR_CANCEL">IMMEDIATE_OR_CANCEL</a>: u8 = 1;
</code></pre>



<a name="0xdee9_clob_MAX_PRICE"></a>



<pre><code><b>const</b> <a href="clob.md#0xdee9_clob_MAX_PRICE">MAX_PRICE</a>: u64 = 9223372036854775808;
</code></pre>



<a name="0xdee9_clob_MIN_ASK_ORDER_ID"></a>



<pre><code><b>const</b> <a href="clob.md#0xdee9_clob_MIN_ASK_ORDER_ID">MIN_ASK_ORDER_ID</a>: u64 = 9223372036854775808;
</code></pre>



<a name="0xdee9_clob_MIN_BID_ORDER_ID"></a>



<pre><code><b>const</b> <a href="clob.md#0xdee9_clob_MIN_BID_ORDER_ID">MIN_BID_ORDER_ID</a>: u64 = 1;
</code></pre>



<a name="0xdee9_clob_MIN_PRICE"></a>



<pre><code><b>const</b> <a href="clob.md#0xdee9_clob_MIN_PRICE">MIN_PRICE</a>: u64 = 0;
</code></pre>



<a name="0xdee9_clob_NO_RESTRICTION"></a>



<pre><code><b>const</b> <a href="clob.md#0xdee9_clob_NO_RESTRICTION">NO_RESTRICTION</a>: u8 = 0;
</code></pre>



<a name="0xdee9_clob_N_RESTRICTIONS"></a>



<pre><code><b>const</b> <a href="clob.md#0xdee9_clob_N_RESTRICTIONS">N_RESTRICTIONS</a>: u8 = 4;
</code></pre>



<a name="0xdee9_clob_POST_OR_ABORT"></a>



<pre><code><b>const</b> <a href="clob.md#0xdee9_clob_POST_OR_ABORT">POST_OR_ABORT</a>: u8 = 3;
</code></pre>



<a name="0xdee9_clob_REFERENCE_MAKER_REBATE_RATE"></a>



<pre><code><b>const</b> <a href="clob.md#0xdee9_clob_REFERENCE_MAKER_REBATE_RATE">REFERENCE_MAKER_REBATE_RATE</a>: u64 = 2500000;
</code></pre>



<a name="0xdee9_clob_REFERENCE_TAKER_FEE_RATE"></a>



<pre><code><b>const</b> <a href="clob.md#0xdee9_clob_REFERENCE_TAKER_FEE_RATE">REFERENCE_TAKER_FEE_RATE</a>: u64 = 5000000;
</code></pre>



<a name="0xdee9_clob_TIMESTAMP_INF"></a>



<pre><code><b>const</b> <a href="clob.md#0xdee9_clob_TIMESTAMP_INF">TIMESTAMP_INF</a>: u64 = 9223372036854775808;
</code></pre>



<a name="0xdee9_clob_destroy_empty_level"></a>

## Function `destroy_empty_level`



<pre><code><b>fun</b> <a href="clob.md#0xdee9_clob_destroy_empty_level">destroy_empty_level</a>(level: <a href="clob.md#0xdee9_clob_TickLevel">clob::TickLevel</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="clob.md#0xdee9_clob_destroy_empty_level">destroy_empty_level</a>(level: <a href="clob.md#0xdee9_clob_TickLevel">TickLevel</a>) {
    <b>let</b> <a href="clob.md#0xdee9_clob_TickLevel">TickLevel</a> {
        price: _,
        open_orders: orders,
    } = level;

    <a href="../../../.././build/Sui/docs/linked_table.md#0x2_linked_table_destroy_empty">linked_table::destroy_empty</a>(orders);
}
</code></pre>



</details>

<a name="0xdee9_clob_create_account"></a>

## Function `create_account`



<pre><code><b>public</b> <b>fun</b> <a href="clob.md#0xdee9_clob_create_account">create_account</a>(_ctx: &<b>mut</b> <a href="../../../.././build/Sui/docs/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="custodian.md#0xdee9_custodian_AccountCap">custodian::AccountCap</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="clob.md#0xdee9_clob_create_account">create_account</a>(_ctx: &<b>mut</b> TxContext): AccountCap {
    <b>abort</b> <a href="clob.md#0xdee9_clob_DEPRECATED">DEPRECATED</a>
}
</code></pre>



</details>

<a name="0xdee9_clob_create_pool_"></a>

## Function `create_pool_`



<pre><code><b>fun</b> <a href="clob.md#0xdee9_clob_create_pool_">create_pool_</a>&lt;BaseAsset, QuoteAsset&gt;(taker_fee_rate: u64, maker_rebate_rate: u64, tick_size: u64, lot_size: u64, creation_fee: <a href="../../../.././build/Sui/docs/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="../../../.././build/Sui/docs/sui.md#0x2_sui_SUI">sui::SUI</a>&gt;, ctx: &<b>mut</b> <a href="../../../.././build/Sui/docs/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="clob.md#0xdee9_clob_create_pool_">create_pool_</a>&lt;BaseAsset, QuoteAsset&gt;(
    taker_fee_rate: u64,
    maker_rebate_rate: u64,
    tick_size: u64,
    lot_size: u64,
    creation_fee: Balance&lt;SUI&gt;,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> base_type_name = <a href="_get">type_name::get</a>&lt;BaseAsset&gt;();
    <b>let</b> quote_type_name = <a href="_get">type_name::get</a>&lt;QuoteAsset&gt;();

    <b>assert</b>!(clob_math::unsafe_mul(lot_size, tick_size) &gt; 0, <a href="clob.md#0xdee9_clob_EInvalidTickSizeLotSize">EInvalidTickSizeLotSize</a>);
    <b>assert</b>!(base_type_name != quote_type_name, <a href="clob.md#0xdee9_clob_EInvalidPair">EInvalidPair</a>);
    <b>assert</b>!(taker_fee_rate &gt;= maker_rebate_rate, <a href="clob.md#0xdee9_clob_EInvalidFeeRateRebateRate">EInvalidFeeRateRebateRate</a>);

    <b>let</b> pool_uid = <a href="../../../.././build/Sui/docs/object.md#0x2_object_new">object::new</a>(ctx);
    <b>let</b> pool_id = *<a href="../../../.././build/Sui/docs/object.md#0x2_object_uid_as_inner">object::uid_as_inner</a>(&pool_uid);
    <a href="../../../.././build/Sui/docs/transfer.md#0x2_transfer_share_object">transfer::share_object</a>(
        <a href="clob.md#0xdee9_clob_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt; {
            id: pool_uid,
            bids: <a href="critbit.md#0xdee9_critbit_new">critbit::new</a>(ctx),
            asks: <a href="critbit.md#0xdee9_critbit_new">critbit::new</a>(ctx),
            next_bid_order_id: <a href="clob.md#0xdee9_clob_MIN_BID_ORDER_ID">MIN_BID_ORDER_ID</a>,
            next_ask_order_id: <a href="clob.md#0xdee9_clob_MIN_ASK_ORDER_ID">MIN_ASK_ORDER_ID</a>,
            usr_open_orders: <a href="../../../.././build/Sui/docs/table.md#0x2_table_new">table::new</a>(ctx),
            taker_fee_rate,
            maker_rebate_rate,
            tick_size,
            lot_size,
            base_custodian: <a href="custodian.md#0xdee9_custodian_new">custodian::new</a>&lt;BaseAsset&gt;(ctx),
            quote_custodian: <a href="custodian.md#0xdee9_custodian_new">custodian::new</a>&lt;QuoteAsset&gt;(ctx),
            creation_fee,
            base_asset_trading_fees: <a href="../../../.././build/Sui/docs/balance.md#0x2_balance_zero">balance::zero</a>(),
            quote_asset_trading_fees: <a href="../../../.././build/Sui/docs/balance.md#0x2_balance_zero">balance::zero</a>(),
        }
    );
    <a href="../../../.././build/Sui/docs/event.md#0x2_event_emit">event::emit</a>(<a href="clob.md#0xdee9_clob_PoolCreated">PoolCreated</a> {
        pool_id,
        base_asset: base_type_name,
        quote_asset: quote_type_name,
        taker_fee_rate,
        maker_rebate_rate,
        tick_size,
        lot_size,
    })
}
</code></pre>



</details>

<a name="0xdee9_clob_create_pool"></a>

## Function `create_pool`



<pre><code><b>public</b> <b>fun</b> <a href="clob.md#0xdee9_clob_create_pool">create_pool</a>&lt;BaseAsset, QuoteAsset&gt;(_tick_size: u64, _lot_size: u64, _creation_fee: <a href="../../../.././build/Sui/docs/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;<a href="../../../.././build/Sui/docs/sui.md#0x2_sui_SUI">sui::SUI</a>&gt;, _ctx: &<b>mut</b> <a href="../../../.././build/Sui/docs/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="clob.md#0xdee9_clob_create_pool">create_pool</a>&lt;BaseAsset, QuoteAsset&gt;(
    _tick_size: u64,
    _lot_size: u64,
    _creation_fee: Coin&lt;SUI&gt;,
    _ctx: &<b>mut</b> TxContext,
) {
    <b>abort</b> <a href="clob.md#0xdee9_clob_DEPRECATED">DEPRECATED</a>
}
</code></pre>



</details>

<a name="0xdee9_clob_deposit_base"></a>

## Function `deposit_base`



<pre><code><b>public</b> <b>fun</b> <a href="clob.md#0xdee9_clob_deposit_base">deposit_base</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<b>mut</b> <a href="clob.md#0xdee9_clob_Pool">clob::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, <a href="../../../.././build/Sui/docs/coin.md#0x2_coin">coin</a>: <a href="../../../.././build/Sui/docs/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;BaseAsset&gt;, account_cap: &<a href="custodian.md#0xdee9_custodian_AccountCap">custodian::AccountCap</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="clob.md#0xdee9_clob_deposit_base">deposit_base</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool: &<b>mut</b> <a href="clob.md#0xdee9_clob_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    <a href="../../../.././build/Sui/docs/coin.md#0x2_coin">coin</a>: Coin&lt;BaseAsset&gt;,
    account_cap: &AccountCap
) {
    <b>assert</b>!(<a href="../../../.././build/Sui/docs/coin.md#0x2_coin_value">coin::value</a>(&<a href="../../../.././build/Sui/docs/coin.md#0x2_coin">coin</a>) != 0, <a href="clob.md#0xdee9_clob_EInsufficientBaseCoin">EInsufficientBaseCoin</a>);
    <a href="custodian.md#0xdee9_custodian_increase_user_available_balance">custodian::increase_user_available_balance</a>(
        &<b>mut</b> pool.base_custodian,
        <a href="../../../.././build/Sui/docs/object.md#0x2_object_id">object::id</a>(account_cap),
        <a href="../../../.././build/Sui/docs/coin.md#0x2_coin_into_balance">coin::into_balance</a>(<a href="../../../.././build/Sui/docs/coin.md#0x2_coin">coin</a>)
    )
}
</code></pre>



</details>

<a name="0xdee9_clob_deposit_quote"></a>

## Function `deposit_quote`



<pre><code><b>public</b> <b>fun</b> <a href="clob.md#0xdee9_clob_deposit_quote">deposit_quote</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<b>mut</b> <a href="clob.md#0xdee9_clob_Pool">clob::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, <a href="../../../.././build/Sui/docs/coin.md#0x2_coin">coin</a>: <a href="../../../.././build/Sui/docs/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;QuoteAsset&gt;, account_cap: &<a href="custodian.md#0xdee9_custodian_AccountCap">custodian::AccountCap</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="clob.md#0xdee9_clob_deposit_quote">deposit_quote</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool: &<b>mut</b> <a href="clob.md#0xdee9_clob_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    <a href="../../../.././build/Sui/docs/coin.md#0x2_coin">coin</a>: Coin&lt;QuoteAsset&gt;,
    account_cap: &AccountCap
) {
    <b>assert</b>!(<a href="../../../.././build/Sui/docs/coin.md#0x2_coin_value">coin::value</a>(&<a href="../../../.././build/Sui/docs/coin.md#0x2_coin">coin</a>) != 0, <a href="clob.md#0xdee9_clob_EInsufficientQuoteCoin">EInsufficientQuoteCoin</a>);
    <a href="custodian.md#0xdee9_custodian_increase_user_available_balance">custodian::increase_user_available_balance</a>(
        &<b>mut</b> pool.quote_custodian,
        <a href="../../../.././build/Sui/docs/object.md#0x2_object_id">object::id</a>(account_cap),
        <a href="../../../.././build/Sui/docs/coin.md#0x2_coin_into_balance">coin::into_balance</a>(<a href="../../../.././build/Sui/docs/coin.md#0x2_coin">coin</a>)
    )
}
</code></pre>



</details>

<a name="0xdee9_clob_withdraw_base"></a>

## Function `withdraw_base`



<pre><code><b>public</b> <b>fun</b> <a href="clob.md#0xdee9_clob_withdraw_base">withdraw_base</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<b>mut</b> <a href="clob.md#0xdee9_clob_Pool">clob::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, quantity: u64, account_cap: &<a href="custodian.md#0xdee9_custodian_AccountCap">custodian::AccountCap</a>, ctx: &<b>mut</b> <a href="../../../.././build/Sui/docs/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="../../../.././build/Sui/docs/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;BaseAsset&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="clob.md#0xdee9_clob_withdraw_base">withdraw_base</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool: &<b>mut</b> <a href="clob.md#0xdee9_clob_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    quantity: u64,
    account_cap: &AccountCap,
    ctx: &<b>mut</b> TxContext
): Coin&lt;BaseAsset&gt; {
    <b>assert</b>!(quantity &gt; 0, <a href="clob.md#0xdee9_clob_EInvalidQuantity">EInvalidQuantity</a>);
    <a href="custodian.md#0xdee9_custodian_withdraw_asset">custodian::withdraw_asset</a>(&<b>mut</b> pool.base_custodian, quantity, account_cap, ctx)
}
</code></pre>



</details>

<a name="0xdee9_clob_withdraw_quote"></a>

## Function `withdraw_quote`



<pre><code><b>public</b> <b>fun</b> <a href="clob.md#0xdee9_clob_withdraw_quote">withdraw_quote</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<b>mut</b> <a href="clob.md#0xdee9_clob_Pool">clob::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, quantity: u64, account_cap: &<a href="custodian.md#0xdee9_custodian_AccountCap">custodian::AccountCap</a>, ctx: &<b>mut</b> <a href="../../../.././build/Sui/docs/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="../../../.././build/Sui/docs/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;QuoteAsset&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="clob.md#0xdee9_clob_withdraw_quote">withdraw_quote</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool: &<b>mut</b> <a href="clob.md#0xdee9_clob_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    quantity: u64,
    account_cap: &AccountCap,
    ctx: &<b>mut</b> TxContext
): Coin&lt;QuoteAsset&gt; {
    <b>assert</b>!(quantity &gt; 0, <a href="clob.md#0xdee9_clob_EInvalidQuantity">EInvalidQuantity</a>);
    <a href="custodian.md#0xdee9_custodian_withdraw_asset">custodian::withdraw_asset</a>(&<b>mut</b> pool.quote_custodian, quantity, account_cap, ctx)
}
</code></pre>



</details>

<a name="0xdee9_clob_swap_exact_base_for_quote"></a>

## Function `swap_exact_base_for_quote`



<pre><code><b>public</b> <b>fun</b> <a href="clob.md#0xdee9_clob_swap_exact_base_for_quote">swap_exact_base_for_quote</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<b>mut</b> <a href="clob.md#0xdee9_clob_Pool">clob::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, quantity: u64, base_coin: <a href="../../../.././build/Sui/docs/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;BaseAsset&gt;, quote_coin: <a href="../../../.././build/Sui/docs/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;QuoteAsset&gt;, <a href="../../../.././build/Sui/docs/clock.md#0x2_clock">clock</a>: &<a href="../../../.././build/Sui/docs/clock.md#0x2_clock_Clock">clock::Clock</a>, ctx: &<b>mut</b> <a href="../../../.././build/Sui/docs/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): (<a href="../../../.././build/Sui/docs/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;BaseAsset&gt;, <a href="../../../.././build/Sui/docs/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;QuoteAsset&gt;, u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="clob.md#0xdee9_clob_swap_exact_base_for_quote">swap_exact_base_for_quote</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool: &<b>mut</b> <a href="clob.md#0xdee9_clob_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    quantity: u64,
    base_coin: Coin&lt;BaseAsset&gt;,
    quote_coin: Coin&lt;QuoteAsset&gt;,
    <a href="../../../.././build/Sui/docs/clock.md#0x2_clock">clock</a>: &Clock,
    ctx: &<b>mut</b> TxContext,
): (Coin&lt;BaseAsset&gt;, Coin&lt;QuoteAsset&gt;, u64) {
    <b>assert</b>!(quantity &gt; 0, <a href="clob.md#0xdee9_clob_EInvalidQuantity">EInvalidQuantity</a>);
    <b>assert</b>!(<a href="../../../.././build/Sui/docs/coin.md#0x2_coin_value">coin::value</a>(&base_coin) &gt;= quantity, <a href="clob.md#0xdee9_clob_EInsufficientBaseCoin">EInsufficientBaseCoin</a>);
    <b>let</b> original_val = <a href="../../../.././build/Sui/docs/coin.md#0x2_coin_value">coin::value</a>(&quote_coin);
    <b>let</b> (ret_base_coin, ret_quote_coin) = <a href="clob.md#0xdee9_clob_place_market_order">place_market_order</a>(
        pool,
        quantity,
        <b>false</b>,
        base_coin,
        quote_coin,
        <a href="../../../.././build/Sui/docs/clock.md#0x2_clock">clock</a>,
        ctx
    );
    <b>let</b> ret_val = <a href="../../../.././build/Sui/docs/coin.md#0x2_coin_value">coin::value</a>(&ret_quote_coin);
    (ret_base_coin, ret_quote_coin, ret_val - original_val)
}
</code></pre>



</details>

<a name="0xdee9_clob_swap_exact_quote_for_base"></a>

## Function `swap_exact_quote_for_base`



<pre><code><b>public</b> <b>fun</b> <a href="clob.md#0xdee9_clob_swap_exact_quote_for_base">swap_exact_quote_for_base</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<b>mut</b> <a href="clob.md#0xdee9_clob_Pool">clob::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, quantity: u64, <a href="../../../.././build/Sui/docs/clock.md#0x2_clock">clock</a>: &<a href="../../../.././build/Sui/docs/clock.md#0x2_clock_Clock">clock::Clock</a>, quote_coin: <a href="../../../.././build/Sui/docs/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;QuoteAsset&gt;, ctx: &<b>mut</b> <a href="../../../.././build/Sui/docs/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): (<a href="../../../.././build/Sui/docs/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;BaseAsset&gt;, <a href="../../../.././build/Sui/docs/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;QuoteAsset&gt;, u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="clob.md#0xdee9_clob_swap_exact_quote_for_base">swap_exact_quote_for_base</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool: &<b>mut</b> <a href="clob.md#0xdee9_clob_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    quantity: u64,
    <a href="../../../.././build/Sui/docs/clock.md#0x2_clock">clock</a>: &Clock,
    quote_coin: Coin&lt;QuoteAsset&gt;,
    ctx: &<b>mut</b> TxContext,
): (Coin&lt;BaseAsset&gt;, Coin&lt;QuoteAsset&gt;, u64) {
    <b>assert</b>!(quantity &gt; 0, <a href="clob.md#0xdee9_clob_EInvalidQuantity">EInvalidQuantity</a>);
    <b>assert</b>!(<a href="../../../.././build/Sui/docs/coin.md#0x2_coin_value">coin::value</a>(&quote_coin) &gt;= quantity, <a href="clob.md#0xdee9_clob_EInsufficientQuoteCoin">EInsufficientQuoteCoin</a>);
    <b>let</b> (base_asset_balance, quote_asset_balance) = <a href="clob.md#0xdee9_clob_match_bid_with_quote_quantity">match_bid_with_quote_quantity</a>(
        pool,
        quantity,
        <a href="clob.md#0xdee9_clob_MAX_PRICE">MAX_PRICE</a>,
        <a href="../../../.././build/Sui/docs/clock.md#0x2_clock_timestamp_ms">clock::timestamp_ms</a>(<a href="../../../.././build/Sui/docs/clock.md#0x2_clock">clock</a>),
        <a href="../../../.././build/Sui/docs/coin.md#0x2_coin_into_balance">coin::into_balance</a>(quote_coin)
    );
    <b>let</b> val = <a href="../../../.././build/Sui/docs/balance.md#0x2_balance_value">balance::value</a>(&base_asset_balance);
    (<a href="../../../.././build/Sui/docs/coin.md#0x2_coin_from_balance">coin::from_balance</a>(base_asset_balance, ctx), <a href="../../../.././build/Sui/docs/coin.md#0x2_coin_from_balance">coin::from_balance</a>(quote_asset_balance, ctx), val)
}
</code></pre>



</details>

<a name="0xdee9_clob_match_bid_with_quote_quantity"></a>

## Function `match_bid_with_quote_quantity`



<pre><code><b>fun</b> <a href="clob.md#0xdee9_clob_match_bid_with_quote_quantity">match_bid_with_quote_quantity</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<b>mut</b> <a href="clob.md#0xdee9_clob_Pool">clob::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, quantity: u64, price_limit: u64, current_timestamp: u64, quote_balance: <a href="../../../.././build/Sui/docs/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;QuoteAsset&gt;): (<a href="../../../.././build/Sui/docs/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;BaseAsset&gt;, <a href="../../../.././build/Sui/docs/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;QuoteAsset&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="clob.md#0xdee9_clob_match_bid_with_quote_quantity">match_bid_with_quote_quantity</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool: &<b>mut</b> <a href="clob.md#0xdee9_clob_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    quantity: u64,
    price_limit: u64,
    current_timestamp: u64,
    quote_balance: Balance&lt;QuoteAsset&gt;,
): (Balance&lt;BaseAsset&gt;, Balance&lt;QuoteAsset&gt;) {
    // Base <a href="../../../.././build/Sui/docs/balance.md#0x2_balance">balance</a> received by taker, taking into account of taker commission.
    // Need <b>to</b> individually keep track of the remaining base quantity <b>to</b> be filled <b>to</b> avoid infinite <b>loop</b>.
    <b>let</b> pool_id = *<a href="../../../.././build/Sui/docs/object.md#0x2_object_uid_as_inner">object::uid_as_inner</a>(&pool.id);
    <b>let</b> taker_quote_quantity_remaining = quantity;
    <b>let</b> base_balance_filled = <a href="../../../.././build/Sui/docs/balance.md#0x2_balance_zero">balance::zero</a>&lt;BaseAsset&gt;();
    <b>let</b> quote_balance_left = quote_balance;
    <b>let</b> all_open_orders = &<b>mut</b> pool.asks;
    <b>if</b> (<a href="critbit.md#0xdee9_critbit_is_empty">critbit::is_empty</a>(all_open_orders)) {
        <b>return</b> (base_balance_filled, quote_balance_left)
    };
    <b>let</b> (tick_price, tick_index) = min_leaf(all_open_orders);
    <b>let</b> terminate_loop = <b>false</b>;

    <b>while</b> (!is_empty&lt;<a href="clob.md#0xdee9_clob_TickLevel">TickLevel</a>&gt;(all_open_orders) && tick_price &lt;= price_limit) {
        <b>let</b> tick_level = borrow_mut_leaf_by_index(all_open_orders, tick_index);
        <b>let</b> order_id = *<a href="_borrow">option::borrow</a>(<a href="../../../.././build/Sui/docs/linked_table.md#0x2_linked_table_front">linked_table::front</a>(&tick_level.open_orders));

        <b>while</b> (!<a href="../../../.././build/Sui/docs/linked_table.md#0x2_linked_table_is_empty">linked_table::is_empty</a>(&tick_level.open_orders)) {
            <b>let</b> maker_order = <a href="../../../.././build/Sui/docs/linked_table.md#0x2_linked_table_borrow">linked_table::borrow</a>(&tick_level.open_orders, order_id);
            <b>let</b> maker_base_quantity = maker_order.quantity;
            <b>let</b> skip_order = <b>false</b>;

            <b>if</b> (maker_order.expire_timestamp &lt;= current_timestamp) {
                skip_order = <b>true</b>;
                <a href="custodian.md#0xdee9_custodian_unlock_balance">custodian::unlock_balance</a>(&<b>mut</b> pool.base_custodian, maker_order.owner, maker_order.quantity);
                <a href="clob.md#0xdee9_clob_emit_order_canceled">emit_order_canceled</a>&lt;BaseAsset, QuoteAsset&gt;(pool_id, maker_order);
            } <b>else</b> {
                // Calculate how much quote asset (maker_quote_quantity) is required, including the commission, <b>to</b> fill the maker order.
                <b>let</b> maker_quote_quantity_without_commission = clob_math::mul(
                    maker_base_quantity,
                    maker_order.price
                );
                <b>let</b> (is_round_down, taker_commission)  = clob_math::unsafe_mul_round(
                    maker_quote_quantity_without_commission,
                    pool.taker_fee_rate
                );
                <b>if</b> (is_round_down)  taker_commission = taker_commission + 1;

                <b>let</b> maker_quote_quantity = maker_quote_quantity_without_commission + taker_commission;

                // Total base quantity filled.
                <b>let</b> filled_base_quantity: u64;
                // Total quote quantity filled, excluding commission and rebate.
                <b>let</b> filled_quote_quantity: u64;
                // Total quote quantity paid by taker.
                // filled_quote_quantity_without_commission * (<a href="clob.md#0xdee9_clob_FLOAT_SCALING">FLOAT_SCALING</a> + taker_fee_rate) = filled_quote_quantity
                <b>let</b> filled_quote_quantity_without_commission: u64;
                <b>if</b> (taker_quote_quantity_remaining &gt; maker_quote_quantity) {
                    filled_quote_quantity = maker_quote_quantity;
                    filled_quote_quantity_without_commission = maker_quote_quantity_without_commission;
                    filled_base_quantity = maker_base_quantity;
                } <b>else</b> {
                    terminate_loop = <b>true</b>;
                    // <b>if</b> not enough quote quantity <b>to</b> pay for taker commission, then no quantity will be filled
                    filled_quote_quantity_without_commission = clob_math::unsafe_div(
                        taker_quote_quantity_remaining,
                        <a href="clob.md#0xdee9_clob_FLOAT_SCALING">FLOAT_SCALING</a> + pool.taker_fee_rate
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
                    // <b>if</b> taker_commission = 0 due <b>to</b> underflow, round it up <b>to</b> 1
                    <b>let</b> (round_down, taker_commission) = clob_math::unsafe_mul_round(
                        filled_quote_quantity_without_commission,
                        pool.taker_fee_rate
                    );
                    <b>if</b> (round_down) {
                        taker_commission = taker_commission + 1;
                    };
                    filled_quote_quantity = filled_quote_quantity_without_commission + taker_commission;
                };
                // <b>if</b> maker_rebate = 0 due <b>to</b> underflow, maker will not receive a rebate
                <b>let</b> maker_rebate = clob_math::unsafe_mul(
                    filled_quote_quantity_without_commission,
                    pool.maker_rebate_rate
                );
                maker_base_quantity = maker_base_quantity - filled_base_quantity;

                // maker in ask side, decrease maker's locked base asset, increase maker's available quote asset
                taker_quote_quantity_remaining = taker_quote_quantity_remaining - filled_quote_quantity;
                <b>let</b> locked_base_balance = <a href="custodian.md#0xdee9_custodian_decrease_user_locked_balance">custodian::decrease_user_locked_balance</a>&lt;BaseAsset&gt;(
                    &<b>mut</b> pool.base_custodian,
                    maker_order.owner,
                    filled_base_quantity
                );

                <b>let</b> quote_balance_filled = <a href="../../../.././build/Sui/docs/balance.md#0x2_balance_split">balance::split</a>(
                    &<b>mut</b> quote_balance_left,
                    filled_quote_quantity,
                );
                // Send quote asset including rebate <b>to</b> maker.
                <a href="custodian.md#0xdee9_custodian_increase_user_available_balance">custodian::increase_user_available_balance</a>&lt;QuoteAsset&gt;(
                    &<b>mut</b> pool.quote_custodian,
                    maker_order.owner,
                    <a href="../../../.././build/Sui/docs/balance.md#0x2_balance_split">balance::split</a>(
                        &<b>mut</b> quote_balance_filled,
                        maker_rebate + filled_quote_quantity_without_commission,
                    ),
                );
                // Send remaining of commission - rebate <b>to</b> the protocol.
                // commission - rebate = filled_quote_quantity_without_commission - filled_quote_quantity - maker_rebate
                <a href="../../../.././build/Sui/docs/balance.md#0x2_balance_join">balance::join</a>(&<b>mut</b> pool.quote_asset_trading_fees, quote_balance_filled);
                <a href="../../../.././build/Sui/docs/balance.md#0x2_balance_join">balance::join</a>(&<b>mut</b> base_balance_filled, locked_base_balance);

                <a href="clob.md#0xdee9_clob_emit_order_filled">emit_order_filled</a>&lt;BaseAsset, QuoteAsset&gt;(
                    *<a href="../../../.././build/Sui/docs/object.md#0x2_object_uid_as_inner">object::uid_as_inner</a>(&pool.id),
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
                <b>let</b> maybe_order_id = <a href="../../../.././build/Sui/docs/linked_table.md#0x2_linked_table_next">linked_table::next</a>(&tick_level.open_orders, order_id);
                <b>if</b> (!<a href="_is_none">option::is_none</a>(maybe_order_id)) {
                    order_id = *<a href="_borrow">option::borrow</a>(maybe_order_id);
                };
                <b>let</b> usr_open_order_ids = <a href="../../../.././build/Sui/docs/table.md#0x2_table_borrow_mut">table::borrow_mut</a>(&<b>mut</b> pool.usr_open_orders, maker_order.owner);
                <a href="../../../.././build/Sui/docs/linked_table.md#0x2_linked_table_remove">linked_table::remove</a>(usr_open_order_ids, old_order_id);
                <a href="../../../.././build/Sui/docs/linked_table.md#0x2_linked_table_remove">linked_table::remove</a>(&<b>mut</b> tick_level.open_orders, old_order_id);
            } <b>else</b> {
                // Update the maker order.
                <b>let</b> maker_order_mut = <a href="../../../.././build/Sui/docs/linked_table.md#0x2_linked_table_borrow_mut">linked_table::borrow_mut</a>(
                    &<b>mut</b> tick_level.open_orders,
                    order_id);
                maker_order_mut.quantity = maker_base_quantity;
            };
            <b>if</b> (terminate_loop) {
                <b>break</b>
            };
        };
        <b>if</b> (<a href="../../../.././build/Sui/docs/linked_table.md#0x2_linked_table_is_empty">linked_table::is_empty</a>(&tick_level.open_orders)) {
            (tick_price, _) = next_leaf(all_open_orders, tick_price);
            <a href="clob.md#0xdee9_clob_destroy_empty_level">destroy_empty_level</a>(remove_leaf_by_index(all_open_orders, tick_index));
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

<a name="0xdee9_clob_match_bid"></a>

## Function `match_bid`



<pre><code><b>fun</b> <a href="clob.md#0xdee9_clob_match_bid">match_bid</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<b>mut</b> <a href="clob.md#0xdee9_clob_Pool">clob::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, quantity: u64, price_limit: u64, current_timestamp: u64, quote_balance: <a href="../../../.././build/Sui/docs/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;QuoteAsset&gt;): (<a href="../../../.././build/Sui/docs/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;BaseAsset&gt;, <a href="../../../.././build/Sui/docs/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;QuoteAsset&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="clob.md#0xdee9_clob_match_bid">match_bid</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool: &<b>mut</b> <a href="clob.md#0xdee9_clob_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    quantity: u64,
    price_limit: u64,
    current_timestamp: u64,
    quote_balance: Balance&lt;QuoteAsset&gt;,
): (Balance&lt;BaseAsset&gt;, Balance&lt;QuoteAsset&gt;) {
    <b>let</b> pool_id = *<a href="../../../.././build/Sui/docs/object.md#0x2_object_uid_as_inner">object::uid_as_inner</a>(&pool.id);
    // Base <a href="../../../.././build/Sui/docs/balance.md#0x2_balance">balance</a> received by taker.
    // Need <b>to</b> individually keep track of the remaining base quantity <b>to</b> be filled <b>to</b> avoid infinite <b>loop</b>.
    <b>let</b> taker_base_quantity_remaining = quantity;
    <b>let</b> base_balance_filled = <a href="../../../.././build/Sui/docs/balance.md#0x2_balance_zero">balance::zero</a>&lt;BaseAsset&gt;();
    <b>let</b> quote_balance_left = quote_balance;
    <b>let</b> all_open_orders = &<b>mut</b> pool.asks;
    <b>if</b> (<a href="critbit.md#0xdee9_critbit_is_empty">critbit::is_empty</a>(all_open_orders)) {
        <b>return</b> (base_balance_filled, quote_balance_left)
    };
    <b>let</b> (tick_price, tick_index) = min_leaf(all_open_orders);

    <b>while</b> (!is_empty&lt;<a href="clob.md#0xdee9_clob_TickLevel">TickLevel</a>&gt;(all_open_orders) && tick_price &lt;= price_limit) {
        <b>let</b> tick_level = borrow_mut_leaf_by_index(all_open_orders, tick_index);
        <b>let</b> order_id = *<a href="_borrow">option::borrow</a>(<a href="../../../.././build/Sui/docs/linked_table.md#0x2_linked_table_front">linked_table::front</a>(&tick_level.open_orders));

        <b>while</b> (!<a href="../../../.././build/Sui/docs/linked_table.md#0x2_linked_table_is_empty">linked_table::is_empty</a>(&tick_level.open_orders)) {
            <b>let</b> maker_order = <a href="../../../.././build/Sui/docs/linked_table.md#0x2_linked_table_borrow">linked_table::borrow</a>(&tick_level.open_orders, order_id);
            <b>let</b> maker_base_quantity = maker_order.quantity;
            <b>let</b> skip_order = <b>false</b>;

            <b>if</b> (maker_order.expire_timestamp &lt;= current_timestamp) {
                skip_order = <b>true</b>;
                <a href="custodian.md#0xdee9_custodian_unlock_balance">custodian::unlock_balance</a>(&<b>mut</b> pool.base_custodian, maker_order.owner, maker_order.quantity);
                <a href="clob.md#0xdee9_clob_emit_order_canceled">emit_order_canceled</a>&lt;BaseAsset, QuoteAsset&gt;(pool_id, maker_order);
            } <b>else</b> {
                <b>let</b> filled_base_quantity =
                    <b>if</b> (taker_base_quantity_remaining &gt; maker_base_quantity) { maker_base_quantity }
                    <b>else</b> { taker_base_quantity_remaining };

                <b>let</b> filled_quote_quantity = clob_math::mul(filled_base_quantity, maker_order.price);

                // <b>if</b> maker_rebate = 0 due <b>to</b> underflow, maker will not receive a rebate
                <b>let</b> maker_rebate = clob_math::unsafe_mul(filled_quote_quantity, pool.maker_rebate_rate);
                // <b>if</b> taker_commission = 0 due <b>to</b> underflow, round it up <b>to</b> 1
                <b>let</b> (is_round_down, taker_commission) = clob_math::unsafe_mul_round(
                    filled_quote_quantity,
                    pool.taker_fee_rate
                );
                <b>if</b> (is_round_down) taker_commission = taker_commission + 1;

                maker_base_quantity = maker_base_quantity - filled_base_quantity;

                // maker in ask side, decrease maker's locked base asset, increase maker's available quote asset
                taker_base_quantity_remaining = taker_base_quantity_remaining - filled_base_quantity;
                <b>let</b> locked_base_balance = <a href="custodian.md#0xdee9_custodian_decrease_user_locked_balance">custodian::decrease_user_locked_balance</a>&lt;BaseAsset&gt;(
                    &<b>mut</b> pool.base_custodian,
                    maker_order.owner,
                    filled_base_quantity
                );
                <b>let</b> taker_commission_balance = <a href="../../../.././build/Sui/docs/balance.md#0x2_balance_split">balance::split</a>(
                    &<b>mut</b> quote_balance_left,
                    taker_commission,
                );
                <a href="custodian.md#0xdee9_custodian_increase_user_available_balance">custodian::increase_user_available_balance</a>&lt;QuoteAsset&gt;(
                    &<b>mut</b> pool.quote_custodian,
                    maker_order.owner,
                    <a href="../../../.././build/Sui/docs/balance.md#0x2_balance_split">balance::split</a>(
                        &<b>mut</b> taker_commission_balance,
                        maker_rebate,
                    ),
                );
                <a href="../../../.././build/Sui/docs/balance.md#0x2_balance_join">balance::join</a>(&<b>mut</b> pool.quote_asset_trading_fees, taker_commission_balance);
                <a href="../../../.././build/Sui/docs/balance.md#0x2_balance_join">balance::join</a>(&<b>mut</b> base_balance_filled, locked_base_balance);

                <a href="custodian.md#0xdee9_custodian_increase_user_available_balance">custodian::increase_user_available_balance</a>&lt;QuoteAsset&gt;(
                    &<b>mut</b> pool.quote_custodian,
                    maker_order.owner,
                    <a href="../../../.././build/Sui/docs/balance.md#0x2_balance_split">balance::split</a>(
                        &<b>mut</b> quote_balance_left,
                        filled_quote_quantity,
                    ),
                );

                <a href="clob.md#0xdee9_clob_emit_order_filled">emit_order_filled</a>&lt;BaseAsset, QuoteAsset&gt;(
                    *<a href="../../../.././build/Sui/docs/object.md#0x2_object_uid_as_inner">object::uid_as_inner</a>(&pool.id),
                    maker_order,
                    filled_base_quantity,
                    taker_commission,
                    maker_rebate
                );
            };

            <b>if</b> (skip_order || maker_base_quantity == 0) {
                // Remove the maker order.
                <b>let</b> old_order_id = order_id;
                <b>let</b> maybe_order_id = <a href="../../../.././build/Sui/docs/linked_table.md#0x2_linked_table_next">linked_table::next</a>(&tick_level.open_orders, order_id);
                <b>if</b> (!<a href="_is_none">option::is_none</a>(maybe_order_id)) {
                    order_id = *<a href="_borrow">option::borrow</a>(maybe_order_id);
                };
                <b>let</b> usr_open_order_ids = <a href="../../../.././build/Sui/docs/table.md#0x2_table_borrow_mut">table::borrow_mut</a>(&<b>mut</b> pool.usr_open_orders, maker_order.owner);
                <a href="../../../.././build/Sui/docs/linked_table.md#0x2_linked_table_remove">linked_table::remove</a>(usr_open_order_ids, old_order_id);
                <a href="../../../.././build/Sui/docs/linked_table.md#0x2_linked_table_remove">linked_table::remove</a>(&<b>mut</b> tick_level.open_orders, old_order_id);
            } <b>else</b> {
                // Update the maker order.
                <b>let</b> maker_order_mut = <a href="../../../.././build/Sui/docs/linked_table.md#0x2_linked_table_borrow_mut">linked_table::borrow_mut</a>(
                    &<b>mut</b> tick_level.open_orders,
                    order_id);
                maker_order_mut.quantity = maker_base_quantity;
            };
            <b>if</b> (taker_base_quantity_remaining == 0) {
                <b>break</b>
            };
        };
        <b>if</b> (<a href="../../../.././build/Sui/docs/linked_table.md#0x2_linked_table_is_empty">linked_table::is_empty</a>(&tick_level.open_orders)) {
            (tick_price, _) = next_leaf(all_open_orders, tick_price);
            <a href="clob.md#0xdee9_clob_destroy_empty_level">destroy_empty_level</a>(remove_leaf_by_index(all_open_orders, tick_index));
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

<a name="0xdee9_clob_match_ask"></a>

## Function `match_ask`



<pre><code><b>fun</b> <a href="clob.md#0xdee9_clob_match_ask">match_ask</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<b>mut</b> <a href="clob.md#0xdee9_clob_Pool">clob::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, price_limit: u64, current_timestamp: u64, base_balance: <a href="../../../.././build/Sui/docs/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;BaseAsset&gt;): (<a href="../../../.././build/Sui/docs/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;BaseAsset&gt;, <a href="../../../.././build/Sui/docs/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;QuoteAsset&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="clob.md#0xdee9_clob_match_ask">match_ask</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool: &<b>mut</b> <a href="clob.md#0xdee9_clob_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    price_limit: u64,
    current_timestamp: u64,
    base_balance: Balance&lt;BaseAsset&gt;,
): (Balance&lt;BaseAsset&gt;, Balance&lt;QuoteAsset&gt;) {
    <b>let</b> pool_id = *<a href="../../../.././build/Sui/docs/object.md#0x2_object_uid_as_inner">object::uid_as_inner</a>(&pool.id);
    <b>let</b> base_balance_left = base_balance;
    // Base <a href="../../../.././build/Sui/docs/balance.md#0x2_balance">balance</a> received by taker, taking into account of taker commission.
    <b>let</b> quote_balance_filled = <a href="../../../.././build/Sui/docs/balance.md#0x2_balance_zero">balance::zero</a>&lt;QuoteAsset&gt;();
    <b>let</b> all_open_orders = &<b>mut</b> pool.bids;
    <b>if</b> (<a href="critbit.md#0xdee9_critbit_is_empty">critbit::is_empty</a>(all_open_orders)) {
        <b>return</b> (base_balance_left, quote_balance_filled)
    };
    <b>let</b> (tick_price, tick_index) = max_leaf(all_open_orders);
    <b>while</b> (!is_empty&lt;<a href="clob.md#0xdee9_clob_TickLevel">TickLevel</a>&gt;(all_open_orders) && tick_price &gt;= price_limit) {
        <b>let</b> tick_level = borrow_mut_leaf_by_index(all_open_orders, tick_index);
        <b>let</b> order_id = *<a href="_borrow">option::borrow</a>(<a href="../../../.././build/Sui/docs/linked_table.md#0x2_linked_table_front">linked_table::front</a>(&tick_level.open_orders));
        <b>while</b> (!<a href="../../../.././build/Sui/docs/linked_table.md#0x2_linked_table_is_empty">linked_table::is_empty</a>(&tick_level.open_orders)) {
            <b>let</b> maker_order = <a href="../../../.././build/Sui/docs/linked_table.md#0x2_linked_table_borrow">linked_table::borrow</a>(&tick_level.open_orders, order_id);
            <b>let</b> maker_base_quantity = maker_order.quantity;
            <b>let</b> skip_order = <b>false</b>;

            <b>if</b> (maker_order.expire_timestamp &lt;= current_timestamp) {
                skip_order = <b>true</b>;
                <b>let</b> maker_quote_quantity = clob_math::mul(maker_order.quantity, maker_order.price);
                <a href="custodian.md#0xdee9_custodian_unlock_balance">custodian::unlock_balance</a>(&<b>mut</b> pool.quote_custodian, maker_order.owner, maker_quote_quantity);
                <a href="clob.md#0xdee9_clob_emit_order_canceled">emit_order_canceled</a>&lt;BaseAsset, QuoteAsset&gt;(pool_id, maker_order);
            } <b>else</b> {
                <b>let</b> taker_base_quantity_remaining = <a href="../../../.././build/Sui/docs/balance.md#0x2_balance_value">balance::value</a>(&base_balance_left);
                <b>let</b> filled_base_quantity =
                    <b>if</b> (taker_base_quantity_remaining &gt;= maker_base_quantity) { maker_base_quantity }
                    <b>else</b> { taker_base_quantity_remaining };

                <b>let</b> filled_quote_quantity = clob_math::mul(filled_base_quantity, maker_order.price);

                // <b>if</b> maker_rebate = 0 due <b>to</b> underflow, maker will not receive a rebate
                <b>let</b> maker_rebate = clob_math::unsafe_mul(filled_quote_quantity, pool.maker_rebate_rate);
                // <b>if</b> taker_commission = 0 due <b>to</b> underflow, round it up <b>to</b> 1
                <b>let</b> (is_round_down, taker_commission) = clob_math::unsafe_mul_round(
                    filled_quote_quantity,
                    pool.taker_fee_rate
                );
                <b>if</b> (is_round_down) taker_commission = taker_commission + 1;

                maker_base_quantity = maker_base_quantity - filled_base_quantity;
                // maker in bid side, decrease maker's locked quote asset, increase maker's available base asset
                <b>let</b> locked_quote_balance = <a href="custodian.md#0xdee9_custodian_decrease_user_locked_balance">custodian::decrease_user_locked_balance</a>&lt;QuoteAsset&gt;(
                    &<b>mut</b> pool.quote_custodian,
                    maker_order.owner,
                    filled_quote_quantity
                );
                <b>let</b> taker_commission_balance = <a href="../../../.././build/Sui/docs/balance.md#0x2_balance_split">balance::split</a>(
                    &<b>mut</b> locked_quote_balance,
                    taker_commission,
                );
                <a href="custodian.md#0xdee9_custodian_increase_user_available_balance">custodian::increase_user_available_balance</a>&lt;QuoteAsset&gt;(
                    &<b>mut</b> pool.quote_custodian,
                    maker_order.owner,
                    <a href="../../../.././build/Sui/docs/balance.md#0x2_balance_split">balance::split</a>(
                        &<b>mut</b> taker_commission_balance,
                        maker_rebate,
                    ),
                );
                <a href="../../../.././build/Sui/docs/balance.md#0x2_balance_join">balance::join</a>(&<b>mut</b> pool.quote_asset_trading_fees, taker_commission_balance);
                <a href="../../../.././build/Sui/docs/balance.md#0x2_balance_join">balance::join</a>(&<b>mut</b> quote_balance_filled, locked_quote_balance);

                <a href="custodian.md#0xdee9_custodian_increase_user_available_balance">custodian::increase_user_available_balance</a>&lt;BaseAsset&gt;(
                    &<b>mut</b> pool.base_custodian,
                    maker_order.owner,
                    <a href="../../../.././build/Sui/docs/balance.md#0x2_balance_split">balance::split</a>(
                        &<b>mut</b> base_balance_left,
                        filled_base_quantity,
                    ),
                );

                <a href="clob.md#0xdee9_clob_emit_order_filled">emit_order_filled</a>&lt;BaseAsset, QuoteAsset&gt;(
                    *<a href="../../../.././build/Sui/docs/object.md#0x2_object_uid_as_inner">object::uid_as_inner</a>(&pool.id),
                    maker_order,
                    filled_base_quantity,
                    taker_commission,
                    maker_rebate
                );
            };

            <b>if</b> (skip_order || maker_base_quantity == 0) {
                // Remove the maker order.
                <b>let</b> old_order_id = order_id;
                <b>let</b> maybe_order_id = <a href="../../../.././build/Sui/docs/linked_table.md#0x2_linked_table_next">linked_table::next</a>(&tick_level.open_orders, order_id);
                <b>if</b> (!<a href="_is_none">option::is_none</a>(maybe_order_id)) {
                    order_id = *<a href="_borrow">option::borrow</a>(maybe_order_id);
                };
                <b>let</b> usr_open_order_ids = <a href="../../../.././build/Sui/docs/table.md#0x2_table_borrow_mut">table::borrow_mut</a>(&<b>mut</b> pool.usr_open_orders, maker_order.owner);
                <a href="../../../.././build/Sui/docs/linked_table.md#0x2_linked_table_remove">linked_table::remove</a>(usr_open_order_ids, old_order_id);
                <a href="../../../.././build/Sui/docs/linked_table.md#0x2_linked_table_remove">linked_table::remove</a>(&<b>mut</b> tick_level.open_orders, old_order_id);
            } <b>else</b> {
                // Update the maker order.
                <b>let</b> maker_order_mut = <a href="../../../.././build/Sui/docs/linked_table.md#0x2_linked_table_borrow_mut">linked_table::borrow_mut</a>(
                    &<b>mut</b> tick_level.open_orders,
                    order_id);
                maker_order_mut.quantity = maker_base_quantity;
            };
            <b>if</b> (<a href="../../../.././build/Sui/docs/balance.md#0x2_balance_value">balance::value</a>(&base_balance_left) == 0) {
                <b>break</b>
            };
        };
        <b>if</b> (<a href="../../../.././build/Sui/docs/linked_table.md#0x2_linked_table_is_empty">linked_table::is_empty</a>(&tick_level.open_orders)) {
            (tick_price, _) = previous_leaf(all_open_orders, tick_price);
            <a href="clob.md#0xdee9_clob_destroy_empty_level">destroy_empty_level</a>(remove_leaf_by_index(all_open_orders, tick_index));
            (_, tick_index) = find_leaf(all_open_orders, tick_price);
        };
        <b>if</b> (<a href="../../../.././build/Sui/docs/balance.md#0x2_balance_value">balance::value</a>(&base_balance_left) == 0) {
            <b>break</b>
        };
    };
    <b>return</b> (base_balance_left, quote_balance_filled)
}
</code></pre>



</details>

<a name="0xdee9_clob_place_market_order"></a>

## Function `place_market_order`

Place a market order to the order book.


<pre><code><b>public</b> <b>fun</b> <a href="clob.md#0xdee9_clob_place_market_order">place_market_order</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<b>mut</b> <a href="clob.md#0xdee9_clob_Pool">clob::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, quantity: u64, is_bid: bool, base_coin: <a href="../../../.././build/Sui/docs/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;BaseAsset&gt;, quote_coin: <a href="../../../.././build/Sui/docs/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;QuoteAsset&gt;, <a href="../../../.././build/Sui/docs/clock.md#0x2_clock">clock</a>: &<a href="../../../.././build/Sui/docs/clock.md#0x2_clock_Clock">clock::Clock</a>, ctx: &<b>mut</b> <a href="../../../.././build/Sui/docs/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): (<a href="../../../.././build/Sui/docs/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;BaseAsset&gt;, <a href="../../../.././build/Sui/docs/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;QuoteAsset&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="clob.md#0xdee9_clob_place_market_order">place_market_order</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool: &<b>mut</b> <a href="clob.md#0xdee9_clob_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    quantity: u64,
    is_bid: bool,
    base_coin: Coin&lt;BaseAsset&gt;,
    quote_coin: Coin&lt;QuoteAsset&gt;,
    <a href="../../../.././build/Sui/docs/clock.md#0x2_clock">clock</a>: &Clock,
    ctx: &<b>mut</b> TxContext,
): (Coin&lt;BaseAsset&gt;, Coin&lt;QuoteAsset&gt;) {
    // If market bid order, match against the open ask orders. Otherwise, match against the open bid orders.
    // Take market bid order for example.
    // We first retrieve the PriceLevel <b>with</b> the lowest price by calling min_leaf on the asks Critbit Tree.
    // We then match the market order by iterating through open orders on that price level in ascending order of the order id.
    // Open orders that are being filled are removed from the order book.
    // We stop the iteration untill all quantities are filled.
    // If the total quantity of open orders at the lowest price level is not large enough <b>to</b> fully fill the market order,
    // we <b>move</b> on <b>to</b> the next price level by calling next_leaf on the asks Critbit Tree and repeat the same procedure.
    // Continue iterating over the price levels in ascending order until the market order is completely filled.
    // If ther market order cannot be completely filled even after consuming all the open ask orders,
    // the unfilled quantity will be cancelled.
    // Market ask order follows similar procedure.
    // The difference is that market ask order is matched against the open bid orders.
    // We start <b>with</b> the bid PriceLeve <b>with</b> the highest price by calling max_leaf on the bids Critbit Tree.
    // The inner <b>loop</b> for iterating over the open orders in ascending orders of order id is the same <b>as</b> above.
    // Then iterate over the price levels in descending order until the market order is completely filled.
    <b>assert</b>!(quantity % pool.lot_size == 0, <a href="clob.md#0xdee9_clob_EInvalidQuantity">EInvalidQuantity</a>);
    <b>assert</b>!(quantity != 0, <a href="clob.md#0xdee9_clob_EInvalidQuantity">EInvalidQuantity</a>);
    <b>if</b> (is_bid) {
        <b>let</b> (base_balance_filled, quote_balance_left) = <a href="clob.md#0xdee9_clob_match_bid">match_bid</a>(
            pool,
            quantity,
            <a href="clob.md#0xdee9_clob_MAX_PRICE">MAX_PRICE</a>,
            <a href="../../../.././build/Sui/docs/clock.md#0x2_clock_timestamp_ms">clock::timestamp_ms</a>(<a href="../../../.././build/Sui/docs/clock.md#0x2_clock">clock</a>),
            <a href="../../../.././build/Sui/docs/coin.md#0x2_coin_into_balance">coin::into_balance</a>(quote_coin),
        );
        join(
            &<b>mut</b> base_coin,
            <a href="../../../.././build/Sui/docs/coin.md#0x2_coin_from_balance">coin::from_balance</a>(base_balance_filled, ctx),
        );
        quote_coin = <a href="../../../.././build/Sui/docs/coin.md#0x2_coin_from_balance">coin::from_balance</a>(quote_balance_left, ctx);
    } <b>else</b> {
        <b>assert</b>!(quantity &lt;= <a href="../../../.././build/Sui/docs/coin.md#0x2_coin_value">coin::value</a>(&base_coin), <a href="clob.md#0xdee9_clob_EInsufficientBaseCoin">EInsufficientBaseCoin</a>);
        <b>let</b> (base_balance_left, quote_balance_filled) = <a href="clob.md#0xdee9_clob_match_ask">match_ask</a>(
            pool,
            <a href="clob.md#0xdee9_clob_MIN_PRICE">MIN_PRICE</a>,
            <a href="../../../.././build/Sui/docs/clock.md#0x2_clock_timestamp_ms">clock::timestamp_ms</a>(<a href="../../../.././build/Sui/docs/clock.md#0x2_clock">clock</a>),
            <a href="../../../.././build/Sui/docs/coin.md#0x2_coin_into_balance">coin::into_balance</a>(base_coin),
        );
        base_coin = <a href="../../../.././build/Sui/docs/coin.md#0x2_coin_from_balance">coin::from_balance</a>(base_balance_left, ctx);
        join(
            &<b>mut</b> quote_coin,
            <a href="../../../.././build/Sui/docs/coin.md#0x2_coin_from_balance">coin::from_balance</a>(quote_balance_filled, ctx),
        );
    };
    (base_coin, quote_coin)
}
</code></pre>



</details>

<a name="0xdee9_clob_inject_limit_order"></a>

## Function `inject_limit_order`

Injects a maker order to the order book.
Returns the order id.


<pre><code><b>fun</b> <a href="clob.md#0xdee9_clob_inject_limit_order">inject_limit_order</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<b>mut</b> <a href="clob.md#0xdee9_clob_Pool">clob::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, price: u64, quantity: u64, is_bid: bool, expire_timestamp: u64, account_cap: &<a href="custodian.md#0xdee9_custodian_AccountCap">custodian::AccountCap</a>, ctx: &<b>mut</b> <a href="../../../.././build/Sui/docs/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="clob.md#0xdee9_clob_inject_limit_order">inject_limit_order</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool: &<b>mut</b> <a href="clob.md#0xdee9_clob_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    price: u64,
    quantity: u64,
    is_bid: bool,
    expire_timestamp: u64,
    account_cap: &AccountCap,
    ctx: &<b>mut</b> TxContext
): u64 {
    <b>let</b> user = <a href="../../../.././build/Sui/docs/object.md#0x2_object_id">object::id</a>(account_cap);
    <b>let</b> order_id: u64;
    <b>let</b> open_orders: &<b>mut</b> CritbitTree&lt;<a href="clob.md#0xdee9_clob_TickLevel">TickLevel</a>&gt;;
    <b>if</b> (is_bid) {
        <b>let</b> quote_quantity = clob_math::mul(quantity, price);
        <a href="custodian.md#0xdee9_custodian_lock_balance">custodian::lock_balance</a>&lt;QuoteAsset&gt;(&<b>mut</b> pool.quote_custodian, account_cap, quote_quantity);
        order_id = pool.next_bid_order_id;
        pool.next_bid_order_id = pool.next_bid_order_id + 1;
        open_orders = &<b>mut</b> pool.bids;
    } <b>else</b> {
        <a href="custodian.md#0xdee9_custodian_lock_balance">custodian::lock_balance</a>&lt;BaseAsset&gt;(&<b>mut</b> pool.base_custodian, account_cap, quantity);
        order_id = pool.next_ask_order_id;
        pool.next_ask_order_id = pool.next_ask_order_id + 1;
        open_orders = &<b>mut</b> pool.asks;
    };
    <b>let</b> order = <a href="clob.md#0xdee9_clob_Order">Order</a> {
        order_id,
        price,
        quantity,
        is_bid,
        owner: user,
        expire_timestamp,
    };
    <b>let</b> (tick_exists, tick_index) = find_leaf(open_orders, price);
    <b>if</b> (!tick_exists) {
        tick_index = insert_leaf(
            open_orders,
            price,
            <a href="clob.md#0xdee9_clob_TickLevel">TickLevel</a> {
                price,
                open_orders: <a href="../../../.././build/Sui/docs/linked_table.md#0x2_linked_table_new">linked_table::new</a>(ctx),
            });
    };

    <b>let</b> tick_level = borrow_mut_leaf_by_index(open_orders, tick_index);
    <a href="../../../.././build/Sui/docs/linked_table.md#0x2_linked_table_push_back">linked_table::push_back</a>(&<b>mut</b> tick_level.open_orders, order_id, order);
    <a href="../../../.././build/Sui/docs/event.md#0x2_event_emit">event::emit</a>(<a href="clob.md#0xdee9_clob_OrderPlacedV2">OrderPlacedV2</a>&lt;BaseAsset, QuoteAsset&gt; {
        pool_id: *<a href="../../../.././build/Sui/docs/object.md#0x2_object_uid_as_inner">object::uid_as_inner</a>(&pool.id),
        order_id,
        is_bid,
        owner: user,
        base_asset_quantity_placed: quantity,
        price,
        expire_timestamp
    });
    <b>if</b> (!contains(&pool.usr_open_orders, user)) {
        add(&<b>mut</b> pool.usr_open_orders, user, <a href="../../../.././build/Sui/docs/linked_table.md#0x2_linked_table_new">linked_table::new</a>(ctx));
    };
    <a href="../../../.././build/Sui/docs/linked_table.md#0x2_linked_table_push_back">linked_table::push_back</a>(borrow_mut(&<b>mut</b> pool.usr_open_orders, user), order_id, price);

    <b>return</b> order_id
}
</code></pre>



</details>

<a name="0xdee9_clob_place_limit_order"></a>

## Function `place_limit_order`

Place a limit order to the order book.
Returns (base quantity filled, quote quantity filled, whether a maker order is being placed, order id of the maker order).
When the limit order is not successfully placed, we return false to indicate that and also returns a meaningless order_id 0.
When the limit order is successfully placed, we return true to indicate that and also the corresponding order_id.
So please check that boolean value first before using the order id.


<pre><code><b>public</b> <b>fun</b> <a href="clob.md#0xdee9_clob_place_limit_order">place_limit_order</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<b>mut</b> <a href="clob.md#0xdee9_clob_Pool">clob::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, price: u64, quantity: u64, is_bid: bool, expire_timestamp: u64, restriction: u8, <a href="../../../.././build/Sui/docs/clock.md#0x2_clock">clock</a>: &<a href="../../../.././build/Sui/docs/clock.md#0x2_clock_Clock">clock::Clock</a>, account_cap: &<a href="custodian.md#0xdee9_custodian_AccountCap">custodian::AccountCap</a>, ctx: &<b>mut</b> <a href="../../../.././build/Sui/docs/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): (u64, u64, bool, u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="clob.md#0xdee9_clob_place_limit_order">place_limit_order</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool: &<b>mut</b> <a href="clob.md#0xdee9_clob_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    price: u64,
    quantity: u64,
    is_bid: bool,
    expire_timestamp: u64, // Expiration timestamp in ms in absolute value inclusive.
    restriction: u8,
    <a href="../../../.././build/Sui/docs/clock.md#0x2_clock">clock</a>: &Clock,
    account_cap: &AccountCap,
    ctx: &<b>mut</b> TxContext
): (u64, u64, bool, u64) {
    // If limit bid order, check whether the price is lower than the lowest ask order by checking the min_leaf of asks Critbit Tree.
    // If so, assign the sequence id of the order <b>to</b> be next_bid_order_id and increment next_bid_order_id by 1.
    // Inject the new order <b>to</b> the bids Critbit Tree according <b>to</b> the price and order id.
    // Otherwise, find the price level from the asks Critbit Tree that is no greater than the input price.
    // Match the bid order against the asks Critbit Tree in the same way <b>as</b> a market order but up until the price level found in the previous step.
    // If the bid order is not completely filled, inject the remaining quantity <b>to</b> the bids Critbit Tree according <b>to</b> the input price and order id.
    // If limit ask order, vice versa.
    <b>assert</b>!(quantity &gt; 0, <a href="clob.md#0xdee9_clob_EInvalidQuantity">EInvalidQuantity</a>);
    <b>assert</b>!(price &gt; 0, <a href="clob.md#0xdee9_clob_EInvalidPrice">EInvalidPrice</a>);
    <b>assert</b>!(price % pool.tick_size == 0, <a href="clob.md#0xdee9_clob_EInvalidPrice">EInvalidPrice</a>);
    <b>assert</b>!(quantity % pool.lot_size == 0, <a href="clob.md#0xdee9_clob_EInvalidQuantity">EInvalidQuantity</a>);
    <b>assert</b>!(expire_timestamp &gt; <a href="../../../.././build/Sui/docs/clock.md#0x2_clock_timestamp_ms">clock::timestamp_ms</a>(<a href="../../../.././build/Sui/docs/clock.md#0x2_clock">clock</a>), <a href="clob.md#0xdee9_clob_EInvalidExpireTimestamp">EInvalidExpireTimestamp</a>);
    <b>let</b> user = <a href="../../../.././build/Sui/docs/object.md#0x2_object_id">object::id</a>(account_cap);
    <b>let</b> base_quantity_filled;
    <b>let</b> quote_quantity_filled;

    <b>if</b> (is_bid) {
        <b>let</b> quote_quantity_original = <a href="custodian.md#0xdee9_custodian_account_available_balance">custodian::account_available_balance</a>&lt;QuoteAsset&gt;(
            &pool.quote_custodian,
            user,
        );
        <b>let</b> quote_balance = <a href="custodian.md#0xdee9_custodian_decrease_user_available_balance">custodian::decrease_user_available_balance</a>&lt;QuoteAsset&gt;(
            &<b>mut</b> pool.quote_custodian,
            account_cap,
            quote_quantity_original,
        );
        <b>let</b> (base_balance_filled, quote_balance_left) = <a href="clob.md#0xdee9_clob_match_bid">match_bid</a>(
            pool,
            quantity,
            price,
            <a href="../../../.././build/Sui/docs/clock.md#0x2_clock_timestamp_ms">clock::timestamp_ms</a>(<a href="../../../.././build/Sui/docs/clock.md#0x2_clock">clock</a>),
            quote_balance,
        );
        base_quantity_filled = <a href="../../../.././build/Sui/docs/balance.md#0x2_balance_value">balance::value</a>(&base_balance_filled);
        quote_quantity_filled = quote_quantity_original - <a href="../../../.././build/Sui/docs/balance.md#0x2_balance_value">balance::value</a>(&quote_balance_left);

        <a href="custodian.md#0xdee9_custodian_increase_user_available_balance">custodian::increase_user_available_balance</a>&lt;BaseAsset&gt;(
            &<b>mut</b> pool.base_custodian,
            user,
            base_balance_filled,
        );
        <a href="custodian.md#0xdee9_custodian_increase_user_available_balance">custodian::increase_user_available_balance</a>&lt;QuoteAsset&gt;(
            &<b>mut</b> pool.quote_custodian,
            user,
            quote_balance_left,
        );
    } <b>else</b> {
        <b>let</b> base_balance = <a href="custodian.md#0xdee9_custodian_decrease_user_available_balance">custodian::decrease_user_available_balance</a>&lt;BaseAsset&gt;(
            &<b>mut</b> pool.base_custodian,
            account_cap,
            quantity,
        );
        <b>let</b> (base_balance_left, quote_balance_filled) = <a href="clob.md#0xdee9_clob_match_ask">match_ask</a>(
            pool,
            price,
            <a href="../../../.././build/Sui/docs/clock.md#0x2_clock_timestamp_ms">clock::timestamp_ms</a>(<a href="../../../.././build/Sui/docs/clock.md#0x2_clock">clock</a>),
            base_balance,
        );

        base_quantity_filled = quantity - <a href="../../../.././build/Sui/docs/balance.md#0x2_balance_value">balance::value</a>(&base_balance_left);
        quote_quantity_filled = <a href="../../../.././build/Sui/docs/balance.md#0x2_balance_value">balance::value</a>(&quote_balance_filled);

        <a href="custodian.md#0xdee9_custodian_increase_user_available_balance">custodian::increase_user_available_balance</a>&lt;BaseAsset&gt;(
            &<b>mut</b> pool.base_custodian,
            user,
            base_balance_left,
        );
        <a href="custodian.md#0xdee9_custodian_increase_user_available_balance">custodian::increase_user_available_balance</a>&lt;QuoteAsset&gt;(
            &<b>mut</b> pool.quote_custodian,
            user,
            quote_balance_filled,
        );
    };

    <b>let</b> order_id;
    <b>if</b> (restriction == <a href="clob.md#0xdee9_clob_IMMEDIATE_OR_CANCEL">IMMEDIATE_OR_CANCEL</a>) {
        <b>return</b> (base_quantity_filled, quote_quantity_filled, <b>false</b>, 0)
    };
    <b>if</b> (restriction == <a href="clob.md#0xdee9_clob_FILL_OR_KILL">FILL_OR_KILL</a>) {
        <b>assert</b>!(base_quantity_filled == quantity, <a href="clob.md#0xdee9_clob_EOrderCannotBeFullyFilled">EOrderCannotBeFullyFilled</a>);
        <b>return</b> (base_quantity_filled, quote_quantity_filled, <b>false</b>, 0)
    };
    <b>if</b> (restriction == <a href="clob.md#0xdee9_clob_POST_OR_ABORT">POST_OR_ABORT</a>) {
        <b>assert</b>!(base_quantity_filled == 0, <a href="clob.md#0xdee9_clob_EOrderCannotBeFullyPassive">EOrderCannotBeFullyPassive</a>);
        order_id = <a href="clob.md#0xdee9_clob_inject_limit_order">inject_limit_order</a>(pool, price, quantity, is_bid, expire_timestamp, account_cap, ctx);
        <b>return</b> (base_quantity_filled, quote_quantity_filled, <b>true</b>, order_id)
    } <b>else</b> {
        <b>assert</b>!(restriction == <a href="clob.md#0xdee9_clob_NO_RESTRICTION">NO_RESTRICTION</a>, <a href="clob.md#0xdee9_clob_EInvalidRestriction">EInvalidRestriction</a>);
        <b>if</b> (quantity &gt; base_quantity_filled) {
            order_id = <a href="clob.md#0xdee9_clob_inject_limit_order">inject_limit_order</a>(
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

<a name="0xdee9_clob_order_is_bid"></a>

## Function `order_is_bid`



<pre><code><b>fun</b> <a href="clob.md#0xdee9_clob_order_is_bid">order_is_bid</a>(order_id: u64): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="clob.md#0xdee9_clob_order_is_bid">order_is_bid</a>(order_id: u64): bool {
    <b>return</b> order_id &lt; <a href="clob.md#0xdee9_clob_MIN_ASK_ORDER_ID">MIN_ASK_ORDER_ID</a>
}
</code></pre>



</details>

<a name="0xdee9_clob_emit_order_canceled"></a>

## Function `emit_order_canceled`



<pre><code><b>fun</b> <a href="clob.md#0xdee9_clob_emit_order_canceled">emit_order_canceled</a>&lt;BaseAsset, QuoteAsset&gt;(pool_id: <a href="../../../.././build/Sui/docs/object.md#0x2_object_ID">object::ID</a>, order: &<a href="clob.md#0xdee9_clob_Order">clob::Order</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="clob.md#0xdee9_clob_emit_order_canceled">emit_order_canceled</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool_id: ID,
    order: &<a href="clob.md#0xdee9_clob_Order">Order</a>
) {
    <a href="../../../.././build/Sui/docs/event.md#0x2_event_emit">event::emit</a>(<a href="clob.md#0xdee9_clob_OrderCanceled">OrderCanceled</a>&lt;BaseAsset, QuoteAsset&gt; {
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

<a name="0xdee9_clob_emit_order_filled"></a>

## Function `emit_order_filled`



<pre><code><b>fun</b> <a href="clob.md#0xdee9_clob_emit_order_filled">emit_order_filled</a>&lt;BaseAsset, QuoteAsset&gt;(pool_id: <a href="../../../.././build/Sui/docs/object.md#0x2_object_ID">object::ID</a>, order: &<a href="clob.md#0xdee9_clob_Order">clob::Order</a>, base_asset_quantity_filled: u64, taker_commission: u64, maker_rebates: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="clob.md#0xdee9_clob_emit_order_filled">emit_order_filled</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool_id: ID,
    order: &<a href="clob.md#0xdee9_clob_Order">Order</a>,
    base_asset_quantity_filled: u64,
    taker_commission: u64,
    maker_rebates: u64
) {
    <a href="../../../.././build/Sui/docs/event.md#0x2_event_emit">event::emit</a>(<a href="clob.md#0xdee9_clob_OrderFilledV2">OrderFilledV2</a>&lt;BaseAsset, QuoteAsset&gt; {
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

<a name="0xdee9_clob_cancel_order"></a>

## Function `cancel_order`

Cancel and opening order.
Abort if order_id is invalid or if the order is not submitted by the transaction sender.


<pre><code><b>public</b> <b>fun</b> <a href="clob.md#0xdee9_clob_cancel_order">cancel_order</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<b>mut</b> <a href="clob.md#0xdee9_clob_Pool">clob::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, order_id: u64, account_cap: &<a href="custodian.md#0xdee9_custodian_AccountCap">custodian::AccountCap</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="clob.md#0xdee9_clob_cancel_order">cancel_order</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool: &<b>mut</b> <a href="clob.md#0xdee9_clob_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    order_id: u64,
    account_cap: &AccountCap
) {
    // First check the highest bit of the order id <b>to</b> see whether it's bid or ask.
    // Then retrieve the price using the order id.
    // Using the price <b>to</b> retrieve the corresponding PriceLevel from the bids / asks Critbit Tree.
    // Retrieve and remove the order from open orders of the PriceLevel.
    <b>let</b> user = <a href="../../../.././build/Sui/docs/object.md#0x2_object_id">object::id</a>(account_cap);
    <b>assert</b>!(contains(&pool.usr_open_orders, user), <a href="clob.md#0xdee9_clob_EInvalidUser">EInvalidUser</a>);
    <b>let</b> usr_open_orders = borrow_mut(&<b>mut</b> pool.usr_open_orders, user);
    <b>assert</b>!(<a href="../../../.././build/Sui/docs/linked_table.md#0x2_linked_table_contains">linked_table::contains</a>(usr_open_orders, order_id), <a href="clob.md#0xdee9_clob_EInvalidOrderId">EInvalidOrderId</a>);
    <b>let</b> tick_price = *<a href="../../../.././build/Sui/docs/linked_table.md#0x2_linked_table_borrow">linked_table::borrow</a>(usr_open_orders, order_id);
    <b>let</b> is_bid = <a href="clob.md#0xdee9_clob_order_is_bid">order_is_bid</a>(order_id);
    <b>let</b> (tick_exists, tick_index) = find_leaf(
        <b>if</b> (is_bid) { &pool.bids } <b>else</b> { &pool.asks },
        tick_price);
    <b>assert</b>!(tick_exists, <a href="clob.md#0xdee9_clob_EInvalidOrderId">EInvalidOrderId</a>);
    <b>let</b> order = <a href="clob.md#0xdee9_clob_remove_order">remove_order</a>&lt;BaseAsset, QuoteAsset&gt;(
        <b>if</b> (is_bid) { &<b>mut</b> pool.bids } <b>else</b> { &<b>mut</b> pool.asks },
        usr_open_orders,
        tick_index,
        order_id,
        user
    );
    <b>if</b> (is_bid) {
        <b>let</b> balance_locked = clob_math::mul(order.quantity, order.price);
        <a href="custodian.md#0xdee9_custodian_unlock_balance">custodian::unlock_balance</a>(&<b>mut</b> pool.quote_custodian, user, balance_locked);
    } <b>else</b> {
        <a href="custodian.md#0xdee9_custodian_unlock_balance">custodian::unlock_balance</a>(&<b>mut</b> pool.base_custodian, user, order.quantity);
    };
    <a href="clob.md#0xdee9_clob_emit_order_canceled">emit_order_canceled</a>&lt;BaseAsset, QuoteAsset&gt;(*<a href="../../../.././build/Sui/docs/object.md#0x2_object_uid_as_inner">object::uid_as_inner</a>(&pool.id), &order);
}
</code></pre>



</details>

<a name="0xdee9_clob_remove_order"></a>

## Function `remove_order`



<pre><code><b>fun</b> <a href="clob.md#0xdee9_clob_remove_order">remove_order</a>&lt;BaseAsset, QuoteAsset&gt;(open_orders: &<b>mut</b> <a href="critbit.md#0xdee9_critbit_CritbitTree">critbit::CritbitTree</a>&lt;<a href="clob.md#0xdee9_clob_TickLevel">clob::TickLevel</a>&gt;, usr_open_orders: &<b>mut</b> <a href="../../../.././build/Sui/docs/linked_table.md#0x2_linked_table_LinkedTable">linked_table::LinkedTable</a>&lt;u64, u64&gt;, tick_index: u64, order_id: u64, user: <a href="../../../.././build/Sui/docs/object.md#0x2_object_ID">object::ID</a>): <a href="clob.md#0xdee9_clob_Order">clob::Order</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="clob.md#0xdee9_clob_remove_order">remove_order</a>&lt;BaseAsset, QuoteAsset&gt;(
    open_orders: &<b>mut</b> CritbitTree&lt;<a href="clob.md#0xdee9_clob_TickLevel">TickLevel</a>&gt;,
    usr_open_orders: &<b>mut</b> LinkedTable&lt;u64, u64&gt;,
    tick_index: u64,
    order_id: u64,
    user: ID,
): <a href="clob.md#0xdee9_clob_Order">Order</a> {
    <a href="../../../.././build/Sui/docs/linked_table.md#0x2_linked_table_remove">linked_table::remove</a>(usr_open_orders, order_id);
    <b>let</b> tick_level = borrow_leaf_by_index(open_orders, tick_index);
    <b>assert</b>!(<a href="../../../.././build/Sui/docs/linked_table.md#0x2_linked_table_contains">linked_table::contains</a>(&tick_level.open_orders, order_id), <a href="clob.md#0xdee9_clob_EInvalidOrderId">EInvalidOrderId</a>);
    <b>let</b> mut_tick_level = borrow_mut_leaf_by_index(open_orders, tick_index);
    <b>let</b> order = <a href="../../../.././build/Sui/docs/linked_table.md#0x2_linked_table_remove">linked_table::remove</a>(&<b>mut</b> mut_tick_level.open_orders, order_id);
    <b>assert</b>!(order.owner == user, <a href="clob.md#0xdee9_clob_EUnauthorizedCancel">EUnauthorizedCancel</a>);
    <b>if</b> (<a href="../../../.././build/Sui/docs/linked_table.md#0x2_linked_table_is_empty">linked_table::is_empty</a>(&mut_tick_level.open_orders)) {
        <a href="clob.md#0xdee9_clob_destroy_empty_level">destroy_empty_level</a>(remove_leaf_by_index(open_orders, tick_index));
    };
    order
}
</code></pre>



</details>

<a name="0xdee9_clob_cancel_all_orders"></a>

## Function `cancel_all_orders`



<pre><code><b>public</b> <b>fun</b> <a href="clob.md#0xdee9_clob_cancel_all_orders">cancel_all_orders</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<b>mut</b> <a href="clob.md#0xdee9_clob_Pool">clob::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, account_cap: &<a href="custodian.md#0xdee9_custodian_AccountCap">custodian::AccountCap</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="clob.md#0xdee9_clob_cancel_all_orders">cancel_all_orders</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool: &<b>mut</b> <a href="clob.md#0xdee9_clob_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    account_cap: &AccountCap
) {
    <b>let</b> pool_id = *<a href="../../../.././build/Sui/docs/object.md#0x2_object_uid_as_inner">object::uid_as_inner</a>(&pool.id);
    <b>let</b> user = <a href="../../../.././build/Sui/docs/object.md#0x2_object_id">object::id</a>(account_cap);
    <b>assert</b>!(contains(&pool.usr_open_orders, user), <a href="clob.md#0xdee9_clob_EInvalidUser">EInvalidUser</a>);
    <b>let</b> usr_open_order_ids = <a href="../../../.././build/Sui/docs/table.md#0x2_table_borrow_mut">table::borrow_mut</a>(&<b>mut</b> pool.usr_open_orders, user);
    <b>while</b> (!<a href="../../../.././build/Sui/docs/linked_table.md#0x2_linked_table_is_empty">linked_table::is_empty</a>(usr_open_order_ids)) {
        <b>let</b> order_id = *<a href="_borrow">option::borrow</a>(<a href="../../../.././build/Sui/docs/linked_table.md#0x2_linked_table_back">linked_table::back</a>(usr_open_order_ids));
        <b>let</b> order_price = *<a href="../../../.././build/Sui/docs/linked_table.md#0x2_linked_table_borrow">linked_table::borrow</a>(usr_open_order_ids, order_id);
        <b>let</b> is_bid = <a href="clob.md#0xdee9_clob_order_is_bid">order_is_bid</a>(order_id);
        <b>let</b> open_orders =
            <b>if</b> (is_bid) { &<b>mut</b> pool.bids }
            <b>else</b> { &<b>mut</b> pool.asks };
        <b>let</b> (_, tick_index) = <a href="critbit.md#0xdee9_critbit_find_leaf">critbit::find_leaf</a>(open_orders, order_price);
        <b>let</b> order = <a href="clob.md#0xdee9_clob_remove_order">remove_order</a>&lt;BaseAsset, QuoteAsset&gt;(
            open_orders,
            usr_open_order_ids,
            tick_index,
            order_id,
            user
        );
        <b>if</b> (is_bid) {
            <b>let</b> balance_locked = clob_math::mul(order.quantity, order.price);
            <a href="custodian.md#0xdee9_custodian_unlock_balance">custodian::unlock_balance</a>(&<b>mut</b> pool.quote_custodian, user, balance_locked);
        } <b>else</b> {
            <a href="custodian.md#0xdee9_custodian_unlock_balance">custodian::unlock_balance</a>(&<b>mut</b> pool.base_custodian, user, order.quantity);
        };
        <a href="clob.md#0xdee9_clob_emit_order_canceled">emit_order_canceled</a>&lt;BaseAsset, QuoteAsset&gt;(pool_id, &order);
    };
}
</code></pre>



</details>

<a name="0xdee9_clob_batch_cancel_order"></a>

## Function `batch_cancel_order`

Batch cancel limit orders to save gas cost.
Abort if any of the order_ids are not submitted by the sender.
Skip any order_id that is invalid.
Note that this function can reduce gas cost even further if caller has multiple orders at the same price level,
and if orders with the same price are grouped together in the vector.
For example, if we have the following order_id to price mapping, {0: 100., 1: 200., 2: 100., 3: 200.}.
Grouping order_ids like [0, 2, 1, 3] would make it the most gas efficient.


<pre><code><b>public</b> <b>fun</b> <a href="clob.md#0xdee9_clob_batch_cancel_order">batch_cancel_order</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<b>mut</b> <a href="clob.md#0xdee9_clob_Pool">clob::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, order_ids: <a href="">vector</a>&lt;u64&gt;, account_cap: &<a href="custodian.md#0xdee9_custodian_AccountCap">custodian::AccountCap</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="clob.md#0xdee9_clob_batch_cancel_order">batch_cancel_order</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool: &<b>mut</b> <a href="clob.md#0xdee9_clob_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    order_ids: <a href="">vector</a>&lt;u64&gt;,
    account_cap: &AccountCap
) {
    <b>let</b> pool_id = *<a href="../../../.././build/Sui/docs/object.md#0x2_object_uid_as_inner">object::uid_as_inner</a>(&pool.id);
    // First group the order ids according <b>to</b> price level,
    // so that we don't have <b>to</b> retrieve the PriceLevel multiple times <b>if</b> there are orders at the same price level.
    // Iterate over each price level, retrieve the corresponding PriceLevel.
    // Iterate over the order ids that need <b>to</b> be canceled at that price level,
    // retrieve and remove the order from open orders of the PriceLevel.
    <b>let</b> user = <a href="../../../.././build/Sui/docs/object.md#0x2_object_id">object::id</a>(account_cap);
    <b>assert</b>!(contains(&pool.usr_open_orders, user), 0);
    <b>let</b> tick_index: u64 = 0;
    <b>let</b> tick_price: u64 = 0;
    <b>let</b> n_order = <a href="_length">vector::length</a>(&order_ids);
    <b>let</b> i_order = 0;
    <b>let</b> usr_open_orders = borrow_mut(&<b>mut</b> pool.usr_open_orders, user);
    <b>while</b> (i_order &lt; n_order) {
        <b>let</b> order_id = *<a href="_borrow">vector::borrow</a>(&order_ids, i_order);
        <b>assert</b>!(<a href="../../../.././build/Sui/docs/linked_table.md#0x2_linked_table_contains">linked_table::contains</a>(usr_open_orders, order_id), <a href="clob.md#0xdee9_clob_EInvalidOrderId">EInvalidOrderId</a>);
        <b>let</b> new_tick_price = *<a href="../../../.././build/Sui/docs/linked_table.md#0x2_linked_table_borrow">linked_table::borrow</a>(usr_open_orders, order_id);
        <b>let</b> is_bid = <a href="clob.md#0xdee9_clob_order_is_bid">order_is_bid</a>(order_id);
        <b>if</b> (new_tick_price != tick_price) {
            tick_price = new_tick_price;
            <b>let</b> (tick_exists, new_tick_index) = find_leaf(
                <b>if</b> (is_bid) { &pool.bids } <b>else</b> { &pool.asks },
                tick_price
            );
            <b>assert</b>!(tick_exists, <a href="clob.md#0xdee9_clob_EInvalidTickPrice">EInvalidTickPrice</a>);
            tick_index = new_tick_index;
        };
        <b>let</b> order = <a href="clob.md#0xdee9_clob_remove_order">remove_order</a>&lt;BaseAsset, QuoteAsset&gt;(
            <b>if</b> (is_bid) { &<b>mut</b> pool.bids } <b>else</b> { &<b>mut</b> pool.asks },
            usr_open_orders,
            tick_index,
            order_id,
            user
        );
        <b>if</b> (is_bid) {
            <b>let</b> balance_locked = clob_math::mul(order.quantity, order.price);
            <a href="custodian.md#0xdee9_custodian_unlock_balance">custodian::unlock_balance</a>(&<b>mut</b> pool.quote_custodian, user, balance_locked);
        } <b>else</b> {
            <a href="custodian.md#0xdee9_custodian_unlock_balance">custodian::unlock_balance</a>(&<b>mut</b> pool.base_custodian, user, order.quantity);
        };
        <a href="clob.md#0xdee9_clob_emit_order_canceled">emit_order_canceled</a>&lt;BaseAsset, QuoteAsset&gt;(pool_id, &order);
        i_order = i_order + 1;
    }
}
</code></pre>



</details>

<a name="0xdee9_clob_list_open_orders"></a>

## Function `list_open_orders`



<pre><code><b>public</b> <b>fun</b> <a href="clob.md#0xdee9_clob_list_open_orders">list_open_orders</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<a href="clob.md#0xdee9_clob_Pool">clob::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, account_cap: &<a href="custodian.md#0xdee9_custodian_AccountCap">custodian::AccountCap</a>): <a href="">vector</a>&lt;<a href="clob.md#0xdee9_clob_Order">clob::Order</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="clob.md#0xdee9_clob_list_open_orders">list_open_orders</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool: &<a href="clob.md#0xdee9_clob_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    account_cap: &AccountCap
): <a href="">vector</a>&lt;<a href="clob.md#0xdee9_clob_Order">Order</a>&gt; {
    <b>let</b> user = <a href="../../../.././build/Sui/docs/object.md#0x2_object_id">object::id</a>(account_cap);
    <b>let</b> usr_open_order_ids = <a href="../../../.././build/Sui/docs/table.md#0x2_table_borrow">table::borrow</a>(&pool.usr_open_orders, user);
    <b>let</b> open_orders = <a href="_empty">vector::empty</a>&lt;<a href="clob.md#0xdee9_clob_Order">Order</a>&gt;();
    <b>let</b> order_id = <a href="../../../.././build/Sui/docs/linked_table.md#0x2_linked_table_front">linked_table::front</a>(usr_open_order_ids);
    <b>while</b> (!<a href="_is_none">option::is_none</a>(order_id)) {
        <b>let</b> order_price = *<a href="../../../.././build/Sui/docs/linked_table.md#0x2_linked_table_borrow">linked_table::borrow</a>(usr_open_order_ids, *<a href="_borrow">option::borrow</a>(order_id));
        <b>let</b> tick_level =
            <b>if</b> (<a href="clob.md#0xdee9_clob_order_is_bid">order_is_bid</a>(*<a href="_borrow">option::borrow</a>(order_id))) borrow_leaf_by_key(&pool.bids, order_price)
            <b>else</b> borrow_leaf_by_key(&pool.asks, order_price);
        <b>let</b> order = <a href="../../../.././build/Sui/docs/linked_table.md#0x2_linked_table_borrow">linked_table::borrow</a>(&tick_level.open_orders, *<a href="_borrow">option::borrow</a>(order_id));
        <a href="_push_back">vector::push_back</a>(&<b>mut</b> open_orders, <a href="clob.md#0xdee9_clob_Order">Order</a> {
            order_id: order.order_id,
            price: order.price,
            quantity: order.quantity,
            is_bid: order.is_bid,
            owner: order.owner,
            expire_timestamp: order.expire_timestamp
        });
        order_id = <a href="../../../.././build/Sui/docs/linked_table.md#0x2_linked_table_next">linked_table::next</a>(usr_open_order_ids, *<a href="_borrow">option::borrow</a>(order_id));
    };
    open_orders
}
</code></pre>



</details>

<a name="0xdee9_clob_account_balance"></a>

## Function `account_balance`

query user balance inside custodian


<pre><code><b>public</b> <b>fun</b> <a href="clob.md#0xdee9_clob_account_balance">account_balance</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<a href="clob.md#0xdee9_clob_Pool">clob::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, account_cap: &<a href="custodian.md#0xdee9_custodian_AccountCap">custodian::AccountCap</a>): (u64, u64, u64, u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="clob.md#0xdee9_clob_account_balance">account_balance</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool: &<a href="clob.md#0xdee9_clob_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    account_cap: &AccountCap
): (u64, u64, u64, u64) {
    <b>let</b> user = <a href="../../../.././build/Sui/docs/object.md#0x2_object_id">object::id</a>(account_cap);
    <b>let</b> (base_avail, base_locked) = <a href="custodian.md#0xdee9_custodian_account_balance">custodian::account_balance</a>(&pool.base_custodian, user);
    <b>let</b> (quote_avail, quote_locked) = <a href="custodian.md#0xdee9_custodian_account_balance">custodian::account_balance</a>(&pool.quote_custodian, user);
    (base_avail, base_locked, quote_avail, quote_locked)
}
</code></pre>



</details>

<a name="0xdee9_clob_get_market_price"></a>

## Function `get_market_price`

Query the market price of order book
returns (best_bid_price, best_ask_price)


<pre><code><b>public</b> <b>fun</b> <a href="clob.md#0xdee9_clob_get_market_price">get_market_price</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<a href="clob.md#0xdee9_clob_Pool">clob::Pool</a>&lt;BaseAsset, QuoteAsset&gt;): (u64, u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="clob.md#0xdee9_clob_get_market_price">get_market_price</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool: &<a href="clob.md#0xdee9_clob_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;
): (u64, u64){
    <b>let</b> (bid_price, _) = <a href="critbit.md#0xdee9_critbit_max_leaf">critbit::max_leaf</a>(&pool.bids);
    <b>let</b> (ask_price, _) = <a href="critbit.md#0xdee9_critbit_min_leaf">critbit::min_leaf</a>(&pool.asks);
    <b>return</b> (bid_price, ask_price)
}
</code></pre>



</details>

<a name="0xdee9_clob_get_level2_book_status_bid_side"></a>

## Function `get_level2_book_status_bid_side`

Enter a price range and return the level2 order depth of all valid prices within this price range in bid side
returns two vectors of u64
The previous is a list of all valid prices
The latter is the corresponding depth list


<pre><code><b>public</b> <b>fun</b> <a href="clob.md#0xdee9_clob_get_level2_book_status_bid_side">get_level2_book_status_bid_side</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<a href="clob.md#0xdee9_clob_Pool">clob::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, price_low: u64, price_high: u64, <a href="../../../.././build/Sui/docs/clock.md#0x2_clock">clock</a>: &<a href="../../../.././build/Sui/docs/clock.md#0x2_clock_Clock">clock::Clock</a>): (<a href="">vector</a>&lt;u64&gt;, <a href="">vector</a>&lt;u64&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="clob.md#0xdee9_clob_get_level2_book_status_bid_side">get_level2_book_status_bid_side</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool: &<a href="clob.md#0xdee9_clob_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    price_low: u64,
    price_high: u64,
    <a href="../../../.././build/Sui/docs/clock.md#0x2_clock">clock</a>: &Clock
): (<a href="">vector</a>&lt;u64&gt;, <a href="">vector</a>&lt;u64&gt;) {
    <b>let</b> (price_low_, _) = <a href="critbit.md#0xdee9_critbit_min_leaf">critbit::min_leaf</a>(&pool.bids);
    <b>if</b> (price_low &lt; price_low_) price_low = price_low_;
    <b>let</b> (price_high_, _) = <a href="critbit.md#0xdee9_critbit_max_leaf">critbit::max_leaf</a>(&pool.bids);
    <b>if</b> (price_high &gt; price_high_) price_high = price_high_;
    price_low = <a href="critbit.md#0xdee9_critbit_find_closest_key">critbit::find_closest_key</a>(&pool.bids, price_low);
    price_high = <a href="critbit.md#0xdee9_critbit_find_closest_key">critbit::find_closest_key</a>(&pool.bids, price_high);
    <b>let</b> price_vec = <a href="_empty">vector::empty</a>&lt;u64&gt;();
    <b>let</b> depth_vec = <a href="_empty">vector::empty</a>&lt;u64&gt;();
    <b>if</b> (price_low == 0) { <b>return</b> (price_vec, depth_vec) };
    <b>while</b> (price_low &lt;= price_high) {
        <b>let</b> depth = <a href="clob.md#0xdee9_clob_get_level2_book_status">get_level2_book_status</a>&lt;BaseAsset, QuoteAsset&gt;(
            &pool.bids,
            price_low,
            <a href="../../../.././build/Sui/docs/clock.md#0x2_clock_timestamp_ms">clock::timestamp_ms</a>(<a href="../../../.././build/Sui/docs/clock.md#0x2_clock">clock</a>)
        );
        <a href="_push_back">vector::push_back</a>(&<b>mut</b> price_vec, price_low);
        <a href="_push_back">vector::push_back</a>(&<b>mut</b> depth_vec, depth);
        <b>let</b> (next_price, _) = <a href="critbit.md#0xdee9_critbit_next_leaf">critbit::next_leaf</a>(&pool.bids, price_low);
        <b>if</b> (next_price == 0) { <b>break</b> }
        <b>else</b> { price_low = next_price };
    };
    (price_vec, depth_vec)
}
</code></pre>



</details>

<a name="0xdee9_clob_get_level2_book_status_ask_side"></a>

## Function `get_level2_book_status_ask_side`

Enter a price range and return the level2 order depth of all valid prices within this price range in ask side
returns two vectors of u64
The previous is a list of all valid prices
The latter is the corresponding depth list


<pre><code><b>public</b> <b>fun</b> <a href="clob.md#0xdee9_clob_get_level2_book_status_ask_side">get_level2_book_status_ask_side</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<a href="clob.md#0xdee9_clob_Pool">clob::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, price_low: u64, price_high: u64, <a href="../../../.././build/Sui/docs/clock.md#0x2_clock">clock</a>: &<a href="../../../.././build/Sui/docs/clock.md#0x2_clock_Clock">clock::Clock</a>): (<a href="">vector</a>&lt;u64&gt;, <a href="">vector</a>&lt;u64&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="clob.md#0xdee9_clob_get_level2_book_status_ask_side">get_level2_book_status_ask_side</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool: &<a href="clob.md#0xdee9_clob_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    price_low: u64,
    price_high: u64,
    <a href="../../../.././build/Sui/docs/clock.md#0x2_clock">clock</a>: &Clock
): (<a href="">vector</a>&lt;u64&gt;, <a href="">vector</a>&lt;u64&gt;) {
    <b>let</b> (price_low_, _) = <a href="critbit.md#0xdee9_critbit_min_leaf">critbit::min_leaf</a>(&pool.asks);
    <b>if</b> (price_low &lt; price_low_) price_low = price_low_;
    <b>let</b> (price_high_, _) = <a href="critbit.md#0xdee9_critbit_max_leaf">critbit::max_leaf</a>(&pool.asks);
    <b>if</b> (price_high &gt; price_high_) price_high = price_high_;
    price_low = <a href="critbit.md#0xdee9_critbit_find_closest_key">critbit::find_closest_key</a>(&pool.asks, price_low);
    price_high = <a href="critbit.md#0xdee9_critbit_find_closest_key">critbit::find_closest_key</a>(&pool.asks, price_high);
    <b>let</b> price_vec = <a href="_empty">vector::empty</a>&lt;u64&gt;();
    <b>let</b> depth_vec = <a href="_empty">vector::empty</a>&lt;u64&gt;();
    <b>if</b> (price_low == 0) { <b>return</b> (price_vec, depth_vec) };
    <b>while</b> (price_low &lt;= price_high) {
        <b>let</b> depth = <a href="clob.md#0xdee9_clob_get_level2_book_status">get_level2_book_status</a>&lt;BaseAsset, QuoteAsset&gt;(
            &pool.asks,
            price_low,
            <a href="../../../.././build/Sui/docs/clock.md#0x2_clock_timestamp_ms">clock::timestamp_ms</a>(<a href="../../../.././build/Sui/docs/clock.md#0x2_clock">clock</a>)
        );
        <a href="_push_back">vector::push_back</a>(&<b>mut</b> price_vec, price_low);
        <a href="_push_back">vector::push_back</a>(&<b>mut</b> depth_vec, depth);
        <b>let</b> (next_price, _) = <a href="critbit.md#0xdee9_critbit_next_leaf">critbit::next_leaf</a>(&pool.asks, price_low);
        <b>if</b> (next_price == 0) { <b>break</b> }
        <b>else</b> { price_low = next_price };
    };
    (price_vec, depth_vec)
}
</code></pre>



</details>

<a name="0xdee9_clob_get_level2_book_status"></a>

## Function `get_level2_book_status`

internal func to retrive single depth of a tick price


<pre><code><b>fun</b> <a href="clob.md#0xdee9_clob_get_level2_book_status">get_level2_book_status</a>&lt;BaseAsset, QuoteAsset&gt;(open_orders: &<a href="critbit.md#0xdee9_critbit_CritbitTree">critbit::CritbitTree</a>&lt;<a href="clob.md#0xdee9_clob_TickLevel">clob::TickLevel</a>&gt;, price: u64, time_stamp: u64): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="clob.md#0xdee9_clob_get_level2_book_status">get_level2_book_status</a>&lt;BaseAsset, QuoteAsset&gt;(
    open_orders: &CritbitTree&lt;<a href="clob.md#0xdee9_clob_TickLevel">TickLevel</a>&gt;,
    price: u64,
    time_stamp: u64
): u64 {
    <b>let</b> tick_level = <a href="critbit.md#0xdee9_critbit_borrow_leaf_by_key">critbit::borrow_leaf_by_key</a>(open_orders, price);
    <b>let</b> tick_open_orders = &tick_level.open_orders;
    <b>let</b> depth = 0;
    <b>let</b> order_id = <a href="../../../.././build/Sui/docs/linked_table.md#0x2_linked_table_front">linked_table::front</a>(tick_open_orders);
    <b>let</b> order: &<a href="clob.md#0xdee9_clob_Order">Order</a>;
    <b>while</b> (!<a href="_is_none">option::is_none</a>(order_id)) {
        order = <a href="../../../.././build/Sui/docs/linked_table.md#0x2_linked_table_borrow">linked_table::borrow</a>(tick_open_orders, *<a href="_borrow">option::borrow</a>(order_id));
        <b>if</b> (order.expire_timestamp &gt; time_stamp) depth = depth + order.quantity;
        order_id = <a href="../../../.././build/Sui/docs/linked_table.md#0x2_linked_table_next">linked_table::next</a>(tick_open_orders, *<a href="_borrow">option::borrow</a>(order_id));
    };
    depth
}
</code></pre>



</details>

<a name="0xdee9_clob_get_order_status"></a>

## Function `get_order_status`



<pre><code><b>public</b> <b>fun</b> <a href="clob.md#0xdee9_clob_get_order_status">get_order_status</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<a href="clob.md#0xdee9_clob_Pool">clob::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, order_id: u64, account_cap: &<a href="custodian.md#0xdee9_custodian_AccountCap">custodian::AccountCap</a>): &<a href="clob.md#0xdee9_clob_Order">clob::Order</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="clob.md#0xdee9_clob_get_order_status">get_order_status</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool: &<a href="clob.md#0xdee9_clob_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    order_id: u64,
    account_cap: &AccountCap
): &<a href="clob.md#0xdee9_clob_Order">Order</a> {
    <b>let</b> user = <a href="../../../.././build/Sui/docs/object.md#0x2_object_id">object::id</a>(account_cap);
    <b>assert</b>!(<a href="../../../.././build/Sui/docs/table.md#0x2_table_contains">table::contains</a>(&pool.usr_open_orders, user), <a href="clob.md#0xdee9_clob_EInvalidUser">EInvalidUser</a>);
    <b>let</b> usr_open_order_ids = <a href="../../../.././build/Sui/docs/table.md#0x2_table_borrow">table::borrow</a>(&pool.usr_open_orders, user);
    <b>assert</b>!(<a href="../../../.././build/Sui/docs/linked_table.md#0x2_linked_table_contains">linked_table::contains</a>(usr_open_order_ids, order_id), <a href="clob.md#0xdee9_clob_EInvalidOrderId">EInvalidOrderId</a>);
    <b>let</b> order_price = *<a href="../../../.././build/Sui/docs/linked_table.md#0x2_linked_table_borrow">linked_table::borrow</a>(usr_open_order_ids, order_id);
    <b>let</b> open_orders =
        <b>if</b> (order_id &lt; <a href="clob.md#0xdee9_clob_MIN_ASK_ORDER_ID">MIN_ASK_ORDER_ID</a>) { &pool.bids }
        <b>else</b> { &pool.asks };
    <b>let</b> tick_level = <a href="critbit.md#0xdee9_critbit_borrow_leaf_by_key">critbit::borrow_leaf_by_key</a>(open_orders, order_price);
    <b>let</b> tick_open_orders = &tick_level.open_orders;
    <b>let</b> order = <a href="../../../.././build/Sui/docs/linked_table.md#0x2_linked_table_borrow">linked_table::borrow</a>(tick_open_orders, order_id);
    order
}
</code></pre>



</details>
