
<a name="0xdee9_order_query"></a>

# Module `0xdee9::order_query`



-  [Struct `OrderPage`](#0xdee9_order_query_OrderPage)
-  [Constants](#@Constants_0)
-  [Function `iter_bids`](#0xdee9_order_query_iter_bids)
-  [Function `iter_asks`](#0xdee9_order_query_iter_asks)
-  [Function `iter_ticks_internal`](#0xdee9_order_query_iter_ticks_internal)
-  [Function `orders`](#0xdee9_order_query_orders)
-  [Function `has_next_page`](#0xdee9_order_query_has_next_page)
-  [Function `next_tick_level`](#0xdee9_order_query_next_tick_level)
-  [Function `next_order_id`](#0xdee9_order_query_next_order_id)
-  [Function `order_id`](#0xdee9_order_query_order_id)
-  [Function `tick_level`](#0xdee9_order_query_tick_level)


<pre><code><b>use</b> <a href="dependencies/move-stdlib/option.md#0x1_option">0x1::option</a>;
<b>use</b> <a href="dependencies/sui-framework/linked_table.md#0x2_linked_table">0x2::linked_table</a>;
<b>use</b> <a href="clob_v2.md#0xdee9_clob_v2">0xdee9::clob_v2</a>;
<b>use</b> <a href="critbit.md#0xdee9_critbit">0xdee9::critbit</a>;
</code></pre>



<a name="0xdee9_order_query_OrderPage"></a>

## Struct `OrderPage`



<pre><code><b>struct</b> <a href="order_query.md#0xdee9_order_query_OrderPage">OrderPage</a> <b>has</b> drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>orders: <a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="clob_v2.md#0xdee9_clob_v2_Order">clob_v2::Order</a>&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>has_next_page: bool</code>
</dt>
<dd>

</dd>
<dt>
<code>next_tick_level: <a href="dependencies/move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;u64&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>next_order_id: <a href="dependencies/move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;u64&gt;</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0xdee9_order_query_PAGE_LIMIT"></a>



<pre><code><b>const</b> <a href="order_query.md#0xdee9_order_query_PAGE_LIMIT">PAGE_LIMIT</a>: u64 = 100;
</code></pre>



<a name="0xdee9_order_query_iter_bids"></a>

## Function `iter_bids`



<pre><code><b>public</b> <b>fun</b> <a href="order_query.md#0xdee9_order_query_iter_bids">iter_bids</a>&lt;T1, T2&gt;(pool: &<a href="clob_v2.md#0xdee9_clob_v2_Pool">clob_v2::Pool</a>&lt;T1, T2&gt;, start_tick_level: <a href="dependencies/move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;u64&gt;, start_order_id: <a href="dependencies/move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;u64&gt;, min_expire_timestamp: <a href="dependencies/move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;u64&gt;, max_id: <a href="dependencies/move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;u64&gt;, ascending: bool): <a href="order_query.md#0xdee9_order_query_OrderPage">order_query::OrderPage</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="order_query.md#0xdee9_order_query_iter_bids">iter_bids</a>&lt;T1, T2&gt;(
    pool: &Pool&lt;T1, T2&gt;,
    // tick level <b>to</b> start from
    start_tick_level: Option&lt;u64&gt;,
    // order id within that tick level <b>to</b> start from
    start_order_id: Option&lt;u64&gt;,
    // <b>if</b> provided, do not <b>include</b> orders <b>with</b> an expire timestamp less than the provided value (expired order),
    // value is in microseconds
    min_expire_timestamp: Option&lt;u64&gt;,
    // do not show orders <b>with</b> an ID larger than max_id--
    // i.e., orders added later than this one
    max_id: Option&lt;u64&gt;,
    // <b>if</b> <b>true</b>, the orders are returned in ascending tick level.
    ascending: bool,
): <a href="order_query.md#0xdee9_order_query_OrderPage">OrderPage</a> {
    <b>let</b> bids = <a href="clob_v2.md#0xdee9_clob_v2_bids">clob_v2::bids</a>(pool);
    <b>let</b> orders = <a href="order_query.md#0xdee9_order_query_iter_ticks_internal">iter_ticks_internal</a>(
        bids,
        start_tick_level,
        start_order_id,
        min_expire_timestamp,
        max_id,
        ascending
    );
    <b>let</b> (orders, has_next_page, next_tick_level, next_order_id) = <b>if</b> (<a href="dependencies/move-stdlib/vector.md#0x1_vector_length">vector::length</a>(&orders) &gt; <a href="order_query.md#0xdee9_order_query_PAGE_LIMIT">PAGE_LIMIT</a>) {
        <b>let</b> last_order = <a href="dependencies/move-stdlib/vector.md#0x1_vector_pop_back">vector::pop_back</a>(&<b>mut</b> orders);
        (orders, <b>true</b>, some(<a href="clob_v2.md#0xdee9_clob_v2_tick_level">clob_v2::tick_level</a>(&last_order)), some(<a href="clob_v2.md#0xdee9_clob_v2_order_id">clob_v2::order_id</a>(&last_order)))
    } <b>else</b> {
        (orders, <b>false</b>, none(), none())
    };

    <a href="order_query.md#0xdee9_order_query_OrderPage">OrderPage</a> {
        orders,
        has_next_page,
        next_tick_level,
        next_order_id
    }
}
</code></pre>



</details>

<a name="0xdee9_order_query_iter_asks"></a>

## Function `iter_asks`



<pre><code><b>public</b> <b>fun</b> <a href="order_query.md#0xdee9_order_query_iter_asks">iter_asks</a>&lt;T1, T2&gt;(pool: &<a href="clob_v2.md#0xdee9_clob_v2_Pool">clob_v2::Pool</a>&lt;T1, T2&gt;, start_tick_level: <a href="dependencies/move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;u64&gt;, start_order_id: <a href="dependencies/move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;u64&gt;, min_expire_timestamp: <a href="dependencies/move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;u64&gt;, max_id: <a href="dependencies/move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;u64&gt;, ascending: bool): <a href="order_query.md#0xdee9_order_query_OrderPage">order_query::OrderPage</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="order_query.md#0xdee9_order_query_iter_asks">iter_asks</a>&lt;T1, T2&gt;(
    pool: &Pool&lt;T1, T2&gt;,
    // tick level <b>to</b> start from
    start_tick_level: Option&lt;u64&gt;,
    // order id within that tick level <b>to</b> start from
    start_order_id: Option&lt;u64&gt;,
    // <b>if</b> provided, do not <b>include</b> orders <b>with</b> an expire timestamp less than the provided value (expired order),
    // value is in microseconds
    min_expire_timestamp: Option&lt;u64&gt;,
    // do not show orders <b>with</b> an ID larger than max_id--
    // i.e., orders added later than this one
    max_id: Option&lt;u64&gt;,
    // <b>if</b> <b>true</b>, the orders are returned in ascending tick level.
    ascending: bool,
): <a href="order_query.md#0xdee9_order_query_OrderPage">OrderPage</a> {
    <b>let</b> asks = <a href="clob_v2.md#0xdee9_clob_v2_asks">clob_v2::asks</a>(pool);
    <b>let</b> orders = <a href="order_query.md#0xdee9_order_query_iter_ticks_internal">iter_ticks_internal</a>(
        asks,
        start_tick_level,
        start_order_id,
        min_expire_timestamp,
        max_id,
        ascending
    );
    <b>let</b> (orders, has_next_page, next_tick_level, next_order_id) = <b>if</b> (<a href="dependencies/move-stdlib/vector.md#0x1_vector_length">vector::length</a>(&orders) &gt; <a href="order_query.md#0xdee9_order_query_PAGE_LIMIT">PAGE_LIMIT</a>) {
        <b>let</b> last_order = <a href="dependencies/move-stdlib/vector.md#0x1_vector_pop_back">vector::pop_back</a>(&<b>mut</b> orders);
        (orders, <b>true</b>, some(<a href="clob_v2.md#0xdee9_clob_v2_tick_level">clob_v2::tick_level</a>(&last_order)), some(<a href="clob_v2.md#0xdee9_clob_v2_order_id">clob_v2::order_id</a>(&last_order)))
    } <b>else</b> {
        (orders, <b>false</b>, none(), none())
    };

    <a href="order_query.md#0xdee9_order_query_OrderPage">OrderPage</a> {
        orders,
        has_next_page,
        next_tick_level,
        next_order_id
    }
}
</code></pre>



</details>

<a name="0xdee9_order_query_iter_ticks_internal"></a>

## Function `iter_ticks_internal`



<pre><code><b>fun</b> <a href="order_query.md#0xdee9_order_query_iter_ticks_internal">iter_ticks_internal</a>(ticks: &<a href="critbit.md#0xdee9_critbit_CritbitTree">critbit::CritbitTree</a>&lt;<a href="clob_v2.md#0xdee9_clob_v2_TickLevel">clob_v2::TickLevel</a>&gt;, start_tick_level: <a href="dependencies/move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;u64&gt;, start_order_id: <a href="dependencies/move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;u64&gt;, min_expire_timestamp: <a href="dependencies/move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;u64&gt;, max_id: <a href="dependencies/move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;u64&gt;, ascending: bool): <a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="clob_v2.md#0xdee9_clob_v2_Order">clob_v2::Order</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="order_query.md#0xdee9_order_query_iter_ticks_internal">iter_ticks_internal</a>(
    ticks: &CritbitTree&lt;TickLevel&gt;,
    // tick level <b>to</b> start from
    start_tick_level: Option&lt;u64&gt;,
    // order id within that tick level <b>to</b> start from
    start_order_id: Option&lt;u64&gt;,
    // <b>if</b> provided, do not <b>include</b> orders <b>with</b> an expire timestamp less than the provided value (expired order),
    // value is in microseconds
    min_expire_timestamp: Option&lt;u64&gt;,
    // do not show orders <b>with</b> an ID larger than max_id--
    // i.e., orders added later than this one
    max_id: Option&lt;u64&gt;,
    // <b>if</b> <b>true</b>, the orders are returned in ascending tick level.
    ascending: bool,
): <a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;Order&gt; {
    <b>let</b> tick_level_key = <b>if</b> (<a href="dependencies/move-stdlib/option.md#0x1_option_is_some">option::is_some</a>(&start_tick_level)) {
        <a href="dependencies/move-stdlib/option.md#0x1_option_destroy_some">option::destroy_some</a>(start_tick_level)
    } <b>else</b> {
        <b>let</b> (key, _) = <b>if</b> (ascending) {
            <a href="critbit.md#0xdee9_critbit_min_leaf">critbit::min_leaf</a>(ticks)
        }<b>else</b> {
            <a href="critbit.md#0xdee9_critbit_max_leaf">critbit::max_leaf</a>(ticks)
        };
        key
    };

    <b>let</b> orders = <a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>[];

    <b>while</b> (tick_level_key != 0 && <a href="dependencies/move-stdlib/vector.md#0x1_vector_length">vector::length</a>(&orders) &lt; <a href="order_query.md#0xdee9_order_query_PAGE_LIMIT">PAGE_LIMIT</a> + 1) {
        <b>let</b> tick_level = <a href="critbit.md#0xdee9_critbit_borrow_leaf_by_key">critbit::borrow_leaf_by_key</a>(ticks, tick_level_key);
        <b>let</b> open_orders = <a href="clob_v2.md#0xdee9_clob_v2_open_orders">clob_v2::open_orders</a>(tick_level);

        <b>let</b> next_order_key = <b>if</b> (<a href="dependencies/move-stdlib/option.md#0x1_option_is_some">option::is_some</a>(&start_order_id)) {
            <b>let</b> key = <a href="dependencies/move-stdlib/option.md#0x1_option_destroy_some">option::destroy_some</a>(start_order_id);
            <b>if</b> (!<a href="dependencies/sui-framework/linked_table.md#0x2_linked_table_contains">linked_table::contains</a>(open_orders, key)) {
                <b>let</b> (next_leaf, _) = <b>if</b> (ascending) {
                    <a href="critbit.md#0xdee9_critbit_next_leaf">critbit::next_leaf</a>(ticks, tick_level_key)
                }<b>else</b> {
                    <a href="critbit.md#0xdee9_critbit_previous_leaf">critbit::previous_leaf</a>(ticks, tick_level_key)
                };
                tick_level_key = next_leaf;
                <b>continue</b>
            };
            start_order_id = <a href="dependencies/move-stdlib/option.md#0x1_option_none">option::none</a>();
            some(key)
        }<b>else</b> {
            *<a href="dependencies/sui-framework/linked_table.md#0x2_linked_table_front">linked_table::front</a>(open_orders)
        };

        <b>while</b> (<a href="dependencies/move-stdlib/option.md#0x1_option_is_some">option::is_some</a>(&next_order_key) && <a href="dependencies/move-stdlib/vector.md#0x1_vector_length">vector::length</a>(&orders) &lt; <a href="order_query.md#0xdee9_order_query_PAGE_LIMIT">PAGE_LIMIT</a> + 1) {
            <b>let</b> key = <a href="dependencies/move-stdlib/option.md#0x1_option_destroy_some">option::destroy_some</a>(next_order_key);
            <b>let</b> order = <a href="dependencies/sui-framework/linked_table.md#0x2_linked_table_borrow">linked_table::borrow</a>(open_orders, key);

            // <b>if</b> the order id is greater than max_id, we end the iteration for this tick level.
            <b>if</b> (<a href="dependencies/move-stdlib/option.md#0x1_option_is_some">option::is_some</a>(&max_id) && key &gt; <a href="dependencies/move-stdlib/option.md#0x1_option_destroy_some">option::destroy_some</a>(max_id)) {
                <b>break</b>
            };

            next_order_key = *<a href="dependencies/sui-framework/linked_table.md#0x2_linked_table_next">linked_table::next</a>(open_orders, key);

            // <b>if</b> expire timestamp is set, and <b>if</b> the order is expired, we skip it.
            <b>if</b> (<a href="dependencies/move-stdlib/option.md#0x1_option_is_none">option::is_none</a>(&min_expire_timestamp) ||
                <a href="clob_v2.md#0xdee9_clob_v2_expire_timestamp">clob_v2::expire_timestamp</a>(order) &gt; <a href="dependencies/move-stdlib/option.md#0x1_option_destroy_some">option::destroy_some</a>(min_expire_timestamp)) {
                <a href="dependencies/move-stdlib/vector.md#0x1_vector_push_back">vector::push_back</a>(&<b>mut</b> orders, <a href="clob_v2.md#0xdee9_clob_v2_clone_order">clob_v2::clone_order</a>(order));
            };
        };
        <b>let</b> (next_leaf, _) = <b>if</b> (ascending) {
            <a href="critbit.md#0xdee9_critbit_next_leaf">critbit::next_leaf</a>(ticks, tick_level_key)
        }<b>else</b> {
            <a href="critbit.md#0xdee9_critbit_previous_leaf">critbit::previous_leaf</a>(ticks, tick_level_key)
        };
        tick_level_key = next_leaf;
    };
    orders
}
</code></pre>



</details>

<a name="0xdee9_order_query_orders"></a>

## Function `orders`



<pre><code><b>public</b> <b>fun</b> <a href="order_query.md#0xdee9_order_query_orders">orders</a>(page: &<a href="order_query.md#0xdee9_order_query_OrderPage">order_query::OrderPage</a>): &<a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="clob_v2.md#0xdee9_clob_v2_Order">clob_v2::Order</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="order_query.md#0xdee9_order_query_orders">orders</a>(page: &<a href="order_query.md#0xdee9_order_query_OrderPage">OrderPage</a>): &<a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;Order&gt; {
    &page.orders
}
</code></pre>



</details>

<a name="0xdee9_order_query_has_next_page"></a>

## Function `has_next_page`



<pre><code><b>public</b> <b>fun</b> <a href="order_query.md#0xdee9_order_query_has_next_page">has_next_page</a>(page: &<a href="order_query.md#0xdee9_order_query_OrderPage">order_query::OrderPage</a>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="order_query.md#0xdee9_order_query_has_next_page">has_next_page</a>(page: &<a href="order_query.md#0xdee9_order_query_OrderPage">OrderPage</a>): bool {
    page.has_next_page
}
</code></pre>



</details>

<a name="0xdee9_order_query_next_tick_level"></a>

## Function `next_tick_level`



<pre><code><b>public</b> <b>fun</b> <a href="order_query.md#0xdee9_order_query_next_tick_level">next_tick_level</a>(page: &<a href="order_query.md#0xdee9_order_query_OrderPage">order_query::OrderPage</a>): <a href="dependencies/move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;u64&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="order_query.md#0xdee9_order_query_next_tick_level">next_tick_level</a>(page: &<a href="order_query.md#0xdee9_order_query_OrderPage">OrderPage</a>): Option&lt;u64&gt; {
    page.next_tick_level
}
</code></pre>



</details>

<a name="0xdee9_order_query_next_order_id"></a>

## Function `next_order_id`



<pre><code><b>public</b> <b>fun</b> <a href="order_query.md#0xdee9_order_query_next_order_id">next_order_id</a>(page: &<a href="order_query.md#0xdee9_order_query_OrderPage">order_query::OrderPage</a>): <a href="dependencies/move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;u64&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="order_query.md#0xdee9_order_query_next_order_id">next_order_id</a>(page: &<a href="order_query.md#0xdee9_order_query_OrderPage">OrderPage</a>): Option&lt;u64&gt; {
    page.next_order_id
}
</code></pre>



</details>

<a name="0xdee9_order_query_order_id"></a>

## Function `order_id`



<pre><code><b>public</b> <b>fun</b> <a href="order_query.md#0xdee9_order_query_order_id">order_id</a>(order: &<a href="clob_v2.md#0xdee9_clob_v2_Order">clob_v2::Order</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="order_query.md#0xdee9_order_query_order_id">order_id</a>(order: &Order): u64 {
    <a href="clob_v2.md#0xdee9_clob_v2_order_id">clob_v2::order_id</a>(order)
}
</code></pre>



</details>

<a name="0xdee9_order_query_tick_level"></a>

## Function `tick_level`



<pre><code><b>public</b> <b>fun</b> <a href="order_query.md#0xdee9_order_query_tick_level">tick_level</a>(order: &<a href="clob_v2.md#0xdee9_clob_v2_Order">clob_v2::Order</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="order_query.md#0xdee9_order_query_tick_level">tick_level</a>(order: &Order): u64 {
    <a href="clob_v2.md#0xdee9_clob_v2_tick_level">clob_v2::tick_level</a>(order)
}
</code></pre>



</details>
