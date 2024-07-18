---
title: Module `0xb::limiter`
---



-  [Struct `TransferLimiter`](#0xb_limiter_TransferLimiter)
-  [Struct `TransferRecord`](#0xb_limiter_TransferRecord)
-  [Struct `UpdateRouteLimitEvent`](#0xb_limiter_UpdateRouteLimitEvent)
-  [Constants](#@Constants_0)
-  [Function `get_route_limit`](#0xb_limiter_get_route_limit)
-  [Function `new`](#0xb_limiter_new)
-  [Function `check_and_record_sending_transfer`](#0xb_limiter_check_and_record_sending_transfer)
-  [Function `update_route_limit`](#0xb_limiter_update_route_limit)
-  [Function `current_hour_since_epoch`](#0xb_limiter_current_hour_since_epoch)
-  [Function `adjust_transfer_records`](#0xb_limiter_adjust_transfer_records)
-  [Function `initial_transfer_limits`](#0xb_limiter_initial_transfer_limits)


<pre><code><b>use</b> <a href="../move-stdlib/option.md#0x1_option">0x1::option</a>;
<b>use</b> <a href="../move-stdlib/vector.md#0x1_vector">0x1::vector</a>;
<b>use</b> <a href="../sui-framework/clock.md#0x2_clock">0x2::clock</a>;
<b>use</b> <a href="../sui-framework/event.md#0x2_event">0x2::event</a>;
<b>use</b> <a href="../sui-framework/vec_map.md#0x2_vec_map">0x2::vec_map</a>;
<b>use</b> <a href="chain_ids.md#0xb_chain_ids">0xb::chain_ids</a>;
<b>use</b> <a href="treasury.md#0xb_treasury">0xb::treasury</a>;
</code></pre>



<a name="0xb_limiter_TransferLimiter"></a>

## Struct `TransferLimiter`



<pre><code><b>struct</b> <a href="limiter.md#0xb_limiter_TransferLimiter">TransferLimiter</a> <b>has</b> store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>transfer_limits: <a href="../sui-framework/vec_map.md#0x2_vec_map_VecMap">vec_map::VecMap</a>&lt;<a href="chain_ids.md#0xb_chain_ids_BridgeRoute">chain_ids::BridgeRoute</a>, <a href="../move-stdlib/u64.md#0x1_u64">u64</a>&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>transfer_records: <a href="../sui-framework/vec_map.md#0x2_vec_map_VecMap">vec_map::VecMap</a>&lt;<a href="chain_ids.md#0xb_chain_ids_BridgeRoute">chain_ids::BridgeRoute</a>, <a href="limiter.md#0xb_limiter_TransferRecord">limiter::TransferRecord</a>&gt;</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0xb_limiter_TransferRecord"></a>

## Struct `TransferRecord`



<pre><code><b>struct</b> <a href="limiter.md#0xb_limiter_TransferRecord">TransferRecord</a> <b>has</b> store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>hour_head: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>

</dd>
<dt>
<code>hour_tail: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>

</dd>
<dt>
<code>per_hour_amounts: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="../move-stdlib/u64.md#0x1_u64">u64</a>&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>total_amount: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0xb_limiter_UpdateRouteLimitEvent"></a>

## Struct `UpdateRouteLimitEvent`



<pre><code><b>struct</b> <a href="limiter.md#0xb_limiter_UpdateRouteLimitEvent">UpdateRouteLimitEvent</a> <b>has</b> <b>copy</b>, drop
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
<code>new_limit: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0xb_limiter_ELimitNotFoundForRoute"></a>



<pre><code><b>const</b> <a href="limiter.md#0xb_limiter_ELimitNotFoundForRoute">ELimitNotFoundForRoute</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 0;
</code></pre>



<a name="0xb_limiter_MAX_TRANSFER_LIMIT"></a>



<pre><code><b>const</b> <a href="limiter.md#0xb_limiter_MAX_TRANSFER_LIMIT">MAX_TRANSFER_LIMIT</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 18446744073709551615;
</code></pre>



<a name="0xb_limiter_USD_VALUE_MULTIPLIER"></a>



<pre><code><b>const</b> <a href="limiter.md#0xb_limiter_USD_VALUE_MULTIPLIER">USD_VALUE_MULTIPLIER</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 100000000;
</code></pre>



<a name="0xb_limiter_get_route_limit"></a>

## Function `get_route_limit`



<pre><code><b>public</b> <b>fun</b> <a href="limiter.md#0xb_limiter_get_route_limit">get_route_limit</a>(self: &<a href="limiter.md#0xb_limiter_TransferLimiter">limiter::TransferLimiter</a>, route: &<a href="chain_ids.md#0xb_chain_ids_BridgeRoute">chain_ids::BridgeRoute</a>): <a href="../move-stdlib/u64.md#0x1_u64">u64</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="limiter.md#0xb_limiter_get_route_limit">get_route_limit</a>(self: &<a href="limiter.md#0xb_limiter_TransferLimiter">TransferLimiter</a>, route: &BridgeRoute): <a href="../move-stdlib/u64.md#0x1_u64">u64</a> {
    self.transfer_limits[route]
}
</code></pre>



</details>

<a name="0xb_limiter_new"></a>

## Function `new`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="limiter.md#0xb_limiter_new">new</a>(): <a href="limiter.md#0xb_limiter_TransferLimiter">limiter::TransferLimiter</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui-framework/package.md#0x2_package">package</a>) <b>fun</b> <a href="limiter.md#0xb_limiter_new">new</a>(): <a href="limiter.md#0xb_limiter_TransferLimiter">TransferLimiter</a> {
    // hardcoded limit for <a href="bridge.md#0xb_bridge">bridge</a> <a href="../sui-system/genesis.md#0x3_genesis">genesis</a>
    <a href="limiter.md#0xb_limiter_TransferLimiter">TransferLimiter</a> {
        transfer_limits: <a href="limiter.md#0xb_limiter_initial_transfer_limits">initial_transfer_limits</a>(),
        transfer_records: <a href="../sui-framework/vec_map.md#0x2_vec_map_empty">vec_map::empty</a>()
    }
}
</code></pre>



</details>

<a name="0xb_limiter_check_and_record_sending_transfer"></a>

## Function `check_and_record_sending_transfer`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="limiter.md#0xb_limiter_check_and_record_sending_transfer">check_and_record_sending_transfer</a>&lt;T&gt;(self: &<b>mut</b> <a href="limiter.md#0xb_limiter_TransferLimiter">limiter::TransferLimiter</a>, <a href="treasury.md#0xb_treasury">treasury</a>: &<a href="treasury.md#0xb_treasury_BridgeTreasury">treasury::BridgeTreasury</a>, <a href="../sui-framework/clock.md#0x2_clock">clock</a>: &<a href="../sui-framework/clock.md#0x2_clock_Clock">clock::Clock</a>, route: <a href="chain_ids.md#0xb_chain_ids_BridgeRoute">chain_ids::BridgeRoute</a>, amount: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui-framework/package.md#0x2_package">package</a>) <b>fun</b> <a href="limiter.md#0xb_limiter_check_and_record_sending_transfer">check_and_record_sending_transfer</a>&lt;T&gt;(
    self: &<b>mut</b> <a href="limiter.md#0xb_limiter_TransferLimiter">TransferLimiter</a>,
    <a href="treasury.md#0xb_treasury">treasury</a>: &BridgeTreasury,
    <a href="../sui-framework/clock.md#0x2_clock">clock</a>: &Clock,
    route: BridgeRoute,
    amount: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>
): bool {
    // Create record for route <b>if</b> not exists
    <b>if</b> (!self.transfer_records.contains(&route)) {
        self.transfer_records.insert(route, <a href="limiter.md#0xb_limiter_TransferRecord">TransferRecord</a> {
            hour_head: 0,
            hour_tail: 0,
            per_hour_amounts: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>[],
            total_amount: 0
        })
    };
    <b>let</b> record = self.transfer_records.get_mut(&route);
    <b>let</b> current_hour_since_epoch = <a href="limiter.md#0xb_limiter_current_hour_since_epoch">current_hour_since_epoch</a>(<a href="../sui-framework/clock.md#0x2_clock">clock</a>);

    record.<a href="limiter.md#0xb_limiter_adjust_transfer_records">adjust_transfer_records</a>(current_hour_since_epoch);

    // Get limit for the route
    <b>let</b> route_limit = self.transfer_limits.try_get(&route);
    <b>assert</b>!(route_limit.is_some(), <a href="limiter.md#0xb_limiter_ELimitNotFoundForRoute">ELimitNotFoundForRoute</a>);
    <b>let</b> route_limit = route_limit.destroy_some();
    <b>let</b> route_limit_adjusted =
        (route_limit <b>as</b> u128) * (<a href="treasury.md#0xb_treasury">treasury</a>.decimal_multiplier&lt;T&gt;() <b>as</b> u128);

    // Compute notional amount
    // Upcast <b>to</b> u128 <b>to</b> prevent overflow, <b>to</b> not miss out on small amounts.
    <b>let</b> value = (<a href="treasury.md#0xb_treasury">treasury</a>.notional_value&lt;T&gt;() <b>as</b> u128);
    <b>let</b> notional_amount_with_token_multiplier = value * (amount <b>as</b> u128);

    // Check <b>if</b> <a href="../sui-framework/transfer.md#0x2_transfer">transfer</a> amount exceed limit
    // Upscale them <b>to</b> the token's decimal.
    <b>if</b> ((record.total_amount <b>as</b> u128)
        * (<a href="treasury.md#0xb_treasury">treasury</a>.decimal_multiplier&lt;T&gt;() <b>as</b> u128)
        + notional_amount_with_token_multiplier &gt; route_limit_adjusted
    ) {
        <b>return</b> <b>false</b>
    };

    // Now scale down <b>to</b> notional value
    <b>let</b> notional_amount = notional_amount_with_token_multiplier
        / (<a href="treasury.md#0xb_treasury">treasury</a>.decimal_multiplier&lt;T&gt;() <b>as</b> u128);
    // Should be safe <b>to</b> downcast <b>to</b> <a href="../move-stdlib/u64.md#0x1_u64">u64</a> after dividing by the decimals
    <b>let</b> notional_amount = (notional_amount <b>as</b> <a href="../move-stdlib/u64.md#0x1_u64">u64</a>);

    // Record <a href="../sui-framework/transfer.md#0x2_transfer">transfer</a> value
    <b>let</b> new_amount = record.per_hour_amounts.pop_back() + notional_amount;
    record.per_hour_amounts.push_back(new_amount);
    record.total_amount = record.total_amount + notional_amount;
    <b>true</b>
}
</code></pre>



</details>

<a name="0xb_limiter_update_route_limit"></a>

## Function `update_route_limit`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="limiter.md#0xb_limiter_update_route_limit">update_route_limit</a>(self: &<b>mut</b> <a href="limiter.md#0xb_limiter_TransferLimiter">limiter::TransferLimiter</a>, route: &<a href="chain_ids.md#0xb_chain_ids_BridgeRoute">chain_ids::BridgeRoute</a>, new_usd_limit: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui-framework/package.md#0x2_package">package</a>) <b>fun</b> <a href="limiter.md#0xb_limiter_update_route_limit">update_route_limit</a>(
    self: &<b>mut</b> <a href="limiter.md#0xb_limiter_TransferLimiter">TransferLimiter</a>,
    route: &BridgeRoute,
    new_usd_limit: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>
) {
    <b>let</b> receiving_chain = *route.destination();

    <b>if</b> (!self.transfer_limits.contains(route)) {
        self.transfer_limits.insert(*route, new_usd_limit);
    } <b>else</b> {
        *&<b>mut</b> self.transfer_limits[route] = new_usd_limit;
    };

    emit(<a href="limiter.md#0xb_limiter_UpdateRouteLimitEvent">UpdateRouteLimitEvent</a> {
        sending_chain: *route.source(),
        receiving_chain,
        new_limit: new_usd_limit,
    })
}
</code></pre>



</details>

<a name="0xb_limiter_current_hour_since_epoch"></a>

## Function `current_hour_since_epoch`



<pre><code><b>fun</b> <a href="limiter.md#0xb_limiter_current_hour_since_epoch">current_hour_since_epoch</a>(<a href="../sui-framework/clock.md#0x2_clock">clock</a>: &<a href="../sui-framework/clock.md#0x2_clock_Clock">clock::Clock</a>): <a href="../move-stdlib/u64.md#0x1_u64">u64</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="limiter.md#0xb_limiter_current_hour_since_epoch">current_hour_since_epoch</a>(<a href="../sui-framework/clock.md#0x2_clock">clock</a>: &Clock): <a href="../move-stdlib/u64.md#0x1_u64">u64</a> {
    <a href="../sui-framework/clock.md#0x2_clock_timestamp_ms">clock::timestamp_ms</a>(<a href="../sui-framework/clock.md#0x2_clock">clock</a>) / 3600000
}
</code></pre>



</details>

<a name="0xb_limiter_adjust_transfer_records"></a>

## Function `adjust_transfer_records`



<pre><code><b>fun</b> <a href="limiter.md#0xb_limiter_adjust_transfer_records">adjust_transfer_records</a>(self: &<b>mut</b> <a href="limiter.md#0xb_limiter_TransferRecord">limiter::TransferRecord</a>, current_hour_since_epoch: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="limiter.md#0xb_limiter_adjust_transfer_records">adjust_transfer_records</a>(self: &<b>mut</b> <a href="limiter.md#0xb_limiter_TransferRecord">TransferRecord</a>, current_hour_since_epoch: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>) {
    <b>if</b> (self.hour_head == current_hour_since_epoch) {
        <b>return</b> // nothing <b>to</b> backfill
    };

    <b>let</b> target_tail = current_hour_since_epoch - 23;

    // If `hour_head` is even older than 24 hours ago, it means all items in
    // `per_hour_amounts` are <b>to</b> be evicted.
    <b>if</b> (self.hour_head &lt; target_tail) {
        self.per_hour_amounts = <a href="../move-stdlib/vector.md#0x1_vector">vector</a>[];
        self.total_amount = 0;
        self.hour_tail = target_tail;
        self.hour_head = target_tail;
        // Don't forget <b>to</b> insert this hour's record
        self.per_hour_amounts.push_back(0);
    } <b>else</b> {
        // self.hour_head is within 24 hour range.
        // some items in `per_hour_amounts` are still valid, we remove stale hours.
        <b>while</b> (self.hour_tail &lt; target_tail) {
            self.total_amount = self.total_amount - self.per_hour_amounts.remove(0);
            self.hour_tail = self.hour_tail + 1;
        }
    };

    // Backfill from hour_head <b>to</b> current hour
    <b>while</b> (self.hour_head &lt; current_hour_since_epoch) {
        self.per_hour_amounts.push_back(0);
        self.hour_head = self.hour_head + 1;
    }
}
</code></pre>



</details>

<a name="0xb_limiter_initial_transfer_limits"></a>

## Function `initial_transfer_limits`



<pre><code><b>fun</b> <a href="limiter.md#0xb_limiter_initial_transfer_limits">initial_transfer_limits</a>(): <a href="../sui-framework/vec_map.md#0x2_vec_map_VecMap">vec_map::VecMap</a>&lt;<a href="chain_ids.md#0xb_chain_ids_BridgeRoute">chain_ids::BridgeRoute</a>, <a href="../move-stdlib/u64.md#0x1_u64">u64</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="limiter.md#0xb_limiter_initial_transfer_limits">initial_transfer_limits</a>(): VecMap&lt;BridgeRoute, <a href="../move-stdlib/u64.md#0x1_u64">u64</a>&gt; {
    <b>let</b> <b>mut</b> transfer_limits = <a href="../sui-framework/vec_map.md#0x2_vec_map_empty">vec_map::empty</a>();
    // 5M limit on Sui -&gt; Ethereum mainnet
    transfer_limits.insert(
        <a href="chain_ids.md#0xb_chain_ids_get_route">chain_ids::get_route</a>(<a href="chain_ids.md#0xb_chain_ids_eth_mainnet">chain_ids::eth_mainnet</a>(), <a href="chain_ids.md#0xb_chain_ids_sui_mainnet">chain_ids::sui_mainnet</a>()),
        5_000_000 * <a href="limiter.md#0xb_limiter_USD_VALUE_MULTIPLIER">USD_VALUE_MULTIPLIER</a>
    );

    // MAX limit for testnet and devnet
    transfer_limits.insert(
        <a href="chain_ids.md#0xb_chain_ids_get_route">chain_ids::get_route</a>(<a href="chain_ids.md#0xb_chain_ids_eth_sepolia">chain_ids::eth_sepolia</a>(), <a href="chain_ids.md#0xb_chain_ids_sui_testnet">chain_ids::sui_testnet</a>()),
        <a href="limiter.md#0xb_limiter_MAX_TRANSFER_LIMIT">MAX_TRANSFER_LIMIT</a>
    );

    transfer_limits.insert(
        <a href="chain_ids.md#0xb_chain_ids_get_route">chain_ids::get_route</a>(<a href="chain_ids.md#0xb_chain_ids_eth_sepolia">chain_ids::eth_sepolia</a>(), <a href="chain_ids.md#0xb_chain_ids_sui_custom">chain_ids::sui_custom</a>()),
        <a href="limiter.md#0xb_limiter_MAX_TRANSFER_LIMIT">MAX_TRANSFER_LIMIT</a>
    );

    transfer_limits.insert(
        <a href="chain_ids.md#0xb_chain_ids_get_route">chain_ids::get_route</a>(<a href="chain_ids.md#0xb_chain_ids_eth_custom">chain_ids::eth_custom</a>(), <a href="chain_ids.md#0xb_chain_ids_sui_testnet">chain_ids::sui_testnet</a>()),
        <a href="limiter.md#0xb_limiter_MAX_TRANSFER_LIMIT">MAX_TRANSFER_LIMIT</a>
    );

    transfer_limits.insert(
        <a href="chain_ids.md#0xb_chain_ids_get_route">chain_ids::get_route</a>(<a href="chain_ids.md#0xb_chain_ids_eth_custom">chain_ids::eth_custom</a>(), <a href="chain_ids.md#0xb_chain_ids_sui_custom">chain_ids::sui_custom</a>()),
        <a href="limiter.md#0xb_limiter_MAX_TRANSFER_LIMIT">MAX_TRANSFER_LIMIT</a>
    );

    transfer_limits
}
</code></pre>



</details>
