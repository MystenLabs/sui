---
title: Module `bridge::message_types`
---



-  [Constants](#@Constants_0)
-  [Function `token`](#bridge_message_types_token)
-  [Function `committee_blocklist`](#bridge_message_types_committee_blocklist)
-  [Function `emergency_op`](#bridge_message_types_emergency_op)
-  [Function `update_bridge_limit`](#bridge_message_types_update_bridge_limit)
-  [Function `update_asset_price`](#bridge_message_types_update_asset_price)
-  [Function `add_tokens_on_sui`](#bridge_message_types_add_tokens_on_sui)


<pre><code></code></pre>



<a name="@Constants_0"></a>

## Constants


<a name="bridge_message_types_ADD_TOKENS_ON_SUI"></a>



<pre><code><b>const</b> <a href="../bridge/message_types.md#bridge_message_types_ADD_TOKENS_ON_SUI">ADD_TOKENS_ON_SUI</a>: u8 = 6;
</code></pre>



<a name="bridge_message_types_COMMITTEE_BLOCKLIST"></a>



<pre><code><b>const</b> <a href="../bridge/message_types.md#bridge_message_types_COMMITTEE_BLOCKLIST">COMMITTEE_BLOCKLIST</a>: u8 = 1;
</code></pre>



<a name="bridge_message_types_EMERGENCY_OP"></a>



<pre><code><b>const</b> <a href="../bridge/message_types.md#bridge_message_types_EMERGENCY_OP">EMERGENCY_OP</a>: u8 = 2;
</code></pre>



<a name="bridge_message_types_TOKEN"></a>



<pre><code><b>const</b> <a href="../bridge/message_types.md#bridge_message_types_TOKEN">TOKEN</a>: u8 = 0;
</code></pre>



<a name="bridge_message_types_UPDATE_ASSET_PRICE"></a>



<pre><code><b>const</b> <a href="../bridge/message_types.md#bridge_message_types_UPDATE_ASSET_PRICE">UPDATE_ASSET_PRICE</a>: u8 = 4;
</code></pre>



<a name="bridge_message_types_UPDATE_BRIDGE_LIMIT"></a>



<pre><code><b>const</b> <a href="../bridge/message_types.md#bridge_message_types_UPDATE_BRIDGE_LIMIT">UPDATE_BRIDGE_LIMIT</a>: u8 = 3;
</code></pre>



<a name="bridge_message_types_token"></a>

## Function `token`



<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message_types.md#bridge_message_types_token">token</a>(): u8
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message_types.md#bridge_message_types_token">token</a>(): u8 { <a href="../bridge/message_types.md#bridge_message_types_TOKEN">TOKEN</a> }
</code></pre>



</details>

<a name="bridge_message_types_committee_blocklist"></a>

## Function `committee_blocklist`



<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message_types.md#bridge_message_types_committee_blocklist">committee_blocklist</a>(): u8
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message_types.md#bridge_message_types_committee_blocklist">committee_blocklist</a>(): u8 { <a href="../bridge/message_types.md#bridge_message_types_COMMITTEE_BLOCKLIST">COMMITTEE_BLOCKLIST</a> }
</code></pre>



</details>

<a name="bridge_message_types_emergency_op"></a>

## Function `emergency_op`



<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message_types.md#bridge_message_types_emergency_op">emergency_op</a>(): u8
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message_types.md#bridge_message_types_emergency_op">emergency_op</a>(): u8 { <a href="../bridge/message_types.md#bridge_message_types_EMERGENCY_OP">EMERGENCY_OP</a> }
</code></pre>



</details>

<a name="bridge_message_types_update_bridge_limit"></a>

## Function `update_bridge_limit`



<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message_types.md#bridge_message_types_update_bridge_limit">update_bridge_limit</a>(): u8
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message_types.md#bridge_message_types_update_bridge_limit">update_bridge_limit</a>(): u8 { <a href="../bridge/message_types.md#bridge_message_types_UPDATE_BRIDGE_LIMIT">UPDATE_BRIDGE_LIMIT</a> }
</code></pre>



</details>

<a name="bridge_message_types_update_asset_price"></a>

## Function `update_asset_price`



<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message_types.md#bridge_message_types_update_asset_price">update_asset_price</a>(): u8
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message_types.md#bridge_message_types_update_asset_price">update_asset_price</a>(): u8 { <a href="../bridge/message_types.md#bridge_message_types_UPDATE_ASSET_PRICE">UPDATE_ASSET_PRICE</a> }
</code></pre>



</details>

<a name="bridge_message_types_add_tokens_on_sui"></a>

## Function `add_tokens_on_sui`



<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message_types.md#bridge_message_types_add_tokens_on_sui">add_tokens_on_sui</a>(): u8
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message_types.md#bridge_message_types_add_tokens_on_sui">add_tokens_on_sui</a>(): u8 { <a href="../bridge/message_types.md#bridge_message_types_ADD_TOKENS_ON_SUI">ADD_TOKENS_ON_SUI</a> }
</code></pre>



</details>
