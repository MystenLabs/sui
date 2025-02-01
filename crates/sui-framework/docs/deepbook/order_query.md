---
title: Module `deepbook::order_query`
---



-  [Struct `OrderPage`](#deepbook_order_query_OrderPage)
-  [Constants](#@Constants_0)
-  [Function `iter_bids`](#deepbook_order_query_iter_bids)
-  [Function `iter_asks`](#deepbook_order_query_iter_asks)
-  [Function `iter_ticks_internal`](#deepbook_order_query_iter_ticks_internal)
-  [Function `orders`](#deepbook_order_query_orders)
-  [Function `has_next_page`](#deepbook_order_query_has_next_page)
-  [Function `next_tick_level`](#deepbook_order_query_next_tick_level)
-  [Function `next_order_id`](#deepbook_order_query_next_order_id)
-  [Function `order_id`](#deepbook_order_query_order_id)
-  [Function `tick_level`](#deepbook_order_query_tick_level)


<pre><code><b>use</b> <a href="../deepbook/clob_v2.md#deepbook_clob_v2">deepbook::clob_v2</a>;
<b>use</b> <a href="../deepbook/critbit.md#deepbook_critbit">deepbook::critbit</a>;
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



<a name="deepbook_order_query_OrderPage"></a>

## Struct `OrderPage`



<pre><code><b>public</b> <b>struct</b> <a href="../deepbook/order_query.md#deepbook_order_query_OrderPage">OrderPage</a> <b>has</b> drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code><a href="../deepbook/order_query.md#deepbook_order_query_orders">orders</a>: vector&lt;<a href="../deepbook/clob_v2.md#deepbook_clob_v2_Order">deepbook::clob_v2::Order</a>&gt;</code>
</dt>
<dd>
</dd>
<dt>
<code><a href="../deepbook/order_query.md#deepbook_order_query_has_next_page">has_next_page</a>: bool</code>
</dt>
<dd>
</dd>
<dt>
<code><a href="../deepbook/order_query.md#deepbook_order_query_next_tick_level">next_tick_level</a>: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;u64&gt;</code>
</dt>
<dd>
</dd>
<dt>
<code><a href="../deepbook/order_query.md#deepbook_order_query_next_order_id">next_order_id</a>: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;u64&gt;</code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="deepbook_order_query_PAGE_LIMIT"></a>



<pre><code><b>const</b> <a href="../deepbook/order_query.md#deepbook_order_query_PAGE_LIMIT">PAGE_LIMIT</a>: u64 = 100;
</code></pre>



<a name="deepbook_order_query_iter_bids"></a>

## Function `iter_bids`



<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/order_query.md#deepbook_order_query_iter_bids">iter_bids</a>&lt;T1, T2&gt;(pool: &<a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">deepbook::clob_v2::Pool</a>&lt;T1, T2&gt;, start_tick_level: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;u64&gt;, start_order_id: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;u64&gt;, min_expire_timestamp: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;u64&gt;, max_id: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;u64&gt;, ascending: bool): <a href="../deepbook/order_query.md#deepbook_order_query_OrderPage">deepbook::order_query::OrderPage</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/order_query.md#deepbook_order_query_iter_bids">iter_bids</a>&lt;T1, T2&gt;(
    pool: &Pool&lt;T1, T2&gt;,
    // tick level to start from
    start_tick_level: Option&lt;u64&gt;,
    // order id within that tick level to start from
    start_order_id: Option&lt;u64&gt;,
    // <b>if</b> provided, do not include <a href="../deepbook/order_query.md#deepbook_order_query_orders">orders</a> with an expire timestamp less than the provided value (expired order),
    // value is in microseconds
    min_expire_timestamp: Option&lt;u64&gt;,
    // do not show <a href="../deepbook/order_query.md#deepbook_order_query_orders">orders</a> with an ID larger than max_id--
    // i.e., <a href="../deepbook/order_query.md#deepbook_order_query_orders">orders</a> added later than this one
    max_id: Option&lt;u64&gt;,
    // <b>if</b> <b>true</b>, the <a href="../deepbook/order_query.md#deepbook_order_query_orders">orders</a> are returned in ascending tick level.
    ascending: bool,
): <a href="../deepbook/order_query.md#deepbook_order_query_OrderPage">OrderPage</a> {
    <b>let</b> bids = <a href="../deepbook/clob_v2.md#deepbook_clob_v2_bids">clob_v2::bids</a>(pool);
    <b>let</b> <b>mut</b> <a href="../deepbook/order_query.md#deepbook_order_query_orders">orders</a> = <a href="../deepbook/order_query.md#deepbook_order_query_iter_ticks_internal">iter_ticks_internal</a>(
        bids,
        start_tick_level,
        start_order_id,
        min_expire_timestamp,
        max_id,
        ascending
    );
    <b>let</b> (<a href="../deepbook/order_query.md#deepbook_order_query_orders">orders</a>, <a href="../deepbook/order_query.md#deepbook_order_query_has_next_page">has_next_page</a>, <a href="../deepbook/order_query.md#deepbook_order_query_next_tick_level">next_tick_level</a>, <a href="../deepbook/order_query.md#deepbook_order_query_next_order_id">next_order_id</a>) = <b>if</b> (vector::length(&<a href="../deepbook/order_query.md#deepbook_order_query_orders">orders</a>) &gt; <a href="../deepbook/order_query.md#deepbook_order_query_PAGE_LIMIT">PAGE_LIMIT</a>) {
        <b>let</b> last_order = vector::pop_back(&<b>mut</b> <a href="../deepbook/order_query.md#deepbook_order_query_orders">orders</a>);
        (<a href="../deepbook/order_query.md#deepbook_order_query_orders">orders</a>, <b>true</b>, some(<a href="../deepbook/clob_v2.md#deepbook_clob_v2_tick_level">clob_v2::tick_level</a>(&last_order)), some(<a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">clob_v2::order_id</a>(&last_order)))
    } <b>else</b> {
        (<a href="../deepbook/order_query.md#deepbook_order_query_orders">orders</a>, <b>false</b>, none(), none())
    };
    <a href="../deepbook/order_query.md#deepbook_order_query_OrderPage">OrderPage</a> {
        <a href="../deepbook/order_query.md#deepbook_order_query_orders">orders</a>,
        <a href="../deepbook/order_query.md#deepbook_order_query_has_next_page">has_next_page</a>,
        <a href="../deepbook/order_query.md#deepbook_order_query_next_tick_level">next_tick_level</a>,
        <a href="../deepbook/order_query.md#deepbook_order_query_next_order_id">next_order_id</a>
    }
}
</code></pre>



</details>

<a name="deepbook_order_query_iter_asks"></a>

## Function `iter_asks`



<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/order_query.md#deepbook_order_query_iter_asks">iter_asks</a>&lt;T1, T2&gt;(pool: &<a href="../deepbook/clob_v2.md#deepbook_clob_v2_Pool">deepbook::clob_v2::Pool</a>&lt;T1, T2&gt;, start_tick_level: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;u64&gt;, start_order_id: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;u64&gt;, min_expire_timestamp: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;u64&gt;, max_id: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;u64&gt;, ascending: bool): <a href="../deepbook/order_query.md#deepbook_order_query_OrderPage">deepbook::order_query::OrderPage</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/order_query.md#deepbook_order_query_iter_asks">iter_asks</a>&lt;T1, T2&gt;(
    pool: &Pool&lt;T1, T2&gt;,
    // tick level to start from
    start_tick_level: Option&lt;u64&gt;,
    // order id within that tick level to start from
    start_order_id: Option&lt;u64&gt;,
    // <b>if</b> provided, do not include <a href="../deepbook/order_query.md#deepbook_order_query_orders">orders</a> with an expire timestamp less than the provided value (expired order),
    // value is in microseconds
    min_expire_timestamp: Option&lt;u64&gt;,
    // do not show <a href="../deepbook/order_query.md#deepbook_order_query_orders">orders</a> with an ID larger than max_id--
    // i.e., <a href="../deepbook/order_query.md#deepbook_order_query_orders">orders</a> added later than this one
    max_id: Option&lt;u64&gt;,
    // <b>if</b> <b>true</b>, the <a href="../deepbook/order_query.md#deepbook_order_query_orders">orders</a> are returned in ascending tick level.
    ascending: bool,
): <a href="../deepbook/order_query.md#deepbook_order_query_OrderPage">OrderPage</a> {
    <b>let</b> asks = <a href="../deepbook/clob_v2.md#deepbook_clob_v2_asks">clob_v2::asks</a>(pool);
    <b>let</b> <b>mut</b> <a href="../deepbook/order_query.md#deepbook_order_query_orders">orders</a> = <a href="../deepbook/order_query.md#deepbook_order_query_iter_ticks_internal">iter_ticks_internal</a>(
        asks,
        start_tick_level,
        start_order_id,
        min_expire_timestamp,
        max_id,
        ascending
    );
    <b>let</b> (<a href="../deepbook/order_query.md#deepbook_order_query_orders">orders</a>, <a href="../deepbook/order_query.md#deepbook_order_query_has_next_page">has_next_page</a>, <a href="../deepbook/order_query.md#deepbook_order_query_next_tick_level">next_tick_level</a>, <a href="../deepbook/order_query.md#deepbook_order_query_next_order_id">next_order_id</a>) = <b>if</b> (vector::length(&<a href="../deepbook/order_query.md#deepbook_order_query_orders">orders</a>) &gt; <a href="../deepbook/order_query.md#deepbook_order_query_PAGE_LIMIT">PAGE_LIMIT</a>) {
        <b>let</b> last_order = vector::pop_back(&<b>mut</b> <a href="../deepbook/order_query.md#deepbook_order_query_orders">orders</a>);
        (<a href="../deepbook/order_query.md#deepbook_order_query_orders">orders</a>, <b>true</b>, some(<a href="../deepbook/clob_v2.md#deepbook_clob_v2_tick_level">clob_v2::tick_level</a>(&last_order)), some(<a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">clob_v2::order_id</a>(&last_order)))
    } <b>else</b> {
        (<a href="../deepbook/order_query.md#deepbook_order_query_orders">orders</a>, <b>false</b>, none(), none())
    };
    <a href="../deepbook/order_query.md#deepbook_order_query_OrderPage">OrderPage</a> {
        <a href="../deepbook/order_query.md#deepbook_order_query_orders">orders</a>,
        <a href="../deepbook/order_query.md#deepbook_order_query_has_next_page">has_next_page</a>,
        <a href="../deepbook/order_query.md#deepbook_order_query_next_tick_level">next_tick_level</a>,
        <a href="../deepbook/order_query.md#deepbook_order_query_next_order_id">next_order_id</a>
    }
}
</code></pre>



</details>

<a name="deepbook_order_query_iter_ticks_internal"></a>

## Function `iter_ticks_internal`



<pre><code><b>fun</b> <a href="../deepbook/order_query.md#deepbook_order_query_iter_ticks_internal">iter_ticks_internal</a>(ticks: &<a href="../deepbook/critbit.md#deepbook_critbit_CritbitTree">deepbook::critbit::CritbitTree</a>&lt;<a href="../deepbook/clob_v2.md#deepbook_clob_v2_TickLevel">deepbook::clob_v2::TickLevel</a>&gt;, start_tick_level: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;u64&gt;, start_order_id: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;u64&gt;, min_expire_timestamp: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;u64&gt;, max_id: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;u64&gt;, ascending: bool): vector&lt;<a href="../deepbook/clob_v2.md#deepbook_clob_v2_Order">deepbook::clob_v2::Order</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../deepbook/order_query.md#deepbook_order_query_iter_ticks_internal">iter_ticks_internal</a>(
    ticks: &CritbitTree&lt;TickLevel&gt;,
    // tick level to start from
    start_tick_level: Option&lt;u64&gt;,
    // order id within that tick level to start from
    <b>mut</b> start_order_id: Option&lt;u64&gt;,
    // <b>if</b> provided, do not include <a href="../deepbook/order_query.md#deepbook_order_query_orders">orders</a> with an expire timestamp less than the provided value (expired order),
    // value is in microseconds
    min_expire_timestamp: Option&lt;u64&gt;,
    // do not show <a href="../deepbook/order_query.md#deepbook_order_query_orders">orders</a> with an ID larger than max_id--
    // i.e., <a href="../deepbook/order_query.md#deepbook_order_query_orders">orders</a> added later than this one
    max_id: Option&lt;u64&gt;,
    // <b>if</b> <b>true</b>, the <a href="../deepbook/order_query.md#deepbook_order_query_orders">orders</a> are returned in ascending tick level.
    ascending: bool,
): vector&lt;Order&gt; {
    <b>let</b> <b>mut</b> tick_level_key = <b>if</b> (option::is_some(&start_tick_level)) {
        option::destroy_some(start_tick_level)
    } <b>else</b> {
        <b>let</b> (key, _) = <b>if</b> (ascending) {
            <a href="../deepbook/critbit.md#deepbook_critbit_min_leaf">critbit::min_leaf</a>(ticks)
        }<b>else</b> {
            <a href="../deepbook/critbit.md#deepbook_critbit_max_leaf">critbit::max_leaf</a>(ticks)
        };
        key
    };
    <b>let</b> <b>mut</b> <a href="../deepbook/order_query.md#deepbook_order_query_orders">orders</a> = vector[];
    <b>while</b> (tick_level_key != 0 && vector::length(&<a href="../deepbook/order_query.md#deepbook_order_query_orders">orders</a>) &lt; <a href="../deepbook/order_query.md#deepbook_order_query_PAGE_LIMIT">PAGE_LIMIT</a> + 1) {
        <b>let</b> <a href="../deepbook/order_query.md#deepbook_order_query_tick_level">tick_level</a> = <a href="../deepbook/critbit.md#deepbook_critbit_borrow_leaf_by_key">critbit::borrow_leaf_by_key</a>(ticks, tick_level_key);
        <b>let</b> open_orders = <a href="../deepbook/clob_v2.md#deepbook_clob_v2_open_orders">clob_v2::open_orders</a>(<a href="../deepbook/order_query.md#deepbook_order_query_tick_level">tick_level</a>);
        <b>let</b> <b>mut</b> next_order_key = <b>if</b> (option::is_some(&start_order_id)) {
            <b>let</b> key = option::destroy_some(start_order_id);
            <b>if</b> (!linked_table::contains(open_orders, key)) {
                <b>let</b> (next_leaf, _) = <b>if</b> (ascending) {
                    <a href="../deepbook/critbit.md#deepbook_critbit_next_leaf">critbit::next_leaf</a>(ticks, tick_level_key)
                }<b>else</b> {
                    <a href="../deepbook/critbit.md#deepbook_critbit_previous_leaf">critbit::previous_leaf</a>(ticks, tick_level_key)
                };
                tick_level_key = next_leaf;
                <b>continue</b>
            };
            start_order_id = option::none();
            some(key)
        }<b>else</b> {
            *linked_table::front(open_orders)
        };
        <b>while</b> (option::is_some(&next_order_key) && vector::length(&<a href="../deepbook/order_query.md#deepbook_order_query_orders">orders</a>) &lt; <a href="../deepbook/order_query.md#deepbook_order_query_PAGE_LIMIT">PAGE_LIMIT</a> + 1) {
            <b>let</b> key = option::destroy_some(next_order_key);
            <b>let</b> order = linked_table::borrow(open_orders, key);
            // <b>if</b> the order id is greater than max_id, we end the iteration <b>for</b> this tick level.
            <b>if</b> (option::is_some(&max_id) && key &gt; option::destroy_some(max_id)) {
                <b>break</b>
            };
            next_order_key = *linked_table::next(open_orders, key);
            // <b>if</b> expire timestamp is set, and <b>if</b> the order is expired, we skip it.
            <b>if</b> (option::is_none(&min_expire_timestamp) ||
                <a href="../deepbook/clob_v2.md#deepbook_clob_v2_expire_timestamp">clob_v2::expire_timestamp</a>(order) &gt; option::destroy_some(min_expire_timestamp)) {
                vector::push_back(&<b>mut</b> <a href="../deepbook/order_query.md#deepbook_order_query_orders">orders</a>, <a href="../deepbook/clob_v2.md#deepbook_clob_v2_clone_order">clob_v2::clone_order</a>(order));
            };
        };
        <b>let</b> (next_leaf, _) = <b>if</b> (ascending) {
            <a href="../deepbook/critbit.md#deepbook_critbit_next_leaf">critbit::next_leaf</a>(ticks, tick_level_key)
        }<b>else</b> {
            <a href="../deepbook/critbit.md#deepbook_critbit_previous_leaf">critbit::previous_leaf</a>(ticks, tick_level_key)
        };
        tick_level_key = next_leaf;
    };
    <a href="../deepbook/order_query.md#deepbook_order_query_orders">orders</a>
}
</code></pre>



</details>

<a name="deepbook_order_query_orders"></a>

## Function `orders`



<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/order_query.md#deepbook_order_query_orders">orders</a>(page: &<a href="../deepbook/order_query.md#deepbook_order_query_OrderPage">deepbook::order_query::OrderPage</a>): &vector&lt;<a href="../deepbook/clob_v2.md#deepbook_clob_v2_Order">deepbook::clob_v2::Order</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/order_query.md#deepbook_order_query_orders">orders</a>(page: &<a href="../deepbook/order_query.md#deepbook_order_query_OrderPage">OrderPage</a>): &vector&lt;Order&gt; {
    &page.<a href="../deepbook/order_query.md#deepbook_order_query_orders">orders</a>
}
</code></pre>



</details>

<a name="deepbook_order_query_has_next_page"></a>

## Function `has_next_page`



<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/order_query.md#deepbook_order_query_has_next_page">has_next_page</a>(page: &<a href="../deepbook/order_query.md#deepbook_order_query_OrderPage">deepbook::order_query::OrderPage</a>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/order_query.md#deepbook_order_query_has_next_page">has_next_page</a>(page: &<a href="../deepbook/order_query.md#deepbook_order_query_OrderPage">OrderPage</a>): bool {
    page.<a href="../deepbook/order_query.md#deepbook_order_query_has_next_page">has_next_page</a>
}
</code></pre>



</details>

<a name="deepbook_order_query_next_tick_level"></a>

## Function `next_tick_level`



<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/order_query.md#deepbook_order_query_next_tick_level">next_tick_level</a>(page: &<a href="../deepbook/order_query.md#deepbook_order_query_OrderPage">deepbook::order_query::OrderPage</a>): <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;u64&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/order_query.md#deepbook_order_query_next_tick_level">next_tick_level</a>(page: &<a href="../deepbook/order_query.md#deepbook_order_query_OrderPage">OrderPage</a>): Option&lt;u64&gt; {
    page.<a href="../deepbook/order_query.md#deepbook_order_query_next_tick_level">next_tick_level</a>
}
</code></pre>



</details>

<a name="deepbook_order_query_next_order_id"></a>

## Function `next_order_id`



<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/order_query.md#deepbook_order_query_next_order_id">next_order_id</a>(page: &<a href="../deepbook/order_query.md#deepbook_order_query_OrderPage">deepbook::order_query::OrderPage</a>): <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;u64&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/order_query.md#deepbook_order_query_next_order_id">next_order_id</a>(page: &<a href="../deepbook/order_query.md#deepbook_order_query_OrderPage">OrderPage</a>): Option&lt;u64&gt; {
    page.<a href="../deepbook/order_query.md#deepbook_order_query_next_order_id">next_order_id</a>
}
</code></pre>



</details>

<a name="deepbook_order_query_order_id"></a>

## Function `order_id`



<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/order_query.md#deepbook_order_query_order_id">order_id</a>(order: &<a href="../deepbook/clob_v2.md#deepbook_clob_v2_Order">deepbook::clob_v2::Order</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/order_query.md#deepbook_order_query_order_id">order_id</a>(order: &Order): u64 {
    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_order_id">clob_v2::order_id</a>(order)
}
</code></pre>



</details>

<a name="deepbook_order_query_tick_level"></a>

## Function `tick_level`



<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/order_query.md#deepbook_order_query_tick_level">tick_level</a>(order: &<a href="../deepbook/clob_v2.md#deepbook_clob_v2_Order">deepbook::clob_v2::Order</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/order_query.md#deepbook_order_query_tick_level">tick_level</a>(order: &Order): u64 {
    <a href="../deepbook/clob_v2.md#deepbook_clob_v2_tick_level">clob_v2::tick_level</a>(order)
}
</code></pre>



</details>
