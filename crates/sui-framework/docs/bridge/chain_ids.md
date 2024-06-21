---
title: Module `0xb::chain_ids`
---



-  [Struct `BridgeRoute`](#0xb_chain_ids_BridgeRoute)
-  [Constants](#@Constants_0)
-  [Function `sui_mainnet`](#0xb_chain_ids_sui_mainnet)
-  [Function `sui_testnet`](#0xb_chain_ids_sui_testnet)
-  [Function `sui_custom`](#0xb_chain_ids_sui_custom)
-  [Function `eth_mainnet`](#0xb_chain_ids_eth_mainnet)
-  [Function `eth_sepolia`](#0xb_chain_ids_eth_sepolia)
-  [Function `eth_custom`](#0xb_chain_ids_eth_custom)
-  [Function `route_source`](#0xb_chain_ids_route_source)
-  [Function `route_destination`](#0xb_chain_ids_route_destination)
-  [Function `assert_valid_chain_id`](#0xb_chain_ids_assert_valid_chain_id)
-  [Function `valid_routes`](#0xb_chain_ids_valid_routes)
-  [Function `is_valid_route`](#0xb_chain_ids_is_valid_route)
-  [Function `get_route`](#0xb_chain_ids_get_route)


<pre><code><b>use</b> <a href="../move-stdlib/vector.md#0x1_vector">0x1::vector</a>;
</code></pre>



<a name="0xb_chain_ids_BridgeRoute"></a>

## Struct `BridgeRoute`



<pre><code><b>struct</b> <a href="chain_ids.md#0xb_chain_ids_BridgeRoute">BridgeRoute</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>source: u8</code>
</dt>
<dd>

</dd>
<dt>
<code>destination: u8</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0xb_chain_ids_EInvalidBridgeRoute"></a>



<pre><code><b>const</b> <a href="chain_ids.md#0xb_chain_ids_EInvalidBridgeRoute">EInvalidBridgeRoute</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 0;
</code></pre>



<a name="0xb_chain_ids_EthCustom"></a>



<pre><code><b>const</b> <a href="chain_ids.md#0xb_chain_ids_EthCustom">EthCustom</a>: u8 = 12;
</code></pre>



<a name="0xb_chain_ids_EthMainnet"></a>



<pre><code><b>const</b> <a href="chain_ids.md#0xb_chain_ids_EthMainnet">EthMainnet</a>: u8 = 10;
</code></pre>



<a name="0xb_chain_ids_EthSepolia"></a>



<pre><code><b>const</b> <a href="chain_ids.md#0xb_chain_ids_EthSepolia">EthSepolia</a>: u8 = 11;
</code></pre>



<a name="0xb_chain_ids_SuiCustom"></a>



<pre><code><b>const</b> <a href="chain_ids.md#0xb_chain_ids_SuiCustom">SuiCustom</a>: u8 = 2;
</code></pre>



<a name="0xb_chain_ids_SuiMainnet"></a>



<pre><code><b>const</b> <a href="chain_ids.md#0xb_chain_ids_SuiMainnet">SuiMainnet</a>: u8 = 0;
</code></pre>



<a name="0xb_chain_ids_SuiTestnet"></a>



<pre><code><b>const</b> <a href="chain_ids.md#0xb_chain_ids_SuiTestnet">SuiTestnet</a>: u8 = 1;
</code></pre>



<a name="0xb_chain_ids_sui_mainnet"></a>

## Function `sui_mainnet`



<pre><code><b>public</b> <b>fun</b> <a href="chain_ids.md#0xb_chain_ids_sui_mainnet">sui_mainnet</a>(): u8
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="chain_ids.md#0xb_chain_ids_sui_mainnet">sui_mainnet</a>(): u8 { <a href="chain_ids.md#0xb_chain_ids_SuiMainnet">SuiMainnet</a> }
</code></pre>



</details>

<a name="0xb_chain_ids_sui_testnet"></a>

## Function `sui_testnet`



<pre><code><b>public</b> <b>fun</b> <a href="chain_ids.md#0xb_chain_ids_sui_testnet">sui_testnet</a>(): u8
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="chain_ids.md#0xb_chain_ids_sui_testnet">sui_testnet</a>(): u8 { <a href="chain_ids.md#0xb_chain_ids_SuiTestnet">SuiTestnet</a> }
</code></pre>



</details>

<a name="0xb_chain_ids_sui_custom"></a>

## Function `sui_custom`



<pre><code><b>public</b> <b>fun</b> <a href="chain_ids.md#0xb_chain_ids_sui_custom">sui_custom</a>(): u8
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="chain_ids.md#0xb_chain_ids_sui_custom">sui_custom</a>(): u8 { <a href="chain_ids.md#0xb_chain_ids_SuiCustom">SuiCustom</a> }
</code></pre>



</details>

<a name="0xb_chain_ids_eth_mainnet"></a>

## Function `eth_mainnet`



<pre><code><b>public</b> <b>fun</b> <a href="chain_ids.md#0xb_chain_ids_eth_mainnet">eth_mainnet</a>(): u8
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="chain_ids.md#0xb_chain_ids_eth_mainnet">eth_mainnet</a>(): u8 { <a href="chain_ids.md#0xb_chain_ids_EthMainnet">EthMainnet</a> }
</code></pre>



</details>

<a name="0xb_chain_ids_eth_sepolia"></a>

## Function `eth_sepolia`



<pre><code><b>public</b> <b>fun</b> <a href="chain_ids.md#0xb_chain_ids_eth_sepolia">eth_sepolia</a>(): u8
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="chain_ids.md#0xb_chain_ids_eth_sepolia">eth_sepolia</a>(): u8 { <a href="chain_ids.md#0xb_chain_ids_EthSepolia">EthSepolia</a> }
</code></pre>



</details>

<a name="0xb_chain_ids_eth_custom"></a>

## Function `eth_custom`



<pre><code><b>public</b> <b>fun</b> <a href="chain_ids.md#0xb_chain_ids_eth_custom">eth_custom</a>(): u8
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="chain_ids.md#0xb_chain_ids_eth_custom">eth_custom</a>(): u8 { <a href="chain_ids.md#0xb_chain_ids_EthCustom">EthCustom</a> }
</code></pre>



</details>

<a name="0xb_chain_ids_route_source"></a>

## Function `route_source`



<pre><code><b>public</b> <b>fun</b> <a href="chain_ids.md#0xb_chain_ids_route_source">route_source</a>(route: &<a href="chain_ids.md#0xb_chain_ids_BridgeRoute">chain_ids::BridgeRoute</a>): &u8
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="chain_ids.md#0xb_chain_ids_route_source">route_source</a>(route: &<a href="chain_ids.md#0xb_chain_ids_BridgeRoute">BridgeRoute</a>): &u8 {
    &route.source
}
</code></pre>



</details>

<a name="0xb_chain_ids_route_destination"></a>

## Function `route_destination`



<pre><code><b>public</b> <b>fun</b> <a href="chain_ids.md#0xb_chain_ids_route_destination">route_destination</a>(route: &<a href="chain_ids.md#0xb_chain_ids_BridgeRoute">chain_ids::BridgeRoute</a>): &u8
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="chain_ids.md#0xb_chain_ids_route_destination">route_destination</a>(route: &<a href="chain_ids.md#0xb_chain_ids_BridgeRoute">BridgeRoute</a>): &u8 {
    &route.destination
}
</code></pre>



</details>

<a name="0xb_chain_ids_assert_valid_chain_id"></a>

## Function `assert_valid_chain_id`



<pre><code><b>public</b> <b>fun</b> <a href="chain_ids.md#0xb_chain_ids_assert_valid_chain_id">assert_valid_chain_id</a>(id: u8)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="chain_ids.md#0xb_chain_ids_assert_valid_chain_id">assert_valid_chain_id</a>(id: u8) {
    <b>assert</b>!(
        id == <a href="chain_ids.md#0xb_chain_ids_SuiMainnet">SuiMainnet</a> ||
        id == <a href="chain_ids.md#0xb_chain_ids_SuiTestnet">SuiTestnet</a> ||
        id == <a href="chain_ids.md#0xb_chain_ids_SuiCustom">SuiCustom</a> ||
        id == <a href="chain_ids.md#0xb_chain_ids_EthMainnet">EthMainnet</a> ||
        id == <a href="chain_ids.md#0xb_chain_ids_EthSepolia">EthSepolia</a> ||
        id == <a href="chain_ids.md#0xb_chain_ids_EthCustom">EthCustom</a>,
        <a href="chain_ids.md#0xb_chain_ids_EInvalidBridgeRoute">EInvalidBridgeRoute</a>
    )
}
</code></pre>



</details>

<a name="0xb_chain_ids_valid_routes"></a>

## Function `valid_routes`



<pre><code><b>public</b> <b>fun</b> <a href="chain_ids.md#0xb_chain_ids_valid_routes">valid_routes</a>(): <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="chain_ids.md#0xb_chain_ids_BridgeRoute">chain_ids::BridgeRoute</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="chain_ids.md#0xb_chain_ids_valid_routes">valid_routes</a>(): <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="chain_ids.md#0xb_chain_ids_BridgeRoute">BridgeRoute</a>&gt; {
    <a href="../move-stdlib/vector.md#0x1_vector">vector</a>[
        <a href="chain_ids.md#0xb_chain_ids_BridgeRoute">BridgeRoute</a> { source: <a href="chain_ids.md#0xb_chain_ids_SuiMainnet">SuiMainnet</a>, destination: <a href="chain_ids.md#0xb_chain_ids_EthMainnet">EthMainnet</a> },
        <a href="chain_ids.md#0xb_chain_ids_BridgeRoute">BridgeRoute</a> { source: <a href="chain_ids.md#0xb_chain_ids_EthMainnet">EthMainnet</a>, destination: <a href="chain_ids.md#0xb_chain_ids_SuiMainnet">SuiMainnet</a> },

        <a href="chain_ids.md#0xb_chain_ids_BridgeRoute">BridgeRoute</a> { source: <a href="chain_ids.md#0xb_chain_ids_SuiTestnet">SuiTestnet</a>, destination: <a href="chain_ids.md#0xb_chain_ids_EthSepolia">EthSepolia</a> },
        <a href="chain_ids.md#0xb_chain_ids_BridgeRoute">BridgeRoute</a> { source: <a href="chain_ids.md#0xb_chain_ids_SuiTestnet">SuiTestnet</a>, destination: <a href="chain_ids.md#0xb_chain_ids_EthCustom">EthCustom</a> },
        <a href="chain_ids.md#0xb_chain_ids_BridgeRoute">BridgeRoute</a> { source: <a href="chain_ids.md#0xb_chain_ids_SuiCustom">SuiCustom</a>, destination: <a href="chain_ids.md#0xb_chain_ids_EthCustom">EthCustom</a> },
        <a href="chain_ids.md#0xb_chain_ids_BridgeRoute">BridgeRoute</a> { source: <a href="chain_ids.md#0xb_chain_ids_SuiCustom">SuiCustom</a>, destination: <a href="chain_ids.md#0xb_chain_ids_EthSepolia">EthSepolia</a> },
        <a href="chain_ids.md#0xb_chain_ids_BridgeRoute">BridgeRoute</a> { source: <a href="chain_ids.md#0xb_chain_ids_EthSepolia">EthSepolia</a>, destination: <a href="chain_ids.md#0xb_chain_ids_SuiTestnet">SuiTestnet</a> },
        <a href="chain_ids.md#0xb_chain_ids_BridgeRoute">BridgeRoute</a> { source: <a href="chain_ids.md#0xb_chain_ids_EthSepolia">EthSepolia</a>, destination: <a href="chain_ids.md#0xb_chain_ids_SuiCustom">SuiCustom</a> },
        <a href="chain_ids.md#0xb_chain_ids_BridgeRoute">BridgeRoute</a> { source: <a href="chain_ids.md#0xb_chain_ids_EthCustom">EthCustom</a>, destination: <a href="chain_ids.md#0xb_chain_ids_SuiTestnet">SuiTestnet</a> },
        <a href="chain_ids.md#0xb_chain_ids_BridgeRoute">BridgeRoute</a> { source: <a href="chain_ids.md#0xb_chain_ids_EthCustom">EthCustom</a>, destination: <a href="chain_ids.md#0xb_chain_ids_SuiCustom">SuiCustom</a> }
    ]
}
</code></pre>



</details>

<a name="0xb_chain_ids_is_valid_route"></a>

## Function `is_valid_route`



<pre><code><b>public</b> <b>fun</b> <a href="chain_ids.md#0xb_chain_ids_is_valid_route">is_valid_route</a>(source: u8, destination: u8): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="chain_ids.md#0xb_chain_ids_is_valid_route">is_valid_route</a>(source: u8, destination: u8): bool {
    <b>let</b> route = <a href="chain_ids.md#0xb_chain_ids_BridgeRoute">BridgeRoute</a> { source, destination };
    <a href="chain_ids.md#0xb_chain_ids_valid_routes">valid_routes</a>().contains(&route)
}
</code></pre>



</details>

<a name="0xb_chain_ids_get_route"></a>

## Function `get_route`



<pre><code><b>public</b> <b>fun</b> <a href="chain_ids.md#0xb_chain_ids_get_route">get_route</a>(source: u8, destination: u8): <a href="chain_ids.md#0xb_chain_ids_BridgeRoute">chain_ids::BridgeRoute</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="chain_ids.md#0xb_chain_ids_get_route">get_route</a>(source: u8, destination: u8): <a href="chain_ids.md#0xb_chain_ids_BridgeRoute">BridgeRoute</a> {
    <b>let</b> route = <a href="chain_ids.md#0xb_chain_ids_BridgeRoute">BridgeRoute</a> { source, destination };
    <b>assert</b>!(<a href="chain_ids.md#0xb_chain_ids_valid_routes">valid_routes</a>().contains(&route), <a href="chain_ids.md#0xb_chain_ids_EInvalidBridgeRoute">EInvalidBridgeRoute</a>);
    route
}
</code></pre>



</details>
