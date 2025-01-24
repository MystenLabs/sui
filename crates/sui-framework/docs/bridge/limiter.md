---
title: Module `bridge::limiter`
---



-  [Struct `TransferLimiter`](#bridge_limiter_TransferLimiter)
-  [Struct `TransferRecord`](#bridge_limiter_TransferRecord)
-  [Struct `UpdateRouteLimitEvent`](#bridge_limiter_UpdateRouteLimitEvent)
-  [Constants](#@Constants_0)
-  [Function `get_route_limit`](#bridge_limiter_get_route_limit)
-  [Function `new`](#bridge_limiter_new)
-  [Function `check_and_record_sending_transfer`](#bridge_limiter_check_and_record_sending_transfer)
-  [Function `update_route_limit`](#bridge_limiter_update_route_limit)
-  [Function `current_hour_since_epoch`](#bridge_limiter_current_hour_since_epoch)
-  [Function `adjust_transfer_records`](#bridge_limiter_adjust_transfer_records)
-  [Function `initial_transfer_limits`](#bridge_limiter_initial_transfer_limits)


<pre><code><b>use</b> <a href="../bridge/chain_ids.md#bridge_chain_ids">bridge::chain_ids</a>;
<b>use</b> <a href="../bridge/treasury.md#bridge_treasury">bridge::treasury</a>;
<b>use</b> <a href="../std/address.md#std_address">std::address</a>;
<b>use</b> <a href="../std/ascii.md#std_ascii">std::ascii</a>;
<b>use</b> <a href="../std/bcs.md#std_bcs">std::bcs</a>;
<b>use</b> <a href="../std/option.md#std_option">std::option</a>;
<b>use</b> <a href="../std/string.md#std_string">std::string</a>;
<b>use</b> <a href="../std/type_name.md#std_type_name">std::type_name</a>;
<b>use</b> <a href="../std/u64.md#std_u64">std::u64</a>;
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
<b>use</b> <a href="../sui/object.md#sui_object">sui::object</a>;
<b>use</b> <a href="../sui/object_bag.md#sui_object_bag">sui::object_bag</a>;
<b>use</b> <a href="../sui/package.md#sui_package">sui::package</a>;
<b>use</b> <a href="../sui/table.md#sui_table">sui::table</a>;
<b>use</b> <a href="../sui/transfer.md#sui_transfer">sui::transfer</a>;
<b>use</b> <a href="../sui/tx_context.md#sui_tx_context">sui::tx_context</a>;
<b>use</b> <a href="../sui/types.md#sui_types">sui::types</a>;
<b>use</b> <a href="../sui/url.md#sui_url">sui::url</a>;
<b>use</b> <a href="../sui/vec_map.md#sui_vec_map">sui::vec_map</a>;
<b>use</b> <a href="../sui/vec_set.md#sui_vec_set">sui::vec_set</a>;
</code></pre>



<a name="bridge_limiter_TransferLimiter"></a>

## Struct `TransferLimiter`



<pre><code><b>public</b> <b>struct</b> <a href="../bridge/limiter.md#bridge_limiter_TransferLimiter">TransferLimiter</a> <b>has</b> store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>transfer_limits: <a href="../sui/vec_map.md#sui_vec_map_VecMap">sui::vec_map::VecMap</a>&lt;<a href="../bridge/chain_ids.md#bridge_chain_ids_BridgeRoute">bridge::chain_ids::BridgeRoute</a>, u64&gt;</code>
</dt>
<dd>
</dd>
<dt>
<code>transfer_records: <a href="../sui/vec_map.md#sui_vec_map_VecMap">sui::vec_map::VecMap</a>&lt;<a href="../bridge/chain_ids.md#bridge_chain_ids_BridgeRoute">bridge::chain_ids::BridgeRoute</a>, <a href="../bridge/limiter.md#bridge_limiter_TransferRecord">bridge::limiter::TransferRecord</a>&gt;</code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="bridge_limiter_TransferRecord"></a>

## Struct `TransferRecord`



<pre><code><b>public</b> <b>struct</b> <a href="../bridge/limiter.md#bridge_limiter_TransferRecord">TransferRecord</a> <b>has</b> store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>hour_head: u64</code>
</dt>
<dd>
</dd>
<dt>
<code>hour_tail: u64</code>
</dt>
<dd>
</dd>
<dt>
<code>per_hour_amounts: vector&lt;u64&gt;</code>
</dt>
<dd>
</dd>
<dt>
<code>total_amount: u64</code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="bridge_limiter_UpdateRouteLimitEvent"></a>

## Struct `UpdateRouteLimitEvent`



<pre><code><b>public</b> <b>struct</b> <a href="../bridge/limiter.md#bridge_limiter_UpdateRouteLimitEvent">UpdateRouteLimitEvent</a> <b>has</b> <b>copy</b>, drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>sending_chain: u8</code>
</dt>
<dd>
</dd>
<dt>
<code>receiving_chain: u8</code>
</dt>
<dd>
</dd>
<dt>
<code>new_limit: u64</code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="bridge_limiter_ELimitNotFoundForRoute"></a>



<pre><code><b>const</b> <a href="../bridge/limiter.md#bridge_limiter_ELimitNotFoundForRoute">ELimitNotFoundForRoute</a>: u64 = 0;
</code></pre>



<a name="bridge_limiter_MAX_TRANSFER_LIMIT"></a>



<pre><code><b>const</b> <a href="../bridge/limiter.md#bridge_limiter_MAX_TRANSFER_LIMIT">MAX_TRANSFER_LIMIT</a>: u64 = 18446744073709551615;
</code></pre>



<a name="bridge_limiter_USD_VALUE_MULTIPLIER"></a>



<pre><code><b>const</b> <a href="../bridge/limiter.md#bridge_limiter_USD_VALUE_MULTIPLIER">USD_VALUE_MULTIPLIER</a>: u64 = 100000000;
</code></pre>



<a name="bridge_limiter_get_route_limit"></a>

## Function `get_route_limit`



<pre><code><b>public</b> <b>fun</b> <a href="../bridge/limiter.md#bridge_limiter_get_route_limit">get_route_limit</a>(self: &<a href="../bridge/limiter.md#bridge_limiter_TransferLimiter">bridge::limiter::TransferLimiter</a>, route: &<a href="../bridge/chain_ids.md#bridge_chain_ids_BridgeRoute">bridge::chain_ids::BridgeRoute</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../bridge/limiter.md#bridge_limiter_get_route_limit">get_route_limit</a>(self: &<a href="../bridge/limiter.md#bridge_limiter_TransferLimiter">TransferLimiter</a>, route: &BridgeRoute): u64 {
    self.transfer_limits[route]
}
</code></pre>



</details>

<a name="bridge_limiter_new"></a>

## Function `new`



<pre><code><b>public</b>(package) <b>fun</b> <a href="../bridge/limiter.md#bridge_limiter_new">new</a>(): <a href="../bridge/limiter.md#bridge_limiter_TransferLimiter">bridge::limiter::TransferLimiter</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../bridge/limiter.md#bridge_limiter_new">new</a>(): <a href="../bridge/limiter.md#bridge_limiter_TransferLimiter">TransferLimiter</a> {
    // hardcoded limit <b>for</b> <a href="../bridge/bridge.md#bridge_bridge">bridge</a> genesis
    <a href="../bridge/limiter.md#bridge_limiter_TransferLimiter">TransferLimiter</a> {
        transfer_limits: <a href="../bridge/limiter.md#bridge_limiter_initial_transfer_limits">initial_transfer_limits</a>(),
        transfer_records: vec_map::empty()
    }
}
</code></pre>



</details>

<a name="bridge_limiter_check_and_record_sending_transfer"></a>

## Function `check_and_record_sending_transfer`



<pre><code><b>public</b>(package) <b>fun</b> <a href="../bridge/limiter.md#bridge_limiter_check_and_record_sending_transfer">check_and_record_sending_transfer</a>&lt;T&gt;(self: &<b>mut</b> <a href="../bridge/limiter.md#bridge_limiter_TransferLimiter">bridge::limiter::TransferLimiter</a>, <a href="../bridge/treasury.md#bridge_treasury">treasury</a>: &<a href="../bridge/treasury.md#bridge_treasury_BridgeTreasury">bridge::treasury::BridgeTreasury</a>, clock: &<a href="../sui/clock.md#sui_clock_Clock">sui::clock::Clock</a>, route: <a href="../bridge/chain_ids.md#bridge_chain_ids_BridgeRoute">bridge::chain_ids::BridgeRoute</a>, amount: u64): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../bridge/limiter.md#bridge_limiter_check_and_record_sending_transfer">check_and_record_sending_transfer</a>&lt;T&gt;(
    self: &<b>mut</b> <a href="../bridge/limiter.md#bridge_limiter_TransferLimiter">TransferLimiter</a>,
    <a href="../bridge/treasury.md#bridge_treasury">treasury</a>: &BridgeTreasury,
    clock: &Clock,
    route: BridgeRoute,
    amount: u64
): bool {
    // Create record <b>for</b> route <b>if</b> not exists
    <b>if</b> (!self.transfer_records.contains(&route)) {
        self.transfer_records.insert(route, <a href="../bridge/limiter.md#bridge_limiter_TransferRecord">TransferRecord</a> {
            hour_head: 0,
            hour_tail: 0,
            per_hour_amounts: vector[],
            total_amount: 0
        })
    };
    <b>let</b> record = self.transfer_records.get_mut(&route);
    <b>let</b> <a href="../bridge/limiter.md#bridge_limiter_current_hour_since_epoch">current_hour_since_epoch</a> = <a href="../bridge/limiter.md#bridge_limiter_current_hour_since_epoch">current_hour_since_epoch</a>(clock);
    record.<a href="../bridge/limiter.md#bridge_limiter_adjust_transfer_records">adjust_transfer_records</a>(<a href="../bridge/limiter.md#bridge_limiter_current_hour_since_epoch">current_hour_since_epoch</a>);
    // Get limit <b>for</b> the route
    <b>let</b> route_limit = self.transfer_limits.try_get(&route);
    <b>assert</b>!(route_limit.is_some(), <a href="../bridge/limiter.md#bridge_limiter_ELimitNotFoundForRoute">ELimitNotFoundForRoute</a>);
    <b>let</b> route_limit = route_limit.destroy_some();
    <b>let</b> route_limit_adjusted =
        (route_limit <b>as</b> u128) * (<a href="../bridge/treasury.md#bridge_treasury">treasury</a>.decimal_multiplier&lt;T&gt;() <b>as</b> u128);
    // Compute notional amount
    // Upcast to u128 to prevent overflow, to not miss out on small amounts.
    <b>let</b> value = (<a href="../bridge/treasury.md#bridge_treasury">treasury</a>.notional_value&lt;T&gt;() <b>as</b> u128);
    <b>let</b> notional_amount_with_token_multiplier = value * (amount <b>as</b> u128);
    // Check <b>if</b> transfer amount exceed limit
    // Upscale them to the token's decimal.
    <b>if</b> ((record.total_amount <b>as</b> u128)
        * (<a href="../bridge/treasury.md#bridge_treasury">treasury</a>.decimal_multiplier&lt;T&gt;() <b>as</b> u128)
        + notional_amount_with_token_multiplier &gt; route_limit_adjusted
    ) {
        <b>return</b> <b>false</b>
    };
    // Now scale down to notional value
    <b>let</b> notional_amount = notional_amount_with_token_multiplier
        / (<a href="../bridge/treasury.md#bridge_treasury">treasury</a>.decimal_multiplier&lt;T&gt;() <b>as</b> u128);
    // Should be safe to downcast to u64 after dividing by the decimals
    <b>let</b> notional_amount = (notional_amount <b>as</b> u64);
    // Record transfer value
    <b>let</b> new_amount = record.per_hour_amounts.pop_back() + notional_amount;
    record.per_hour_amounts.push_back(new_amount);
    record.total_amount = record.total_amount + notional_amount;
    <b>true</b>
}
</code></pre>



</details>

<a name="bridge_limiter_update_route_limit"></a>

## Function `update_route_limit`



<pre><code><b>public</b>(package) <b>fun</b> <a href="../bridge/limiter.md#bridge_limiter_update_route_limit">update_route_limit</a>(self: &<b>mut</b> <a href="../bridge/limiter.md#bridge_limiter_TransferLimiter">bridge::limiter::TransferLimiter</a>, route: &<a href="../bridge/chain_ids.md#bridge_chain_ids_BridgeRoute">bridge::chain_ids::BridgeRoute</a>, new_usd_limit: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../bridge/limiter.md#bridge_limiter_update_route_limit">update_route_limit</a>(
    self: &<b>mut</b> <a href="../bridge/limiter.md#bridge_limiter_TransferLimiter">TransferLimiter</a>,
    route: &BridgeRoute,
    new_usd_limit: u64
) {
    <b>let</b> receiving_chain = *route.destination();
    <b>if</b> (!self.transfer_limits.contains(route)) {
        self.transfer_limits.insert(*route, new_usd_limit);
    } <b>else</b> {
        *&<b>mut</b> self.transfer_limits[route] = new_usd_limit;
    };
    emit(<a href="../bridge/limiter.md#bridge_limiter_UpdateRouteLimitEvent">UpdateRouteLimitEvent</a> {
        sending_chain: *route.source(),
        receiving_chain,
        new_limit: new_usd_limit,
    })
}
</code></pre>



</details>

<a name="bridge_limiter_current_hour_since_epoch"></a>

## Function `current_hour_since_epoch`



<pre><code><b>fun</b> <a href="../bridge/limiter.md#bridge_limiter_current_hour_since_epoch">current_hour_since_epoch</a>(clock: &<a href="../sui/clock.md#sui_clock_Clock">sui::clock::Clock</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../bridge/limiter.md#bridge_limiter_current_hour_since_epoch">current_hour_since_epoch</a>(clock: &Clock): u64 {
    clock::timestamp_ms(clock) / 3600000
}
</code></pre>



</details>

<a name="bridge_limiter_adjust_transfer_records"></a>

## Function `adjust_transfer_records`



<pre><code><b>fun</b> <a href="../bridge/limiter.md#bridge_limiter_adjust_transfer_records">adjust_transfer_records</a>(self: &<b>mut</b> <a href="../bridge/limiter.md#bridge_limiter_TransferRecord">bridge::limiter::TransferRecord</a>, <a href="../bridge/limiter.md#bridge_limiter_current_hour_since_epoch">current_hour_since_epoch</a>: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../bridge/limiter.md#bridge_limiter_adjust_transfer_records">adjust_transfer_records</a>(self: &<b>mut</b> <a href="../bridge/limiter.md#bridge_limiter_TransferRecord">TransferRecord</a>, <a href="../bridge/limiter.md#bridge_limiter_current_hour_since_epoch">current_hour_since_epoch</a>: u64) {
    <b>if</b> (self.hour_head == <a href="../bridge/limiter.md#bridge_limiter_current_hour_since_epoch">current_hour_since_epoch</a>) {
        <b>return</b> // nothing to backfill
    };
    <b>let</b> target_tail = <a href="../bridge/limiter.md#bridge_limiter_current_hour_since_epoch">current_hour_since_epoch</a> - 23;
    // If `hour_head` is even older than 24 hours ago, it means all items in
    // `per_hour_amounts` are to be evicted.
    <b>if</b> (self.hour_head &lt; target_tail) {
        self.per_hour_amounts = vector[];
        self.total_amount = 0;
        self.hour_tail = target_tail;
        self.hour_head = target_tail;
        // Don't forget to insert this hour's record
        self.per_hour_amounts.push_back(0);
    } <b>else</b> {
        // self.hour_head is within 24 hour range.
        // some items in `per_hour_amounts` are still valid, we remove stale hours.
        <b>while</b> (self.hour_tail &lt; target_tail) {
            self.total_amount = self.total_amount - self.per_hour_amounts.remove(0);
            self.hour_tail = self.hour_tail + 1;
        }
    };
    // Backfill from hour_head to current hour
    <b>while</b> (self.hour_head &lt; <a href="../bridge/limiter.md#bridge_limiter_current_hour_since_epoch">current_hour_since_epoch</a>) {
        self.per_hour_amounts.push_back(0);
        self.hour_head = self.hour_head + 1;
    }
}
</code></pre>



</details>

<a name="bridge_limiter_initial_transfer_limits"></a>

## Function `initial_transfer_limits`



<pre><code><b>fun</b> <a href="../bridge/limiter.md#bridge_limiter_initial_transfer_limits">initial_transfer_limits</a>(): <a href="../sui/vec_map.md#sui_vec_map_VecMap">sui::vec_map::VecMap</a>&lt;<a href="../bridge/chain_ids.md#bridge_chain_ids_BridgeRoute">bridge::chain_ids::BridgeRoute</a>, u64&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../bridge/limiter.md#bridge_limiter_initial_transfer_limits">initial_transfer_limits</a>(): VecMap&lt;BridgeRoute, u64&gt; {
    <b>let</b> <b>mut</b> transfer_limits = vec_map::empty();
    // 5M limit on Sui -&gt; Ethereum mainnet
    transfer_limits.insert(
        <a href="../bridge/chain_ids.md#bridge_chain_ids_get_route">chain_ids::get_route</a>(<a href="../bridge/chain_ids.md#bridge_chain_ids_eth_mainnet">chain_ids::eth_mainnet</a>(), <a href="../bridge/chain_ids.md#bridge_chain_ids_sui_mainnet">chain_ids::sui_mainnet</a>()),
        5_000_000 * <a href="../bridge/limiter.md#bridge_limiter_USD_VALUE_MULTIPLIER">USD_VALUE_MULTIPLIER</a>
    );
    // MAX limit <b>for</b> testnet and devnet
    transfer_limits.insert(
        <a href="../bridge/chain_ids.md#bridge_chain_ids_get_route">chain_ids::get_route</a>(<a href="../bridge/chain_ids.md#bridge_chain_ids_eth_sepolia">chain_ids::eth_sepolia</a>(), <a href="../bridge/chain_ids.md#bridge_chain_ids_sui_testnet">chain_ids::sui_testnet</a>()),
        <a href="../bridge/limiter.md#bridge_limiter_MAX_TRANSFER_LIMIT">MAX_TRANSFER_LIMIT</a>
    );
    transfer_limits.insert(
        <a href="../bridge/chain_ids.md#bridge_chain_ids_get_route">chain_ids::get_route</a>(<a href="../bridge/chain_ids.md#bridge_chain_ids_eth_sepolia">chain_ids::eth_sepolia</a>(), <a href="../bridge/chain_ids.md#bridge_chain_ids_sui_custom">chain_ids::sui_custom</a>()),
        <a href="../bridge/limiter.md#bridge_limiter_MAX_TRANSFER_LIMIT">MAX_TRANSFER_LIMIT</a>
    );
    transfer_limits.insert(
        <a href="../bridge/chain_ids.md#bridge_chain_ids_get_route">chain_ids::get_route</a>(<a href="../bridge/chain_ids.md#bridge_chain_ids_eth_custom">chain_ids::eth_custom</a>(), <a href="../bridge/chain_ids.md#bridge_chain_ids_sui_testnet">chain_ids::sui_testnet</a>()),
        <a href="../bridge/limiter.md#bridge_limiter_MAX_TRANSFER_LIMIT">MAX_TRANSFER_LIMIT</a>
    );
    transfer_limits.insert(
        <a href="../bridge/chain_ids.md#bridge_chain_ids_get_route">chain_ids::get_route</a>(<a href="../bridge/chain_ids.md#bridge_chain_ids_eth_custom">chain_ids::eth_custom</a>(), <a href="../bridge/chain_ids.md#bridge_chain_ids_sui_custom">chain_ids::sui_custom</a>()),
        <a href="../bridge/limiter.md#bridge_limiter_MAX_TRANSFER_LIMIT">MAX_TRANSFER_LIMIT</a>
    );
    transfer_limits
}
</code></pre>



</details>
