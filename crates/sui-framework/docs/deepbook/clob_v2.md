---
title: Module `deepbook::clob_v2`
---



-  [Struct `PoolCreated`](#deepbook_clob_v2_PoolCreated)
-  [Struct `OrderPlaced`](#deepbook_clob_v2_OrderPlaced)
-  [Struct `OrderCanceled`](#deepbook_clob_v2_OrderCanceled)
-  [Struct `AllOrdersCanceledComponent`](#deepbook_clob_v2_AllOrdersCanceledComponent)
-  [Struct `AllOrdersCanceled`](#deepbook_clob_v2_AllOrdersCanceled)
-  [Struct `OrderFilled`](#deepbook_clob_v2_OrderFilled)
-  [Struct `DepositAsset`](#deepbook_clob_v2_DepositAsset)
-  [Struct `WithdrawAsset`](#deepbook_clob_v2_WithdrawAsset)
-  [Struct `MatchedOrderMetadata`](#deepbook_clob_v2_MatchedOrderMetadata)
-  [Struct `Order`](#deepbook_clob_v2_Order)
-  [Struct `TickLevel`](#deepbook_clob_v2_TickLevel)
-  [Struct `Pool`](#deepbook_clob_v2_Pool)
-  [Struct `PoolOwnerCap`](#deepbook_clob_v2_PoolOwnerCap)
-  [Constants](#@Constants_0)
-  [Function `usr_open_orders_exist`](#deepbook_clob_v2_usr_open_orders_exist)
-  [Function `usr_open_orders_for_address`](#deepbook_clob_v2_usr_open_orders_for_address)
-  [Function `usr_open_orders`](#deepbook_clob_v2_usr_open_orders)
-  [Function `withdraw_fees`](#deepbook_clob_v2_withdraw_fees)
-  [Function `delete_pool_owner_cap`](#deepbook_clob_v2_delete_pool_owner_cap)
-  [Function `destroy_empty_level`](#deepbook_clob_v2_destroy_empty_level)
-  [Function `create_account`](#deepbook_clob_v2_create_account)
-  [Function `create_pool_`](#deepbook_clob_v2_create_pool_)
-  [Function `create_pool`](#deepbook_clob_v2_create_pool)
-  [Function `create_customized_pool`](#deepbook_clob_v2_create_customized_pool)
-  [Function `create_pool_with_return_`](#deepbook_clob_v2_create_pool_with_return_)
-  [Function `create_pool_with_return`](#deepbook_clob_v2_create_pool_with_return)
-  [Function `create_customized_pool_with_return`](#deepbook_clob_v2_create_customized_pool_with_return)
-  [Function `create_customized_pool_v2`](#deepbook_clob_v2_create_customized_pool_v2)
-  [Function `deposit_base`](#deepbook_clob_v2_deposit_base)
-  [Function `deposit_quote`](#deepbook_clob_v2_deposit_quote)
-  [Function `withdraw_base`](#deepbook_clob_v2_withdraw_base)
-  [Function `withdraw_quote`](#deepbook_clob_v2_withdraw_quote)
-  [Function `swap_exact_base_for_quote`](#deepbook_clob_v2_swap_exact_base_for_quote)
-  [Function `swap_exact_base_for_quote_with_metadata`](#deepbook_clob_v2_swap_exact_base_for_quote_with_metadata)
-  [Function `swap_exact_quote_for_base`](#deepbook_clob_v2_swap_exact_quote_for_base)
-  [Function `swap_exact_quote_for_base_with_metadata`](#deepbook_clob_v2_swap_exact_quote_for_base_with_metadata)
-  [Function `match_bid_with_quote_quantity`](#deepbook_clob_v2_match_bid_with_quote_quantity)
-  [Function `match_bid`](#deepbook_clob_v2_match_bid)
-  [Function `match_ask`](#deepbook_clob_v2_match_ask)
-  [Function `place_market_order`](#deepbook_clob_v2_place_market_order)
-  [Function `place_market_order_with_metadata`](#deepbook_clob_v2_place_market_order_with_metadata)
-  [Function `place_market_order_int`](#deepbook_clob_v2_place_market_order_int)
-  [Function `inject_limit_order`](#deepbook_clob_v2_inject_limit_order)
-  [Function `place_limit_order`](#deepbook_clob_v2_place_limit_order)
-  [Function `place_limit_order_with_metadata`](#deepbook_clob_v2_place_limit_order_with_metadata)
-  [Function `place_limit_order_int`](#deepbook_clob_v2_place_limit_order_int)
-  [Function `order_is_bid`](#deepbook_clob_v2_order_is_bid)
-  [Function `emit_order_canceled`](#deepbook_clob_v2_emit_order_canceled)
-  [Function `emit_order_filled`](#deepbook_clob_v2_emit_order_filled)
-  [Function `cancel_order`](#deepbook_clob_v2_cancel_order)
-  [Function `remove_order`](#deepbook_clob_v2_remove_order)
-  [Function `cancel_all_orders`](#deepbook_clob_v2_cancel_all_orders)
-  [Function `batch_cancel_order`](#deepbook_clob_v2_batch_cancel_order)
-  [Function `clean_up_expired_orders`](#deepbook_clob_v2_clean_up_expired_orders)
-  [Function `list_open_orders`](#deepbook_clob_v2_list_open_orders)
-  [Function `account_balance`](#deepbook_clob_v2_account_balance)
-  [Function `get_market_price`](#deepbook_clob_v2_get_market_price)
-  [Function `get_level2_book_status_bid_side`](#deepbook_clob_v2_get_level2_book_status_bid_side)
-  [Function `get_level2_book_status_ask_side`](#deepbook_clob_v2_get_level2_book_status_ask_side)
-  [Function `get_level2_book_status`](#deepbook_clob_v2_get_level2_book_status)
-  [Function `get_order_status`](#deepbook_clob_v2_get_order_status)
-  [Function `matched_order_metadata`](#deepbook_clob_v2_matched_order_metadata)
-  [Function `matched_order_metadata_info`](#deepbook_clob_v2_matched_order_metadata_info)
-  [Function `asks`](#deepbook_clob_v2_asks)
-  [Function `bids`](#deepbook_clob_v2_bids)
-  [Function `tick_size`](#deepbook_clob_v2_tick_size)
-  [Function `maker_rebate_rate`](#deepbook_clob_v2_maker_rebate_rate)
-  [Function `taker_fee_rate`](#deepbook_clob_v2_taker_fee_rate)
-  [Function `pool_size`](#deepbook_clob_v2_pool_size)
-  [Function `open_orders`](#deepbook_clob_v2_open_orders)
-  [Function `order_id`](#deepbook_clob_v2_order_id)
-  [Function `tick_level`](#deepbook_clob_v2_tick_level)
-  [Function `original_quantity`](#deepbook_clob_v2_original_quantity)
-  [Function `quantity`](#deepbook_clob_v2_quantity)
-  [Function `is_bid`](#deepbook_clob_v2_is_bid)
-  [Function `owner`](#deepbook_clob_v2_owner)
-  [Function `expire_timestamp`](#deepbook_clob_v2_expire_timestamp)
-  [Function `quote_asset_trading_fees_value`](#deepbook_clob_v2_quote_asset_trading_fees_value)
-  [Function `clone_order`](#deepbook_clob_v2_clone_order)


<pre><code><b>use</b> <a href="../deepbook/critbit.md#deepbook_critbit">deepbook::critbit</a>;
<b>use</b> <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2">deepbook::custodian_v2</a>;
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



<a name="deepbook_clob_v2_PoolCreated"></a>

## Struct `PoolCreated`

Emitted when a new pool is created


<pre><code><b>public</b> <b>struct</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_PoolCreated">PoolCreated</a> <b>has</b> <b>copy</b>, drop, store
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
<code><a href="../deepbook/clob_v2.md#deepbook_clob_v2_taker_fee_rate">taker_fee_rate</a>: u64</code>
</dt>
<dd>
</dd>
<dt>
<code><a href="../deepbook/clob_v2.md#deepbook_clob_v2_maker_rebate_rate">maker_rebate_rate</a>: u64</code>
</dt>
<dd>
</dd>
<dt>
<code><a href="../deepbook/clob_v2.md#deepbook_clob_v2_tick_size">tick_size</a>: u64</code>
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

<a name="deepbook_clob_v2_OrderPlaced"></a>

## Struct `OrderPlaced`

Emitted when a maker order is injected into the order book.


<pre><code><b>public</b> <b>struct</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_OrderPlaced">OrderPlaced</a>&lt;<b>phantom</b> BaseAsset, <b>phantom</b> QuoteAsset&gt; <b>has</b> <b>copy</b>, drop, store
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
<code><a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>: u64</code>
</dt>
<dd>
 ID of the order within the pool
</dd>
<dt>
<code>client_order_id: u64</code>
</dt>
<dd>
 ID of the order defined by client
</dd>
<dt>
<code><a href="../deepbook/clob_v2.md#deepbook_clob_v2_is_bid">is_bid</a>: bool</code>
</dt>
<dd>
</dd>
<dt>
<code><a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>: <b>address</b></code>
</dt>
<dd>
 owner ID of the <code>AccountCap</code> that placed the order
</dd>
<dt>
<code><a href="../deepbook/clob_v2.md#deepbook_clob_v2_original_quantity">original_quantity</a>: u64</code>
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
<code><a href="../deepbook/clob_v2.md#deepbook_clob_v2_expire_timestamp">expire_timestamp</a>: u64</code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="deepbook_clob_v2_OrderCanceled"></a>

## Struct `OrderCanceled`

Emitted when a maker order is canceled.


<pre><code><b>public</b> <b>struct</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_OrderCanceled">OrderCanceled</a>&lt;<b>phantom</b> BaseAsset, <b>phantom</b> QuoteAsset&gt; <b>has</b> <b>copy</b>, drop, store
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
<code><a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>: u64</code>
</dt>
<dd>
 ID of the order within the pool
</dd>
<dt>
<code>client_order_id: u64</code>
</dt>
<dd>
 ID of the order defined by client
</dd>
<dt>
<code><a href="../deepbook/clob_v2.md#deepbook_clob_v2_is_bid">is_bid</a>: bool</code>
</dt>
<dd>
</dd>
<dt>
<code><a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>: <b>address</b></code>
</dt>
<dd>
 owner ID of the <code>AccountCap</code> that canceled the order
</dd>
<dt>
<code><a href="../deepbook/clob_v2.md#deepbook_clob_v2_original_quantity">original_quantity</a>: u64</code>
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

<a name="deepbook_clob_v2_AllOrdersCanceledComponent"></a>

## Struct `AllOrdersCanceledComponent`

A struct to make all orders canceled a more efficient struct


<pre><code><b>public</b> <b>struct</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_AllOrdersCanceledComponent">AllOrdersCanceledComponent</a>&lt;<b>phantom</b> BaseAsset, <b>phantom</b> QuoteAsset&gt; <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code><a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>: u64</code>
</dt>
<dd>
 ID of the order within the pool
</dd>
<dt>
<code>client_order_id: u64</code>
</dt>
<dd>
 ID of the order defined by client
</dd>
<dt>
<code><a href="../deepbook/clob_v2.md#deepbook_clob_v2_is_bid">is_bid</a>: bool</code>
</dt>
<dd>
</dd>
<dt>
<code><a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>: <b>address</b></code>
</dt>
<dd>
 owner ID of the <code>AccountCap</code> that canceled the order
</dd>
<dt>
<code><a href="../deepbook/clob_v2.md#deepbook_clob_v2_original_quantity">original_quantity</a>: u64</code>
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

<a name="deepbook_clob_v2_AllOrdersCanceled"></a>

## Struct `AllOrdersCanceled`

Emitted when batch of orders are canceled.


<pre><code><b>public</b> <b>struct</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_AllOrdersCanceled">AllOrdersCanceled</a>&lt;<b>phantom</b> BaseAsset, <b>phantom</b> QuoteAsset&gt; <b>has</b> <b>copy</b>, drop, store
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
<code>orders_canceled: vector&lt;<a href="../deepbook/clob_v2.md#deepbook_clob_v2_AllOrdersCanceledComponent">deepbook::clob_v2::AllOrdersCanceledComponent</a>&lt;BaseAsset, QuoteAsset&gt;&gt;</code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="deepbook_clob_v2_OrderFilled"></a>

## Struct `OrderFilled`

Emitted only when a maker order is filled.


<pre><code><b>public</b> <b>struct</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_OrderFilled">OrderFilled</a>&lt;<b>phantom</b> BaseAsset, <b>phantom</b> QuoteAsset&gt; <b>has</b> <b>copy</b>, drop, store
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
<code><a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>: u64</code>
</dt>
<dd>
 ID of the order within the pool
</dd>
<dt>
<code>taker_client_order_id: u64</code>
</dt>
<dd>
 ID of the order defined by taker client
</dd>
<dt>
<code>maker_client_order_id: u64</code>
</dt>
<dd>
 ID of the order defined by maker client
</dd>
<dt>
<code><a href="../deepbook/clob_v2.md#deepbook_clob_v2_is_bid">is_bid</a>: bool</code>
</dt>
<dd>
</dd>
<dt>
<code>taker_address: <b>address</b></code>
</dt>
<dd>
 owner ID of the <code>AccountCap</code> that filled the order
</dd>
<dt>
<code>maker_address: <b>address</b></code>
</dt>
<dd>
 owner ID of the <code>AccountCap</code> that placed the order
</dd>
<dt>
<code><a href="../deepbook/clob_v2.md#deepbook_clob_v2_original_quantity">original_quantity</a>: u64</code>
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

<a name="deepbook_clob_v2_DepositAsset"></a>

## Struct `DepositAsset`

Emitted when user deposit asset to custodian


<pre><code><b>public</b> <b>struct</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_DepositAsset">DepositAsset</a>&lt;<b>phantom</b> Asset&gt; <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>pool_id: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a></code>
</dt>
<dd>
 object id of the pool that asset deposit to
</dd>
<dt>
<code><a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>: u64</code>
</dt>
<dd>
 quantity of the asset deposited
</dd>
<dt>
<code><a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>: <b>address</b></code>
</dt>
<dd>
 owner address of the <code>AccountCap</code> that deposit the asset
</dd>
</dl>


</details>

<a name="deepbook_clob_v2_WithdrawAsset"></a>

## Struct `WithdrawAsset`

Emitted when user withdraw asset from custodian


<pre><code><b>public</b> <b>struct</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_WithdrawAsset">WithdrawAsset</a>&lt;<b>phantom</b> Asset&gt; <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>pool_id: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a></code>
</dt>
<dd>
 object id of the pool that asset withdraw from
</dd>
<dt>
<code><a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>: u64</code>
</dt>
<dd>
 quantity of the asset user withdrew
</dd>
<dt>
<code><a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>: <b>address</b></code>
</dt>
<dd>
 owner ID of the <code>AccountCap</code> that withdrew the asset
</dd>
</dl>


</details>

<a name="deepbook_clob_v2_MatchedOrderMetadata"></a>

## Struct `MatchedOrderMetadata`

Returned as metadata only when a maker order is filled from place order functions.


<pre><code><b>public</b> <b>struct</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_MatchedOrderMetadata">MatchedOrderMetadata</a>&lt;<b>phantom</b> BaseAsset, <b>phantom</b> QuoteAsset&gt; <b>has</b> <b>copy</b>, drop, store
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
<code><a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>: u64</code>
</dt>
<dd>
 ID of the order within the pool
</dd>
<dt>
<code><a href="../deepbook/clob_v2.md#deepbook_clob_v2_is_bid">is_bid</a>: bool</code>
</dt>
<dd>
 Direction of order.
</dd>
<dt>
<code>taker_address: <b>address</b></code>
</dt>
<dd>
 owner ID of the <code>AccountCap</code> that filled the order
</dd>
<dt>
<code>maker_address: <b>address</b></code>
</dt>
<dd>
 owner ID of the <code>AccountCap</code> that placed the order
</dd>
<dt>
<code>base_asset_quantity_filled: u64</code>
</dt>
<dd>
 qty of base asset filled.
</dd>
<dt>
<code>price: u64</code>
</dt>
<dd>
 price at which basset asset filled.
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

<a name="deepbook_clob_v2_Order"></a>

## Struct `Order`



<pre><code><b>public</b> <b>struct</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_Order">Order</a> <b>has</b> drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code><a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>: u64</code>
</dt>
<dd>
</dd>
<dt>
<code>client_order_id: u64</code>
</dt>
<dd>
</dd>
<dt>
<code>price: u64</code>
</dt>
<dd>
</dd>
<dt>
<code><a href="../deepbook/clob_v2.md#deepbook_clob_v2_original_quantity">original_quantity</a>: u64</code>
</dt>
<dd>
</dd>
<dt>
<code><a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>: u64</code>
</dt>
<dd>
</dd>
<dt>
<code><a href="../deepbook/clob_v2.md#deepbook_clob_v2_is_bid">is_bid</a>: bool</code>
</dt>
<dd>
</dd>
<dt>
<code><a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>: <b>address</b></code>
</dt>
<dd>
 Order can only be canceled by the <code>AccountCap</code> with this owner ID
</dd>
<dt>
<code><a href="../deepbook/clob_v2.md#deepbook_clob_v2_expire_timestamp">expire_timestamp</a>: u64</code>
</dt>
<dd>
</dd>
<dt>
<code>self_matching_prevention: u8</code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="deepbook_clob_v2_TickLevel"></a>

## Struct `TickLevel`



<pre><code><b>public</b> <b>struct</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_TickLevel">TickLevel</a> <b>has</b> store
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
<code><a href="../deepbook/clob_v2.md#deepbook_clob_v2_open_orders">open_orders</a>: <a href="../sui/linked_table.md#sui_linked_table_LinkedTable">sui::linked_table::LinkedTable</a>&lt;u64, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_Order">deepbook::clob_v2::Order</a>&gt;</code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="deepbook_clob_v2_Pool"></a>

## Struct `Pool`



<pre><code><b>public</b> <b>struct</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">Pool</a>&lt;<b>phantom</b> BaseAsset, <b>phantom</b> QuoteAsset&gt; <b>has</b> key, store
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
<code><a href="../deepbook/clob_v2.md#deepbook_clob_v2_bids">bids</a>: <a href="../deepbook/critbit.md#deepbook_critbit_CritbitTree">deepbook::critbit::CritbitTree</a>&lt;<a href="../deepbook/clob_v2.md#deepbook_clob_v2_TickLevel">deepbook::clob_v2::TickLevel</a>&gt;</code>
</dt>
<dd>
</dd>
<dt>
<code><a href="../deepbook/clob_v2.md#deepbook_clob_v2_asks">asks</a>: <a href="../deepbook/critbit.md#deepbook_critbit_CritbitTree">deepbook::critbit::CritbitTree</a>&lt;<a href="../deepbook/clob_v2.md#deepbook_clob_v2_TickLevel">deepbook::clob_v2::TickLevel</a>&gt;</code>
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
<code><a href="../deepbook/clob_v2.md#deepbook_clob_v2_usr_open_orders">usr_open_orders</a>: <a href="../sui/table.md#sui_table_Table">sui::table::Table</a>&lt;<b>address</b>, <a href="../sui/linked_table.md#sui_linked_table_LinkedTable">sui::linked_table::LinkedTable</a>&lt;u64, u64&gt;&gt;</code>
</dt>
<dd>
</dd>
<dt>
<code><a href="../deepbook/clob_v2.md#deepbook_clob_v2_taker_fee_rate">taker_fee_rate</a>: u64</code>
</dt>
<dd>
</dd>
<dt>
<code><a href="../deepbook/clob_v2.md#deepbook_clob_v2_maker_rebate_rate">maker_rebate_rate</a>: u64</code>
</dt>
<dd>
</dd>
<dt>
<code><a href="../deepbook/clob_v2.md#deepbook_clob_v2_tick_size">tick_size</a>: u64</code>
</dt>
<dd>
</dd>
<dt>
<code>lot_size: u64</code>
</dt>
<dd>
</dd>
<dt>
<code>base_custodian: <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_Custodian">deepbook::custodian_v2::Custodian</a>&lt;BaseAsset&gt;</code>
</dt>
<dd>
</dd>
<dt>
<code>quote_custodian: <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_Custodian">deepbook::custodian_v2::Custodian</a>&lt;QuoteAsset&gt;</code>
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

<a name="deepbook_clob_v2_PoolOwnerCap"></a>

## Struct `PoolOwnerCap`

Capability granting permission to access an entry in <code><a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">Pool</a>.quote_asset_trading_fees</code>.
The pool objects created for older pools do not have a PoolOwnerCap because they were created
prior to the addition of this feature. Here is a list of 11 pools on mainnet that
do not have this capability:
0x31d1790e617eef7f516555124155b28d663e5c600317c769a75ee6336a54c07f
0x6e417ee1c12ad5f2600a66bc80c7bd52ff3cb7c072d508700d17cf1325324527
0x17625f1a241d34d2da0dc113086f67a2b832e3e8cd8006887c195cd24d3598a3
0x276ff4d99ecb3175091ba4baffa9b07590f84e2344e3f16e95d30d2c1678b84c
0xd1f0a9baacc1864ab19534e2d4c5d6c14f2e071a1f075e8e7f9d51f2c17dc238
0x4405b50d791fd3346754e8171aaab6bc2ed26c2c46efdd033c14b30ae507ac33
0xf0f663cf87f1eb124da2fc9be813e0ce262146f3df60bc2052d738eb41a25899
0xd9e45ab5440d61cc52e3b2bd915cdd643146f7593d587c715bc7bfa48311d826
0x5deafda22b6b86127ea4299503362638bea0ca33bb212ea3a67b029356b8b955
0x7f526b1263c4b91b43c9e646419b5696f424de28dda3c1e6658cc0a54558baa7
0x18d871e3c3da99046dfc0d3de612c5d88859bc03b8f0568bd127d0e70dbc58be


<pre><code><b>public</b> <b>struct</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_PoolOwnerCap">PoolOwnerCap</a> <b>has</b> key, store
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
<code><a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>: <b>address</b></code>
</dt>
<dd>
 The owner of this AccountCap. Note: this is
 derived from an object ID, not a user address
</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="deepbook_clob_v2_CANCEL_OLDEST"></a>



<pre><code><b>const</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_CANCEL_OLDEST">CANCEL_OLDEST</a>: u8 = 0;
</code></pre>



<a name="deepbook_clob_v2_EIncorrectPoolOwner"></a>



<pre><code><b>const</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_EIncorrectPoolOwner">EIncorrectPoolOwner</a>: u64 = 1;
</code></pre>



<a name="deepbook_clob_v2_EInsufficientBaseCoin"></a>



<pre><code><b>const</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_EInsufficientBaseCoin">EInsufficientBaseCoin</a>: u64 = 7;
</code></pre>



<a name="deepbook_clob_v2_EInsufficientQuoteCoin"></a>



<pre><code><b>const</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_EInsufficientQuoteCoin">EInsufficientQuoteCoin</a>: u64 = 8;
</code></pre>



<a name="deepbook_clob_v2_EInvalidExpireTimestamp"></a>



<pre><code><b>const</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_EInvalidExpireTimestamp">EInvalidExpireTimestamp</a>: u64 = 19;
</code></pre>



<a name="deepbook_clob_v2_EInvalidFee"></a>



<pre><code><b>const</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_EInvalidFee">EInvalidFee</a>: u64 = 18;
</code></pre>



<a name="deepbook_clob_v2_EInvalidFeeRateRebateRate"></a>



<pre><code><b>const</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_EInvalidFeeRateRebateRate">EInvalidFeeRateRebateRate</a>: u64 = 2;
</code></pre>



<a name="deepbook_clob_v2_EInvalidOrderId"></a>



<pre><code><b>const</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_EInvalidOrderId">EInvalidOrderId</a>: u64 = 3;
</code></pre>



<a name="deepbook_clob_v2_EInvalidPair"></a>



<pre><code><b>const</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_EInvalidPair">EInvalidPair</a>: u64 = 16;
</code></pre>



<a name="deepbook_clob_v2_EInvalidPrice"></a>



<pre><code><b>const</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_EInvalidPrice">EInvalidPrice</a>: u64 = 5;
</code></pre>



<a name="deepbook_clob_v2_EInvalidQuantity"></a>



<pre><code><b>const</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_EInvalidQuantity">EInvalidQuantity</a>: u64 = 6;
</code></pre>



<a name="deepbook_clob_v2_EInvalidRestriction"></a>



<pre><code><b>const</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_EInvalidRestriction">EInvalidRestriction</a>: u64 = 14;
</code></pre>



<a name="deepbook_clob_v2_EInvalidSelfMatchingPreventionArg"></a>



<pre><code><b>const</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_EInvalidSelfMatchingPreventionArg">EInvalidSelfMatchingPreventionArg</a>: u64 = 21;
</code></pre>



<a name="deepbook_clob_v2_EInvalidTickPrice"></a>



<pre><code><b>const</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_EInvalidTickPrice">EInvalidTickPrice</a>: u64 = 11;
</code></pre>



<a name="deepbook_clob_v2_EInvalidTickSizeLotSize"></a>



<pre><code><b>const</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_EInvalidTickSizeLotSize">EInvalidTickSizeLotSize</a>: u64 = 20;
</code></pre>



<a name="deepbook_clob_v2_EInvalidUser"></a>



<pre><code><b>const</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_EInvalidUser">EInvalidUser</a>: u64 = 12;
</code></pre>



<a name="deepbook_clob_v2_ENotEqual"></a>



<pre><code><b>const</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_ENotEqual">ENotEqual</a>: u64 = 13;
</code></pre>



<a name="deepbook_clob_v2_EOrderCannotBeFullyFilled"></a>



<pre><code><b>const</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_EOrderCannotBeFullyFilled">EOrderCannotBeFullyFilled</a>: u64 = 9;
</code></pre>



<a name="deepbook_clob_v2_EOrderCannotBeFullyPassive"></a>



<pre><code><b>const</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_EOrderCannotBeFullyPassive">EOrderCannotBeFullyPassive</a>: u64 = 10;
</code></pre>



<a name="deepbook_clob_v2_EUnauthorizedCancel"></a>



<pre><code><b>const</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_EUnauthorizedCancel">EUnauthorizedCancel</a>: u64 = 4;
</code></pre>



<a name="deepbook_clob_v2_FEE_AMOUNT_FOR_CREATE_POOL"></a>



<pre><code><b>const</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_FEE_AMOUNT_FOR_CREATE_POOL">FEE_AMOUNT_FOR_CREATE_POOL</a>: u64 = 100000000000;
</code></pre>



<a name="deepbook_clob_v2_FILL_OR_KILL"></a>



<pre><code><b>const</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_FILL_OR_KILL">FILL_OR_KILL</a>: u8 = 2;
</code></pre>



<a name="deepbook_clob_v2_FLOAT_SCALING"></a>



<pre><code><b>const</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_FLOAT_SCALING">FLOAT_SCALING</a>: u64 = 1000000000;
</code></pre>



<a name="deepbook_clob_v2_IMMEDIATE_OR_CANCEL"></a>



<pre><code><b>const</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_IMMEDIATE_OR_CANCEL">IMMEDIATE_OR_CANCEL</a>: u8 = 1;
</code></pre>



<a name="deepbook_clob_v2_MAX_PRICE"></a>



<pre><code><b>const</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_MAX_PRICE">MAX_PRICE</a>: u64 = 9223372036854775808;
</code></pre>



<a name="deepbook_clob_v2_MIN_ASK_ORDER_ID"></a>



<pre><code><b>const</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_MIN_ASK_ORDER_ID">MIN_ASK_ORDER_ID</a>: u64 = 9223372036854775808;
</code></pre>



<a name="deepbook_clob_v2_MIN_BID_ORDER_ID"></a>



<pre><code><b>const</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_MIN_BID_ORDER_ID">MIN_BID_ORDER_ID</a>: u64 = 1;
</code></pre>



<a name="deepbook_clob_v2_MIN_PRICE"></a>



<pre><code><b>const</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_MIN_PRICE">MIN_PRICE</a>: u64 = 0;
</code></pre>



<a name="deepbook_clob_v2_NO_RESTRICTION"></a>



<pre><code><b>const</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_NO_RESTRICTION">NO_RESTRICTION</a>: u8 = 0;
</code></pre>



<a name="deepbook_clob_v2_POST_OR_ABORT"></a>



<pre><code><b>const</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_POST_OR_ABORT">POST_OR_ABORT</a>: u8 = 3;
</code></pre>



<a name="deepbook_clob_v2_REFERENCE_MAKER_REBATE_RATE"></a>



<pre><code><b>const</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_REFERENCE_MAKER_REBATE_RATE">REFERENCE_MAKER_REBATE_RATE</a>: u64 = 1500000;
</code></pre>



<a name="deepbook_clob_v2_REFERENCE_TAKER_FEE_RATE"></a>



<pre><code><b>const</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_REFERENCE_TAKER_FEE_RATE">REFERENCE_TAKER_FEE_RATE</a>: u64 = 2500000;
</code></pre>



<a name="deepbook_clob_v2_usr_open_orders_exist"></a>

## Function `usr_open_orders_exist`

Accessor functions


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_usr_open_orders_exist">usr_open_orders_exist</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">deepbook::clob_v2::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>: <b>address</b>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_usr_open_orders_exist">usr_open_orders_exist</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool: &<a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>: <b>address</b>
): bool {
    table::contains(&pool.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_usr_open_orders">usr_open_orders</a>, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>)
}
</code></pre>



</details>

<a name="deepbook_clob_v2_usr_open_orders_for_address"></a>

## Function `usr_open_orders_for_address`



<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_usr_open_orders_for_address">usr_open_orders_for_address</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">deepbook::clob_v2::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>: <b>address</b>): &<a href="../sui/linked_table.md#sui_linked_table_LinkedTable">sui::linked_table::LinkedTable</a>&lt;u64, u64&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_usr_open_orders_for_address">usr_open_orders_for_address</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool: &<a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>: <b>address</b>
): &LinkedTable&lt;u64, u64&gt; {
    table::borrow(&pool.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_usr_open_orders">usr_open_orders</a>, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>)
}
</code></pre>



</details>

<a name="deepbook_clob_v2_usr_open_orders"></a>

## Function `usr_open_orders`



<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_usr_open_orders">usr_open_orders</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">deepbook::clob_v2::Pool</a>&lt;BaseAsset, QuoteAsset&gt;): &<a href="../sui/table.md#sui_table_Table">sui::table::Table</a>&lt;<b>address</b>, <a href="../sui/linked_table.md#sui_linked_table_LinkedTable">sui::linked_table::LinkedTable</a>&lt;u64, u64&gt;&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_usr_open_orders">usr_open_orders</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool: &<a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
): &Table&lt;<b>address</b>, LinkedTable&lt;u64, u64&gt;&gt; {
    &pool.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_usr_open_orders">usr_open_orders</a>
}
</code></pre>



</details>

<a name="deepbook_clob_v2_withdraw_fees"></a>

## Function `withdraw_fees`

Function to withdraw fees created from a pool


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_withdraw_fees">withdraw_fees</a>&lt;BaseAsset, QuoteAsset&gt;(pool_owner_cap: &<a href="../deepbook/clob_v2.md#deepbook_clob_v2_PoolOwnerCap">deepbook::clob_v2::PoolOwnerCap</a>, pool: &<b>mut</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">deepbook::clob_v2::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <a href="../sui/coin.md#sui_coin_Coin">sui::coin::Coin</a>&lt;QuoteAsset&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_withdraw_fees">withdraw_fees</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool_owner_cap: &<a href="../deepbook/clob_v2.md#deepbook_clob_v2_PoolOwnerCap">PoolOwnerCap</a>,
    pool: &<b>mut</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    ctx: &<b>mut</b> TxContext,
): Coin&lt;QuoteAsset&gt; {
    <b>assert</b>!(pool_owner_cap.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a> == object::uid_to_address(&pool.id), <a href="../deepbook/clob_v2.md#deepbook_clob_v2_EIncorrectPoolOwner">EIncorrectPoolOwner</a>);
    <b>let</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a> = <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quote_asset_trading_fees_value">quote_asset_trading_fees_value</a>(pool);
    <b>let</b> to_withdraw = balance::split(&<b>mut</b> pool.quote_asset_trading_fees, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>);
    coin::from_balance(to_withdraw, ctx)
}
</code></pre>



</details>

<a name="deepbook_clob_v2_delete_pool_owner_cap"></a>

## Function `delete_pool_owner_cap`

Destroy the given <code>pool_owner_cap</code> object


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_delete_pool_owner_cap">delete_pool_owner_cap</a>(pool_owner_cap: <a href="../deepbook/clob_v2.md#deepbook_clob_v2_PoolOwnerCap">deepbook::clob_v2::PoolOwnerCap</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_delete_pool_owner_cap">delete_pool_owner_cap</a>(pool_owner_cap: <a href="../deepbook/clob_v2.md#deepbook_clob_v2_PoolOwnerCap">PoolOwnerCap</a>) {
    <b>let</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_PoolOwnerCap">PoolOwnerCap</a> { id, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>: _ } = pool_owner_cap;
    object::delete(id)
}
</code></pre>



</details>

<a name="deepbook_clob_v2_destroy_empty_level"></a>

## Function `destroy_empty_level`



<pre><code><b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_destroy_empty_level">destroy_empty_level</a>(level: <a href="../deepbook/clob_v2.md#deepbook_clob_v2_TickLevel">deepbook::clob_v2::TickLevel</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_destroy_empty_level">destroy_empty_level</a>(level: <a href="../deepbook/clob_v2.md#deepbook_clob_v2_TickLevel">TickLevel</a>) {
    <b>let</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_TickLevel">TickLevel</a> {
        price: _,
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_open_orders">open_orders</a>: orders,
    } = level;
    linked_table::destroy_empty(orders);
}
</code></pre>



</details>

<a name="deepbook_clob_v2_create_account"></a>

## Function `create_account`



<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_create_account">create_account</a>(ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_AccountCap">deepbook::custodian_v2::AccountCap</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_create_account">create_account</a>(ctx: &<b>mut</b> TxContext): AccountCap {
    mint_account_cap(ctx)
}
</code></pre>



</details>

<a name="deepbook_clob_v2_create_pool_"></a>

## Function `create_pool_`



<pre><code><b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_create_pool_">create_pool_</a>&lt;BaseAsset, QuoteAsset&gt;(<a href="../deepbook/clob_v2.md#deepbook_clob_v2_taker_fee_rate">taker_fee_rate</a>: u64, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_maker_rebate_rate">maker_rebate_rate</a>: u64, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_tick_size">tick_size</a>: u64, lot_size: u64, creation_fee: <a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;<a href="../sui/sui.md#sui_sui_SUI">sui::sui::SUI</a>&gt;, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_create_pool_">create_pool_</a>&lt;BaseAsset, QuoteAsset&gt;(
    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_taker_fee_rate">taker_fee_rate</a>: u64,
    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_maker_rebate_rate">maker_rebate_rate</a>: u64,
    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_tick_size">tick_size</a>: u64,
    lot_size: u64,
    creation_fee: Balance&lt;SUI&gt;,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> (pool, pool_owner_cap) = <a href="../deepbook/clob_v2.md#deepbook_clob_v2_create_pool_with_return_">create_pool_with_return_</a>&lt;BaseAsset, QuoteAsset&gt;(
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_taker_fee_rate">taker_fee_rate</a>,
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_maker_rebate_rate">maker_rebate_rate</a>,
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_tick_size">tick_size</a>,
        lot_size,
        creation_fee,
        ctx
    );
    transfer::public_transfer(pool_owner_cap, tx_context::sender(ctx));
    transfer::share_object(pool);
}
</code></pre>



</details>

<a name="deepbook_clob_v2_create_pool"></a>

## Function `create_pool`



<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_create_pool">create_pool</a>&lt;BaseAsset, QuoteAsset&gt;(<a href="../deepbook/clob_v2.md#deepbook_clob_v2_tick_size">tick_size</a>: u64, lot_size: u64, creation_fee: <a href="../sui/coin.md#sui_coin_Coin">sui::coin::Coin</a>&lt;<a href="../sui/sui.md#sui_sui_SUI">sui::sui::SUI</a>&gt;, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_create_pool">create_pool</a>&lt;BaseAsset, QuoteAsset&gt;(
    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_tick_size">tick_size</a>: u64,
    lot_size: u64,
    creation_fee: Coin&lt;SUI&gt;,
    ctx: &<b>mut</b> TxContext,
) {
    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_create_customized_pool">create_customized_pool</a>&lt;BaseAsset, QuoteAsset&gt;(
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_tick_size">tick_size</a>,
        lot_size,
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_REFERENCE_TAKER_FEE_RATE">REFERENCE_TAKER_FEE_RATE</a>,
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_REFERENCE_MAKER_REBATE_RATE">REFERENCE_MAKER_REBATE_RATE</a>,
        creation_fee,
        ctx,
    );
}
</code></pre>



</details>

<a name="deepbook_clob_v2_create_customized_pool"></a>

## Function `create_customized_pool`

Function for creating pool with customized taker fee rate and maker rebate rate.
The taker_fee_rate should be greater than or equal to the maker_rebate_rate, and both should have a scaling of 10^9.
Taker_fee_rate of 0.25% should be 2_500_000 for example


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_create_customized_pool">create_customized_pool</a>&lt;BaseAsset, QuoteAsset&gt;(<a href="../deepbook/clob_v2.md#deepbook_clob_v2_tick_size">tick_size</a>: u64, lot_size: u64, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_taker_fee_rate">taker_fee_rate</a>: u64, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_maker_rebate_rate">maker_rebate_rate</a>: u64, creation_fee: <a href="../sui/coin.md#sui_coin_Coin">sui::coin::Coin</a>&lt;<a href="../sui/sui.md#sui_sui_SUI">sui::sui::SUI</a>&gt;, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_create_customized_pool">create_customized_pool</a>&lt;BaseAsset, QuoteAsset&gt;(
    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_tick_size">tick_size</a>: u64,
    lot_size: u64,
    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_taker_fee_rate">taker_fee_rate</a>: u64,
    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_maker_rebate_rate">maker_rebate_rate</a>: u64,
    creation_fee: Coin&lt;SUI&gt;,
    ctx: &<b>mut</b> TxContext,
) {
    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_create_pool_">create_pool_</a>&lt;BaseAsset, QuoteAsset&gt;(
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_taker_fee_rate">taker_fee_rate</a>,
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_maker_rebate_rate">maker_rebate_rate</a>,
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_tick_size">tick_size</a>,
        lot_size,
        coin::into_balance(creation_fee),
        ctx
    )
}
</code></pre>



</details>

<a name="deepbook_clob_v2_create_pool_with_return_"></a>

## Function `create_pool_with_return_`

Helper function that all the create pools now call to create pools.


<pre><code><b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_create_pool_with_return_">create_pool_with_return_</a>&lt;BaseAsset, QuoteAsset&gt;(<a href="../deepbook/clob_v2.md#deepbook_clob_v2_taker_fee_rate">taker_fee_rate</a>: u64, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_maker_rebate_rate">maker_rebate_rate</a>: u64, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_tick_size">tick_size</a>: u64, lot_size: u64, creation_fee: <a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;<a href="../sui/sui.md#sui_sui_SUI">sui::sui::SUI</a>&gt;, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): (<a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">deepbook::clob_v2::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_PoolOwnerCap">deepbook::clob_v2::PoolOwnerCap</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_create_pool_with_return_">create_pool_with_return_</a>&lt;BaseAsset, QuoteAsset&gt;(
    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_taker_fee_rate">taker_fee_rate</a>: u64,
    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_maker_rebate_rate">maker_rebate_rate</a>: u64,
    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_tick_size">tick_size</a>: u64,
    lot_size: u64,
    creation_fee: Balance&lt;SUI&gt;,
    ctx: &<b>mut</b> TxContext,
): (<a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_PoolOwnerCap">PoolOwnerCap</a>) {
    <b>assert</b>!(balance::value(&creation_fee) == <a href="../deepbook/clob_v2.md#deepbook_clob_v2_FEE_AMOUNT_FOR_CREATE_POOL">FEE_AMOUNT_FOR_CREATE_POOL</a>, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_EInvalidFee">EInvalidFee</a>);
    <b>let</b> base_type_name = type_name::get&lt;BaseAsset&gt;();
    <b>let</b> quote_type_name = type_name::get&lt;QuoteAsset&gt;();
    <b>assert</b>!(clob_math::unsafe_mul(lot_size, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_tick_size">tick_size</a>) &gt; 0, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_EInvalidTickSizeLotSize">EInvalidTickSizeLotSize</a>);
    <b>assert</b>!(base_type_name != quote_type_name, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_EInvalidPair">EInvalidPair</a>);
    <b>assert</b>!(<a href="../deepbook/clob_v2.md#deepbook_clob_v2_taker_fee_rate">taker_fee_rate</a> &gt;= <a href="../deepbook/clob_v2.md#deepbook_clob_v2_maker_rebate_rate">maker_rebate_rate</a>, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_EInvalidFeeRateRebateRate">EInvalidFeeRateRebateRate</a>);
    <b>let</b> pool_uid = object::new(ctx);
    <b>let</b> pool_id = *object::uid_as_inner(&pool_uid);
    // Creates the capability to mark a pool <a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>.
    <b>let</b> id = object::new(ctx);
    <b>let</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a> = object::uid_to_address(&pool_uid);
    <b>let</b> pool_owner_cap = <a href="../deepbook/clob_v2.md#deepbook_clob_v2_PoolOwnerCap">PoolOwnerCap</a> { id, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a> };
    event::emit(<a href="../deepbook/clob_v2.md#deepbook_clob_v2_PoolCreated">PoolCreated</a> {
        pool_id,
        base_asset: base_type_name,
        quote_asset: quote_type_name,
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_taker_fee_rate">taker_fee_rate</a>,
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_maker_rebate_rate">maker_rebate_rate</a>,
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_tick_size">tick_size</a>,
        lot_size,
    });
    (<a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt; {
        id: pool_uid,
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_bids">bids</a>: <a href="../deepbook/critbit.md#deepbook_critbit_new">critbit::new</a>(ctx),
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_asks">asks</a>: <a href="../deepbook/critbit.md#deepbook_critbit_new">critbit::new</a>(ctx),
        next_bid_order_id: <a href="../deepbook/clob_v2.md#deepbook_clob_v2_MIN_BID_ORDER_ID">MIN_BID_ORDER_ID</a>,
        next_ask_order_id: <a href="../deepbook/clob_v2.md#deepbook_clob_v2_MIN_ASK_ORDER_ID">MIN_ASK_ORDER_ID</a>,
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_usr_open_orders">usr_open_orders</a>: table::new(ctx),
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_taker_fee_rate">taker_fee_rate</a>,
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_maker_rebate_rate">maker_rebate_rate</a>,
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_tick_size">tick_size</a>,
        lot_size,
        base_custodian: <a href="../deepbook/custodian.md#deepbook_custodian_new">custodian::new</a>&lt;BaseAsset&gt;(ctx),
        quote_custodian: <a href="../deepbook/custodian.md#deepbook_custodian_new">custodian::new</a>&lt;QuoteAsset&gt;(ctx),
        creation_fee,
        base_asset_trading_fees: balance::zero(),
        quote_asset_trading_fees: balance::zero(),
    }, pool_owner_cap)
}
</code></pre>



</details>

<a name="deepbook_clob_v2_create_pool_with_return"></a>

## Function `create_pool_with_return`

Function for creating an external pool. This API can be used to wrap deepbook pools into other objects.


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_create_pool_with_return">create_pool_with_return</a>&lt;BaseAsset, QuoteAsset&gt;(<a href="../deepbook/clob_v2.md#deepbook_clob_v2_tick_size">tick_size</a>: u64, lot_size: u64, creation_fee: <a href="../sui/coin.md#sui_coin_Coin">sui::coin::Coin</a>&lt;<a href="../sui/sui.md#sui_sui_SUI">sui::sui::SUI</a>&gt;, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">deepbook::clob_v2::Pool</a>&lt;BaseAsset, QuoteAsset&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_create_pool_with_return">create_pool_with_return</a>&lt;BaseAsset, QuoteAsset&gt;(
    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_tick_size">tick_size</a>: u64,
    lot_size: u64,
    creation_fee: Coin&lt;SUI&gt;,
    ctx: &<b>mut</b> TxContext,
): <a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt; {
    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_create_customized_pool_with_return">create_customized_pool_with_return</a>&lt;BaseAsset, QuoteAsset&gt;(
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_tick_size">tick_size</a>,
        lot_size,
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_REFERENCE_TAKER_FEE_RATE">REFERENCE_TAKER_FEE_RATE</a>,
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_REFERENCE_MAKER_REBATE_RATE">REFERENCE_MAKER_REBATE_RATE</a>,
        creation_fee,
        ctx,
    )
}
</code></pre>



</details>

<a name="deepbook_clob_v2_create_customized_pool_with_return"></a>

## Function `create_customized_pool_with_return`

Function for creating pool with customized taker fee rate and maker rebate rate.
The taker_fee_rate should be greater than or equal to the maker_rebate_rate, and both should have a scaling of 10^9.
Taker_fee_rate of 0.25% should be 2_500_000 for example


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_create_customized_pool_with_return">create_customized_pool_with_return</a>&lt;BaseAsset, QuoteAsset&gt;(<a href="../deepbook/clob_v2.md#deepbook_clob_v2_tick_size">tick_size</a>: u64, lot_size: u64, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_taker_fee_rate">taker_fee_rate</a>: u64, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_maker_rebate_rate">maker_rebate_rate</a>: u64, creation_fee: <a href="../sui/coin.md#sui_coin_Coin">sui::coin::Coin</a>&lt;<a href="../sui/sui.md#sui_sui_SUI">sui::sui::SUI</a>&gt;, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">deepbook::clob_v2::Pool</a>&lt;BaseAsset, QuoteAsset&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_create_customized_pool_with_return">create_customized_pool_with_return</a>&lt;BaseAsset, QuoteAsset&gt;(
    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_tick_size">tick_size</a>: u64,
    lot_size: u64,
    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_taker_fee_rate">taker_fee_rate</a>: u64,
    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_maker_rebate_rate">maker_rebate_rate</a>: u64,
    creation_fee: Coin&lt;SUI&gt;,
    ctx: &<b>mut</b> TxContext,
) : <a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt; {
    <b>let</b> (pool, pool_owner_cap) = <a href="../deepbook/clob_v2.md#deepbook_clob_v2_create_pool_with_return_">create_pool_with_return_</a>&lt;BaseAsset, QuoteAsset&gt;(
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_taker_fee_rate">taker_fee_rate</a>,
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_maker_rebate_rate">maker_rebate_rate</a>,
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_tick_size">tick_size</a>,
        lot_size,
        coin::into_balance(creation_fee),
        ctx
    );
    transfer::public_transfer(pool_owner_cap, tx_context::sender(ctx));
    pool
}
</code></pre>



</details>

<a name="deepbook_clob_v2_create_customized_pool_v2"></a>

## Function `create_customized_pool_v2`

A V2 function for creating customized pools for better PTB friendliness/compostability.
If a user wants to create a pool and then destroy/lock the pool_owner_cap one can do
so with this function.


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_create_customized_pool_v2">create_customized_pool_v2</a>&lt;BaseAsset, QuoteAsset&gt;(<a href="../deepbook/clob_v2.md#deepbook_clob_v2_tick_size">tick_size</a>: u64, lot_size: u64, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_taker_fee_rate">taker_fee_rate</a>: u64, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_maker_rebate_rate">maker_rebate_rate</a>: u64, creation_fee: <a href="../sui/coin.md#sui_coin_Coin">sui::coin::Coin</a>&lt;<a href="../sui/sui.md#sui_sui_SUI">sui::sui::SUI</a>&gt;, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): (<a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">deepbook::clob_v2::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_PoolOwnerCap">deepbook::clob_v2::PoolOwnerCap</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_create_customized_pool_v2">create_customized_pool_v2</a>&lt;BaseAsset, QuoteAsset&gt;(
    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_tick_size">tick_size</a>: u64,
    lot_size: u64,
    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_taker_fee_rate">taker_fee_rate</a>: u64,
    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_maker_rebate_rate">maker_rebate_rate</a>: u64,
    creation_fee: Coin&lt;SUI&gt;,
    ctx: &<b>mut</b> TxContext,
) : (<a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_PoolOwnerCap">PoolOwnerCap</a>) {
    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_create_pool_with_return_">create_pool_with_return_</a>&lt;BaseAsset, QuoteAsset&gt;(
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_taker_fee_rate">taker_fee_rate</a>,
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_maker_rebate_rate">maker_rebate_rate</a>,
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_tick_size">tick_size</a>,
        lot_size,
        coin::into_balance(creation_fee),
        ctx
    )
}
</code></pre>



</details>

<a name="deepbook_clob_v2_deposit_base"></a>

## Function `deposit_base`



<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_deposit_base">deposit_base</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<b>mut</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">deepbook::clob_v2::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, coin: <a href="../sui/coin.md#sui_coin_Coin">sui::coin::Coin</a>&lt;BaseAsset&gt;, account_cap: &<a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_AccountCap">deepbook::custodian_v2::AccountCap</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_deposit_base">deposit_base</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool: &<b>mut</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    coin: Coin&lt;BaseAsset&gt;,
    account_cap: &AccountCap
) {
    <b>let</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a> = coin::value(&coin);
    <b>assert</b>!(<a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a> != 0, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_EInsufficientBaseCoin">EInsufficientBaseCoin</a>);
    <a href="../deepbook/custodian.md#deepbook_custodian_increase_user_available_balance">custodian::increase_user_available_balance</a>(
        &<b>mut</b> pool.base_custodian,
        account_owner(account_cap),
        coin::into_balance(coin)
    );
    event::emit(<a href="../deepbook/clob_v2.md#deepbook_clob_v2_DepositAsset">DepositAsset</a>&lt;BaseAsset&gt;{
        pool_id: *object::uid_as_inner(&pool.id),
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>,
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>: account_owner(account_cap)
    })
}
</code></pre>



</details>

<a name="deepbook_clob_v2_deposit_quote"></a>

## Function `deposit_quote`



<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_deposit_quote">deposit_quote</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<b>mut</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">deepbook::clob_v2::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, coin: <a href="../sui/coin.md#sui_coin_Coin">sui::coin::Coin</a>&lt;QuoteAsset&gt;, account_cap: &<a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_AccountCap">deepbook::custodian_v2::AccountCap</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_deposit_quote">deposit_quote</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool: &<b>mut</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    coin: Coin&lt;QuoteAsset&gt;,
    account_cap: &AccountCap
) {
    <b>let</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a> = coin::value(&coin);
    <b>assert</b>!(<a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a> != 0, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_EInsufficientQuoteCoin">EInsufficientQuoteCoin</a>);
    <a href="../deepbook/custodian.md#deepbook_custodian_increase_user_available_balance">custodian::increase_user_available_balance</a>(
        &<b>mut</b> pool.quote_custodian,
        account_owner(account_cap),
        coin::into_balance(coin)
    );
    event::emit(<a href="../deepbook/clob_v2.md#deepbook_clob_v2_DepositAsset">DepositAsset</a>&lt;QuoteAsset&gt;{
        pool_id: *object::uid_as_inner(&pool.id),
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>,
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>: account_owner(account_cap)
    })
}
</code></pre>



</details>

<a name="deepbook_clob_v2_withdraw_base"></a>

## Function `withdraw_base`



<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_withdraw_base">withdraw_base</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<b>mut</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">deepbook::clob_v2::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>: u64, account_cap: &<a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_AccountCap">deepbook::custodian_v2::AccountCap</a>, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <a href="../sui/coin.md#sui_coin_Coin">sui::coin::Coin</a>&lt;BaseAsset&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_withdraw_base">withdraw_base</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool: &<b>mut</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>: u64,
    account_cap: &AccountCap,
    ctx: &<b>mut</b> TxContext
): Coin&lt;BaseAsset&gt; {
    <b>assert</b>!(<a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a> &gt; 0, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_EInvalidQuantity">EInvalidQuantity</a>);
    event::emit(<a href="../deepbook/clob_v2.md#deepbook_clob_v2_WithdrawAsset">WithdrawAsset</a>&lt;BaseAsset&gt;{
        pool_id: *object::uid_as_inner(&pool.id),
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>,
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>: account_owner(account_cap)
    });
    <a href="../deepbook/custodian.md#deepbook_custodian_withdraw_asset">custodian::withdraw_asset</a>(&<b>mut</b> pool.base_custodian, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>, account_cap, ctx)
}
</code></pre>



</details>

<a name="deepbook_clob_v2_withdraw_quote"></a>

## Function `withdraw_quote`



<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_withdraw_quote">withdraw_quote</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<b>mut</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">deepbook::clob_v2::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>: u64, account_cap: &<a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_AccountCap">deepbook::custodian_v2::AccountCap</a>, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <a href="../sui/coin.md#sui_coin_Coin">sui::coin::Coin</a>&lt;QuoteAsset&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_withdraw_quote">withdraw_quote</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool: &<b>mut</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>: u64,
    account_cap: &AccountCap,
    ctx: &<b>mut</b> TxContext
): Coin&lt;QuoteAsset&gt; {
    <b>assert</b>!(<a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a> &gt; 0, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_EInvalidQuantity">EInvalidQuantity</a>);
    event::emit(<a href="../deepbook/clob_v2.md#deepbook_clob_v2_WithdrawAsset">WithdrawAsset</a>&lt;QuoteAsset&gt;{
        pool_id: *object::uid_as_inner(&pool.id),
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>,
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>: account_owner(account_cap)
    });
    <a href="../deepbook/custodian.md#deepbook_custodian_withdraw_asset">custodian::withdraw_asset</a>(&<b>mut</b> pool.quote_custodian, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>, account_cap, ctx)
}
</code></pre>



</details>

<a name="deepbook_clob_v2_swap_exact_base_for_quote"></a>

## Function `swap_exact_base_for_quote`



<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_swap_exact_base_for_quote">swap_exact_base_for_quote</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<b>mut</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">deepbook::clob_v2::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, client_order_id: u64, account_cap: &<a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_AccountCap">deepbook::custodian_v2::AccountCap</a>, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>: u64, base_coin: <a href="../sui/coin.md#sui_coin_Coin">sui::coin::Coin</a>&lt;BaseAsset&gt;, quote_coin: <a href="../sui/coin.md#sui_coin_Coin">sui::coin::Coin</a>&lt;QuoteAsset&gt;, clock: &<a href="../sui/clock.md#sui_clock_Clock">sui::clock::Clock</a>, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): (<a href="../sui/coin.md#sui_coin_Coin">sui::coin::Coin</a>&lt;BaseAsset&gt;, <a href="../sui/coin.md#sui_coin_Coin">sui::coin::Coin</a>&lt;QuoteAsset&gt;, u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_swap_exact_base_for_quote">swap_exact_base_for_quote</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool: &<b>mut</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    client_order_id: u64,
    account_cap: &AccountCap,
    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>: u64,
    base_coin: Coin&lt;BaseAsset&gt;,
    quote_coin: Coin&lt;QuoteAsset&gt;,
    clock: &Clock,
    ctx: &<b>mut</b> TxContext,
): (Coin&lt;BaseAsset&gt;, Coin&lt;QuoteAsset&gt;, u64) {
    <b>assert</b>!(<a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a> &gt; 0, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_EInvalidQuantity">EInvalidQuantity</a>);
    <b>assert</b>!(coin::value(&base_coin) &gt;= <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_EInsufficientBaseCoin">EInsufficientBaseCoin</a>);
    <b>let</b> original_val = coin::value(&quote_coin);
    <b>let</b> (ret_base_coin, ret_quote_coin) = <a href="../deepbook/clob_v2.md#deepbook_clob_v2_place_market_order">place_market_order</a>(
        pool,
        account_cap,
        client_order_id,
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>,
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

<a name="deepbook_clob_v2_swap_exact_base_for_quote_with_metadata"></a>

## Function `swap_exact_base_for_quote_with_metadata`



<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_swap_exact_base_for_quote_with_metadata">swap_exact_base_for_quote_with_metadata</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<b>mut</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">deepbook::clob_v2::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, client_order_id: u64, account_cap: &<a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_AccountCap">deepbook::custodian_v2::AccountCap</a>, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>: u64, base_coin: <a href="../sui/coin.md#sui_coin_Coin">sui::coin::Coin</a>&lt;BaseAsset&gt;, quote_coin: <a href="../sui/coin.md#sui_coin_Coin">sui::coin::Coin</a>&lt;QuoteAsset&gt;, clock: &<a href="../sui/clock.md#sui_clock_Clock">sui::clock::Clock</a>, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): (<a href="../sui/coin.md#sui_coin_Coin">sui::coin::Coin</a>&lt;BaseAsset&gt;, <a href="../sui/coin.md#sui_coin_Coin">sui::coin::Coin</a>&lt;QuoteAsset&gt;, u64, vector&lt;<a href="../deepbook/clob_v2.md#deepbook_clob_v2_MatchedOrderMetadata">deepbook::clob_v2::MatchedOrderMetadata</a>&lt;BaseAsset, QuoteAsset&gt;&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_swap_exact_base_for_quote_with_metadata">swap_exact_base_for_quote_with_metadata</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool: &<b>mut</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    client_order_id: u64,
    account_cap: &AccountCap,
    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>: u64,
    base_coin: Coin&lt;BaseAsset&gt;,
    quote_coin: Coin&lt;QuoteAsset&gt;,
    clock: &Clock,
    ctx: &<b>mut</b> TxContext,
): (Coin&lt;BaseAsset&gt;, Coin&lt;QuoteAsset&gt;, u64, vector&lt;<a href="../deepbook/clob_v2.md#deepbook_clob_v2_MatchedOrderMetadata">MatchedOrderMetadata</a>&lt;BaseAsset, QuoteAsset&gt;&gt;) {
    <b>let</b> original_val = coin::value(&quote_coin);
    <b>let</b> (ret_base_coin, ret_quote_coin, <b>mut</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_matched_order_metadata">matched_order_metadata</a>) = <a href="../deepbook/clob_v2.md#deepbook_clob_v2_place_market_order_int">place_market_order_int</a>(
        pool,
        account_cap,
        client_order_id,
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>,
        <b>false</b>,
        base_coin,
        quote_coin,
        clock,
        <b>true</b>, // <b>return</b> metadata
        ctx
    );
    <b>let</b> ret_val = coin::value(&ret_quote_coin);
    (ret_base_coin, ret_quote_coin, ret_val - original_val, option::extract(&<b>mut</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_matched_order_metadata">matched_order_metadata</a>))
}
</code></pre>



</details>

<a name="deepbook_clob_v2_swap_exact_quote_for_base"></a>

## Function `swap_exact_quote_for_base`



<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_swap_exact_quote_for_base">swap_exact_quote_for_base</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<b>mut</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">deepbook::clob_v2::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, client_order_id: u64, account_cap: &<a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_AccountCap">deepbook::custodian_v2::AccountCap</a>, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>: u64, clock: &<a href="../sui/clock.md#sui_clock_Clock">sui::clock::Clock</a>, quote_coin: <a href="../sui/coin.md#sui_coin_Coin">sui::coin::Coin</a>&lt;QuoteAsset&gt;, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): (<a href="../sui/coin.md#sui_coin_Coin">sui::coin::Coin</a>&lt;BaseAsset&gt;, <a href="../sui/coin.md#sui_coin_Coin">sui::coin::Coin</a>&lt;QuoteAsset&gt;, u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_swap_exact_quote_for_base">swap_exact_quote_for_base</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool: &<b>mut</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    client_order_id: u64,
    account_cap: &AccountCap,
    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>: u64,
    clock: &Clock,
    quote_coin: Coin&lt;QuoteAsset&gt;,
    ctx: &<b>mut</b> TxContext,
): (Coin&lt;BaseAsset&gt;, Coin&lt;QuoteAsset&gt;, u64) {
    <b>assert</b>!(<a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a> &gt; 0, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_EInvalidQuantity">EInvalidQuantity</a>);
    <b>assert</b>!(coin::value(&quote_coin) &gt;= <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_EInsufficientQuoteCoin">EInsufficientQuoteCoin</a>);
    <b>let</b> (base_asset_balance, quote_asset_balance, _matched_order_metadata) = <a href="../deepbook/clob_v2.md#deepbook_clob_v2_match_bid_with_quote_quantity">match_bid_with_quote_quantity</a>(
        pool,
        account_cap,
        client_order_id,
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>,
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_MAX_PRICE">MAX_PRICE</a>,
        clock::timestamp_ms(clock),
        coin::into_balance(quote_coin),
        <b>false</b> // don't <b>return</b> metadata
    );
    <b>let</b> val = balance::value(&base_asset_balance);
    (coin::from_balance(base_asset_balance, ctx), coin::from_balance(quote_asset_balance, ctx), val)
}
</code></pre>



</details>

<a name="deepbook_clob_v2_swap_exact_quote_for_base_with_metadata"></a>

## Function `swap_exact_quote_for_base_with_metadata`



<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_swap_exact_quote_for_base_with_metadata">swap_exact_quote_for_base_with_metadata</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<b>mut</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">deepbook::clob_v2::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, client_order_id: u64, account_cap: &<a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_AccountCap">deepbook::custodian_v2::AccountCap</a>, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>: u64, clock: &<a href="../sui/clock.md#sui_clock_Clock">sui::clock::Clock</a>, quote_coin: <a href="../sui/coin.md#sui_coin_Coin">sui::coin::Coin</a>&lt;QuoteAsset&gt;, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): (<a href="../sui/coin.md#sui_coin_Coin">sui::coin::Coin</a>&lt;BaseAsset&gt;, <a href="../sui/coin.md#sui_coin_Coin">sui::coin::Coin</a>&lt;QuoteAsset&gt;, u64, vector&lt;<a href="../deepbook/clob_v2.md#deepbook_clob_v2_MatchedOrderMetadata">deepbook::clob_v2::MatchedOrderMetadata</a>&lt;BaseAsset, QuoteAsset&gt;&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_swap_exact_quote_for_base_with_metadata">swap_exact_quote_for_base_with_metadata</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool: &<b>mut</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    client_order_id: u64,
    account_cap: &AccountCap,
    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>: u64,
    clock: &Clock,
    quote_coin: Coin&lt;QuoteAsset&gt;,
    ctx: &<b>mut</b> TxContext,
): (Coin&lt;BaseAsset&gt;, Coin&lt;QuoteAsset&gt;, u64, vector&lt;<a href="../deepbook/clob_v2.md#deepbook_clob_v2_MatchedOrderMetadata">MatchedOrderMetadata</a>&lt;BaseAsset, QuoteAsset&gt;&gt;) {
    <b>assert</b>!(<a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a> &gt; 0, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_EInvalidQuantity">EInvalidQuantity</a>);
    <b>assert</b>!(coin::value(&quote_coin) &gt;= <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_EInsufficientQuoteCoin">EInsufficientQuoteCoin</a>);
    <b>let</b> (base_asset_balance, quote_asset_balance, <b>mut</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_matched_order_metadata">matched_order_metadata</a>) = <a href="../deepbook/clob_v2.md#deepbook_clob_v2_match_bid_with_quote_quantity">match_bid_with_quote_quantity</a>(
        pool,
        account_cap,
        client_order_id,
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>,
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_MAX_PRICE">MAX_PRICE</a>,
        clock::timestamp_ms(clock),
        coin::into_balance(quote_coin),
        <b>true</b> // <b>return</b> metadata
    );
    <b>let</b> val = balance::value(&base_asset_balance);
    (coin::from_balance(base_asset_balance, ctx), coin::from_balance(quote_asset_balance, ctx), val, option::extract(&<b>mut</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_matched_order_metadata">matched_order_metadata</a>))
}
</code></pre>



</details>

<a name="deepbook_clob_v2_match_bid_with_quote_quantity"></a>

## Function `match_bid_with_quote_quantity`



<pre><code><b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_match_bid_with_quote_quantity">match_bid_with_quote_quantity</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<b>mut</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">deepbook::clob_v2::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, account_cap: &<a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_AccountCap">deepbook::custodian_v2::AccountCap</a>, client_order_id: u64, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>: u64, price_limit: u64, current_timestamp: u64, quote_balance: <a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;QuoteAsset&gt;, compute_metadata: bool): (<a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;BaseAsset&gt;, <a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;QuoteAsset&gt;, <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;vector&lt;<a href="../deepbook/clob_v2.md#deepbook_clob_v2_MatchedOrderMetadata">deepbook::clob_v2::MatchedOrderMetadata</a>&lt;BaseAsset, QuoteAsset&gt;&gt;&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_match_bid_with_quote_quantity">match_bid_with_quote_quantity</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool: &<b>mut</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    account_cap: &AccountCap,
    client_order_id: u64,
    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>: u64,
    price_limit: u64,
    current_timestamp: u64,
    quote_balance: Balance&lt;QuoteAsset&gt;,
    compute_metadata: bool,
): (Balance&lt;BaseAsset&gt;, Balance&lt;QuoteAsset&gt;, Option&lt;vector&lt;<a href="../deepbook/clob_v2.md#deepbook_clob_v2_MatchedOrderMetadata">MatchedOrderMetadata</a>&lt;BaseAsset, QuoteAsset&gt;&gt;&gt;) {
    // Base balance received by taker, taking into account of taker commission.
    // Need to individually keep track of the remaining base <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a> to be filled to avoid infinite <b>loop</b>.
    <b>let</b> pool_id = *object::uid_as_inner(&pool.id);
    <b>let</b> <b>mut</b> taker_quote_quantity_remaining = <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>;
    <b>let</b> <b>mut</b> base_balance_filled = balance::zero&lt;BaseAsset&gt;();
    <b>let</b> <b>mut</b> quote_balance_left = quote_balance;
    <b>let</b> all_open_orders = &<b>mut</b> pool.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_asks">asks</a>;
    <b>let</b> <b>mut</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_matched_order_metadata">matched_order_metadata</a> = vector::empty&lt;<a href="../deepbook/clob_v2.md#deepbook_clob_v2_MatchedOrderMetadata">MatchedOrderMetadata</a>&lt;BaseAsset, QuoteAsset&gt;&gt;();
    <b>if</b> (<a href="../deepbook/critbit.md#deepbook_critbit_is_empty">critbit::is_empty</a>(all_open_orders)) {
        <b>return</b> (base_balance_filled, quote_balance_left, option::none())
    };
    <b>let</b> (<b>mut</b> tick_price, <b>mut</b> tick_index) = min_leaf(all_open_orders);
    <b>let</b> <b>mut</b> terminate_loop = <b>false</b>;
    <b>let</b> <b>mut</b> canceled_order_events = vector[];
    <b>while</b> (!is_empty&lt;<a href="../deepbook/clob_v2.md#deepbook_clob_v2_TickLevel">TickLevel</a>&gt;(all_open_orders) && tick_price &lt;= price_limit) {
        <b>let</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_tick_level">tick_level</a> = borrow_mut_leaf_by_index(all_open_orders, tick_index);
        <b>let</b> <b>mut</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a> = *option::borrow(linked_table::front(&<a href="../deepbook/clob_v2.md#deepbook_clob_v2_tick_level">tick_level</a>.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_open_orders">open_orders</a>));
        <b>while</b> (!linked_table::is_empty(&<a href="../deepbook/clob_v2.md#deepbook_clob_v2_tick_level">tick_level</a>.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_open_orders">open_orders</a>)) {
            <b>let</b> maker_order = linked_table::borrow(&<a href="../deepbook/clob_v2.md#deepbook_clob_v2_tick_level">tick_level</a>.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_open_orders">open_orders</a>, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>);
            <b>let</b> <b>mut</b> maker_base_quantity = maker_order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>;
            <b>let</b> <b>mut</b> skip_order = <b>false</b>;
            <b>if</b> (maker_order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_expire_timestamp">expire_timestamp</a> &lt;= current_timestamp || account_owner(account_cap) == maker_order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>) {
                skip_order = <b>true</b>;
                <a href="../deepbook/custodian.md#deepbook_custodian_unlock_balance">custodian::unlock_balance</a>(&<b>mut</b> pool.base_custodian, maker_order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>, maker_order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>);
                <b>let</b> canceled_order_event = <a href="../deepbook/clob_v2.md#deepbook_clob_v2_AllOrdersCanceledComponent">AllOrdersCanceledComponent</a>&lt;BaseAsset, QuoteAsset&gt; {
                    client_order_id: maker_order.client_order_id,
                    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>: maker_order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>,
                    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_is_bid">is_bid</a>: maker_order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_is_bid">is_bid</a>,
                    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>: maker_order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>,
                    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_original_quantity">original_quantity</a>: maker_order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_original_quantity">original_quantity</a>,
                    base_asset_quantity_canceled: maker_order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>,
                    price: maker_order.price
                };
                vector::push_back(&<b>mut</b> canceled_order_events, canceled_order_event);
            } <b>else</b> {
                // Calculate how much quote asset (maker_quote_quantity) is required, including the commission, to fill the maker order.
                <b>let</b> maker_quote_quantity_without_commission = clob_math::mul(
                    maker_base_quantity,
                    maker_order.price
                );
                <b>let</b> (is_round_down, <b>mut</b> taker_commission)  = clob_math::unsafe_mul_round(
                    maker_quote_quantity_without_commission,
                    pool.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_taker_fee_rate">taker_fee_rate</a>
                );
                <b>if</b> (is_round_down)  taker_commission = taker_commission + 1;
                <b>let</b> maker_quote_quantity = maker_quote_quantity_without_commission + taker_commission;
                // Total base <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a> filled.
                <b>let</b> <b>mut</b> filled_base_quantity: u64;
                // Total quote <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a> filled, excluding commission and rebate.
                <b>let</b> <b>mut</b> filled_quote_quantity: u64;
                // Total quote <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a> paid by taker.
                // filled_quote_quantity_without_commission * (<a href="../deepbook/clob_v2.md#deepbook_clob_v2_FLOAT_SCALING">FLOAT_SCALING</a> + <a href="../deepbook/clob_v2.md#deepbook_clob_v2_taker_fee_rate">taker_fee_rate</a>) = filled_quote_quantity
                <b>let</b> <b>mut</b> filled_quote_quantity_without_commission: u64;
                <b>if</b> (taker_quote_quantity_remaining &gt; maker_quote_quantity) {
                    filled_quote_quantity = maker_quote_quantity;
                    filled_quote_quantity_without_commission = maker_quote_quantity_without_commission;
                    filled_base_quantity = maker_base_quantity;
                } <b>else</b> {
                    terminate_loop = <b>true</b>;
                    // <b>if</b> not enough quote <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a> to pay <b>for</b> taker commission, then no <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a> will be filled
                    filled_quote_quantity_without_commission = clob_math::unsafe_div(
                        taker_quote_quantity_remaining,
                        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_FLOAT_SCALING">FLOAT_SCALING</a> + pool.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_taker_fee_rate">taker_fee_rate</a>
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
                        pool.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_taker_fee_rate">taker_fee_rate</a>
                    );
                    <b>if</b> (round_down) {
                        taker_commission = taker_commission + 1;
                    };
                    filled_quote_quantity = filled_quote_quantity_without_commission + taker_commission;
                };
                // <b>if</b> maker_rebate = 0 due to underflow, maker will not receive a rebate
                <b>let</b> maker_rebate = clob_math::unsafe_mul(
                    filled_quote_quantity_without_commission,
                    pool.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_maker_rebate_rate">maker_rebate_rate</a>
                );
                maker_base_quantity = maker_base_quantity - filled_base_quantity;
                // maker in ask side, decrease maker's locked base asset, increase maker's available quote asset
                taker_quote_quantity_remaining = taker_quote_quantity_remaining - filled_quote_quantity;
                <b>let</b> locked_base_balance = <a href="../deepbook/custodian.md#deepbook_custodian_decrease_user_locked_balance">custodian::decrease_user_locked_balance</a>&lt;BaseAsset&gt;(
                    &<b>mut</b> pool.base_custodian,
                    maker_order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>,
                    filled_base_quantity
                );
                <b>let</b> <b>mut</b> quote_balance_filled = balance::split(
                    &<b>mut</b> quote_balance_left,
                    filled_quote_quantity,
                );
                // Send quote asset including rebate to maker.
                <a href="../deepbook/custodian.md#deepbook_custodian_increase_user_available_balance">custodian::increase_user_available_balance</a>&lt;QuoteAsset&gt;(
                    &<b>mut</b> pool.quote_custodian,
                    maker_order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>,
                    balance::split(
                        &<b>mut</b> quote_balance_filled,
                        maker_rebate + filled_quote_quantity_without_commission,
                    ),
                );
                // Send remaining of commission - rebate to the protocol.
                // commission - rebate = filled_quote_quantity_without_commission - filled_quote_quantity - maker_rebate
                balance::join(&<b>mut</b> pool.quote_asset_trading_fees, quote_balance_filled);
                balance::join(&<b>mut</b> base_balance_filled, locked_base_balance);
                <a href="../deepbook/clob_v2.md#deepbook_clob_v2_emit_order_filled">emit_order_filled</a>&lt;BaseAsset, QuoteAsset&gt;(
                    *object::uid_as_inner(&pool.id),
                    client_order_id,
                    account_owner(account_cap),
                    maker_order,
                    filled_base_quantity,
                    // taker_commission = filled_quote_quantity - filled_quote_quantity_without_commission
                    // This guarantees that the subtraction will not underflow
                    filled_quote_quantity - filled_quote_quantity_without_commission,
                    maker_rebate
                );
                <b>if</b>(compute_metadata) {
                    vector::push_back(
                        &<b>mut</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_matched_order_metadata">matched_order_metadata</a>,
                        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_matched_order_metadata">matched_order_metadata</a>(
                            *object::uid_as_inner(&pool.id),
                            account_owner(account_cap),
                            maker_order,
                            filled_base_quantity,
                            // taker_commission = filled_quote_quantity - filled_quote_quantity_without_commission
                            // This guarantees that the subtraction will not underflow
                            filled_quote_quantity - filled_quote_quantity_without_commission,
                            maker_rebate
                        )
                    );
                };
            };
            <b>if</b> (skip_order || maker_base_quantity == 0) {
                // Remove the maker order.
                <b>let</b> old_order_id = <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>;
                <b>let</b> maybe_order_id = linked_table::next(&<a href="../deepbook/clob_v2.md#deepbook_clob_v2_tick_level">tick_level</a>.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_open_orders">open_orders</a>, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>);
                <b>if</b> (!option::is_none(maybe_order_id)) {
                    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a> = *option::borrow(maybe_order_id);
                };
                <b>let</b> usr_open_order_ids = table::borrow_mut(&<b>mut</b> pool.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_usr_open_orders">usr_open_orders</a>, maker_order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>);
                linked_table::remove(usr_open_order_ids, old_order_id);
                linked_table::remove(&<b>mut</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_tick_level">tick_level</a>.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_open_orders">open_orders</a>, old_order_id);
            } <b>else</b> {
                // Update the maker order.
                <b>let</b> maker_order_mut = linked_table::borrow_mut(
                    &<b>mut</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_tick_level">tick_level</a>.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_open_orders">open_orders</a>,
                    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>);
                maker_order_mut.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a> = maker_base_quantity;
            };
            <b>if</b> (terminate_loop) {
                <b>break</b>
            };
        };
        <b>if</b> (linked_table::is_empty(&<a href="../deepbook/clob_v2.md#deepbook_clob_v2_tick_level">tick_level</a>.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_open_orders">open_orders</a>)) {
            (tick_price, _) = next_leaf(all_open_orders, tick_price);
            <a href="../deepbook/clob_v2.md#deepbook_clob_v2_destroy_empty_level">destroy_empty_level</a>(remove_leaf_by_index(all_open_orders, tick_index));
            (_, tick_index) = find_leaf(all_open_orders, tick_price);
        };
        <b>if</b> (terminate_loop) {
            <b>break</b>
        };
    };
    <b>if</b> (!vector::is_empty(&canceled_order_events)) {
        event::emit(<a href="../deepbook/clob_v2.md#deepbook_clob_v2_AllOrdersCanceled">AllOrdersCanceled</a>&lt;BaseAsset, QuoteAsset&gt; {
            pool_id,
            orders_canceled: canceled_order_events,
        });
    };
    <b>return</b> (base_balance_filled, quote_balance_left, <b>if</b>(compute_metadata) option::some(<a href="../deepbook/clob_v2.md#deepbook_clob_v2_matched_order_metadata">matched_order_metadata</a>) <b>else</b> option::none())
}
</code></pre>



</details>

<a name="deepbook_clob_v2_match_bid"></a>

## Function `match_bid`



<pre><code><b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_match_bid">match_bid</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<b>mut</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">deepbook::clob_v2::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, account_cap: &<a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_AccountCap">deepbook::custodian_v2::AccountCap</a>, client_order_id: u64, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>: u64, price_limit: u64, current_timestamp: u64, quote_balance: <a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;QuoteAsset&gt;, compute_metadata: bool): (<a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;BaseAsset&gt;, <a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;QuoteAsset&gt;, <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;vector&lt;<a href="../deepbook/clob_v2.md#deepbook_clob_v2_MatchedOrderMetadata">deepbook::clob_v2::MatchedOrderMetadata</a>&lt;BaseAsset, QuoteAsset&gt;&gt;&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_match_bid">match_bid</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool: &<b>mut</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    account_cap: &AccountCap,
    client_order_id: u64,
    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>: u64,
    price_limit: u64,
    current_timestamp: u64,
    quote_balance: Balance&lt;QuoteAsset&gt;,
    compute_metadata: bool,
): (Balance&lt;BaseAsset&gt;, Balance&lt;QuoteAsset&gt;, Option&lt;vector&lt;<a href="../deepbook/clob_v2.md#deepbook_clob_v2_MatchedOrderMetadata">MatchedOrderMetadata</a>&lt;BaseAsset, QuoteAsset&gt;&gt;&gt;) {
    <b>let</b> pool_id = *object::uid_as_inner(&pool.id);
    // Base balance received by taker.
    // Need to individually keep track of the remaining base <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a> to be filled to avoid infinite <b>loop</b>.
    <b>let</b> <b>mut</b> taker_base_quantity_remaining = <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>;
    <b>let</b> <b>mut</b> base_balance_filled = balance::zero&lt;BaseAsset&gt;();
    <b>let</b> <b>mut</b> quote_balance_left = quote_balance;
    <b>let</b> all_open_orders = &<b>mut</b> pool.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_asks">asks</a>;
    <b>let</b> <b>mut</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_matched_order_metadata">matched_order_metadata</a> = vector::empty&lt;<a href="../deepbook/clob_v2.md#deepbook_clob_v2_MatchedOrderMetadata">MatchedOrderMetadata</a>&lt;BaseAsset, QuoteAsset&gt;&gt;();
    <b>if</b> (<a href="../deepbook/critbit.md#deepbook_critbit_is_empty">critbit::is_empty</a>(all_open_orders)) {
        <b>return</b> (base_balance_filled, quote_balance_left, option::none())
    };
    <b>let</b> (<b>mut</b> tick_price, <b>mut</b> tick_index) = min_leaf(all_open_orders);
    <b>let</b> <b>mut</b> canceled_order_events = vector[];
    <b>while</b> (!is_empty&lt;<a href="../deepbook/clob_v2.md#deepbook_clob_v2_TickLevel">TickLevel</a>&gt;(all_open_orders) && tick_price &lt;= price_limit) {
        <b>let</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_tick_level">tick_level</a> = borrow_mut_leaf_by_index(all_open_orders, tick_index);
        <b>let</b> <b>mut</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a> = *option::borrow(linked_table::front(&<a href="../deepbook/clob_v2.md#deepbook_clob_v2_tick_level">tick_level</a>.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_open_orders">open_orders</a>));
        <b>while</b> (!linked_table::is_empty(&<a href="../deepbook/clob_v2.md#deepbook_clob_v2_tick_level">tick_level</a>.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_open_orders">open_orders</a>)) {
            <b>let</b> maker_order = linked_table::borrow(&<a href="../deepbook/clob_v2.md#deepbook_clob_v2_tick_level">tick_level</a>.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_open_orders">open_orders</a>, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>);
            <b>let</b> <b>mut</b> maker_base_quantity = maker_order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>;
            <b>let</b> <b>mut</b> skip_order = <b>false</b>;
            <b>if</b> (maker_order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_expire_timestamp">expire_timestamp</a> &lt;= current_timestamp || account_owner(account_cap) == maker_order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>) {
                skip_order = <b>true</b>;
                <a href="../deepbook/custodian.md#deepbook_custodian_unlock_balance">custodian::unlock_balance</a>(&<b>mut</b> pool.base_custodian, maker_order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>, maker_order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>);
                <b>let</b> canceled_order_event = <a href="../deepbook/clob_v2.md#deepbook_clob_v2_AllOrdersCanceledComponent">AllOrdersCanceledComponent</a>&lt;BaseAsset, QuoteAsset&gt; {
                    client_order_id: maker_order.client_order_id,
                    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>: maker_order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>,
                    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_is_bid">is_bid</a>: maker_order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_is_bid">is_bid</a>,
                    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>: maker_order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>,
                    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_original_quantity">original_quantity</a>: maker_order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_original_quantity">original_quantity</a>,
                    base_asset_quantity_canceled: maker_order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>,
                    price: maker_order.price
                };
                vector::push_back(&<b>mut</b> canceled_order_events, canceled_order_event);
            } <b>else</b> {
                <b>let</b> filled_base_quantity =
                    <b>if</b> (taker_base_quantity_remaining &gt; maker_base_quantity) { maker_base_quantity }
                    <b>else</b> { taker_base_quantity_remaining };
                // Note that <b>if</b> a user creates a pool that allows orders that are too small, this will fail since we cannot have a filled
                // quote <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a> of 0.
                <b>let</b> filled_quote_quantity = clob_math::mul(filled_base_quantity, maker_order.price);
                // <b>if</b> maker_rebate = 0 due to underflow, maker will not receive a rebate
                <b>let</b> maker_rebate = clob_math::unsafe_mul(filled_quote_quantity, pool.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_maker_rebate_rate">maker_rebate_rate</a>);
                // <b>if</b> taker_commission = 0 due to underflow, round it up to 1
                <b>let</b> (is_round_down, <b>mut</b> taker_commission) = clob_math::unsafe_mul_round(
                    filled_quote_quantity,
                    pool.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_taker_fee_rate">taker_fee_rate</a>
                );
                <b>if</b> (is_round_down) taker_commission = taker_commission + 1;
                maker_base_quantity = maker_base_quantity - filled_base_quantity;
                // maker in ask side, decrease maker's locked base asset, increase maker's available quote asset
                taker_base_quantity_remaining = taker_base_quantity_remaining - filled_base_quantity;
                <b>let</b> locked_base_balance = <a href="../deepbook/custodian.md#deepbook_custodian_decrease_user_locked_balance">custodian::decrease_user_locked_balance</a>&lt;BaseAsset&gt;(
                    &<b>mut</b> pool.base_custodian,
                    maker_order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>,
                    filled_base_quantity
                );
                <b>let</b> <b>mut</b> taker_commission_balance = balance::split(
                    &<b>mut</b> quote_balance_left,
                    taker_commission,
                );
                <a href="../deepbook/custodian.md#deepbook_custodian_increase_user_available_balance">custodian::increase_user_available_balance</a>&lt;QuoteAsset&gt;(
                    &<b>mut</b> pool.quote_custodian,
                    maker_order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>,
                    balance::split(
                        &<b>mut</b> taker_commission_balance,
                        maker_rebate,
                    ),
                );
                balance::join(&<b>mut</b> pool.quote_asset_trading_fees, taker_commission_balance);
                balance::join(&<b>mut</b> base_balance_filled, locked_base_balance);
                <a href="../deepbook/custodian.md#deepbook_custodian_increase_user_available_balance">custodian::increase_user_available_balance</a>&lt;QuoteAsset&gt;(
                    &<b>mut</b> pool.quote_custodian,
                    maker_order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>,
                    balance::split(
                        &<b>mut</b> quote_balance_left,
                        filled_quote_quantity,
                    ),
                );
                <a href="../deepbook/clob_v2.md#deepbook_clob_v2_emit_order_filled">emit_order_filled</a>&lt;BaseAsset, QuoteAsset&gt;(
                    *object::uid_as_inner(&pool.id),
                    client_order_id,
                    account_owner(account_cap),
                    maker_order,
                    filled_base_quantity,
                    taker_commission,
                    maker_rebate
                );
                <b>if</b>(compute_metadata){
                    vector::push_back(
                        &<b>mut</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_matched_order_metadata">matched_order_metadata</a>,
                        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_matched_order_metadata">matched_order_metadata</a>(
                            *object::uid_as_inner(&pool.id),
                            account_owner(account_cap),
                            maker_order,
                            filled_base_quantity,
                            taker_commission,
                            maker_rebate
                        )
                    );
                };
            };
            <b>if</b> (skip_order || maker_base_quantity == 0) {
                // Remove the maker order.
                <b>let</b> old_order_id = <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>;
                <b>let</b> maybe_order_id = linked_table::next(&<a href="../deepbook/clob_v2.md#deepbook_clob_v2_tick_level">tick_level</a>.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_open_orders">open_orders</a>, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>);
                <b>if</b> (!option::is_none(maybe_order_id)) {
                    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a> = *option::borrow(maybe_order_id);
                };
                <b>let</b> usr_open_order_ids = table::borrow_mut(&<b>mut</b> pool.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_usr_open_orders">usr_open_orders</a>, maker_order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>);
                linked_table::remove(usr_open_order_ids, old_order_id);
                linked_table::remove(&<b>mut</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_tick_level">tick_level</a>.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_open_orders">open_orders</a>, old_order_id);
            } <b>else</b> {
                // Update the maker order.
                <b>let</b> maker_order_mut = linked_table::borrow_mut(
                    &<b>mut</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_tick_level">tick_level</a>.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_open_orders">open_orders</a>,
                    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>);
                maker_order_mut.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a> = maker_base_quantity;
            };
            <b>if</b> (taker_base_quantity_remaining == 0) {
                <b>break</b>
            };
        };
        <b>if</b> (linked_table::is_empty(&<a href="../deepbook/clob_v2.md#deepbook_clob_v2_tick_level">tick_level</a>.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_open_orders">open_orders</a>)) {
            (tick_price, _) = next_leaf(all_open_orders, tick_price);
            <a href="../deepbook/clob_v2.md#deepbook_clob_v2_destroy_empty_level">destroy_empty_level</a>(remove_leaf_by_index(all_open_orders, tick_index));
            (_, tick_index) = find_leaf(all_open_orders, tick_price);
        };
        <b>if</b> (taker_base_quantity_remaining == 0) {
            <b>break</b>
        };
    };
    <b>if</b> (!vector::is_empty(&canceled_order_events)) {
        event::emit(<a href="../deepbook/clob_v2.md#deepbook_clob_v2_AllOrdersCanceled">AllOrdersCanceled</a>&lt;BaseAsset, QuoteAsset&gt; {
            pool_id,
            orders_canceled: canceled_order_events,
        });
    };
    <b>return</b> (base_balance_filled, quote_balance_left, <b>if</b>(compute_metadata) option::some(<a href="../deepbook/clob_v2.md#deepbook_clob_v2_matched_order_metadata">matched_order_metadata</a>) <b>else</b> option::none())
}
</code></pre>



</details>

<a name="deepbook_clob_v2_match_ask"></a>

## Function `match_ask`



<pre><code><b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_match_ask">match_ask</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<b>mut</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">deepbook::clob_v2::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, account_cap: &<a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_AccountCap">deepbook::custodian_v2::AccountCap</a>, client_order_id: u64, price_limit: u64, current_timestamp: u64, base_balance: <a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;BaseAsset&gt;, compute_metadata: bool): (<a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;BaseAsset&gt;, <a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;QuoteAsset&gt;, <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;vector&lt;<a href="../deepbook/clob_v2.md#deepbook_clob_v2_MatchedOrderMetadata">deepbook::clob_v2::MatchedOrderMetadata</a>&lt;BaseAsset, QuoteAsset&gt;&gt;&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_match_ask">match_ask</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool: &<b>mut</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    account_cap: &AccountCap,
    client_order_id: u64,
    price_limit: u64,
    current_timestamp: u64,
    base_balance: Balance&lt;BaseAsset&gt;,
    compute_metadata: bool,
): (Balance&lt;BaseAsset&gt;, Balance&lt;QuoteAsset&gt;, Option&lt;vector&lt;<a href="../deepbook/clob_v2.md#deepbook_clob_v2_MatchedOrderMetadata">MatchedOrderMetadata</a>&lt;BaseAsset, QuoteAsset&gt;&gt;&gt;) {
    <b>let</b> pool_id = *object::uid_as_inner(&pool.id);
    <b>let</b> <b>mut</b> base_balance_left = base_balance;
    // Base balance received by taker, taking into account of taker commission.
    <b>let</b> <b>mut</b> quote_balance_filled = balance::zero&lt;QuoteAsset&gt;();
    <b>let</b> all_open_orders = &<b>mut</b> pool.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_bids">bids</a>;
    <b>let</b> <b>mut</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_matched_order_metadata">matched_order_metadata</a> = vector::empty&lt;<a href="../deepbook/clob_v2.md#deepbook_clob_v2_MatchedOrderMetadata">MatchedOrderMetadata</a>&lt;BaseAsset, QuoteAsset&gt;&gt;();
    <b>if</b> (<a href="../deepbook/critbit.md#deepbook_critbit_is_empty">critbit::is_empty</a>(all_open_orders)) {
        <b>return</b> (base_balance_left, quote_balance_filled, option::none())
    };
    <b>let</b> (<b>mut</b> tick_price, <b>mut</b> tick_index) = max_leaf(all_open_orders);
    <b>let</b> <b>mut</b> canceled_order_events = vector[];
    <b>while</b> (!is_empty&lt;<a href="../deepbook/clob_v2.md#deepbook_clob_v2_TickLevel">TickLevel</a>&gt;(all_open_orders) && tick_price &gt;= price_limit) {
        <b>let</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_tick_level">tick_level</a> = borrow_mut_leaf_by_index(all_open_orders, tick_index);
        <b>let</b> <b>mut</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a> = *option::borrow(linked_table::front(&<a href="../deepbook/clob_v2.md#deepbook_clob_v2_tick_level">tick_level</a>.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_open_orders">open_orders</a>));
        <b>while</b> (!linked_table::is_empty(&<a href="../deepbook/clob_v2.md#deepbook_clob_v2_tick_level">tick_level</a>.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_open_orders">open_orders</a>)) {
            <b>let</b> maker_order = linked_table::borrow(&<a href="../deepbook/clob_v2.md#deepbook_clob_v2_tick_level">tick_level</a>.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_open_orders">open_orders</a>, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>);
            <b>let</b> <b>mut</b> maker_base_quantity = maker_order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>;
            <b>let</b> <b>mut</b> skip_order = <b>false</b>;
            <b>if</b> (maker_order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_expire_timestamp">expire_timestamp</a> &lt;= current_timestamp || account_owner(account_cap) == maker_order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>) {
                skip_order = <b>true</b>;
                <b>let</b> maker_quote_quantity = clob_math::mul(maker_order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>, maker_order.price);
                <a href="../deepbook/custodian.md#deepbook_custodian_unlock_balance">custodian::unlock_balance</a>(&<b>mut</b> pool.quote_custodian, maker_order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>, maker_quote_quantity);
                <b>let</b> canceled_order_event = <a href="../deepbook/clob_v2.md#deepbook_clob_v2_AllOrdersCanceledComponent">AllOrdersCanceledComponent</a>&lt;BaseAsset, QuoteAsset&gt; {
                    client_order_id: maker_order.client_order_id,
                    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>: maker_order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>,
                    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_is_bid">is_bid</a>: maker_order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_is_bid">is_bid</a>,
                    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>: maker_order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>,
                    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_original_quantity">original_quantity</a>: maker_order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_original_quantity">original_quantity</a>,
                    base_asset_quantity_canceled: maker_order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>,
                    price: maker_order.price
                };
                vector::push_back(&<b>mut</b> canceled_order_events, canceled_order_event);
            } <b>else</b> {
                <b>let</b> taker_base_quantity_remaining = balance::value(&base_balance_left);
                <b>let</b> filled_base_quantity =
                    <b>if</b> (taker_base_quantity_remaining &gt;= maker_base_quantity) { maker_base_quantity }
                    <b>else</b> { taker_base_quantity_remaining };
                // If a bit is rounded down, the pool will take this <b>as</b> a fee.
                <b>let</b> (is_round_down, filled_quote_quantity) = clob_math::unsafe_mul_round(filled_base_quantity, maker_order.price);
                <b>if</b> (is_round_down) {
                    <b>let</b> rounded_down_quantity = <a href="../deepbook/custodian.md#deepbook_custodian_decrease_user_locked_balance">custodian::decrease_user_locked_balance</a>&lt;QuoteAsset&gt;(
                        &<b>mut</b> pool.quote_custodian,
                        maker_order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>,
                        1
                    );
                    balance::join(&<b>mut</b> pool.quote_asset_trading_fees, rounded_down_quantity);
                };
                // <b>if</b> maker_rebate = 0 due to underflow, maker will not receive a rebate
                <b>let</b> maker_rebate = clob_math::unsafe_mul(filled_quote_quantity, pool.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_maker_rebate_rate">maker_rebate_rate</a>);
                // <b>if</b> taker_commission = 0 due to underflow, round it up to 1
                <b>let</b> (is_round_down, <b>mut</b> taker_commission) = clob_math::unsafe_mul_round(
                    filled_quote_quantity,
                    pool.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_taker_fee_rate">taker_fee_rate</a>
                );
                <b>if</b> (is_round_down) taker_commission = taker_commission + 1;
                maker_base_quantity = maker_base_quantity - filled_base_quantity;
                // maker in bid side, decrease maker's locked quote asset, increase maker's available base asset
                <b>let</b> <b>mut</b> locked_quote_balance = <a href="../deepbook/custodian.md#deepbook_custodian_decrease_user_locked_balance">custodian::decrease_user_locked_balance</a>&lt;QuoteAsset&gt;(
                    &<b>mut</b> pool.quote_custodian,
                    maker_order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>,
                    filled_quote_quantity
                );
                <b>let</b> <b>mut</b> taker_commission_balance = balance::split(
                    &<b>mut</b> locked_quote_balance,
                    taker_commission,
                );
                <a href="../deepbook/custodian.md#deepbook_custodian_increase_user_available_balance">custodian::increase_user_available_balance</a>&lt;QuoteAsset&gt;(
                    &<b>mut</b> pool.quote_custodian,
                    maker_order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>,
                    balance::split(
                        &<b>mut</b> taker_commission_balance,
                        maker_rebate,
                    ),
                );
                balance::join(&<b>mut</b> pool.quote_asset_trading_fees, taker_commission_balance);
                balance::join(&<b>mut</b> quote_balance_filled, locked_quote_balance);
                <a href="../deepbook/custodian.md#deepbook_custodian_increase_user_available_balance">custodian::increase_user_available_balance</a>&lt;BaseAsset&gt;(
                    &<b>mut</b> pool.base_custodian,
                    maker_order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>,
                    balance::split(
                        &<b>mut</b> base_balance_left,
                        filled_base_quantity,
                    ),
                );
                <a href="../deepbook/clob_v2.md#deepbook_clob_v2_emit_order_filled">emit_order_filled</a>&lt;BaseAsset, QuoteAsset&gt;(
                    *object::uid_as_inner(&pool.id),
                    client_order_id,
                    account_owner(account_cap),
                    maker_order,
                    filled_base_quantity,
                    taker_commission,
                    maker_rebate
                );
                <b>if</b>(compute_metadata) {
                    vector::push_back(
                        &<b>mut</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_matched_order_metadata">matched_order_metadata</a>,
                        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_matched_order_metadata">matched_order_metadata</a>(
                            *object::uid_as_inner(&pool.id),
                            account_owner(account_cap),
                            maker_order,
                            filled_base_quantity,
                            taker_commission,
                            maker_rebate
                        )
                    );
                }
            };
            <b>if</b> (skip_order || maker_base_quantity == 0) {
                // Remove the maker order.
                <b>let</b> old_order_id = <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>;
                <b>let</b> maybe_order_id = linked_table::next(&<a href="../deepbook/clob_v2.md#deepbook_clob_v2_tick_level">tick_level</a>.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_open_orders">open_orders</a>, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>);
                <b>if</b> (!option::is_none(maybe_order_id)) {
                    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a> = *option::borrow(maybe_order_id);
                };
                <b>let</b> usr_open_order_ids = table::borrow_mut(&<b>mut</b> pool.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_usr_open_orders">usr_open_orders</a>, maker_order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>);
                linked_table::remove(usr_open_order_ids, old_order_id);
                linked_table::remove(&<b>mut</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_tick_level">tick_level</a>.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_open_orders">open_orders</a>, old_order_id);
            } <b>else</b> {
                // Update the maker order.
                <b>let</b> maker_order_mut = linked_table::borrow_mut(
                    &<b>mut</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_tick_level">tick_level</a>.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_open_orders">open_orders</a>,
                    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>);
                maker_order_mut.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a> = maker_base_quantity;
            };
            <b>if</b> (balance::value(&base_balance_left) == 0) {
                <b>break</b>
            };
        };
        <b>if</b> (linked_table::is_empty(&<a href="../deepbook/clob_v2.md#deepbook_clob_v2_tick_level">tick_level</a>.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_open_orders">open_orders</a>)) {
            (tick_price, _) = previous_leaf(all_open_orders, tick_price);
            <a href="../deepbook/clob_v2.md#deepbook_clob_v2_destroy_empty_level">destroy_empty_level</a>(remove_leaf_by_index(all_open_orders, tick_index));
            (_, tick_index) = find_leaf(all_open_orders, tick_price);
        };
        <b>if</b> (balance::value(&base_balance_left) == 0) {
            <b>break</b>
        };
    };
    <b>if</b> (!vector::is_empty(&canceled_order_events)) {
        event::emit(<a href="../deepbook/clob_v2.md#deepbook_clob_v2_AllOrdersCanceled">AllOrdersCanceled</a>&lt;BaseAsset, QuoteAsset&gt; {
            pool_id,
            orders_canceled: canceled_order_events,
        });
    };
    <b>return</b> (base_balance_left, quote_balance_filled, <b>if</b>(compute_metadata) option::some(<a href="../deepbook/clob_v2.md#deepbook_clob_v2_matched_order_metadata">matched_order_metadata</a>) <b>else</b> option::none())
}
</code></pre>



</details>

<a name="deepbook_clob_v2_place_market_order"></a>

## Function `place_market_order`

Place a market order to the order book.


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_place_market_order">place_market_order</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<b>mut</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">deepbook::clob_v2::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, account_cap: &<a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_AccountCap">deepbook::custodian_v2::AccountCap</a>, client_order_id: u64, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>: u64, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_is_bid">is_bid</a>: bool, base_coin: <a href="../sui/coin.md#sui_coin_Coin">sui::coin::Coin</a>&lt;BaseAsset&gt;, quote_coin: <a href="../sui/coin.md#sui_coin_Coin">sui::coin::Coin</a>&lt;QuoteAsset&gt;, clock: &<a href="../sui/clock.md#sui_clock_Clock">sui::clock::Clock</a>, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): (<a href="../sui/coin.md#sui_coin_Coin">sui::coin::Coin</a>&lt;BaseAsset&gt;, <a href="../sui/coin.md#sui_coin_Coin">sui::coin::Coin</a>&lt;QuoteAsset&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_place_market_order">place_market_order</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool: &<b>mut</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    account_cap: &AccountCap,
    client_order_id: u64,
    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>: u64,
    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_is_bid">is_bid</a>: bool,
    base_coin: Coin&lt;BaseAsset&gt;,
    quote_coin: Coin&lt;QuoteAsset&gt;,
    clock: &Clock,
    ctx: &<b>mut</b> TxContext,
): (Coin&lt;BaseAsset&gt;, Coin&lt;QuoteAsset&gt;) {
    <b>let</b> (base_coin, quote_coin, _metadata) = <a href="../deepbook/clob_v2.md#deepbook_clob_v2_place_market_order_int">place_market_order_int</a>(
        pool,
        account_cap,
        client_order_id,
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>,
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_is_bid">is_bid</a>,
        base_coin,
        quote_coin,
        clock,
        <b>false</b>, // don't <b>return</b> metadata
        ctx
    );
    (base_coin, quote_coin)
}
</code></pre>



</details>

<a name="deepbook_clob_v2_place_market_order_with_metadata"></a>

## Function `place_market_order_with_metadata`



<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_place_market_order_with_metadata">place_market_order_with_metadata</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<b>mut</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">deepbook::clob_v2::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, account_cap: &<a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_AccountCap">deepbook::custodian_v2::AccountCap</a>, client_order_id: u64, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>: u64, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_is_bid">is_bid</a>: bool, base_coin: <a href="../sui/coin.md#sui_coin_Coin">sui::coin::Coin</a>&lt;BaseAsset&gt;, quote_coin: <a href="../sui/coin.md#sui_coin_Coin">sui::coin::Coin</a>&lt;QuoteAsset&gt;, clock: &<a href="../sui/clock.md#sui_clock_Clock">sui::clock::Clock</a>, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): (<a href="../sui/coin.md#sui_coin_Coin">sui::coin::Coin</a>&lt;BaseAsset&gt;, <a href="../sui/coin.md#sui_coin_Coin">sui::coin::Coin</a>&lt;QuoteAsset&gt;, vector&lt;<a href="../deepbook/clob_v2.md#deepbook_clob_v2_MatchedOrderMetadata">deepbook::clob_v2::MatchedOrderMetadata</a>&lt;BaseAsset, QuoteAsset&gt;&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_place_market_order_with_metadata">place_market_order_with_metadata</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool: &<b>mut</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    account_cap: &AccountCap,
    client_order_id: u64,
    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>: u64,
    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_is_bid">is_bid</a>: bool,
    base_coin: Coin&lt;BaseAsset&gt;,
    quote_coin: Coin&lt;QuoteAsset&gt;,
    clock: &Clock,
    ctx: &<b>mut</b> TxContext,
): (Coin&lt;BaseAsset&gt;, Coin&lt;QuoteAsset&gt;, vector&lt;<a href="../deepbook/clob_v2.md#deepbook_clob_v2_MatchedOrderMetadata">MatchedOrderMetadata</a>&lt;BaseAsset, QuoteAsset&gt;&gt;) {
    <b>let</b> (base_coin, quote_coin, <b>mut</b> metadata) = <a href="../deepbook/clob_v2.md#deepbook_clob_v2_place_market_order_int">place_market_order_int</a>(
        pool,
        account_cap,
        client_order_id,
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>,
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_is_bid">is_bid</a>,
        base_coin,
        quote_coin,
        clock,
        <b>true</b>, // <b>return</b> metadata
        ctx
    );
    (base_coin, quote_coin, option::extract(&<b>mut</b> metadata))
}
</code></pre>



</details>

<a name="deepbook_clob_v2_place_market_order_int"></a>

## Function `place_market_order_int`

Place a market order to the order book.


<pre><code><b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_place_market_order_int">place_market_order_int</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<b>mut</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">deepbook::clob_v2::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, account_cap: &<a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_AccountCap">deepbook::custodian_v2::AccountCap</a>, client_order_id: u64, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>: u64, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_is_bid">is_bid</a>: bool, base_coin: <a href="../sui/coin.md#sui_coin_Coin">sui::coin::Coin</a>&lt;BaseAsset&gt;, quote_coin: <a href="../sui/coin.md#sui_coin_Coin">sui::coin::Coin</a>&lt;QuoteAsset&gt;, clock: &<a href="../sui/clock.md#sui_clock_Clock">sui::clock::Clock</a>, compute_metadata: bool, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): (<a href="../sui/coin.md#sui_coin_Coin">sui::coin::Coin</a>&lt;BaseAsset&gt;, <a href="../sui/coin.md#sui_coin_Coin">sui::coin::Coin</a>&lt;QuoteAsset&gt;, <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;vector&lt;<a href="../deepbook/clob_v2.md#deepbook_clob_v2_MatchedOrderMetadata">deepbook::clob_v2::MatchedOrderMetadata</a>&lt;BaseAsset, QuoteAsset&gt;&gt;&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_place_market_order_int">place_market_order_int</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool: &<b>mut</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    account_cap: &AccountCap,
    client_order_id: u64,
    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>: u64,
    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_is_bid">is_bid</a>: bool,
    <b>mut</b> base_coin: Coin&lt;BaseAsset&gt;,
    <b>mut</b> quote_coin: Coin&lt;QuoteAsset&gt;,
    clock: &Clock,
    compute_metadata: bool,
    ctx: &<b>mut</b> TxContext,
): (Coin&lt;BaseAsset&gt;, Coin&lt;QuoteAsset&gt;, Option&lt;vector&lt;<a href="../deepbook/clob_v2.md#deepbook_clob_v2_MatchedOrderMetadata">MatchedOrderMetadata</a>&lt;BaseAsset, QuoteAsset&gt;&gt;&gt;) {
    // If market bid order, match against the open ask orders. Otherwise, match against the open bid orders.
    // Take market bid order <b>for</b> example.
    // We first retrieve the PriceLevel with the lowest price by calling min_leaf on the <a href="../deepbook/clob_v2.md#deepbook_clob_v2_asks">asks</a> Critbit Tree.
    // We then match the market order by iterating through open orders on that price level in ascending order of the order id.
    // Open orders that are being filled are removed from the order book.
    // We stop the iteration until all quantities are filled.
    // If the total <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a> of open orders at the lowest price level is not large enough to fully fill the market order,
    // we <b>move</b> on to the next price level by calling next_leaf on the <a href="../deepbook/clob_v2.md#deepbook_clob_v2_asks">asks</a> Critbit Tree and repeat the same procedure.
    // Continue iterating over the price levels in ascending order until the market order is completely filled.
    // If the market order cannot be completely filled even after consuming all the open ask orders,
    // the unfilled <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a> will be cancelled.
    // Market ask order follows similar procedure.
    // The difference is that market ask order is matched against the open bid orders.
    // We start with the bid PriceLevel with the highest price by calling max_leaf on the <a href="../deepbook/clob_v2.md#deepbook_clob_v2_bids">bids</a> Critbit Tree.
    // The inner <b>loop</b> <b>for</b> iterating over the open orders in ascending orders of order id is the same <b>as</b> above.
    // Then iterate over the price levels in descending order until the market order is completely filled.
    <b>assert</b>!(<a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a> % pool.lot_size == 0, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_EInvalidQuantity">EInvalidQuantity</a>);
    <b>assert</b>!(<a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a> != 0, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_EInvalidQuantity">EInvalidQuantity</a>);
    <b>let</b> metadata;
    <b>if</b> (<a href="../deepbook/clob_v2.md#deepbook_clob_v2_is_bid">is_bid</a>) {
        <b>let</b> (base_balance_filled, quote_balance_left, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_matched_order_metadata">matched_order_metadata</a>) = <a href="../deepbook/clob_v2.md#deepbook_clob_v2_match_bid">match_bid</a>(
            pool,
            account_cap,
            client_order_id,
            <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>,
            <a href="../deepbook/clob_v2.md#deepbook_clob_v2_MAX_PRICE">MAX_PRICE</a>,
            clock::timestamp_ms(clock),
            coin::into_balance(quote_coin),
            compute_metadata
        );
        join(
            &<b>mut</b> base_coin,
            coin::from_balance(base_balance_filled, ctx),
        );
        quote_coin = coin::from_balance(quote_balance_left, ctx);
        metadata = <a href="../deepbook/clob_v2.md#deepbook_clob_v2_matched_order_metadata">matched_order_metadata</a>;
    } <b>else</b> {
        <b>assert</b>!(<a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a> &lt;= coin::value(&base_coin), <a href="../deepbook/clob_v2.md#deepbook_clob_v2_EInsufficientBaseCoin">EInsufficientBaseCoin</a>);
        <b>let</b> base_coin_to_sell = coin::split(&<b>mut</b> base_coin, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>, ctx);
        <b>let</b> (base_balance_left, quote_balance_filled, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_matched_order_metadata">matched_order_metadata</a>) = <a href="../deepbook/clob_v2.md#deepbook_clob_v2_match_ask">match_ask</a>(
            pool,
            account_cap,
            client_order_id,
            <a href="../deepbook/clob_v2.md#deepbook_clob_v2_MIN_PRICE">MIN_PRICE</a>,
            clock::timestamp_ms(clock),
            coin::into_balance(base_coin_to_sell),
            compute_metadata
        );
        join(
            &<b>mut</b> base_coin,
            coin::from_balance(base_balance_left, ctx));
        join(
            &<b>mut</b> quote_coin,
            coin::from_balance(quote_balance_filled, ctx),
        );
        metadata = <a href="../deepbook/clob_v2.md#deepbook_clob_v2_matched_order_metadata">matched_order_metadata</a>;
    };
    (base_coin, quote_coin, metadata)
}
</code></pre>



</details>

<a name="deepbook_clob_v2_inject_limit_order"></a>

## Function `inject_limit_order`

Injects a maker order to the order book.
Returns the order id.


<pre><code><b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_inject_limit_order">inject_limit_order</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<b>mut</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">deepbook::clob_v2::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, client_order_id: u64, price: u64, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_original_quantity">original_quantity</a>: u64, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>: u64, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_is_bid">is_bid</a>: bool, self_matching_prevention: u8, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_expire_timestamp">expire_timestamp</a>: u64, account_cap: &<a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_AccountCap">deepbook::custodian_v2::AccountCap</a>, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_inject_limit_order">inject_limit_order</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool: &<b>mut</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    client_order_id: u64,
    price: u64,
    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_original_quantity">original_quantity</a>: u64,
    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>: u64,
    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_is_bid">is_bid</a>: bool,
    self_matching_prevention: u8,
    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_expire_timestamp">expire_timestamp</a>: u64,
    account_cap: &AccountCap,
    ctx: &<b>mut</b> TxContext
): u64 {
    <b>let</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a> = account_owner(account_cap);
    <b>let</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>: u64;
    <b>let</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_open_orders">open_orders</a>: &<b>mut</b> CritbitTree&lt;<a href="../deepbook/clob_v2.md#deepbook_clob_v2_TickLevel">TickLevel</a>&gt;;
    <b>if</b> (<a href="../deepbook/clob_v2.md#deepbook_clob_v2_is_bid">is_bid</a>) {
        <b>let</b> quote_quantity = clob_math::mul(<a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>, price);
        <a href="../deepbook/custodian.md#deepbook_custodian_lock_balance">custodian::lock_balance</a>&lt;QuoteAsset&gt;(&<b>mut</b> pool.quote_custodian, account_cap, quote_quantity);
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a> = pool.next_bid_order_id;
        pool.next_bid_order_id = pool.next_bid_order_id + 1;
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_open_orders">open_orders</a> = &<b>mut</b> pool.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_bids">bids</a>;
    } <b>else</b> {
        <a href="../deepbook/custodian.md#deepbook_custodian_lock_balance">custodian::lock_balance</a>&lt;BaseAsset&gt;(&<b>mut</b> pool.base_custodian, account_cap, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>);
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a> = pool.next_ask_order_id;
        pool.next_ask_order_id = pool.next_ask_order_id + 1;
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_open_orders">open_orders</a> = &<b>mut</b> pool.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_asks">asks</a>;
    };
    <b>let</b> order = <a href="../deepbook/clob_v2.md#deepbook_clob_v2_Order">Order</a> {
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>,
        client_order_id,
        price,
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_original_quantity">original_quantity</a>,
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>,
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_is_bid">is_bid</a>,
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>,
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_expire_timestamp">expire_timestamp</a>,
        self_matching_prevention
    };
    <b>let</b> (tick_exists, <b>mut</b> tick_index) = find_leaf(<a href="../deepbook/clob_v2.md#deepbook_clob_v2_open_orders">open_orders</a>, price);
    <b>if</b> (!tick_exists) {
        tick_index = insert_leaf(
            <a href="../deepbook/clob_v2.md#deepbook_clob_v2_open_orders">open_orders</a>,
            price,
            <a href="../deepbook/clob_v2.md#deepbook_clob_v2_TickLevel">TickLevel</a> {
                price,
                <a href="../deepbook/clob_v2.md#deepbook_clob_v2_open_orders">open_orders</a>: linked_table::new(ctx),
            });
    };
    <b>let</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_tick_level">tick_level</a> = borrow_mut_leaf_by_index(<a href="../deepbook/clob_v2.md#deepbook_clob_v2_open_orders">open_orders</a>, tick_index);
    linked_table::push_back(&<b>mut</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_tick_level">tick_level</a>.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_open_orders">open_orders</a>, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>, order);
    event::emit(<a href="../deepbook/clob_v2.md#deepbook_clob_v2_OrderPlaced">OrderPlaced</a>&lt;BaseAsset, QuoteAsset&gt; {
        pool_id: *object::uid_as_inner(&pool.id),
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>,
        client_order_id,
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_is_bid">is_bid</a>,
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>,
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_original_quantity">original_quantity</a>,
        base_asset_quantity_placed: <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>,
        price,
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_expire_timestamp">expire_timestamp</a>
    });
    <b>if</b> (!contains(&pool.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_usr_open_orders">usr_open_orders</a>, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>)) {
        add(&<b>mut</b> pool.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_usr_open_orders">usr_open_orders</a>, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>, linked_table::new(ctx));
    };
    linked_table::push_back(borrow_mut(&<b>mut</b> pool.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_usr_open_orders">usr_open_orders</a>, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>), <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>, price);
    <b>return</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>
}
</code></pre>



</details>

<a name="deepbook_clob_v2_place_limit_order"></a>

## Function `place_limit_order`

Place a limit order to the order book.
Returns (base quantity filled, quote quantity filled, whether a maker order is being placed, order id of the maker order).
When the limit order is not successfully placed, we return false to indicate that and also returns a meaningless order_id 0.
When the limit order is successfully placed, we return true to indicate that and also the corresponding order_id.
So please check that boolean value first before using the order id.


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_place_limit_order">place_limit_order</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<b>mut</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">deepbook::clob_v2::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, client_order_id: u64, price: u64, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>: u64, self_matching_prevention: u8, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_is_bid">is_bid</a>: bool, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_expire_timestamp">expire_timestamp</a>: u64, restriction: u8, clock: &<a href="../sui/clock.md#sui_clock_Clock">sui::clock::Clock</a>, account_cap: &<a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_AccountCap">deepbook::custodian_v2::AccountCap</a>, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): (u64, u64, bool, u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_place_limit_order">place_limit_order</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool: &<b>mut</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    client_order_id: u64,
    price: u64,
    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>: u64,
    self_matching_prevention: u8,
    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_is_bid">is_bid</a>: bool,
    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_expire_timestamp">expire_timestamp</a>: u64, // Expiration timestamp in ms in absolute value inclusive.
    restriction: u8,
    clock: &Clock,
    account_cap: &AccountCap,
    ctx: &<b>mut</b> TxContext
): (u64, u64, bool, u64) {
    <b>let</b> (base_quantity_filled, quote_quantity_filled, is_success, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>, _meta_data) = <a href="../deepbook/clob_v2.md#deepbook_clob_v2_place_limit_order_int">place_limit_order_int</a>(
        pool,
        client_order_id,
        price,
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>,
        self_matching_prevention,
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_is_bid">is_bid</a>,
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_expire_timestamp">expire_timestamp</a>, // Expiration timestamp in ms in absolute value inclusive.
        restriction,
        clock,
        account_cap,
        <b>false</b>, // don't compute metadata
        ctx
    );
    (base_quantity_filled, quote_quantity_filled, is_success, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>)
}
</code></pre>



</details>

<a name="deepbook_clob_v2_place_limit_order_with_metadata"></a>

## Function `place_limit_order_with_metadata`

Place a limit order to the order book.
Returns (base quantity filled, quote quantity filled, whether a maker order is being placed, order id of the maker order).
When the limit order is not successfully placed, we return false to indicate that and also returns a meaningless order_id 0.
When the limit order is successfully placed, we return true to indicate that and also the corresponding order_id.
So please check that boolean value first before using the order id.


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_place_limit_order_with_metadata">place_limit_order_with_metadata</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<b>mut</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">deepbook::clob_v2::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, client_order_id: u64, price: u64, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>: u64, self_matching_prevention: u8, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_is_bid">is_bid</a>: bool, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_expire_timestamp">expire_timestamp</a>: u64, restriction: u8, clock: &<a href="../sui/clock.md#sui_clock_Clock">sui::clock::Clock</a>, account_cap: &<a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_AccountCap">deepbook::custodian_v2::AccountCap</a>, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): (u64, u64, bool, u64, vector&lt;<a href="../deepbook/clob_v2.md#deepbook_clob_v2_MatchedOrderMetadata">deepbook::clob_v2::MatchedOrderMetadata</a>&lt;BaseAsset, QuoteAsset&gt;&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_place_limit_order_with_metadata">place_limit_order_with_metadata</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool: &<b>mut</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    client_order_id: u64,
    price: u64,
    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>: u64,
    self_matching_prevention: u8,
    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_is_bid">is_bid</a>: bool,
    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_expire_timestamp">expire_timestamp</a>: u64, // Expiration timestamp in ms in absolute value inclusive.
    restriction: u8,
    clock: &Clock,
    account_cap: &AccountCap,
    ctx: &<b>mut</b> TxContext
): (u64, u64, bool, u64, vector&lt;<a href="../deepbook/clob_v2.md#deepbook_clob_v2_MatchedOrderMetadata">MatchedOrderMetadata</a>&lt;BaseAsset, QuoteAsset&gt;&gt;) {
    <b>let</b> (base_quantity_filled, quote_quantity_filled, is_success, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>, <b>mut</b> meta_data) = <a href="../deepbook/clob_v2.md#deepbook_clob_v2_place_limit_order_int">place_limit_order_int</a>(
        pool,
        client_order_id,
        price,
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>,
        self_matching_prevention,
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_is_bid">is_bid</a>,
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_expire_timestamp">expire_timestamp</a>, // Expiration timestamp in ms in absolute value inclusive.
        restriction,
        clock,
        account_cap,
        <b>true</b>, // <b>return</b> metadata
        ctx
    );
    (base_quantity_filled, quote_quantity_filled, is_success, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>, option::extract(&<b>mut</b> meta_data))
}
</code></pre>



</details>

<a name="deepbook_clob_v2_place_limit_order_int"></a>

## Function `place_limit_order_int`



<pre><code><b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_place_limit_order_int">place_limit_order_int</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<b>mut</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">deepbook::clob_v2::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, client_order_id: u64, price: u64, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>: u64, self_matching_prevention: u8, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_is_bid">is_bid</a>: bool, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_expire_timestamp">expire_timestamp</a>: u64, restriction: u8, clock: &<a href="../sui/clock.md#sui_clock_Clock">sui::clock::Clock</a>, account_cap: &<a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_AccountCap">deepbook::custodian_v2::AccountCap</a>, compute_metadata: bool, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): (u64, u64, bool, u64, <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;vector&lt;<a href="../deepbook/clob_v2.md#deepbook_clob_v2_MatchedOrderMetadata">deepbook::clob_v2::MatchedOrderMetadata</a>&lt;BaseAsset, QuoteAsset&gt;&gt;&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_place_limit_order_int">place_limit_order_int</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool: &<b>mut</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    client_order_id: u64,
    price: u64,
    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>: u64,
    self_matching_prevention: u8,
    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_is_bid">is_bid</a>: bool,
    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_expire_timestamp">expire_timestamp</a>: u64, // Expiration timestamp in ms in absolute value inclusive.
    restriction: u8,
    clock: &Clock,
    account_cap: &AccountCap,
    compute_metadata: bool,
    ctx: &<b>mut</b> TxContext
): (u64, u64, bool, u64, Option&lt;vector&lt;<a href="../deepbook/clob_v2.md#deepbook_clob_v2_MatchedOrderMetadata">MatchedOrderMetadata</a>&lt;BaseAsset, QuoteAsset&gt;&gt;&gt;) {
    // If limit bid order, check whether the price is lower than the lowest ask order by checking the min_leaf of <a href="../deepbook/clob_v2.md#deepbook_clob_v2_asks">asks</a> Critbit Tree.
    // If so, assign the sequence id of the order to be next_bid_order_id and increment next_bid_order_id by 1.
    // Inject the new order to the <a href="../deepbook/clob_v2.md#deepbook_clob_v2_bids">bids</a> Critbit Tree according to the price and order id.
    // Otherwise, find the price level from the <a href="../deepbook/clob_v2.md#deepbook_clob_v2_asks">asks</a> Critbit Tree that is no greater than the input price.
    // Match the bid order against the <a href="../deepbook/clob_v2.md#deepbook_clob_v2_asks">asks</a> Critbit Tree in the same way <b>as</b> a market order but up until the price level found in the previous step.
    // If the bid order is not completely filled, inject the remaining <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a> to the <a href="../deepbook/clob_v2.md#deepbook_clob_v2_bids">bids</a> Critbit Tree according to the input price and order id.
    // If limit ask order, vice versa.
    <b>assert</b>!(self_matching_prevention == <a href="../deepbook/clob_v2.md#deepbook_clob_v2_CANCEL_OLDEST">CANCEL_OLDEST</a>, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_EInvalidSelfMatchingPreventionArg">EInvalidSelfMatchingPreventionArg</a>);
    <b>assert</b>!(<a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a> &gt; 0, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_EInvalidQuantity">EInvalidQuantity</a>);
    <b>assert</b>!(price &gt; 0, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_EInvalidPrice">EInvalidPrice</a>);
    <b>assert</b>!(price % pool.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_tick_size">tick_size</a> == 0, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_EInvalidPrice">EInvalidPrice</a>);
    <b>assert</b>!(<a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a> % pool.lot_size == 0, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_EInvalidQuantity">EInvalidQuantity</a>);
    <b>assert</b>!(<a href="../deepbook/clob_v2.md#deepbook_clob_v2_expire_timestamp">expire_timestamp</a> &gt; clock::timestamp_ms(clock), <a href="../deepbook/clob_v2.md#deepbook_clob_v2_EInvalidExpireTimestamp">EInvalidExpireTimestamp</a>);
    <b>let</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a> = account_owner(account_cap);
    <b>let</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_original_quantity">original_quantity</a> = <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>;
    <b>let</b> base_quantity_filled;
    <b>let</b> quote_quantity_filled;
    <b>let</b> meta_data = <b>if</b> (<a href="../deepbook/clob_v2.md#deepbook_clob_v2_is_bid">is_bid</a>) {
        <b>let</b> quote_quantity_original = <a href="../deepbook/custodian.md#deepbook_custodian_account_available_balance">custodian::account_available_balance</a>&lt;QuoteAsset&gt;(
            &pool.quote_custodian,
            <a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>
        );
        <b>let</b> quote_balance = <a href="../deepbook/custodian.md#deepbook_custodian_decrease_user_available_balance">custodian::decrease_user_available_balance</a>&lt;QuoteAsset&gt;(
            &<b>mut</b> pool.quote_custodian,
            account_cap,
            quote_quantity_original,
        );
        <b>let</b> (base_balance_filled, quote_balance_left, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_matched_order_metadata">matched_order_metadata</a>) = <a href="../deepbook/clob_v2.md#deepbook_clob_v2_match_bid">match_bid</a>(
            pool,
            account_cap,
            client_order_id,
            <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>,
            price,
            clock::timestamp_ms(clock),
            quote_balance,
            compute_metadata
        );
        base_quantity_filled = balance::value(&base_balance_filled);
        quote_quantity_filled = quote_quantity_original - balance::value(&quote_balance_left);
        <a href="../deepbook/custodian.md#deepbook_custodian_increase_user_available_balance">custodian::increase_user_available_balance</a>&lt;BaseAsset&gt;(
            &<b>mut</b> pool.base_custodian,
            <a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>,
            base_balance_filled,
        );
        <a href="../deepbook/custodian.md#deepbook_custodian_increase_user_available_balance">custodian::increase_user_available_balance</a>&lt;QuoteAsset&gt;(
            &<b>mut</b> pool.quote_custodian,
            <a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>,
            quote_balance_left,
        );
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_matched_order_metadata">matched_order_metadata</a>
    } <b>else</b> {
        <b>let</b> base_balance = <a href="../deepbook/custodian.md#deepbook_custodian_decrease_user_available_balance">custodian::decrease_user_available_balance</a>&lt;BaseAsset&gt;(
            &<b>mut</b> pool.base_custodian,
            account_cap,
            <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>,
        );
        <b>let</b> (base_balance_left, quote_balance_filled, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_matched_order_metadata">matched_order_metadata</a>) = <a href="../deepbook/clob_v2.md#deepbook_clob_v2_match_ask">match_ask</a>(
            pool,
            account_cap,
            client_order_id,
            price,
            clock::timestamp_ms(clock),
            base_balance,
            compute_metadata
        );
        base_quantity_filled = <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a> - balance::value(&base_balance_left);
        quote_quantity_filled = balance::value(&quote_balance_filled);
        <a href="../deepbook/custodian.md#deepbook_custodian_increase_user_available_balance">custodian::increase_user_available_balance</a>&lt;BaseAsset&gt;(
            &<b>mut</b> pool.base_custodian,
            <a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>,
            base_balance_left,
        );
        <a href="../deepbook/custodian.md#deepbook_custodian_increase_user_available_balance">custodian::increase_user_available_balance</a>&lt;QuoteAsset&gt;(
            &<b>mut</b> pool.quote_custodian,
            <a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>,
            quote_balance_filled,
        );
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_matched_order_metadata">matched_order_metadata</a>
    };
    <b>let</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>;
    <b>if</b> (restriction == <a href="../deepbook/clob_v2.md#deepbook_clob_v2_IMMEDIATE_OR_CANCEL">IMMEDIATE_OR_CANCEL</a>) {
        <b>return</b> (base_quantity_filled, quote_quantity_filled, <b>false</b>, 0, meta_data)
    };
    <b>if</b> (restriction == <a href="../deepbook/clob_v2.md#deepbook_clob_v2_FILL_OR_KILL">FILL_OR_KILL</a>) {
        <b>assert</b>!(base_quantity_filled == <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_EOrderCannotBeFullyFilled">EOrderCannotBeFullyFilled</a>);
        <b>return</b> (base_quantity_filled, quote_quantity_filled, <b>false</b>, 0, meta_data)
    };
    <b>if</b> (restriction == <a href="../deepbook/clob_v2.md#deepbook_clob_v2_POST_OR_ABORT">POST_OR_ABORT</a>) {
        <b>assert</b>!(base_quantity_filled == 0, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_EOrderCannotBeFullyPassive">EOrderCannotBeFullyPassive</a>);
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a> = <a href="../deepbook/clob_v2.md#deepbook_clob_v2_inject_limit_order">inject_limit_order</a>(
            pool,
            client_order_id,
            price,
            <a href="../deepbook/clob_v2.md#deepbook_clob_v2_original_quantity">original_quantity</a>,
            <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>,
            <a href="../deepbook/clob_v2.md#deepbook_clob_v2_is_bid">is_bid</a>,
            self_matching_prevention,
            <a href="../deepbook/clob_v2.md#deepbook_clob_v2_expire_timestamp">expire_timestamp</a>,
            account_cap,
            ctx
        );
        <b>return</b> (base_quantity_filled, quote_quantity_filled, <b>true</b>, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>, meta_data)
    } <b>else</b> {
        <b>assert</b>!(restriction == <a href="../deepbook/clob_v2.md#deepbook_clob_v2_NO_RESTRICTION">NO_RESTRICTION</a>, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_EInvalidRestriction">EInvalidRestriction</a>);
        <b>if</b> (<a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a> &gt; base_quantity_filled) {
            <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a> = <a href="../deepbook/clob_v2.md#deepbook_clob_v2_inject_limit_order">inject_limit_order</a>(
                pool,
                client_order_id,
                price,
                <a href="../deepbook/clob_v2.md#deepbook_clob_v2_original_quantity">original_quantity</a>,
                <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a> - base_quantity_filled,
                <a href="../deepbook/clob_v2.md#deepbook_clob_v2_is_bid">is_bid</a>,
                self_matching_prevention,
                <a href="../deepbook/clob_v2.md#deepbook_clob_v2_expire_timestamp">expire_timestamp</a>,
                account_cap,
                ctx
            );
            <b>return</b> (base_quantity_filled, quote_quantity_filled, <b>true</b>, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>, meta_data)
        };
        <b>return</b> (base_quantity_filled, quote_quantity_filled, <b>false</b>, 0, meta_data)
    }
}
</code></pre>



</details>

<a name="deepbook_clob_v2_order_is_bid"></a>

## Function `order_is_bid`



<pre><code><b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_is_bid">order_is_bid</a>(<a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>: u64): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_is_bid">order_is_bid</a>(<a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>: u64): bool {
    <b>return</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a> &lt; <a href="../deepbook/clob_v2.md#deepbook_clob_v2_MIN_ASK_ORDER_ID">MIN_ASK_ORDER_ID</a>
}
</code></pre>



</details>

<a name="deepbook_clob_v2_emit_order_canceled"></a>

## Function `emit_order_canceled`



<pre><code><b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_emit_order_canceled">emit_order_canceled</a>&lt;BaseAsset, QuoteAsset&gt;(pool_id: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a>, order: &<a href="../deepbook/clob_v2.md#deepbook_clob_v2_Order">deepbook::clob_v2::Order</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_emit_order_canceled">emit_order_canceled</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool_id: ID,
    order: &<a href="../deepbook/clob_v2.md#deepbook_clob_v2_Order">Order</a>
) {
    event::emit(<a href="../deepbook/clob_v2.md#deepbook_clob_v2_OrderCanceled">OrderCanceled</a>&lt;BaseAsset, QuoteAsset&gt; {
        pool_id,
        client_order_id: order.client_order_id,
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>: order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>,
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_is_bid">is_bid</a>: order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_is_bid">is_bid</a>,
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>: order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>,
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_original_quantity">original_quantity</a>: order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_original_quantity">original_quantity</a>,
        base_asset_quantity_canceled: order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>,
        price: order.price
    })
}
</code></pre>



</details>

<a name="deepbook_clob_v2_emit_order_filled"></a>

## Function `emit_order_filled`



<pre><code><b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_emit_order_filled">emit_order_filled</a>&lt;BaseAsset, QuoteAsset&gt;(pool_id: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a>, taker_client_id: u64, taker_address: <b>address</b>, order: &<a href="../deepbook/clob_v2.md#deepbook_clob_v2_Order">deepbook::clob_v2::Order</a>, base_asset_quantity_filled: u64, taker_commission: u64, maker_rebates: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_emit_order_filled">emit_order_filled</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool_id: ID,
    taker_client_id: u64,
    taker_address: <b>address</b>,
    order: &<a href="../deepbook/clob_v2.md#deepbook_clob_v2_Order">Order</a>,
    base_asset_quantity_filled: u64,
    taker_commission: u64,
    maker_rebates: u64
) {
    event::emit(<a href="../deepbook/clob_v2.md#deepbook_clob_v2_OrderFilled">OrderFilled</a>&lt;BaseAsset, QuoteAsset&gt; {
        pool_id,
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>: order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>,
        taker_client_order_id: taker_client_id,
        taker_address,
        maker_client_order_id: order.client_order_id,
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_is_bid">is_bid</a>: order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_is_bid">is_bid</a>,
        maker_address: order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>,
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_original_quantity">original_quantity</a>: order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_original_quantity">original_quantity</a>,
        base_asset_quantity_filled,
        // order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a> = base_asset_quantity_filled + base_asset_quantity_remaining
        // This guarantees that the subtraction will not underflow
        base_asset_quantity_remaining: order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a> - base_asset_quantity_filled,
        price: order.price,
        taker_commission,
        maker_rebates
    })
}
</code></pre>



</details>

<a name="deepbook_clob_v2_cancel_order"></a>

## Function `cancel_order`

Cancel and opening order.
Abort if order_id is invalid or if the order is not submitted by the transaction sender.


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_cancel_order">cancel_order</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<b>mut</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">deepbook::clob_v2::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>: u64, account_cap: &<a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_AccountCap">deepbook::custodian_v2::AccountCap</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_cancel_order">cancel_order</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool: &<b>mut</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>: u64,
    account_cap: &AccountCap
) {
    // First check the highest bit of the order id to see whether it's bid or ask.
    // Then retrieve the price using the order id.
    // Using the price to retrieve the corresponding PriceLevel from the <a href="../deepbook/clob_v2.md#deepbook_clob_v2_bids">bids</a> / <a href="../deepbook/clob_v2.md#deepbook_clob_v2_asks">asks</a> Critbit Tree.
    // Retrieve and remove the order from open orders of the PriceLevel.
    <b>let</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a> = account_owner(account_cap);
    <b>assert</b>!(contains(&pool.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_usr_open_orders">usr_open_orders</a>, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>), <a href="../deepbook/clob_v2.md#deepbook_clob_v2_EInvalidUser">EInvalidUser</a>);
    <b>let</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_usr_open_orders">usr_open_orders</a> = borrow_mut(&<b>mut</b> pool.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_usr_open_orders">usr_open_orders</a>, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>);
    <b>assert</b>!(linked_table::contains(<a href="../deepbook/clob_v2.md#deepbook_clob_v2_usr_open_orders">usr_open_orders</a>, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>), <a href="../deepbook/clob_v2.md#deepbook_clob_v2_EInvalidOrderId">EInvalidOrderId</a>);
    <b>let</b> tick_price = *linked_table::borrow(<a href="../deepbook/clob_v2.md#deepbook_clob_v2_usr_open_orders">usr_open_orders</a>, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>);
    <b>let</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_is_bid">is_bid</a> = <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_is_bid">order_is_bid</a>(<a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>);
    <b>let</b> (tick_exists, tick_index) = find_leaf(
        <b>if</b> (<a href="../deepbook/clob_v2.md#deepbook_clob_v2_is_bid">is_bid</a>) { &pool.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_bids">bids</a> } <b>else</b> { &pool.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_asks">asks</a> },
        tick_price);
    <b>assert</b>!(tick_exists, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_EInvalidOrderId">EInvalidOrderId</a>);
    <b>let</b> order = <a href="../deepbook/clob_v2.md#deepbook_clob_v2_remove_order">remove_order</a>(
        <b>if</b> (<a href="../deepbook/clob_v2.md#deepbook_clob_v2_is_bid">is_bid</a>) { &<b>mut</b> pool.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_bids">bids</a> } <b>else</b> { &<b>mut</b> pool.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_asks">asks</a> },
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_usr_open_orders">usr_open_orders</a>,
        tick_index,
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>,
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>
    );
    <b>if</b> (<a href="../deepbook/clob_v2.md#deepbook_clob_v2_is_bid">is_bid</a>) {
        <b>let</b> (_, balance_locked) = clob_math::unsafe_mul_round(order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>, order.price);
        <a href="../deepbook/custodian.md#deepbook_custodian_unlock_balance">custodian::unlock_balance</a>(&<b>mut</b> pool.quote_custodian, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>, balance_locked);
    } <b>else</b> {
        <a href="../deepbook/custodian.md#deepbook_custodian_unlock_balance">custodian::unlock_balance</a>(&<b>mut</b> pool.base_custodian, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>, order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>);
    };
    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_emit_order_canceled">emit_order_canceled</a>&lt;BaseAsset, QuoteAsset&gt;(*object::uid_as_inner(&pool.id), &order);
}
</code></pre>



</details>

<a name="deepbook_clob_v2_remove_order"></a>

## Function `remove_order`



<pre><code><b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_remove_order">remove_order</a>(<a href="../deepbook/clob_v2.md#deepbook_clob_v2_open_orders">open_orders</a>: &<b>mut</b> <a href="../deepbook/critbit.md#deepbook_critbit_CritbitTree">deepbook::critbit::CritbitTree</a>&lt;<a href="../deepbook/clob_v2.md#deepbook_clob_v2_TickLevel">deepbook::clob_v2::TickLevel</a>&gt;, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_usr_open_orders">usr_open_orders</a>: &<b>mut</b> <a href="../sui/linked_table.md#sui_linked_table_LinkedTable">sui::linked_table::LinkedTable</a>&lt;u64, u64&gt;, tick_index: u64, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>: u64, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>: <b>address</b>): <a href="../deepbook/clob_v2.md#deepbook_clob_v2_Order">deepbook::clob_v2::Order</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_remove_order">remove_order</a>(
    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_open_orders">open_orders</a>: &<b>mut</b> CritbitTree&lt;<a href="../deepbook/clob_v2.md#deepbook_clob_v2_TickLevel">TickLevel</a>&gt;,
    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_usr_open_orders">usr_open_orders</a>: &<b>mut</b> LinkedTable&lt;u64, u64&gt;,
    tick_index: u64,
    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>: u64,
    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>: <b>address</b>,
): <a href="../deepbook/clob_v2.md#deepbook_clob_v2_Order">Order</a> {
    linked_table::remove(<a href="../deepbook/clob_v2.md#deepbook_clob_v2_usr_open_orders">usr_open_orders</a>, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>);
    <b>let</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_tick_level">tick_level</a> = borrow_leaf_by_index(<a href="../deepbook/clob_v2.md#deepbook_clob_v2_open_orders">open_orders</a>, tick_index);
    <b>assert</b>!(linked_table::contains(&<a href="../deepbook/clob_v2.md#deepbook_clob_v2_tick_level">tick_level</a>.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_open_orders">open_orders</a>, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>), <a href="../deepbook/clob_v2.md#deepbook_clob_v2_EInvalidOrderId">EInvalidOrderId</a>);
    <b>let</b> mut_tick_level = borrow_mut_leaf_by_index(<a href="../deepbook/clob_v2.md#deepbook_clob_v2_open_orders">open_orders</a>, tick_index);
    <b>let</b> order = linked_table::remove(&<b>mut</b> mut_tick_level.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_open_orders">open_orders</a>, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>);
    <b>assert</b>!(order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a> == <a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_EUnauthorizedCancel">EUnauthorizedCancel</a>);
    <b>if</b> (linked_table::is_empty(&mut_tick_level.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_open_orders">open_orders</a>)) {
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_destroy_empty_level">destroy_empty_level</a>(remove_leaf_by_index(<a href="../deepbook/clob_v2.md#deepbook_clob_v2_open_orders">open_orders</a>, tick_index));
    };
    order
}
</code></pre>



</details>

<a name="deepbook_clob_v2_cancel_all_orders"></a>

## Function `cancel_all_orders`



<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_cancel_all_orders">cancel_all_orders</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<b>mut</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">deepbook::clob_v2::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, account_cap: &<a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_AccountCap">deepbook::custodian_v2::AccountCap</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_cancel_all_orders">cancel_all_orders</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool: &<b>mut</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    account_cap: &AccountCap
) {
    <b>let</b> pool_id = *object::uid_as_inner(&pool.id);
    <b>let</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a> = account_owner(account_cap);
    <b>assert</b>!(contains(&pool.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_usr_open_orders">usr_open_orders</a>, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>), <a href="../deepbook/clob_v2.md#deepbook_clob_v2_EInvalidUser">EInvalidUser</a>);
    <b>let</b> usr_open_order_ids = table::borrow_mut(&<b>mut</b> pool.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_usr_open_orders">usr_open_orders</a>, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>);
    <b>let</b> <b>mut</b> canceled_order_events = vector[];
    <b>while</b> (!linked_table::is_empty(usr_open_order_ids)) {
        <b>let</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a> = *option::borrow(linked_table::back(usr_open_order_ids));
        <b>let</b> order_price = *linked_table::borrow(usr_open_order_ids, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>);
        <b>let</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_is_bid">is_bid</a> = <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_is_bid">order_is_bid</a>(<a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>);
        <b>let</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_open_orders">open_orders</a> =
            <b>if</b> (<a href="../deepbook/clob_v2.md#deepbook_clob_v2_is_bid">is_bid</a>) { &<b>mut</b> pool.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_bids">bids</a> }
            <b>else</b> { &<b>mut</b> pool.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_asks">asks</a> };
        <b>let</b> (_, tick_index) = <a href="../deepbook/critbit.md#deepbook_critbit_find_leaf">critbit::find_leaf</a>(<a href="../deepbook/clob_v2.md#deepbook_clob_v2_open_orders">open_orders</a>, order_price);
        <b>let</b> order = <a href="../deepbook/clob_v2.md#deepbook_clob_v2_remove_order">remove_order</a>(
            <a href="../deepbook/clob_v2.md#deepbook_clob_v2_open_orders">open_orders</a>,
            usr_open_order_ids,
            tick_index,
            <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>,
            <a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>
        );
        <b>if</b> (<a href="../deepbook/clob_v2.md#deepbook_clob_v2_is_bid">is_bid</a>) {
            <b>let</b> (_, balance_locked) = clob_math::unsafe_mul_round(order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>, order.price);
            <a href="../deepbook/custodian.md#deepbook_custodian_unlock_balance">custodian::unlock_balance</a>(&<b>mut</b> pool.quote_custodian, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>, balance_locked);
        } <b>else</b> {
            <a href="../deepbook/custodian.md#deepbook_custodian_unlock_balance">custodian::unlock_balance</a>(&<b>mut</b> pool.base_custodian, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>, order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>);
        };
        <b>let</b> canceled_order_event = <a href="../deepbook/clob_v2.md#deepbook_clob_v2_AllOrdersCanceledComponent">AllOrdersCanceledComponent</a>&lt;BaseAsset, QuoteAsset&gt; {
            client_order_id: order.client_order_id,
            <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>: order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>,
            <a href="../deepbook/clob_v2.md#deepbook_clob_v2_is_bid">is_bid</a>: order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_is_bid">is_bid</a>,
            <a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>: order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>,
            <a href="../deepbook/clob_v2.md#deepbook_clob_v2_original_quantity">original_quantity</a>: order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_original_quantity">original_quantity</a>,
            base_asset_quantity_canceled: order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>,
            price: order.price
        };
        vector::push_back(&<b>mut</b> canceled_order_events, canceled_order_event);
    };
    <b>if</b> (!vector::is_empty(&canceled_order_events)) {
        event::emit(<a href="../deepbook/clob_v2.md#deepbook_clob_v2_AllOrdersCanceled">AllOrdersCanceled</a>&lt;BaseAsset, QuoteAsset&gt; {
            pool_id,
            orders_canceled: canceled_order_events,
        });
    };
}
</code></pre>



</details>

<a name="deepbook_clob_v2_batch_cancel_order"></a>

## Function `batch_cancel_order`

Batch cancel limit orders to save gas cost.
Abort if any of the order_ids are not submitted by the sender.
Skip any order_id that is invalid.
Note that this function can reduce gas cost even further if caller has multiple orders at the same price level,
and if orders with the same price are grouped together in the vector.
For example, if we have the following order_id to price mapping, {0: 100., 1: 200., 2: 100., 3: 200.}.
Grouping order_ids like [0, 2, 1, 3] would make it the most gas efficient.


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_batch_cancel_order">batch_cancel_order</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<b>mut</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">deepbook::clob_v2::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, order_ids: vector&lt;u64&gt;, account_cap: &<a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_AccountCap">deepbook::custodian_v2::AccountCap</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_batch_cancel_order">batch_cancel_order</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool: &<b>mut</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    order_ids: vector&lt;u64&gt;,
    account_cap: &AccountCap
) {
    <b>let</b> pool_id = *object::uid_as_inner(&pool.id);
    // First group the order ids according to price level,
    // so that we don't have to retrieve the PriceLevel multiple times <b>if</b> there are orders at the same price level.
    // Iterate over each price level, retrieve the corresponding PriceLevel.
    // Iterate over the order ids that need to be canceled at that price level,
    // retrieve and remove the order from open orders of the PriceLevel.
    <b>let</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a> = account_owner(account_cap);
    <b>assert</b>!(contains(&pool.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_usr_open_orders">usr_open_orders</a>, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>), 0);
    <b>let</b> <b>mut</b> tick_index: u64 = 0;
    <b>let</b> <b>mut</b> tick_price: u64 = 0;
    <b>let</b> n_order = vector::length(&order_ids);
    <b>let</b> <b>mut</b> i_order = 0;
    <b>let</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_usr_open_orders">usr_open_orders</a> = borrow_mut(&<b>mut</b> pool.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_usr_open_orders">usr_open_orders</a>, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>);
    <b>let</b> <b>mut</b> canceled_order_events = vector[];
    <b>while</b> (i_order &lt; n_order) {
        <b>let</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a> = *vector::borrow(&order_ids, i_order);
        <b>assert</b>!(linked_table::contains(<a href="../deepbook/clob_v2.md#deepbook_clob_v2_usr_open_orders">usr_open_orders</a>, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>), <a href="../deepbook/clob_v2.md#deepbook_clob_v2_EInvalidOrderId">EInvalidOrderId</a>);
        <b>let</b> new_tick_price = *linked_table::borrow(<a href="../deepbook/clob_v2.md#deepbook_clob_v2_usr_open_orders">usr_open_orders</a>, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>);
        <b>let</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_is_bid">is_bid</a> = <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_is_bid">order_is_bid</a>(<a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>);
        <b>if</b> (new_tick_price != tick_price) {
            tick_price = new_tick_price;
            <b>let</b> (tick_exists, new_tick_index) = find_leaf(
                <b>if</b> (<a href="../deepbook/clob_v2.md#deepbook_clob_v2_is_bid">is_bid</a>) { &pool.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_bids">bids</a> } <b>else</b> { &pool.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_asks">asks</a> },
                tick_price
            );
            <b>assert</b>!(tick_exists, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_EInvalidTickPrice">EInvalidTickPrice</a>);
            tick_index = new_tick_index;
        };
        <b>let</b> order = <a href="../deepbook/clob_v2.md#deepbook_clob_v2_remove_order">remove_order</a>(
            <b>if</b> (<a href="../deepbook/clob_v2.md#deepbook_clob_v2_is_bid">is_bid</a>) { &<b>mut</b> pool.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_bids">bids</a> } <b>else</b> { &<b>mut</b> pool.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_asks">asks</a> },
            <a href="../deepbook/clob_v2.md#deepbook_clob_v2_usr_open_orders">usr_open_orders</a>,
            tick_index,
            <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>,
            <a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>
        );
        <b>if</b> (<a href="../deepbook/clob_v2.md#deepbook_clob_v2_is_bid">is_bid</a>) {
            <b>let</b> (_is_round_down, balance_locked) = clob_math::unsafe_mul_round(order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>, order.price);
            <a href="../deepbook/custodian.md#deepbook_custodian_unlock_balance">custodian::unlock_balance</a>(&<b>mut</b> pool.quote_custodian, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>, balance_locked);
        } <b>else</b> {
            <a href="../deepbook/custodian.md#deepbook_custodian_unlock_balance">custodian::unlock_balance</a>(&<b>mut</b> pool.base_custodian, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>, order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>);
        };
        <b>let</b> canceled_order_event = <a href="../deepbook/clob_v2.md#deepbook_clob_v2_AllOrdersCanceledComponent">AllOrdersCanceledComponent</a>&lt;BaseAsset, QuoteAsset&gt; {
            client_order_id: order.client_order_id,
            <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>: order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>,
            <a href="../deepbook/clob_v2.md#deepbook_clob_v2_is_bid">is_bid</a>: order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_is_bid">is_bid</a>,
            <a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>: order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>,
            <a href="../deepbook/clob_v2.md#deepbook_clob_v2_original_quantity">original_quantity</a>: order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_original_quantity">original_quantity</a>,
            base_asset_quantity_canceled: order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>,
            price: order.price
        };
        vector::push_back(&<b>mut</b> canceled_order_events, canceled_order_event);
        i_order = i_order + 1;
    };
    <b>if</b> (!vector::is_empty(&canceled_order_events)) {
        event::emit(<a href="../deepbook/clob_v2.md#deepbook_clob_v2_AllOrdersCanceled">AllOrdersCanceled</a>&lt;BaseAsset, QuoteAsset&gt; {
            pool_id,
            orders_canceled: canceled_order_events,
        });
    };
}
</code></pre>



</details>

<a name="deepbook_clob_v2_clean_up_expired_orders"></a>

## Function `clean_up_expired_orders`

Clean up expired orders
Note that this function can reduce gas cost if orders
with the same price are grouped together in the vector because we would not need the computation to find the tick_index.
For example, if we have the following order_id to price mapping, {0: 100., 1: 200., 2: 100., 3: 200.}.
Grouping order_ids like [0, 2, 1, 3] would make it the most gas efficient.
Order owners should be the owner addresses from the account capacities which placed the orders,
and they should correspond to the order IDs one by one.


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_clean_up_expired_orders">clean_up_expired_orders</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<b>mut</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">deepbook::clob_v2::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, clock: &<a href="../sui/clock.md#sui_clock_Clock">sui::clock::Clock</a>, order_ids: vector&lt;u64&gt;, order_owners: vector&lt;<b>address</b>&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_clean_up_expired_orders">clean_up_expired_orders</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool: &<b>mut</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    clock: &Clock,
    order_ids: vector&lt;u64&gt;,
    order_owners: vector&lt;<b>address</b>&gt;
) {
    <b>let</b> pool_id = *object::uid_as_inner(&pool.id);
    <b>let</b> now = clock::timestamp_ms(clock);
    <b>let</b> n_order = vector::length(&order_ids);
    <b>assert</b>!(n_order == vector::length(&order_owners), <a href="../deepbook/clob_v2.md#deepbook_clob_v2_ENotEqual">ENotEqual</a>);
    <b>let</b> <b>mut</b> i_order = 0;
    <b>let</b> <b>mut</b> tick_index: u64 = 0;
    <b>let</b> <b>mut</b> tick_price: u64 = 0;
    <b>let</b> <b>mut</b> canceled_order_events = vector[];
    <b>while</b> (i_order &lt; n_order) {
        <b>let</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a> = *vector::borrow(&order_ids, i_order);
        <b>let</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a> = *vector::borrow(&order_owners, i_order);
        <b>if</b> (!table::contains(&pool.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_usr_open_orders">usr_open_orders</a>, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>)) { <b>continue</b> };
        <b>let</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_usr_open_orders">usr_open_orders</a> = borrow_mut(&<b>mut</b> pool.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_usr_open_orders">usr_open_orders</a>, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>);
        <b>if</b> (!linked_table::contains(<a href="../deepbook/clob_v2.md#deepbook_clob_v2_usr_open_orders">usr_open_orders</a>, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>)) { <b>continue</b> };
        <b>let</b> new_tick_price = *linked_table::borrow(<a href="../deepbook/clob_v2.md#deepbook_clob_v2_usr_open_orders">usr_open_orders</a>, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>);
        <b>let</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_is_bid">is_bid</a> = <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_is_bid">order_is_bid</a>(<a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>);
        <b>let</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_open_orders">open_orders</a> = <b>if</b> (<a href="../deepbook/clob_v2.md#deepbook_clob_v2_is_bid">is_bid</a>) { &<b>mut</b> pool.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_bids">bids</a> } <b>else</b> { &<b>mut</b> pool.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_asks">asks</a> };
        <b>if</b> (new_tick_price != tick_price) {
            tick_price = new_tick_price;
            <b>let</b> (tick_exists, new_tick_index) = find_leaf(
                <a href="../deepbook/clob_v2.md#deepbook_clob_v2_open_orders">open_orders</a>,
                tick_price
            );
            <b>assert</b>!(tick_exists, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_EInvalidTickPrice">EInvalidTickPrice</a>);
            tick_index = new_tick_index;
        };
        <b>let</b> order = <a href="../deepbook/clob_v2.md#deepbook_clob_v2_remove_order">remove_order</a>(<a href="../deepbook/clob_v2.md#deepbook_clob_v2_open_orders">open_orders</a>, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_usr_open_orders">usr_open_orders</a>, tick_index, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>);
        <b>assert</b>!(order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_expire_timestamp">expire_timestamp</a> &lt; now, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_EInvalidExpireTimestamp">EInvalidExpireTimestamp</a>);
        <b>if</b> (<a href="../deepbook/clob_v2.md#deepbook_clob_v2_is_bid">is_bid</a>) {
            <b>let</b> (_is_round_down, balance_locked) = clob_math::unsafe_mul_round(order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>, order.price);
            <a href="../deepbook/custodian.md#deepbook_custodian_unlock_balance">custodian::unlock_balance</a>(&<b>mut</b> pool.quote_custodian, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>, balance_locked);
        } <b>else</b> {
            <a href="../deepbook/custodian.md#deepbook_custodian_unlock_balance">custodian::unlock_balance</a>(&<b>mut</b> pool.base_custodian, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>, order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>);
        };
        <b>let</b> canceled_order_event = <a href="../deepbook/clob_v2.md#deepbook_clob_v2_AllOrdersCanceledComponent">AllOrdersCanceledComponent</a>&lt;BaseAsset, QuoteAsset&gt; {
            client_order_id: order.client_order_id,
            <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>: order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>,
            <a href="../deepbook/clob_v2.md#deepbook_clob_v2_is_bid">is_bid</a>: order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_is_bid">is_bid</a>,
            <a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>: order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>,
            <a href="../deepbook/clob_v2.md#deepbook_clob_v2_original_quantity">original_quantity</a>: order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_original_quantity">original_quantity</a>,
            base_asset_quantity_canceled: order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>,
            price: order.price
        };
        vector::push_back(&<b>mut</b> canceled_order_events, canceled_order_event);
        i_order = i_order + 1;
    };
    <b>if</b> (!vector::is_empty(&canceled_order_events)) {
        event::emit(<a href="../deepbook/clob_v2.md#deepbook_clob_v2_AllOrdersCanceled">AllOrdersCanceled</a>&lt;BaseAsset, QuoteAsset&gt; {
            pool_id,
            orders_canceled: canceled_order_events,
        });
    };
}
</code></pre>



</details>

<a name="deepbook_clob_v2_list_open_orders"></a>

## Function `list_open_orders`



<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_list_open_orders">list_open_orders</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">deepbook::clob_v2::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, account_cap: &<a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_AccountCap">deepbook::custodian_v2::AccountCap</a>): vector&lt;<a href="../deepbook/clob_v2.md#deepbook_clob_v2_Order">deepbook::clob_v2::Order</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_list_open_orders">list_open_orders</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool: &<a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    account_cap: &AccountCap
): vector&lt;<a href="../deepbook/clob_v2.md#deepbook_clob_v2_Order">Order</a>&gt; {
    <b>let</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a> = account_owner(account_cap);
    <b>let</b> <b>mut</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_open_orders">open_orders</a> = vector::empty&lt;<a href="../deepbook/clob_v2.md#deepbook_clob_v2_Order">Order</a>&gt;();
    <b>if</b> (!<a href="../deepbook/clob_v2.md#deepbook_clob_v2_usr_open_orders_exist">usr_open_orders_exist</a>(pool, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>)) {
        <b>return</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_open_orders">open_orders</a>
    };
    <b>let</b> usr_open_order_ids = table::borrow(&pool.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_usr_open_orders">usr_open_orders</a>, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>);
    <b>let</b> <b>mut</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a> = linked_table::front(usr_open_order_ids);
    <b>while</b> (!option::is_none(<a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>)) {
        <b>let</b> order_price = *linked_table::borrow(usr_open_order_ids, *option::borrow(<a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>));
        <b>let</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_tick_level">tick_level</a> =
            <b>if</b> (<a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_is_bid">order_is_bid</a>(*option::borrow(<a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>))) borrow_leaf_by_key(&pool.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_bids">bids</a>, order_price)
            <b>else</b> borrow_leaf_by_key(&pool.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_asks">asks</a>, order_price);
        <b>let</b> order = linked_table::borrow(&<a href="../deepbook/clob_v2.md#deepbook_clob_v2_tick_level">tick_level</a>.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_open_orders">open_orders</a>, *option::borrow(<a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>));
        vector::push_back(&<b>mut</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_open_orders">open_orders</a>, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_Order">Order</a> {
            <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>: order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>,
            client_order_id: order.client_order_id,
            price: order.price,
            <a href="../deepbook/clob_v2.md#deepbook_clob_v2_original_quantity">original_quantity</a>: order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_original_quantity">original_quantity</a>,
            <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>: order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>,
            <a href="../deepbook/clob_v2.md#deepbook_clob_v2_is_bid">is_bid</a>: order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_is_bid">is_bid</a>,
            <a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>: order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>,
            <a href="../deepbook/clob_v2.md#deepbook_clob_v2_expire_timestamp">expire_timestamp</a>: order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_expire_timestamp">expire_timestamp</a>,
            self_matching_prevention: order.self_matching_prevention
        });
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a> = linked_table::next(usr_open_order_ids, *option::borrow(<a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>));
    };
    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_open_orders">open_orders</a>
}
</code></pre>



</details>

<a name="deepbook_clob_v2_account_balance"></a>

## Function `account_balance`

query user balance inside custodian


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_account_balance">account_balance</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">deepbook::clob_v2::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, account_cap: &<a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_AccountCap">deepbook::custodian_v2::AccountCap</a>): (u64, u64, u64, u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_account_balance">account_balance</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool: &<a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    account_cap: &AccountCap
): (u64, u64, u64, u64) {
    <b>let</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a> = account_owner(account_cap);
    <b>let</b> (base_avail, base_locked) = <a href="../deepbook/custodian.md#deepbook_custodian_account_balance">custodian::account_balance</a>(&pool.base_custodian, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>);
    <b>let</b> (quote_avail, quote_locked) = <a href="../deepbook/custodian.md#deepbook_custodian_account_balance">custodian::account_balance</a>(&pool.quote_custodian, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>);
    (base_avail, base_locked, quote_avail, quote_locked)
}
</code></pre>



</details>

<a name="deepbook_clob_v2_get_market_price"></a>

## Function `get_market_price`

Query the market price of order book
returns (best_bid_price, best_ask_price) if there exists
bid/ask order in the order book, otherwise returns None


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_get_market_price">get_market_price</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">deepbook::clob_v2::Pool</a>&lt;BaseAsset, QuoteAsset&gt;): (<a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;u64&gt;, <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;u64&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_get_market_price">get_market_price</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool: &<a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;
): (Option&lt;u64&gt;, Option&lt;u64&gt;){
    <b>let</b> bid_price = <b>if</b> (!<a href="../deepbook/critbit.md#deepbook_critbit_is_empty">critbit::is_empty</a>(&pool.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_bids">bids</a>)) {
        <b>let</b> (result, _) = <a href="../deepbook/critbit.md#deepbook_critbit_max_leaf">critbit::max_leaf</a>(&pool.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_bids">bids</a>);
        option::some&lt;u64&gt;(result)
    } <b>else</b> {
        option::none&lt;u64&gt;()
    };
    <b>let</b> ask_price = <b>if</b> (!<a href="../deepbook/critbit.md#deepbook_critbit_is_empty">critbit::is_empty</a>(&pool.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_asks">asks</a>)) {
        <b>let</b> (result, _) = <a href="../deepbook/critbit.md#deepbook_critbit_min_leaf">critbit::min_leaf</a>(&pool.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_asks">asks</a>);
        option::some&lt;u64&gt;(result)
    } <b>else</b> {
        option::none&lt;u64&gt;()
    };
    <b>return</b> (bid_price, ask_price)
}
</code></pre>



</details>

<a name="deepbook_clob_v2_get_level2_book_status_bid_side"></a>

## Function `get_level2_book_status_bid_side`

Enter a price range and return the level2 order depth of all valid prices within this price range in bid side
returns two vectors of u64
The previous is a list of all valid prices
The latter is the corresponding depth list


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_get_level2_book_status_bid_side">get_level2_book_status_bid_side</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">deepbook::clob_v2::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, price_low: u64, price_high: u64, clock: &<a href="../sui/clock.md#sui_clock_Clock">sui::clock::Clock</a>): (vector&lt;u64&gt;, vector&lt;u64&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_get_level2_book_status_bid_side">get_level2_book_status_bid_side</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool: &<a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    <b>mut</b> price_low: u64,
    <b>mut</b> price_high: u64,
    clock: &Clock
): (vector&lt;u64&gt;, vector&lt;u64&gt;) {
    <b>let</b> <b>mut</b> price_vec = vector::empty&lt;u64&gt;();
    <b>let</b> <b>mut</b> depth_vec = vector::empty&lt;u64&gt;();
    <b>if</b> (<a href="../deepbook/critbit.md#deepbook_critbit_is_empty">critbit::is_empty</a>(&pool.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_bids">bids</a>)) { <b>return</b> (price_vec, depth_vec) };
    <b>let</b> (price_low_, _) = <a href="../deepbook/critbit.md#deepbook_critbit_min_leaf">critbit::min_leaf</a>(&pool.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_bids">bids</a>);
    <b>let</b> (price_high_, _) = <a href="../deepbook/critbit.md#deepbook_critbit_max_leaf">critbit::max_leaf</a>(&pool.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_bids">bids</a>);
    // If price_low is greater than the highest element in the tree, we <b>return</b> empty
    <b>if</b> (price_low &gt; price_high_) {
        <b>return</b> (price_vec, depth_vec)
    };
    <b>if</b> (price_low &lt; price_low_) price_low = price_low_;
    <b>if</b> (price_high &gt; price_high_) price_high = price_high_;
    price_low = <a href="../deepbook/critbit.md#deepbook_critbit_find_closest_key">critbit::find_closest_key</a>(&pool.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_bids">bids</a>, price_low);
    price_high = <a href="../deepbook/critbit.md#deepbook_critbit_find_closest_key">critbit::find_closest_key</a>(&pool.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_bids">bids</a>, price_high);
    <b>while</b> (price_low &lt;= price_high) {
        <b>let</b> depth = <a href="../deepbook/clob_v2.md#deepbook_clob_v2_get_level2_book_status">get_level2_book_status</a>(
            &pool.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_bids">bids</a>,
            price_low,
            clock::timestamp_ms(clock)
        );
        <b>if</b> (depth != 0) {
            vector::push_back(&<b>mut</b> price_vec, price_low);
            vector::push_back(&<b>mut</b> depth_vec, depth);
        };
        <b>let</b> (next_price, _) = <a href="../deepbook/critbit.md#deepbook_critbit_next_leaf">critbit::next_leaf</a>(&pool.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_bids">bids</a>, price_low);
        <b>if</b> (next_price == 0) { <b>break</b> }
        <b>else</b> { price_low = next_price };
    };
    (price_vec, depth_vec)
}
</code></pre>



</details>

<a name="deepbook_clob_v2_get_level2_book_status_ask_side"></a>

## Function `get_level2_book_status_ask_side`

Enter a price range and return the level2 order depth of all valid prices within this price range in ask side
returns two vectors of u64
The previous is a list of all valid prices
The latter is the corresponding depth list


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_get_level2_book_status_ask_side">get_level2_book_status_ask_side</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">deepbook::clob_v2::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, price_low: u64, price_high: u64, clock: &<a href="../sui/clock.md#sui_clock_Clock">sui::clock::Clock</a>): (vector&lt;u64&gt;, vector&lt;u64&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_get_level2_book_status_ask_side">get_level2_book_status_ask_side</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool: &<a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    <b>mut</b> price_low: u64,
    <b>mut</b> price_high: u64,
    clock: &Clock
): (vector&lt;u64&gt;, vector&lt;u64&gt;) {
    <b>let</b> <b>mut</b> price_vec = vector::empty&lt;u64&gt;();
    <b>let</b> <b>mut</b> depth_vec = vector::empty&lt;u64&gt;();
    <b>if</b> (<a href="../deepbook/critbit.md#deepbook_critbit_is_empty">critbit::is_empty</a>(&pool.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_asks">asks</a>)) { <b>return</b> (price_vec, depth_vec) };
    <b>let</b> (price_low_, _) = <a href="../deepbook/critbit.md#deepbook_critbit_min_leaf">critbit::min_leaf</a>(&pool.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_asks">asks</a>);
    // Price_high is less than the lowest leaf in the tree then we <b>return</b> an empty array
    <b>if</b> (price_high &lt; price_low_) {
        <b>return</b> (price_vec, depth_vec)
    };
    <b>if</b> (price_low &lt; price_low_) price_low = price_low_;
    <b>let</b> (price_high_, _) = <a href="../deepbook/critbit.md#deepbook_critbit_max_leaf">critbit::max_leaf</a>(&pool.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_asks">asks</a>);
    <b>if</b> (price_high &gt; price_high_) price_high = price_high_;
    price_low = <a href="../deepbook/critbit.md#deepbook_critbit_find_closest_key">critbit::find_closest_key</a>(&pool.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_asks">asks</a>, price_low);
    price_high = <a href="../deepbook/critbit.md#deepbook_critbit_find_closest_key">critbit::find_closest_key</a>(&pool.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_asks">asks</a>, price_high);
    <b>while</b> (price_low &lt;= price_high) {
        <b>let</b> depth = <a href="../deepbook/clob_v2.md#deepbook_clob_v2_get_level2_book_status">get_level2_book_status</a>(
            &pool.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_asks">asks</a>,
            price_low,
            clock::timestamp_ms(clock)
        );
        <b>if</b> (depth != 0) {
            vector::push_back(&<b>mut</b> price_vec, price_low);
            vector::push_back(&<b>mut</b> depth_vec, depth);
        };
        <b>let</b> (next_price, _) = <a href="../deepbook/critbit.md#deepbook_critbit_next_leaf">critbit::next_leaf</a>(&pool.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_asks">asks</a>, price_low);
        <b>if</b> (next_price == 0) { <b>break</b> }
        <b>else</b> { price_low = next_price };
    };
    (price_vec, depth_vec)
}
</code></pre>



</details>

<a name="deepbook_clob_v2_get_level2_book_status"></a>

## Function `get_level2_book_status`

internal func to retrieve single depth of a tick price


<pre><code><b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_get_level2_book_status">get_level2_book_status</a>(<a href="../deepbook/clob_v2.md#deepbook_clob_v2_open_orders">open_orders</a>: &<a href="../deepbook/critbit.md#deepbook_critbit_CritbitTree">deepbook::critbit::CritbitTree</a>&lt;<a href="../deepbook/clob_v2.md#deepbook_clob_v2_TickLevel">deepbook::clob_v2::TickLevel</a>&gt;, price: u64, time_stamp: u64): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_get_level2_book_status">get_level2_book_status</a>(
    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_open_orders">open_orders</a>: &CritbitTree&lt;<a href="../deepbook/clob_v2.md#deepbook_clob_v2_TickLevel">TickLevel</a>&gt;,
    price: u64,
    time_stamp: u64
): u64 {
    <b>let</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_tick_level">tick_level</a> = <a href="../deepbook/critbit.md#deepbook_critbit_borrow_leaf_by_key">critbit::borrow_leaf_by_key</a>(<a href="../deepbook/clob_v2.md#deepbook_clob_v2_open_orders">open_orders</a>, price);
    <b>let</b> tick_open_orders = &<a href="../deepbook/clob_v2.md#deepbook_clob_v2_tick_level">tick_level</a>.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_open_orders">open_orders</a>;
    <b>let</b> <b>mut</b> depth = 0;
    <b>let</b> <b>mut</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a> = linked_table::front(tick_open_orders);
    <b>let</b> <b>mut</b> order: &<a href="../deepbook/clob_v2.md#deepbook_clob_v2_Order">Order</a>;
    <b>while</b> (!option::is_none(<a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>)) {
        order = linked_table::borrow(tick_open_orders, *option::borrow(<a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>));
        <b>if</b> (order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_expire_timestamp">expire_timestamp</a> &gt; time_stamp) depth = depth + order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>;
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a> = linked_table::next(tick_open_orders, *option::borrow(<a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>));
    };
    depth
}
</code></pre>



</details>

<a name="deepbook_clob_v2_get_order_status"></a>

## Function `get_order_status`



<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_get_order_status">get_order_status</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">deepbook::clob_v2::Pool</a>&lt;BaseAsset, QuoteAsset&gt;, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>: u64, account_cap: &<a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_AccountCap">deepbook::custodian_v2::AccountCap</a>): &<a href="../deepbook/clob_v2.md#deepbook_clob_v2_Order">deepbook::clob_v2::Order</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_get_order_status">get_order_status</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool: &<a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;,
    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>: u64,
    account_cap: &AccountCap
): &<a href="../deepbook/clob_v2.md#deepbook_clob_v2_Order">Order</a> {
    <b>let</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a> = account_owner(account_cap);
    <b>assert</b>!(table::contains(&pool.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_usr_open_orders">usr_open_orders</a>, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>), <a href="../deepbook/clob_v2.md#deepbook_clob_v2_EInvalidUser">EInvalidUser</a>);
    <b>let</b> usr_open_order_ids = table::borrow(&pool.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_usr_open_orders">usr_open_orders</a>, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>);
    <b>assert</b>!(linked_table::contains(usr_open_order_ids, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>), <a href="../deepbook/clob_v2.md#deepbook_clob_v2_EInvalidOrderId">EInvalidOrderId</a>);
    <b>let</b> order_price = *linked_table::borrow(usr_open_order_ids, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>);
    <b>let</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_open_orders">open_orders</a> =
        <b>if</b> (<a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a> &lt; <a href="../deepbook/clob_v2.md#deepbook_clob_v2_MIN_ASK_ORDER_ID">MIN_ASK_ORDER_ID</a>) { &pool.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_bids">bids</a> }
        <b>else</b> { &pool.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_asks">asks</a> };
    <b>let</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_tick_level">tick_level</a> = <a href="../deepbook/critbit.md#deepbook_critbit_borrow_leaf_by_key">critbit::borrow_leaf_by_key</a>(<a href="../deepbook/clob_v2.md#deepbook_clob_v2_open_orders">open_orders</a>, order_price);
    <b>let</b> tick_open_orders = &<a href="../deepbook/clob_v2.md#deepbook_clob_v2_tick_level">tick_level</a>.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_open_orders">open_orders</a>;
    <b>let</b> order = linked_table::borrow(tick_open_orders, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>);
    order
}
</code></pre>



</details>

<a name="deepbook_clob_v2_matched_order_metadata"></a>

## Function `matched_order_metadata`



<pre><code><b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_matched_order_metadata">matched_order_metadata</a>&lt;BaseAsset, QuoteAsset&gt;(pool_id: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a>, taker_address: <b>address</b>, order: &<a href="../deepbook/clob_v2.md#deepbook_clob_v2_Order">deepbook::clob_v2::Order</a>, base_asset_quantity_filled: u64, taker_commission: u64, maker_rebates: u64): <a href="../deepbook/clob_v2.md#deepbook_clob_v2_MatchedOrderMetadata">deepbook::clob_v2::MatchedOrderMetadata</a>&lt;BaseAsset, QuoteAsset&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_matched_order_metadata">matched_order_metadata</a>&lt;BaseAsset, QuoteAsset&gt;(
    pool_id: ID,
    taker_address: <b>address</b>,
    order: &<a href="../deepbook/clob_v2.md#deepbook_clob_v2_Order">Order</a>,
    base_asset_quantity_filled: u64,
    taker_commission: u64,
    maker_rebates: u64
): <a href="../deepbook/clob_v2.md#deepbook_clob_v2_MatchedOrderMetadata">MatchedOrderMetadata</a>&lt;BaseAsset, QuoteAsset&gt;{
    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_MatchedOrderMetadata">MatchedOrderMetadata</a>&lt;BaseAsset, QuoteAsset&gt; {
        pool_id,
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>: order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>,
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_is_bid">is_bid</a>: order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_is_bid">is_bid</a>,
        taker_address,
        maker_address: order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>,
        base_asset_quantity_filled,
        price: order.price,
        taker_commission,
        maker_rebates
    }
}
</code></pre>



</details>

<a name="deepbook_clob_v2_matched_order_metadata_info"></a>

## Function `matched_order_metadata_info`



<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_matched_order_metadata_info">matched_order_metadata_info</a>&lt;BaseAsset, QuoteAsset&gt;(<a href="../deepbook/clob_v2.md#deepbook_clob_v2_matched_order_metadata">matched_order_metadata</a>: &<a href="../deepbook/clob_v2.md#deepbook_clob_v2_MatchedOrderMetadata">deepbook::clob_v2::MatchedOrderMetadata</a>&lt;BaseAsset, QuoteAsset&gt;): (<a href="../sui/object.md#sui_object_ID">sui::object::ID</a>, u64, bool, <b>address</b>, <b>address</b>, u64, u64, u64, u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_matched_order_metadata_info">matched_order_metadata_info</a>&lt;BaseAsset, QuoteAsset&gt;(
    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_matched_order_metadata">matched_order_metadata</a>: &<a href="../deepbook/clob_v2.md#deepbook_clob_v2_MatchedOrderMetadata">MatchedOrderMetadata</a>&lt;BaseAsset, QuoteAsset&gt;
) : ( ID, u64, bool, <b>address</b>, <b>address</b>, u64, u64, u64, u64) {
    (
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_matched_order_metadata">matched_order_metadata</a>.pool_id,
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_matched_order_metadata">matched_order_metadata</a>.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>,
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_matched_order_metadata">matched_order_metadata</a>.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_is_bid">is_bid</a>,
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_matched_order_metadata">matched_order_metadata</a>.taker_address,
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_matched_order_metadata">matched_order_metadata</a>.maker_address,
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_matched_order_metadata">matched_order_metadata</a>.base_asset_quantity_filled,
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_matched_order_metadata">matched_order_metadata</a>.price,
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_matched_order_metadata">matched_order_metadata</a>.taker_commission,
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_matched_order_metadata">matched_order_metadata</a>.maker_rebates
    )
}
</code></pre>



</details>

<a name="deepbook_clob_v2_asks"></a>

## Function `asks`



<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_asks">asks</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">deepbook::clob_v2::Pool</a>&lt;BaseAsset, QuoteAsset&gt;): &<a href="../deepbook/critbit.md#deepbook_critbit_CritbitTree">deepbook::critbit::CritbitTree</a>&lt;<a href="../deepbook/clob_v2.md#deepbook_clob_v2_TickLevel">deepbook::clob_v2::TickLevel</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_asks">asks</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;): &CritbitTree&lt;<a href="../deepbook/clob_v2.md#deepbook_clob_v2_TickLevel">TickLevel</a>&gt; {
    &pool.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_asks">asks</a>
}
</code></pre>



</details>

<a name="deepbook_clob_v2_bids"></a>

## Function `bids`



<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_bids">bids</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">deepbook::clob_v2::Pool</a>&lt;BaseAsset, QuoteAsset&gt;): &<a href="../deepbook/critbit.md#deepbook_critbit_CritbitTree">deepbook::critbit::CritbitTree</a>&lt;<a href="../deepbook/clob_v2.md#deepbook_clob_v2_TickLevel">deepbook::clob_v2::TickLevel</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_bids">bids</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;): &CritbitTree&lt;<a href="../deepbook/clob_v2.md#deepbook_clob_v2_TickLevel">TickLevel</a>&gt; {
    &pool.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_bids">bids</a>
}
</code></pre>



</details>

<a name="deepbook_clob_v2_tick_size"></a>

## Function `tick_size`



<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_tick_size">tick_size</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">deepbook::clob_v2::Pool</a>&lt;BaseAsset, QuoteAsset&gt;): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_tick_size">tick_size</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;): u64 {
    pool.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_tick_size">tick_size</a>
}
</code></pre>



</details>

<a name="deepbook_clob_v2_maker_rebate_rate"></a>

## Function `maker_rebate_rate`



<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_maker_rebate_rate">maker_rebate_rate</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">deepbook::clob_v2::Pool</a>&lt;BaseAsset, QuoteAsset&gt;): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_maker_rebate_rate">maker_rebate_rate</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;): u64 {
    pool.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_maker_rebate_rate">maker_rebate_rate</a>
}
</code></pre>



</details>

<a name="deepbook_clob_v2_taker_fee_rate"></a>

## Function `taker_fee_rate`



<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_taker_fee_rate">taker_fee_rate</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">deepbook::clob_v2::Pool</a>&lt;BaseAsset, QuoteAsset&gt;): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_taker_fee_rate">taker_fee_rate</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;): u64 {
    pool.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_taker_fee_rate">taker_fee_rate</a>
}
</code></pre>



</details>

<a name="deepbook_clob_v2_pool_size"></a>

## Function `pool_size`



<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_pool_size">pool_size</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">deepbook::clob_v2::Pool</a>&lt;BaseAsset, QuoteAsset&gt;): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_pool_size">pool_size</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;): u64 {
    <a href="../deepbook/critbit.md#deepbook_critbit_size">critbit::size</a>(&pool.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_asks">asks</a>) + <a href="../deepbook/critbit.md#deepbook_critbit_size">critbit::size</a>(&pool.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_bids">bids</a>)
}
</code></pre>



</details>

<a name="deepbook_clob_v2_open_orders"></a>

## Function `open_orders`



<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_open_orders">open_orders</a>(<a href="../deepbook/clob_v2.md#deepbook_clob_v2_tick_level">tick_level</a>: &<a href="../deepbook/clob_v2.md#deepbook_clob_v2_TickLevel">deepbook::clob_v2::TickLevel</a>): &<a href="../sui/linked_table.md#sui_linked_table_LinkedTable">sui::linked_table::LinkedTable</a>&lt;u64, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_Order">deepbook::clob_v2::Order</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_open_orders">open_orders</a>(<a href="../deepbook/clob_v2.md#deepbook_clob_v2_tick_level">tick_level</a>: &<a href="../deepbook/clob_v2.md#deepbook_clob_v2_TickLevel">TickLevel</a>): &LinkedTable&lt;u64, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_Order">Order</a>&gt; {
    &<a href="../deepbook/clob_v2.md#deepbook_clob_v2_tick_level">tick_level</a>.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_open_orders">open_orders</a>
}
</code></pre>



</details>

<a name="deepbook_clob_v2_order_id"></a>

## Function `order_id`



<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>(order: &<a href="../deepbook/clob_v2.md#deepbook_clob_v2_Order">deepbook::clob_v2::Order</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>(order: &<a href="../deepbook/clob_v2.md#deepbook_clob_v2_Order">Order</a>): u64 {
    order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>
}
</code></pre>



</details>

<a name="deepbook_clob_v2_tick_level"></a>

## Function `tick_level`



<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_tick_level">tick_level</a>(order: &<a href="../deepbook/clob_v2.md#deepbook_clob_v2_Order">deepbook::clob_v2::Order</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_tick_level">tick_level</a>(order: &<a href="../deepbook/clob_v2.md#deepbook_clob_v2_Order">Order</a>): u64 {
    order.price
}
</code></pre>



</details>

<a name="deepbook_clob_v2_original_quantity"></a>

## Function `original_quantity`



<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_original_quantity">original_quantity</a>(order: &<a href="../deepbook/clob_v2.md#deepbook_clob_v2_Order">deepbook::clob_v2::Order</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_original_quantity">original_quantity</a>(order: &<a href="../deepbook/clob_v2.md#deepbook_clob_v2_Order">Order</a>): u64 {
    order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_original_quantity">original_quantity</a>
}
</code></pre>



</details>

<a name="deepbook_clob_v2_quantity"></a>

## Function `quantity`



<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>(order: &<a href="../deepbook/clob_v2.md#deepbook_clob_v2_Order">deepbook::clob_v2::Order</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>(order: &<a href="../deepbook/clob_v2.md#deepbook_clob_v2_Order">Order</a>): u64 {
    order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>
}
</code></pre>



</details>

<a name="deepbook_clob_v2_is_bid"></a>

## Function `is_bid`



<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_is_bid">is_bid</a>(order: &<a href="../deepbook/clob_v2.md#deepbook_clob_v2_Order">deepbook::clob_v2::Order</a>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_is_bid">is_bid</a>(order: &<a href="../deepbook/clob_v2.md#deepbook_clob_v2_Order">Order</a>): bool {
    order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_is_bid">is_bid</a>
}
</code></pre>



</details>

<a name="deepbook_clob_v2_owner"></a>

## Function `owner`



<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>(order: &<a href="../deepbook/clob_v2.md#deepbook_clob_v2_Order">deepbook::clob_v2::Order</a>): <b>address</b>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>(order: &<a href="../deepbook/clob_v2.md#deepbook_clob_v2_Order">Order</a>): <b>address</b> {
    order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>
}
</code></pre>



</details>

<a name="deepbook_clob_v2_expire_timestamp"></a>

## Function `expire_timestamp`



<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_expire_timestamp">expire_timestamp</a>(order: &<a href="../deepbook/clob_v2.md#deepbook_clob_v2_Order">deepbook::clob_v2::Order</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_expire_timestamp">expire_timestamp</a>(order: &<a href="../deepbook/clob_v2.md#deepbook_clob_v2_Order">Order</a>): u64 {
    order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_expire_timestamp">expire_timestamp</a>
}
</code></pre>



</details>

<a name="deepbook_clob_v2_quote_asset_trading_fees_value"></a>

## Function `quote_asset_trading_fees_value`



<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quote_asset_trading_fees_value">quote_asset_trading_fees_value</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">deepbook::clob_v2::Pool</a>&lt;BaseAsset, QuoteAsset&gt;): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quote_asset_trading_fees_value">quote_asset_trading_fees_value</a>&lt;BaseAsset, QuoteAsset&gt;(pool: &<a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">Pool</a>&lt;BaseAsset, QuoteAsset&gt;): u64 {
    balance::value(&pool.quote_asset_trading_fees)
}
</code></pre>



</details>

<a name="deepbook_clob_v2_clone_order"></a>

## Function `clone_order`



<pre><code><b>public</b>(package) <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_clone_order">clone_order</a>(order: &<a href="../deepbook/clob_v2.md#deepbook_clob_v2_Order">deepbook::clob_v2::Order</a>): <a href="../deepbook/clob_v2.md#deepbook_clob_v2_Order">deepbook::clob_v2::Order</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2_clone_order">clone_order</a>(order: &<a href="../deepbook/clob_v2.md#deepbook_clob_v2_Order">Order</a>): <a href="../deepbook/clob_v2.md#deepbook_clob_v2_Order">Order</a> {
    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_Order">Order</a> {
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>: order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">order_id</a>,
        client_order_id: order.client_order_id,
        price: order.price,
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_original_quantity">original_quantity</a>: order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_original_quantity">original_quantity</a>,
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>: order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_quantity">quantity</a>,
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_is_bid">is_bid</a>: order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_is_bid">is_bid</a>,
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>: order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_owner">owner</a>,
        <a href="../deepbook/clob_v2.md#deepbook_clob_v2_expire_timestamp">expire_timestamp</a>: order.<a href="../deepbook/clob_v2.md#deepbook_clob_v2_expire_timestamp">expire_timestamp</a>,
        self_matching_prevention: order.self_matching_prevention
    }
}
</code></pre>



</details>
