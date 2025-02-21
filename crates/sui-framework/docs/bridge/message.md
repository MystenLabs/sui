---
title: Module `bridge::message`
---



-  [Struct `BridgeMessage`](#bridge_message_BridgeMessage)
-  [Struct `BridgeMessageKey`](#bridge_message_BridgeMessageKey)
-  [Struct `TokenTransferPayload`](#bridge_message_TokenTransferPayload)
-  [Struct `EmergencyOp`](#bridge_message_EmergencyOp)
-  [Struct `Blocklist`](#bridge_message_Blocklist)
-  [Struct `UpdateBridgeLimit`](#bridge_message_UpdateBridgeLimit)
-  [Struct `UpdateAssetPrice`](#bridge_message_UpdateAssetPrice)
-  [Struct `AddTokenOnSui`](#bridge_message_AddTokenOnSui)
-  [Struct `ParsedTokenTransferMessage`](#bridge_message_ParsedTokenTransferMessage)
-  [Constants](#@Constants_0)
-  [Function `extract_token_bridge_payload`](#bridge_message_extract_token_bridge_payload)
-  [Function `extract_emergency_op_payload`](#bridge_message_extract_emergency_op_payload)
-  [Function `extract_blocklist_payload`](#bridge_message_extract_blocklist_payload)
-  [Function `extract_update_bridge_limit`](#bridge_message_extract_update_bridge_limit)
-  [Function `extract_update_asset_price`](#bridge_message_extract_update_asset_price)
-  [Function `extract_add_tokens_on_sui`](#bridge_message_extract_add_tokens_on_sui)
-  [Function `serialize_message`](#bridge_message_serialize_message)
-  [Function `create_token_bridge_message`](#bridge_message_create_token_bridge_message)
-  [Function `create_emergency_op_message`](#bridge_message_create_emergency_op_message)
-  [Function `create_blocklist_message`](#bridge_message_create_blocklist_message)
-  [Function `create_update_bridge_limit_message`](#bridge_message_create_update_bridge_limit_message)
-  [Function `create_update_asset_price_message`](#bridge_message_create_update_asset_price_message)
-  [Function `create_add_tokens_on_sui_message`](#bridge_message_create_add_tokens_on_sui_message)
-  [Function `create_key`](#bridge_message_create_key)
-  [Function `key`](#bridge_message_key)
-  [Function `message_version`](#bridge_message_message_version)
-  [Function `message_type`](#bridge_message_message_type)
-  [Function `seq_num`](#bridge_message_seq_num)
-  [Function `source_chain`](#bridge_message_source_chain)
-  [Function `payload`](#bridge_message_payload)
-  [Function `token_target_chain`](#bridge_message_token_target_chain)
-  [Function `token_target_address`](#bridge_message_token_target_address)
-  [Function `token_type`](#bridge_message_token_type)
-  [Function `token_amount`](#bridge_message_token_amount)
-  [Function `emergency_op_type`](#bridge_message_emergency_op_type)
-  [Function `blocklist_type`](#bridge_message_blocklist_type)
-  [Function `blocklist_validator_addresses`](#bridge_message_blocklist_validator_addresses)
-  [Function `update_bridge_limit_payload_sending_chain`](#bridge_message_update_bridge_limit_payload_sending_chain)
-  [Function `update_bridge_limit_payload_receiving_chain`](#bridge_message_update_bridge_limit_payload_receiving_chain)
-  [Function `update_bridge_limit_payload_limit`](#bridge_message_update_bridge_limit_payload_limit)
-  [Function `update_asset_price_payload_token_id`](#bridge_message_update_asset_price_payload_token_id)
-  [Function `update_asset_price_payload_new_price`](#bridge_message_update_asset_price_payload_new_price)
-  [Function `is_native`](#bridge_message_is_native)
-  [Function `token_ids`](#bridge_message_token_ids)
-  [Function `token_type_names`](#bridge_message_token_type_names)
-  [Function `token_prices`](#bridge_message_token_prices)
-  [Function `emergency_op_pause`](#bridge_message_emergency_op_pause)
-  [Function `emergency_op_unpause`](#bridge_message_emergency_op_unpause)
-  [Function `required_voting_power`](#bridge_message_required_voting_power)
-  [Function `to_parsed_token_transfer_message`](#bridge_message_to_parsed_token_transfer_message)
-  [Function `reverse_bytes`](#bridge_message_reverse_bytes)
-  [Function `peel_u64_be`](#bridge_message_peel_u64_be)


<pre><code><b>use</b> <a href="../bridge/chain_ids.md#bridge_chain_ids">bridge::chain_ids</a>;
<b>use</b> <a href="../bridge/message_types.md#bridge_message_types">bridge::message_types</a>;
<b>use</b> <a href="../std/ascii.md#std_ascii">std::ascii</a>;
<b>use</b> <a href="../std/bcs.md#std_bcs">std::bcs</a>;
<b>use</b> <a href="../std/option.md#std_option">std::option</a>;
<b>use</b> <a href="../std/string.md#std_string">std::string</a>;
<b>use</b> <a href="../std/vector.md#std_vector">std::vector</a>;
<b>use</b> <a href="../sui/address.md#sui_address">sui::address</a>;
<b>use</b> <a href="../sui/bcs.md#sui_bcs">sui::bcs</a>;
<b>use</b> <a href="../sui/hex.md#sui_hex">sui::hex</a>;
</code></pre>



<a name="bridge_message_BridgeMessage"></a>

## Struct `BridgeMessage`



<pre><code><b>public</b> <b>struct</b> <a href="../bridge/message.md#bridge_message_BridgeMessage">BridgeMessage</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code><a href="../bridge/message.md#bridge_message_message_type">message_type</a>: u8</code>
</dt>
<dd>
</dd>
<dt>
<code><a href="../bridge/message.md#bridge_message_message_version">message_version</a>: u8</code>
</dt>
<dd>
</dd>
<dt>
<code><a href="../bridge/message.md#bridge_message_seq_num">seq_num</a>: u64</code>
</dt>
<dd>
</dd>
<dt>
<code><a href="../bridge/message.md#bridge_message_source_chain">source_chain</a>: u8</code>
</dt>
<dd>
</dd>
<dt>
<code><a href="../bridge/message.md#bridge_message_payload">payload</a>: vector&lt;u8&gt;</code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="bridge_message_BridgeMessageKey"></a>

## Struct `BridgeMessageKey`



<pre><code><b>public</b> <b>struct</b> <a href="../bridge/message.md#bridge_message_BridgeMessageKey">BridgeMessageKey</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code><a href="../bridge/message.md#bridge_message_source_chain">source_chain</a>: u8</code>
</dt>
<dd>
</dd>
<dt>
<code><a href="../bridge/message.md#bridge_message_message_type">message_type</a>: u8</code>
</dt>
<dd>
</dd>
<dt>
<code>bridge_seq_num: u64</code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="bridge_message_TokenTransferPayload"></a>

## Struct `TokenTransferPayload`



<pre><code><b>public</b> <b>struct</b> <a href="../bridge/message.md#bridge_message_TokenTransferPayload">TokenTransferPayload</a> <b>has</b> drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>sender_address: vector&lt;u8&gt;</code>
</dt>
<dd>
</dd>
<dt>
<code>target_chain: u8</code>
</dt>
<dd>
</dd>
<dt>
<code>target_address: vector&lt;u8&gt;</code>
</dt>
<dd>
</dd>
<dt>
<code><a href="../bridge/message.md#bridge_message_token_type">token_type</a>: u8</code>
</dt>
<dd>
</dd>
<dt>
<code>amount: u64</code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="bridge_message_EmergencyOp"></a>

## Struct `EmergencyOp`



<pre><code><b>public</b> <b>struct</b> <a href="../bridge/message.md#bridge_message_EmergencyOp">EmergencyOp</a> <b>has</b> drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>op_type: u8</code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="bridge_message_Blocklist"></a>

## Struct `Blocklist`



<pre><code><b>public</b> <b>struct</b> <a href="../bridge/message.md#bridge_message_Blocklist">Blocklist</a> <b>has</b> drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code><a href="../bridge/message.md#bridge_message_blocklist_type">blocklist_type</a>: u8</code>
</dt>
<dd>
</dd>
<dt>
<code>validator_eth_addresses: vector&lt;vector&lt;u8&gt;&gt;</code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="bridge_message_UpdateBridgeLimit"></a>

## Struct `UpdateBridgeLimit`



<pre><code><b>public</b> <b>struct</b> <a href="../bridge/message.md#bridge_message_UpdateBridgeLimit">UpdateBridgeLimit</a> <b>has</b> drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>receiving_chain: u8</code>
</dt>
<dd>
</dd>
<dt>
<code>sending_chain: u8</code>
</dt>
<dd>
</dd>
<dt>
<code>limit: u64</code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="bridge_message_UpdateAssetPrice"></a>

## Struct `UpdateAssetPrice`



<pre><code><b>public</b> <b>struct</b> <a href="../bridge/message.md#bridge_message_UpdateAssetPrice">UpdateAssetPrice</a> <b>has</b> drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>token_id: u8</code>
</dt>
<dd>
</dd>
<dt>
<code>new_price: u64</code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="bridge_message_AddTokenOnSui"></a>

## Struct `AddTokenOnSui`



<pre><code><b>public</b> <b>struct</b> <a href="../bridge/message.md#bridge_message_AddTokenOnSui">AddTokenOnSui</a> <b>has</b> drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>native_token: bool</code>
</dt>
<dd>
</dd>
<dt>
<code><a href="../bridge/message.md#bridge_message_token_ids">token_ids</a>: vector&lt;u8&gt;</code>
</dt>
<dd>
</dd>
<dt>
<code><a href="../bridge/message.md#bridge_message_token_type_names">token_type_names</a>: vector&lt;<a href="../std/ascii.md#std_ascii_String">std::ascii::String</a>&gt;</code>
</dt>
<dd>
</dd>
<dt>
<code><a href="../bridge/message.md#bridge_message_token_prices">token_prices</a>: vector&lt;u64&gt;</code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="bridge_message_ParsedTokenTransferMessage"></a>

## Struct `ParsedTokenTransferMessage`



<pre><code><b>public</b> <b>struct</b> <a href="../bridge/message.md#bridge_message_ParsedTokenTransferMessage">ParsedTokenTransferMessage</a> <b>has</b> drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code><a href="../bridge/message.md#bridge_message_message_version">message_version</a>: u8</code>
</dt>
<dd>
</dd>
<dt>
<code><a href="../bridge/message.md#bridge_message_seq_num">seq_num</a>: u64</code>
</dt>
<dd>
</dd>
<dt>
<code><a href="../bridge/message.md#bridge_message_source_chain">source_chain</a>: u8</code>
</dt>
<dd>
</dd>
<dt>
<code><a href="../bridge/message.md#bridge_message_payload">payload</a>: vector&lt;u8&gt;</code>
</dt>
<dd>
</dd>
<dt>
<code>parsed_payload: <a href="../bridge/message.md#bridge_message_TokenTransferPayload">bridge::message::TokenTransferPayload</a></code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="bridge_message_CURRENT_MESSAGE_VERSION"></a>



<pre><code><b>const</b> <a href="../bridge/message.md#bridge_message_CURRENT_MESSAGE_VERSION">CURRENT_MESSAGE_VERSION</a>: u8 = 1;
</code></pre>



<a name="bridge_message_ECDSA_ADDRESS_LENGTH"></a>



<pre><code><b>const</b> <a href="../bridge/message.md#bridge_message_ECDSA_ADDRESS_LENGTH">ECDSA_ADDRESS_LENGTH</a>: u64 = 20;
</code></pre>



<a name="bridge_message_EEmptyList"></a>



<pre><code><b>const</b> <a href="../bridge/message.md#bridge_message_EEmptyList">EEmptyList</a>: u64 = 2;
</code></pre>



<a name="bridge_message_EInvalidAddressLength"></a>



<pre><code><b>const</b> <a href="../bridge/message.md#bridge_message_EInvalidAddressLength">EInvalidAddressLength</a>: u64 = 1;
</code></pre>



<a name="bridge_message_EInvalidEmergencyOpType"></a>



<pre><code><b>const</b> <a href="../bridge/message.md#bridge_message_EInvalidEmergencyOpType">EInvalidEmergencyOpType</a>: u64 = 4;
</code></pre>



<a name="bridge_message_EInvalidMessageType"></a>



<pre><code><b>const</b> <a href="../bridge/message.md#bridge_message_EInvalidMessageType">EInvalidMessageType</a>: u64 = 3;
</code></pre>



<a name="bridge_message_EInvalidPayloadLength"></a>



<pre><code><b>const</b> <a href="../bridge/message.md#bridge_message_EInvalidPayloadLength">EInvalidPayloadLength</a>: u64 = 5;
</code></pre>



<a name="bridge_message_EMustBeTokenMessage"></a>



<pre><code><b>const</b> <a href="../bridge/message.md#bridge_message_EMustBeTokenMessage">EMustBeTokenMessage</a>: u64 = 6;
</code></pre>



<a name="bridge_message_ETrailingBytes"></a>



<pre><code><b>const</b> <a href="../bridge/message.md#bridge_message_ETrailingBytes">ETrailingBytes</a>: u64 = 0;
</code></pre>



<a name="bridge_message_PAUSE"></a>



<pre><code><b>const</b> <a href="../bridge/message.md#bridge_message_PAUSE">PAUSE</a>: u8 = 0;
</code></pre>



<a name="bridge_message_UNPAUSE"></a>



<pre><code><b>const</b> <a href="../bridge/message.md#bridge_message_UNPAUSE">UNPAUSE</a>: u8 = 1;
</code></pre>



<a name="bridge_message_extract_token_bridge_payload"></a>

## Function `extract_token_bridge_payload`



<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_extract_token_bridge_payload">extract_token_bridge_payload</a>(<a href="../bridge/message.md#bridge_message">message</a>: &<a href="../bridge/message.md#bridge_message_BridgeMessage">bridge::message::BridgeMessage</a>): <a href="../bridge/message.md#bridge_message_TokenTransferPayload">bridge::message::TokenTransferPayload</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_extract_token_bridge_payload">extract_token_bridge_payload</a>(<a href="../bridge/message.md#bridge_message">message</a>: &<a href="../bridge/message.md#bridge_message_BridgeMessage">BridgeMessage</a>): <a href="../bridge/message.md#bridge_message_TokenTransferPayload">TokenTransferPayload</a> {
    <b>let</b> <b>mut</b> bcs = bcs::new(<a href="../bridge/message.md#bridge_message">message</a>.<a href="../bridge/message.md#bridge_message_payload">payload</a>);
    <b>let</b> sender_address = bcs.peel_vec_u8();
    <b>let</b> target_chain = bcs.peel_u8();
    <b>let</b> target_address = bcs.peel_vec_u8();
    <b>let</b> <a href="../bridge/message.md#bridge_message_token_type">token_type</a> = bcs.peel_u8();
    <b>let</b> amount = <a href="../bridge/message.md#bridge_message_peel_u64_be">peel_u64_be</a>(&<b>mut</b> bcs);
    <a href="../bridge/chain_ids.md#bridge_chain_ids_assert_valid_chain_id">chain_ids::assert_valid_chain_id</a>(target_chain);
    <b>assert</b>!(bcs.into_remainder_bytes().is_empty(), <a href="../bridge/message.md#bridge_message_ETrailingBytes">ETrailingBytes</a>);
    <a href="../bridge/message.md#bridge_message_TokenTransferPayload">TokenTransferPayload</a> {
        sender_address,
        target_chain,
        target_address,
        <a href="../bridge/message.md#bridge_message_token_type">token_type</a>,
        amount
    }
}
</code></pre>



</details>

<a name="bridge_message_extract_emergency_op_payload"></a>

## Function `extract_emergency_op_payload`

Emergency op payload is just a single byte


<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_extract_emergency_op_payload">extract_emergency_op_payload</a>(<a href="../bridge/message.md#bridge_message">message</a>: &<a href="../bridge/message.md#bridge_message_BridgeMessage">bridge::message::BridgeMessage</a>): <a href="../bridge/message.md#bridge_message_EmergencyOp">bridge::message::EmergencyOp</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_extract_emergency_op_payload">extract_emergency_op_payload</a>(<a href="../bridge/message.md#bridge_message">message</a>: &<a href="../bridge/message.md#bridge_message_BridgeMessage">BridgeMessage</a>): <a href="../bridge/message.md#bridge_message_EmergencyOp">EmergencyOp</a> {
    <b>assert</b>!(<a href="../bridge/message.md#bridge_message">message</a>.<a href="../bridge/message.md#bridge_message_payload">payload</a>.length() == 1, <a href="../bridge/message.md#bridge_message_ETrailingBytes">ETrailingBytes</a>);
    <a href="../bridge/message.md#bridge_message_EmergencyOp">EmergencyOp</a> { op_type: <a href="../bridge/message.md#bridge_message">message</a>.<a href="../bridge/message.md#bridge_message_payload">payload</a>[0] }
}
</code></pre>



</details>

<a name="bridge_message_extract_blocklist_payload"></a>

## Function `extract_blocklist_payload`



<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_extract_blocklist_payload">extract_blocklist_payload</a>(<a href="../bridge/message.md#bridge_message">message</a>: &<a href="../bridge/message.md#bridge_message_BridgeMessage">bridge::message::BridgeMessage</a>): <a href="../bridge/message.md#bridge_message_Blocklist">bridge::message::Blocklist</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_extract_blocklist_payload">extract_blocklist_payload</a>(<a href="../bridge/message.md#bridge_message">message</a>: &<a href="../bridge/message.md#bridge_message_BridgeMessage">BridgeMessage</a>): <a href="../bridge/message.md#bridge_message_Blocklist">Blocklist</a> {
    // blocklist <a href="../bridge/message.md#bridge_message_payload">payload</a> should consist of one byte blocklist type, and list of 20 bytes evm addresses
    // derived from ECDSA <b>public</b> keys
    <b>let</b> <b>mut</b> bcs = bcs::new(<a href="../bridge/message.md#bridge_message">message</a>.<a href="../bridge/message.md#bridge_message_payload">payload</a>);
    <b>let</b> <a href="../bridge/message.md#bridge_message_blocklist_type">blocklist_type</a> = bcs.peel_u8();
    <b>let</b> <b>mut</b> address_count = bcs.peel_u8();
    <b>assert</b>!(address_count != 0, <a href="../bridge/message.md#bridge_message_EEmptyList">EEmptyList</a>);
    <b>let</b> <b>mut</b> validator_eth_addresses = vector[];
    <b>while</b> (address_count &gt; 0) {
        <b>let</b> (<b>mut</b> <b>address</b>, <b>mut</b> i) = (vector[], 0);
        <b>while</b> (i &lt; <a href="../bridge/message.md#bridge_message_ECDSA_ADDRESS_LENGTH">ECDSA_ADDRESS_LENGTH</a>) {
            <b>address</b>.push_back(bcs.peel_u8());
            i = i + 1;
        };
        validator_eth_addresses.push_back(<b>address</b>);
        address_count = address_count - 1;
    };
    <b>assert</b>!(bcs.into_remainder_bytes().is_empty(), <a href="../bridge/message.md#bridge_message_ETrailingBytes">ETrailingBytes</a>);
    <a href="../bridge/message.md#bridge_message_Blocklist">Blocklist</a> {
        <a href="../bridge/message.md#bridge_message_blocklist_type">blocklist_type</a>,
        validator_eth_addresses
    }
}
</code></pre>



</details>

<a name="bridge_message_extract_update_bridge_limit"></a>

## Function `extract_update_bridge_limit`



<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_extract_update_bridge_limit">extract_update_bridge_limit</a>(<a href="../bridge/message.md#bridge_message">message</a>: &<a href="../bridge/message.md#bridge_message_BridgeMessage">bridge::message::BridgeMessage</a>): <a href="../bridge/message.md#bridge_message_UpdateBridgeLimit">bridge::message::UpdateBridgeLimit</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_extract_update_bridge_limit">extract_update_bridge_limit</a>(<a href="../bridge/message.md#bridge_message">message</a>: &<a href="../bridge/message.md#bridge_message_BridgeMessage">BridgeMessage</a>): <a href="../bridge/message.md#bridge_message_UpdateBridgeLimit">UpdateBridgeLimit</a> {
    <b>let</b> <b>mut</b> bcs = bcs::new(<a href="../bridge/message.md#bridge_message">message</a>.<a href="../bridge/message.md#bridge_message_payload">payload</a>);
    <b>let</b> sending_chain = bcs.peel_u8();
    <b>let</b> limit = <a href="../bridge/message.md#bridge_message_peel_u64_be">peel_u64_be</a>(&<b>mut</b> bcs);
    <a href="../bridge/chain_ids.md#bridge_chain_ids_assert_valid_chain_id">chain_ids::assert_valid_chain_id</a>(sending_chain);
    <b>assert</b>!(bcs.into_remainder_bytes().is_empty(), <a href="../bridge/message.md#bridge_message_ETrailingBytes">ETrailingBytes</a>);
    <a href="../bridge/message.md#bridge_message_UpdateBridgeLimit">UpdateBridgeLimit</a> {
        receiving_chain: <a href="../bridge/message.md#bridge_message">message</a>.<a href="../bridge/message.md#bridge_message_source_chain">source_chain</a>,
        sending_chain,
        limit
    }
}
</code></pre>



</details>

<a name="bridge_message_extract_update_asset_price"></a>

## Function `extract_update_asset_price`



<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_extract_update_asset_price">extract_update_asset_price</a>(<a href="../bridge/message.md#bridge_message">message</a>: &<a href="../bridge/message.md#bridge_message_BridgeMessage">bridge::message::BridgeMessage</a>): <a href="../bridge/message.md#bridge_message_UpdateAssetPrice">bridge::message::UpdateAssetPrice</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_extract_update_asset_price">extract_update_asset_price</a>(<a href="../bridge/message.md#bridge_message">message</a>: &<a href="../bridge/message.md#bridge_message_BridgeMessage">BridgeMessage</a>): <a href="../bridge/message.md#bridge_message_UpdateAssetPrice">UpdateAssetPrice</a> {
    <b>let</b> <b>mut</b> bcs = bcs::new(<a href="../bridge/message.md#bridge_message">message</a>.<a href="../bridge/message.md#bridge_message_payload">payload</a>);
    <b>let</b> token_id = bcs.peel_u8();
    <b>let</b> new_price = <a href="../bridge/message.md#bridge_message_peel_u64_be">peel_u64_be</a>(&<b>mut</b> bcs);
    <b>assert</b>!(bcs.into_remainder_bytes().is_empty(), <a href="../bridge/message.md#bridge_message_ETrailingBytes">ETrailingBytes</a>);
    <a href="../bridge/message.md#bridge_message_UpdateAssetPrice">UpdateAssetPrice</a> {
        token_id,
        new_price
    }
}
</code></pre>



</details>

<a name="bridge_message_extract_add_tokens_on_sui"></a>

## Function `extract_add_tokens_on_sui`



<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_extract_add_tokens_on_sui">extract_add_tokens_on_sui</a>(<a href="../bridge/message.md#bridge_message">message</a>: &<a href="../bridge/message.md#bridge_message_BridgeMessage">bridge::message::BridgeMessage</a>): <a href="../bridge/message.md#bridge_message_AddTokenOnSui">bridge::message::AddTokenOnSui</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_extract_add_tokens_on_sui">extract_add_tokens_on_sui</a>(<a href="../bridge/message.md#bridge_message">message</a>: &<a href="../bridge/message.md#bridge_message_BridgeMessage">BridgeMessage</a>): <a href="../bridge/message.md#bridge_message_AddTokenOnSui">AddTokenOnSui</a> {
    <b>let</b> <b>mut</b> bcs = bcs::new(<a href="../bridge/message.md#bridge_message">message</a>.<a href="../bridge/message.md#bridge_message_payload">payload</a>);
    <b>let</b> native_token = bcs.peel_bool();
    <b>let</b> <a href="../bridge/message.md#bridge_message_token_ids">token_ids</a> = bcs.peel_vec_u8();
    <b>let</b> token_type_names_bytes = bcs.peel_vec_vec_u8();
    <b>let</b> <a href="../bridge/message.md#bridge_message_token_prices">token_prices</a> = bcs.peel_vec_u64();
    <b>let</b> <b>mut</b> n = 0;
    <b>let</b> <b>mut</b> <a href="../bridge/message.md#bridge_message_token_type_names">token_type_names</a> = vector[];
    <b>while</b> (n &lt; token_type_names_bytes.length()){
        <a href="../bridge/message.md#bridge_message_token_type_names">token_type_names</a>.push_back(ascii::string(*token_type_names_bytes.borrow(n)));
        n = n + 1;
    };
    <b>assert</b>!(bcs.into_remainder_bytes().is_empty(), <a href="../bridge/message.md#bridge_message_ETrailingBytes">ETrailingBytes</a>);
    <a href="../bridge/message.md#bridge_message_AddTokenOnSui">AddTokenOnSui</a> {
        native_token,
        <a href="../bridge/message.md#bridge_message_token_ids">token_ids</a>,
        <a href="../bridge/message.md#bridge_message_token_type_names">token_type_names</a>,
        <a href="../bridge/message.md#bridge_message_token_prices">token_prices</a>
    }
}
</code></pre>



</details>

<a name="bridge_message_serialize_message"></a>

## Function `serialize_message`



<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_serialize_message">serialize_message</a>(<a href="../bridge/message.md#bridge_message">message</a>: <a href="../bridge/message.md#bridge_message_BridgeMessage">bridge::message::BridgeMessage</a>): vector&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_serialize_message">serialize_message</a>(<a href="../bridge/message.md#bridge_message">message</a>: <a href="../bridge/message.md#bridge_message_BridgeMessage">BridgeMessage</a>): vector&lt;u8&gt; {
    <b>let</b> <a href="../bridge/message.md#bridge_message_BridgeMessage">BridgeMessage</a> {
        <a href="../bridge/message.md#bridge_message_message_type">message_type</a>,
        <a href="../bridge/message.md#bridge_message_message_version">message_version</a>,
        <a href="../bridge/message.md#bridge_message_seq_num">seq_num</a>,
        <a href="../bridge/message.md#bridge_message_source_chain">source_chain</a>,
        <a href="../bridge/message.md#bridge_message_payload">payload</a>
    } = <a href="../bridge/message.md#bridge_message">message</a>;
    <b>let</b> <b>mut</b> <a href="../bridge/message.md#bridge_message">message</a> = vector[
        <a href="../bridge/message.md#bridge_message_message_type">message_type</a>,
        <a href="../bridge/message.md#bridge_message_message_version">message_version</a>,
    ];
    // bcs serializes u64 <b>as</b> 8 bytes
    <a href="../bridge/message.md#bridge_message">message</a>.append(<a href="../bridge/message.md#bridge_message_reverse_bytes">reverse_bytes</a>(bcs::to_bytes(&<a href="../bridge/message.md#bridge_message_seq_num">seq_num</a>)));
    <a href="../bridge/message.md#bridge_message">message</a>.push_back(<a href="../bridge/message.md#bridge_message_source_chain">source_chain</a>);
    <a href="../bridge/message.md#bridge_message">message</a>.append(<a href="../bridge/message.md#bridge_message_payload">payload</a>);
    <a href="../bridge/message.md#bridge_message">message</a>
}
</code></pre>



</details>

<a name="bridge_message_create_token_bridge_message"></a>

## Function `create_token_bridge_message`

Token Transfer Message Format:
[message_type: u8]
[version:u8]
[nonce:u64]
[source_chain: u8]
[sender_address_length:u8]
[sender_address: byte[]]
[target_chain:u8]
[target_address_length:u8]
[target_address: byte[]]
[token_type:u8]
[amount:u64]


<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_create_token_bridge_message">create_token_bridge_message</a>(<a href="../bridge/message.md#bridge_message_source_chain">source_chain</a>: u8, <a href="../bridge/message.md#bridge_message_seq_num">seq_num</a>: u64, sender_address: vector&lt;u8&gt;, target_chain: u8, target_address: vector&lt;u8&gt;, <a href="../bridge/message.md#bridge_message_token_type">token_type</a>: u8, amount: u64): <a href="../bridge/message.md#bridge_message_BridgeMessage">bridge::message::BridgeMessage</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_create_token_bridge_message">create_token_bridge_message</a>(
    <a href="../bridge/message.md#bridge_message_source_chain">source_chain</a>: u8,
    <a href="../bridge/message.md#bridge_message_seq_num">seq_num</a>: u64,
    sender_address: vector&lt;u8&gt;,
    target_chain: u8,
    target_address: vector&lt;u8&gt;,
    <a href="../bridge/message.md#bridge_message_token_type">token_type</a>: u8,
    amount: u64
): <a href="../bridge/message.md#bridge_message_BridgeMessage">BridgeMessage</a> {
    <a href="../bridge/chain_ids.md#bridge_chain_ids_assert_valid_chain_id">chain_ids::assert_valid_chain_id</a>(<a href="../bridge/message.md#bridge_message_source_chain">source_chain</a>);
    <a href="../bridge/chain_ids.md#bridge_chain_ids_assert_valid_chain_id">chain_ids::assert_valid_chain_id</a>(target_chain);
    <b>let</b> <b>mut</b> <a href="../bridge/message.md#bridge_message_payload">payload</a> = vector[];
    // sender <b>address</b> should be less than 255 bytes so can fit into u8
    <a href="../bridge/message.md#bridge_message_payload">payload</a>.push_back((vector::length(&sender_address) <b>as</b> u8));
    <a href="../bridge/message.md#bridge_message_payload">payload</a>.append(sender_address);
    <a href="../bridge/message.md#bridge_message_payload">payload</a>.push_back(target_chain);
    // target <b>address</b> should be less than 255 bytes so can fit into u8
    <a href="../bridge/message.md#bridge_message_payload">payload</a>.push_back((vector::length(&target_address) <b>as</b> u8));
    <a href="../bridge/message.md#bridge_message_payload">payload</a>.append(target_address);
    <a href="../bridge/message.md#bridge_message_payload">payload</a>.push_back(<a href="../bridge/message.md#bridge_message_token_type">token_type</a>);
    // bcs serialzies u64 <b>as</b> 8 bytes
    <a href="../bridge/message.md#bridge_message_payload">payload</a>.append(<a href="../bridge/message.md#bridge_message_reverse_bytes">reverse_bytes</a>(bcs::to_bytes(&amount)));
    <b>assert</b>!(vector::length(&<a href="../bridge/message.md#bridge_message_payload">payload</a>) == 64, <a href="../bridge/message.md#bridge_message_EInvalidPayloadLength">EInvalidPayloadLength</a>);
    <a href="../bridge/message.md#bridge_message_BridgeMessage">BridgeMessage</a> {
        <a href="../bridge/message.md#bridge_message_message_type">message_type</a>: <a href="../bridge/message_types.md#bridge_message_types_token">message_types::token</a>(),
        <a href="../bridge/message.md#bridge_message_message_version">message_version</a>: <a href="../bridge/message.md#bridge_message_CURRENT_MESSAGE_VERSION">CURRENT_MESSAGE_VERSION</a>,
        <a href="../bridge/message.md#bridge_message_seq_num">seq_num</a>,
        <a href="../bridge/message.md#bridge_message_source_chain">source_chain</a>,
        <a href="../bridge/message.md#bridge_message_payload">payload</a>,
    }
}
</code></pre>



</details>

<a name="bridge_message_create_emergency_op_message"></a>

## Function `create_emergency_op_message`

Emergency Op Message Format:
[message_type: u8]
[version:u8]
[nonce:u64]
[chain_id: u8]
[op_type: u8]


<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_create_emergency_op_message">create_emergency_op_message</a>(<a href="../bridge/message.md#bridge_message_source_chain">source_chain</a>: u8, <a href="../bridge/message.md#bridge_message_seq_num">seq_num</a>: u64, op_type: u8): <a href="../bridge/message.md#bridge_message_BridgeMessage">bridge::message::BridgeMessage</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_create_emergency_op_message">create_emergency_op_message</a>(
    <a href="../bridge/message.md#bridge_message_source_chain">source_chain</a>: u8,
    <a href="../bridge/message.md#bridge_message_seq_num">seq_num</a>: u64,
    op_type: u8,
): <a href="../bridge/message.md#bridge_message_BridgeMessage">BridgeMessage</a> {
    <a href="../bridge/chain_ids.md#bridge_chain_ids_assert_valid_chain_id">chain_ids::assert_valid_chain_id</a>(<a href="../bridge/message.md#bridge_message_source_chain">source_chain</a>);
    <a href="../bridge/message.md#bridge_message_BridgeMessage">BridgeMessage</a> {
        <a href="../bridge/message.md#bridge_message_message_type">message_type</a>: <a href="../bridge/message_types.md#bridge_message_types_emergency_op">message_types::emergency_op</a>(),
        <a href="../bridge/message.md#bridge_message_message_version">message_version</a>: <a href="../bridge/message.md#bridge_message_CURRENT_MESSAGE_VERSION">CURRENT_MESSAGE_VERSION</a>,
        <a href="../bridge/message.md#bridge_message_seq_num">seq_num</a>,
        <a href="../bridge/message.md#bridge_message_source_chain">source_chain</a>,
        <a href="../bridge/message.md#bridge_message_payload">payload</a>: vector[op_type],
    }
}
</code></pre>



</details>

<a name="bridge_message_create_blocklist_message"></a>

## Function `create_blocklist_message`

Blocklist Message Format:
[message_type: u8]
[version:u8]
[nonce:u64]
[chain_id: u8]
[blocklist_type: u8]
[validator_length: u8]
[validator_ecdsa_addresses: byte[][]]


<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_create_blocklist_message">create_blocklist_message</a>(<a href="../bridge/message.md#bridge_message_source_chain">source_chain</a>: u8, <a href="../bridge/message.md#bridge_message_seq_num">seq_num</a>: u64, <a href="../bridge/message.md#bridge_message_blocklist_type">blocklist_type</a>: u8, validator_ecdsa_addresses: vector&lt;vector&lt;u8&gt;&gt;): <a href="../bridge/message.md#bridge_message_BridgeMessage">bridge::message::BridgeMessage</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_create_blocklist_message">create_blocklist_message</a>(
    <a href="../bridge/message.md#bridge_message_source_chain">source_chain</a>: u8,
    <a href="../bridge/message.md#bridge_message_seq_num">seq_num</a>: u64,
    // 0: block, 1: unblock
    <a href="../bridge/message.md#bridge_message_blocklist_type">blocklist_type</a>: u8,
    validator_ecdsa_addresses: vector&lt;vector&lt;u8&gt;&gt;,
): <a href="../bridge/message.md#bridge_message_BridgeMessage">BridgeMessage</a> {
    <a href="../bridge/chain_ids.md#bridge_chain_ids_assert_valid_chain_id">chain_ids::assert_valid_chain_id</a>(<a href="../bridge/message.md#bridge_message_source_chain">source_chain</a>);
    <b>let</b> address_length = validator_ecdsa_addresses.length();
    <b>let</b> <b>mut</b> <a href="../bridge/message.md#bridge_message_payload">payload</a> = vector[<a href="../bridge/message.md#bridge_message_blocklist_type">blocklist_type</a>, (address_length <b>as</b> u8)];
    <b>let</b> <b>mut</b> i = 0;
    <b>while</b> (i &lt; address_length) {
        <b>let</b> <b>address</b> = validator_ecdsa_addresses[i];
        <b>assert</b>!(<b>address</b>.length() == <a href="../bridge/message.md#bridge_message_ECDSA_ADDRESS_LENGTH">ECDSA_ADDRESS_LENGTH</a>, <a href="../bridge/message.md#bridge_message_EInvalidAddressLength">EInvalidAddressLength</a>);
        <a href="../bridge/message.md#bridge_message_payload">payload</a>.append(<b>address</b>);
        i = i + 1;
    };
    <a href="../bridge/message.md#bridge_message_BridgeMessage">BridgeMessage</a> {
        <a href="../bridge/message.md#bridge_message_message_type">message_type</a>: <a href="../bridge/message_types.md#bridge_message_types_committee_blocklist">message_types::committee_blocklist</a>(),
        <a href="../bridge/message.md#bridge_message_message_version">message_version</a>: <a href="../bridge/message.md#bridge_message_CURRENT_MESSAGE_VERSION">CURRENT_MESSAGE_VERSION</a>,
        <a href="../bridge/message.md#bridge_message_seq_num">seq_num</a>,
        <a href="../bridge/message.md#bridge_message_source_chain">source_chain</a>,
        <a href="../bridge/message.md#bridge_message_payload">payload</a>,
    }
}
</code></pre>



</details>

<a name="bridge_message_create_update_bridge_limit_message"></a>

## Function `create_update_bridge_limit_message`

Update bridge limit Message Format:
[message_type: u8]
[version:u8]
[nonce:u64]
[receiving_chain_id: u8]
[sending_chain_id: u8]
[new_limit: u64]


<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_create_update_bridge_limit_message">create_update_bridge_limit_message</a>(receiving_chain: u8, <a href="../bridge/message.md#bridge_message_seq_num">seq_num</a>: u64, sending_chain: u8, new_limit: u64): <a href="../bridge/message.md#bridge_message_BridgeMessage">bridge::message::BridgeMessage</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_create_update_bridge_limit_message">create_update_bridge_limit_message</a>(
    receiving_chain: u8,
    <a href="../bridge/message.md#bridge_message_seq_num">seq_num</a>: u64,
    sending_chain: u8,
    new_limit: u64,
): <a href="../bridge/message.md#bridge_message_BridgeMessage">BridgeMessage</a> {
    <a href="../bridge/chain_ids.md#bridge_chain_ids_assert_valid_chain_id">chain_ids::assert_valid_chain_id</a>(receiving_chain);
    <a href="../bridge/chain_ids.md#bridge_chain_ids_assert_valid_chain_id">chain_ids::assert_valid_chain_id</a>(sending_chain);
    <b>let</b> <b>mut</b> <a href="../bridge/message.md#bridge_message_payload">payload</a> = vector[sending_chain];
    <a href="../bridge/message.md#bridge_message_payload">payload</a>.append(<a href="../bridge/message.md#bridge_message_reverse_bytes">reverse_bytes</a>(bcs::to_bytes(&new_limit)));
    <a href="../bridge/message.md#bridge_message_BridgeMessage">BridgeMessage</a> {
        <a href="../bridge/message.md#bridge_message_message_type">message_type</a>: <a href="../bridge/message_types.md#bridge_message_types_update_bridge_limit">message_types::update_bridge_limit</a>(),
        <a href="../bridge/message.md#bridge_message_message_version">message_version</a>: <a href="../bridge/message.md#bridge_message_CURRENT_MESSAGE_VERSION">CURRENT_MESSAGE_VERSION</a>,
        <a href="../bridge/message.md#bridge_message_seq_num">seq_num</a>,
        <a href="../bridge/message.md#bridge_message_source_chain">source_chain</a>: receiving_chain,
        <a href="../bridge/message.md#bridge_message_payload">payload</a>,
    }
}
</code></pre>



</details>

<a name="bridge_message_create_update_asset_price_message"></a>

## Function `create_update_asset_price_message`

Update asset price message
[message_type: u8]
[version:u8]
[nonce:u64]
[chain_id: u8]
[token_id: u8]
[new_price:u64]


<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_create_update_asset_price_message">create_update_asset_price_message</a>(token_id: u8, <a href="../bridge/message.md#bridge_message_source_chain">source_chain</a>: u8, <a href="../bridge/message.md#bridge_message_seq_num">seq_num</a>: u64, new_price: u64): <a href="../bridge/message.md#bridge_message_BridgeMessage">bridge::message::BridgeMessage</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_create_update_asset_price_message">create_update_asset_price_message</a>(
    token_id: u8,
    <a href="../bridge/message.md#bridge_message_source_chain">source_chain</a>: u8,
    <a href="../bridge/message.md#bridge_message_seq_num">seq_num</a>: u64,
    new_price: u64,
): <a href="../bridge/message.md#bridge_message_BridgeMessage">BridgeMessage</a> {
    <a href="../bridge/chain_ids.md#bridge_chain_ids_assert_valid_chain_id">chain_ids::assert_valid_chain_id</a>(<a href="../bridge/message.md#bridge_message_source_chain">source_chain</a>);
    <b>let</b> <b>mut</b> <a href="../bridge/message.md#bridge_message_payload">payload</a> = vector[token_id];
    <a href="../bridge/message.md#bridge_message_payload">payload</a>.append(<a href="../bridge/message.md#bridge_message_reverse_bytes">reverse_bytes</a>(bcs::to_bytes(&new_price)));
    <a href="../bridge/message.md#bridge_message_BridgeMessage">BridgeMessage</a> {
        <a href="../bridge/message.md#bridge_message_message_type">message_type</a>: <a href="../bridge/message_types.md#bridge_message_types_update_asset_price">message_types::update_asset_price</a>(),
        <a href="../bridge/message.md#bridge_message_message_version">message_version</a>: <a href="../bridge/message.md#bridge_message_CURRENT_MESSAGE_VERSION">CURRENT_MESSAGE_VERSION</a>,
        <a href="../bridge/message.md#bridge_message_seq_num">seq_num</a>,
        <a href="../bridge/message.md#bridge_message_source_chain">source_chain</a>,
        <a href="../bridge/message.md#bridge_message_payload">payload</a>,
    }
}
</code></pre>



</details>

<a name="bridge_message_create_add_tokens_on_sui_message"></a>

## Function `create_add_tokens_on_sui_message`

Update Sui token message
[message_type:u8]
[version:u8]
[nonce:u64]
[chain_id: u8]
[native_token:bool]
[token_ids:vector<u8>]
[token_type_name:vector<String>]
[token_prices:vector<u64>]


<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_create_add_tokens_on_sui_message">create_add_tokens_on_sui_message</a>(<a href="../bridge/message.md#bridge_message_source_chain">source_chain</a>: u8, <a href="../bridge/message.md#bridge_message_seq_num">seq_num</a>: u64, native_token: bool, <a href="../bridge/message.md#bridge_message_token_ids">token_ids</a>: vector&lt;u8&gt;, type_names: vector&lt;<a href="../std/ascii.md#std_ascii_String">std::ascii::String</a>&gt;, <a href="../bridge/message.md#bridge_message_token_prices">token_prices</a>: vector&lt;u64&gt;): <a href="../bridge/message.md#bridge_message_BridgeMessage">bridge::message::BridgeMessage</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_create_add_tokens_on_sui_message">create_add_tokens_on_sui_message</a>(
    <a href="../bridge/message.md#bridge_message_source_chain">source_chain</a>: u8,
    <a href="../bridge/message.md#bridge_message_seq_num">seq_num</a>: u64,
    native_token: bool,
    <a href="../bridge/message.md#bridge_message_token_ids">token_ids</a>: vector&lt;u8&gt;,
    type_names: vector&lt;String&gt;,
    <a href="../bridge/message.md#bridge_message_token_prices">token_prices</a>: vector&lt;u64&gt;,
): <a href="../bridge/message.md#bridge_message_BridgeMessage">BridgeMessage</a> {
    <a href="../bridge/chain_ids.md#bridge_chain_ids_assert_valid_chain_id">chain_ids::assert_valid_chain_id</a>(<a href="../bridge/message.md#bridge_message_source_chain">source_chain</a>);
    <b>let</b> <b>mut</b> <a href="../bridge/message.md#bridge_message_payload">payload</a> = bcs::to_bytes(&native_token);
    <a href="../bridge/message.md#bridge_message_payload">payload</a>.append(bcs::to_bytes(&<a href="../bridge/message.md#bridge_message_token_ids">token_ids</a>));
    <a href="../bridge/message.md#bridge_message_payload">payload</a>.append(bcs::to_bytes(&type_names));
    <a href="../bridge/message.md#bridge_message_payload">payload</a>.append(bcs::to_bytes(&<a href="../bridge/message.md#bridge_message_token_prices">token_prices</a>));
    <a href="../bridge/message.md#bridge_message_BridgeMessage">BridgeMessage</a> {
        <a href="../bridge/message.md#bridge_message_message_type">message_type</a>: <a href="../bridge/message_types.md#bridge_message_types_add_tokens_on_sui">message_types::add_tokens_on_sui</a>(),
        <a href="../bridge/message.md#bridge_message_message_version">message_version</a>: <a href="../bridge/message.md#bridge_message_CURRENT_MESSAGE_VERSION">CURRENT_MESSAGE_VERSION</a>,
        <a href="../bridge/message.md#bridge_message_seq_num">seq_num</a>,
        <a href="../bridge/message.md#bridge_message_source_chain">source_chain</a>,
        <a href="../bridge/message.md#bridge_message_payload">payload</a>,
    }
}
</code></pre>



</details>

<a name="bridge_message_create_key"></a>

## Function `create_key`



<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_create_key">create_key</a>(<a href="../bridge/message.md#bridge_message_source_chain">source_chain</a>: u8, <a href="../bridge/message.md#bridge_message_message_type">message_type</a>: u8, bridge_seq_num: u64): <a href="../bridge/message.md#bridge_message_BridgeMessageKey">bridge::message::BridgeMessageKey</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_create_key">create_key</a>(<a href="../bridge/message.md#bridge_message_source_chain">source_chain</a>: u8, <a href="../bridge/message.md#bridge_message_message_type">message_type</a>: u8, bridge_seq_num: u64): <a href="../bridge/message.md#bridge_message_BridgeMessageKey">BridgeMessageKey</a> {
    <a href="../bridge/message.md#bridge_message_BridgeMessageKey">BridgeMessageKey</a> { <a href="../bridge/message.md#bridge_message_source_chain">source_chain</a>, <a href="../bridge/message.md#bridge_message_message_type">message_type</a>, bridge_seq_num }
}
</code></pre>



</details>

<a name="bridge_message_key"></a>

## Function `key`



<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_key">key</a>(self: &<a href="../bridge/message.md#bridge_message_BridgeMessage">bridge::message::BridgeMessage</a>): <a href="../bridge/message.md#bridge_message_BridgeMessageKey">bridge::message::BridgeMessageKey</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_key">key</a>(self: &<a href="../bridge/message.md#bridge_message_BridgeMessage">BridgeMessage</a>): <a href="../bridge/message.md#bridge_message_BridgeMessageKey">BridgeMessageKey</a> {
    <a href="../bridge/message.md#bridge_message_create_key">create_key</a>(self.<a href="../bridge/message.md#bridge_message_source_chain">source_chain</a>, self.<a href="../bridge/message.md#bridge_message_message_type">message_type</a>, self.<a href="../bridge/message.md#bridge_message_seq_num">seq_num</a>)
}
</code></pre>



</details>

<a name="bridge_message_message_version"></a>

## Function `message_version`



<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_message_version">message_version</a>(self: &<a href="../bridge/message.md#bridge_message_BridgeMessage">bridge::message::BridgeMessage</a>): u8
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_message_version">message_version</a>(self: &<a href="../bridge/message.md#bridge_message_BridgeMessage">BridgeMessage</a>): u8 {
    self.<a href="../bridge/message.md#bridge_message_message_version">message_version</a>
}
</code></pre>



</details>

<a name="bridge_message_message_type"></a>

## Function `message_type`



<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_message_type">message_type</a>(self: &<a href="../bridge/message.md#bridge_message_BridgeMessage">bridge::message::BridgeMessage</a>): u8
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_message_type">message_type</a>(self: &<a href="../bridge/message.md#bridge_message_BridgeMessage">BridgeMessage</a>): u8 {
    self.<a href="../bridge/message.md#bridge_message_message_type">message_type</a>
}
</code></pre>



</details>

<a name="bridge_message_seq_num"></a>

## Function `seq_num`



<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_seq_num">seq_num</a>(self: &<a href="../bridge/message.md#bridge_message_BridgeMessage">bridge::message::BridgeMessage</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_seq_num">seq_num</a>(self: &<a href="../bridge/message.md#bridge_message_BridgeMessage">BridgeMessage</a>): u64 {
    self.<a href="../bridge/message.md#bridge_message_seq_num">seq_num</a>
}
</code></pre>



</details>

<a name="bridge_message_source_chain"></a>

## Function `source_chain`



<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_source_chain">source_chain</a>(self: &<a href="../bridge/message.md#bridge_message_BridgeMessage">bridge::message::BridgeMessage</a>): u8
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_source_chain">source_chain</a>(self: &<a href="../bridge/message.md#bridge_message_BridgeMessage">BridgeMessage</a>): u8 {
    self.<a href="../bridge/message.md#bridge_message_source_chain">source_chain</a>
}
</code></pre>



</details>

<a name="bridge_message_payload"></a>

## Function `payload`



<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_payload">payload</a>(self: &<a href="../bridge/message.md#bridge_message_BridgeMessage">bridge::message::BridgeMessage</a>): vector&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_payload">payload</a>(self: &<a href="../bridge/message.md#bridge_message_BridgeMessage">BridgeMessage</a>): vector&lt;u8&gt; {
    self.<a href="../bridge/message.md#bridge_message_payload">payload</a>
}
</code></pre>



</details>

<a name="bridge_message_token_target_chain"></a>

## Function `token_target_chain`



<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_token_target_chain">token_target_chain</a>(self: &<a href="../bridge/message.md#bridge_message_TokenTransferPayload">bridge::message::TokenTransferPayload</a>): u8
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_token_target_chain">token_target_chain</a>(self: &<a href="../bridge/message.md#bridge_message_TokenTransferPayload">TokenTransferPayload</a>): u8 {
    self.target_chain
}
</code></pre>



</details>

<a name="bridge_message_token_target_address"></a>

## Function `token_target_address`



<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_token_target_address">token_target_address</a>(self: &<a href="../bridge/message.md#bridge_message_TokenTransferPayload">bridge::message::TokenTransferPayload</a>): vector&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_token_target_address">token_target_address</a>(self: &<a href="../bridge/message.md#bridge_message_TokenTransferPayload">TokenTransferPayload</a>): vector&lt;u8&gt; {
    self.target_address
}
</code></pre>



</details>

<a name="bridge_message_token_type"></a>

## Function `token_type`



<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_token_type">token_type</a>(self: &<a href="../bridge/message.md#bridge_message_TokenTransferPayload">bridge::message::TokenTransferPayload</a>): u8
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_token_type">token_type</a>(self: &<a href="../bridge/message.md#bridge_message_TokenTransferPayload">TokenTransferPayload</a>): u8 {
    self.<a href="../bridge/message.md#bridge_message_token_type">token_type</a>
}
</code></pre>



</details>

<a name="bridge_message_token_amount"></a>

## Function `token_amount`



<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_token_amount">token_amount</a>(self: &<a href="../bridge/message.md#bridge_message_TokenTransferPayload">bridge::message::TokenTransferPayload</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_token_amount">token_amount</a>(self: &<a href="../bridge/message.md#bridge_message_TokenTransferPayload">TokenTransferPayload</a>): u64 {
    self.amount
}
</code></pre>



</details>

<a name="bridge_message_emergency_op_type"></a>

## Function `emergency_op_type`



<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_emergency_op_type">emergency_op_type</a>(self: &<a href="../bridge/message.md#bridge_message_EmergencyOp">bridge::message::EmergencyOp</a>): u8
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_emergency_op_type">emergency_op_type</a>(self: &<a href="../bridge/message.md#bridge_message_EmergencyOp">EmergencyOp</a>): u8 {
    self.op_type
}
</code></pre>



</details>

<a name="bridge_message_blocklist_type"></a>

## Function `blocklist_type`



<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_blocklist_type">blocklist_type</a>(self: &<a href="../bridge/message.md#bridge_message_Blocklist">bridge::message::Blocklist</a>): u8
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_blocklist_type">blocklist_type</a>(self: &<a href="../bridge/message.md#bridge_message_Blocklist">Blocklist</a>): u8 {
    self.<a href="../bridge/message.md#bridge_message_blocklist_type">blocklist_type</a>
}
</code></pre>



</details>

<a name="bridge_message_blocklist_validator_addresses"></a>

## Function `blocklist_validator_addresses`



<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_blocklist_validator_addresses">blocklist_validator_addresses</a>(self: &<a href="../bridge/message.md#bridge_message_Blocklist">bridge::message::Blocklist</a>): &vector&lt;vector&lt;u8&gt;&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_blocklist_validator_addresses">blocklist_validator_addresses</a>(self: &<a href="../bridge/message.md#bridge_message_Blocklist">Blocklist</a>): &vector&lt;vector&lt;u8&gt;&gt; {
    &self.validator_eth_addresses
}
</code></pre>



</details>

<a name="bridge_message_update_bridge_limit_payload_sending_chain"></a>

## Function `update_bridge_limit_payload_sending_chain`



<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_update_bridge_limit_payload_sending_chain">update_bridge_limit_payload_sending_chain</a>(self: &<a href="../bridge/message.md#bridge_message_UpdateBridgeLimit">bridge::message::UpdateBridgeLimit</a>): u8
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_update_bridge_limit_payload_sending_chain">update_bridge_limit_payload_sending_chain</a>(self: &<a href="../bridge/message.md#bridge_message_UpdateBridgeLimit">UpdateBridgeLimit</a>): u8 {
    self.sending_chain
}
</code></pre>



</details>

<a name="bridge_message_update_bridge_limit_payload_receiving_chain"></a>

## Function `update_bridge_limit_payload_receiving_chain`



<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_update_bridge_limit_payload_receiving_chain">update_bridge_limit_payload_receiving_chain</a>(self: &<a href="../bridge/message.md#bridge_message_UpdateBridgeLimit">bridge::message::UpdateBridgeLimit</a>): u8
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_update_bridge_limit_payload_receiving_chain">update_bridge_limit_payload_receiving_chain</a>(self: &<a href="../bridge/message.md#bridge_message_UpdateBridgeLimit">UpdateBridgeLimit</a>): u8 {
    self.receiving_chain
}
</code></pre>



</details>

<a name="bridge_message_update_bridge_limit_payload_limit"></a>

## Function `update_bridge_limit_payload_limit`



<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_update_bridge_limit_payload_limit">update_bridge_limit_payload_limit</a>(self: &<a href="../bridge/message.md#bridge_message_UpdateBridgeLimit">bridge::message::UpdateBridgeLimit</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_update_bridge_limit_payload_limit">update_bridge_limit_payload_limit</a>(self: &<a href="../bridge/message.md#bridge_message_UpdateBridgeLimit">UpdateBridgeLimit</a>): u64 {
    self.limit
}
</code></pre>



</details>

<a name="bridge_message_update_asset_price_payload_token_id"></a>

## Function `update_asset_price_payload_token_id`



<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_update_asset_price_payload_token_id">update_asset_price_payload_token_id</a>(self: &<a href="../bridge/message.md#bridge_message_UpdateAssetPrice">bridge::message::UpdateAssetPrice</a>): u8
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_update_asset_price_payload_token_id">update_asset_price_payload_token_id</a>(self: &<a href="../bridge/message.md#bridge_message_UpdateAssetPrice">UpdateAssetPrice</a>): u8 {
    self.token_id
}
</code></pre>



</details>

<a name="bridge_message_update_asset_price_payload_new_price"></a>

## Function `update_asset_price_payload_new_price`



<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_update_asset_price_payload_new_price">update_asset_price_payload_new_price</a>(self: &<a href="../bridge/message.md#bridge_message_UpdateAssetPrice">bridge::message::UpdateAssetPrice</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_update_asset_price_payload_new_price">update_asset_price_payload_new_price</a>(self: &<a href="../bridge/message.md#bridge_message_UpdateAssetPrice">UpdateAssetPrice</a>): u64 {
    self.new_price
}
</code></pre>



</details>

<a name="bridge_message_is_native"></a>

## Function `is_native`



<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_is_native">is_native</a>(self: &<a href="../bridge/message.md#bridge_message_AddTokenOnSui">bridge::message::AddTokenOnSui</a>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_is_native">is_native</a>(self: &<a href="../bridge/message.md#bridge_message_AddTokenOnSui">AddTokenOnSui</a>): bool {
    self.native_token
}
</code></pre>



</details>

<a name="bridge_message_token_ids"></a>

## Function `token_ids`



<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_token_ids">token_ids</a>(self: &<a href="../bridge/message.md#bridge_message_AddTokenOnSui">bridge::message::AddTokenOnSui</a>): vector&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_token_ids">token_ids</a>(self: &<a href="../bridge/message.md#bridge_message_AddTokenOnSui">AddTokenOnSui</a>): vector&lt;u8&gt; {
    self.<a href="../bridge/message.md#bridge_message_token_ids">token_ids</a>
}
</code></pre>



</details>

<a name="bridge_message_token_type_names"></a>

## Function `token_type_names`



<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_token_type_names">token_type_names</a>(self: &<a href="../bridge/message.md#bridge_message_AddTokenOnSui">bridge::message::AddTokenOnSui</a>): vector&lt;<a href="../std/ascii.md#std_ascii_String">std::ascii::String</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_token_type_names">token_type_names</a>(self: &<a href="../bridge/message.md#bridge_message_AddTokenOnSui">AddTokenOnSui</a>): vector&lt;String&gt; {
    self.<a href="../bridge/message.md#bridge_message_token_type_names">token_type_names</a>
}
</code></pre>



</details>

<a name="bridge_message_token_prices"></a>

## Function `token_prices`



<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_token_prices">token_prices</a>(self: &<a href="../bridge/message.md#bridge_message_AddTokenOnSui">bridge::message::AddTokenOnSui</a>): vector&lt;u64&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_token_prices">token_prices</a>(self: &<a href="../bridge/message.md#bridge_message_AddTokenOnSui">AddTokenOnSui</a>): vector&lt;u64&gt; {
    self.<a href="../bridge/message.md#bridge_message_token_prices">token_prices</a>
}
</code></pre>



</details>

<a name="bridge_message_emergency_op_pause"></a>

## Function `emergency_op_pause`



<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_emergency_op_pause">emergency_op_pause</a>(): u8
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_emergency_op_pause">emergency_op_pause</a>(): u8 {
    <a href="../bridge/message.md#bridge_message_PAUSE">PAUSE</a>
}
</code></pre>



</details>

<a name="bridge_message_emergency_op_unpause"></a>

## Function `emergency_op_unpause`



<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_emergency_op_unpause">emergency_op_unpause</a>(): u8
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_emergency_op_unpause">emergency_op_unpause</a>(): u8 {
    <a href="../bridge/message.md#bridge_message_UNPAUSE">UNPAUSE</a>
}
</code></pre>



</details>

<a name="bridge_message_required_voting_power"></a>

## Function `required_voting_power`

Return the required signature threshold for the message, values are voting power in the scale of 10000


<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_required_voting_power">required_voting_power</a>(self: &<a href="../bridge/message.md#bridge_message_BridgeMessage">bridge::message::BridgeMessage</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_required_voting_power">required_voting_power</a>(self: &<a href="../bridge/message.md#bridge_message_BridgeMessage">BridgeMessage</a>): u64 {
    <b>let</b> <a href="../bridge/message.md#bridge_message_message_type">message_type</a> = <a href="../bridge/message.md#bridge_message_message_type">message_type</a>(self);
    <b>if</b> (<a href="../bridge/message.md#bridge_message_message_type">message_type</a> == <a href="../bridge/message_types.md#bridge_message_types_token">message_types::token</a>()) {
        3334
    } <b>else</b> <b>if</b> (<a href="../bridge/message.md#bridge_message_message_type">message_type</a> == <a href="../bridge/message_types.md#bridge_message_types_emergency_op">message_types::emergency_op</a>()) {
        <b>let</b> <a href="../bridge/message.md#bridge_message_payload">payload</a> = <a href="../bridge/message.md#bridge_message_extract_emergency_op_payload">extract_emergency_op_payload</a>(self);
        <b>if</b> (<a href="../bridge/message.md#bridge_message_payload">payload</a>.op_type == <a href="../bridge/message.md#bridge_message_PAUSE">PAUSE</a>) {
            450
        } <b>else</b> <b>if</b> (<a href="../bridge/message.md#bridge_message_payload">payload</a>.op_type == <a href="../bridge/message.md#bridge_message_UNPAUSE">UNPAUSE</a>) {
            5001
        } <b>else</b> {
            <b>abort</b> <a href="../bridge/message.md#bridge_message_EInvalidEmergencyOpType">EInvalidEmergencyOpType</a>
        }
    } <b>else</b> <b>if</b> (<a href="../bridge/message.md#bridge_message_message_type">message_type</a> == <a href="../bridge/message_types.md#bridge_message_types_committee_blocklist">message_types::committee_blocklist</a>()) {
        5001
    } <b>else</b> <b>if</b> (<a href="../bridge/message.md#bridge_message_message_type">message_type</a> == <a href="../bridge/message_types.md#bridge_message_types_update_asset_price">message_types::update_asset_price</a>()) {
        5001
    } <b>else</b> <b>if</b> (<a href="../bridge/message.md#bridge_message_message_type">message_type</a> == <a href="../bridge/message_types.md#bridge_message_types_update_bridge_limit">message_types::update_bridge_limit</a>()) {
        5001
    } <b>else</b> <b>if</b> (<a href="../bridge/message.md#bridge_message_message_type">message_type</a> == <a href="../bridge/message_types.md#bridge_message_types_add_tokens_on_sui">message_types::add_tokens_on_sui</a>()) {
        5001
    } <b>else</b> {
        <b>abort</b> <a href="../bridge/message.md#bridge_message_EInvalidMessageType">EInvalidMessageType</a>
    }
}
</code></pre>



</details>

<a name="bridge_message_to_parsed_token_transfer_message"></a>

## Function `to_parsed_token_transfer_message`



<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_to_parsed_token_transfer_message">to_parsed_token_transfer_message</a>(<a href="../bridge/message.md#bridge_message">message</a>: &<a href="../bridge/message.md#bridge_message_BridgeMessage">bridge::message::BridgeMessage</a>): <a href="../bridge/message.md#bridge_message_ParsedTokenTransferMessage">bridge::message::ParsedTokenTransferMessage</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../bridge/message.md#bridge_message_to_parsed_token_transfer_message">to_parsed_token_transfer_message</a>(
    <a href="../bridge/message.md#bridge_message">message</a>: &<a href="../bridge/message.md#bridge_message_BridgeMessage">BridgeMessage</a>,
): <a href="../bridge/message.md#bridge_message_ParsedTokenTransferMessage">ParsedTokenTransferMessage</a> {
    <b>assert</b>!(<a href="../bridge/message.md#bridge_message">message</a>.<a href="../bridge/message.md#bridge_message_message_type">message_type</a>() == <a href="../bridge/message_types.md#bridge_message_types_token">message_types::token</a>(), <a href="../bridge/message.md#bridge_message_EMustBeTokenMessage">EMustBeTokenMessage</a>);
    <b>let</b> <a href="../bridge/message.md#bridge_message_payload">payload</a> = <a href="../bridge/message.md#bridge_message">message</a>.<a href="../bridge/message.md#bridge_message_extract_token_bridge_payload">extract_token_bridge_payload</a>();
    <a href="../bridge/message.md#bridge_message_ParsedTokenTransferMessage">ParsedTokenTransferMessage</a> {
        <a href="../bridge/message.md#bridge_message_message_version">message_version</a>: <a href="../bridge/message.md#bridge_message">message</a>.<a href="../bridge/message.md#bridge_message_message_version">message_version</a>(),
        <a href="../bridge/message.md#bridge_message_seq_num">seq_num</a>: <a href="../bridge/message.md#bridge_message">message</a>.<a href="../bridge/message.md#bridge_message_seq_num">seq_num</a>(),
        <a href="../bridge/message.md#bridge_message_source_chain">source_chain</a>: <a href="../bridge/message.md#bridge_message">message</a>.<a href="../bridge/message.md#bridge_message_source_chain">source_chain</a>(),
        <a href="../bridge/message.md#bridge_message_payload">payload</a>: <a href="../bridge/message.md#bridge_message">message</a>.<a href="../bridge/message.md#bridge_message_payload">payload</a>(),
        parsed_payload: <a href="../bridge/message.md#bridge_message_payload">payload</a>,
    }
}
</code></pre>



</details>

<a name="bridge_message_reverse_bytes"></a>

## Function `reverse_bytes`



<pre><code><b>fun</b> <a href="../bridge/message.md#bridge_message_reverse_bytes">reverse_bytes</a>(bytes: vector&lt;u8&gt;): vector&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../bridge/message.md#bridge_message_reverse_bytes">reverse_bytes</a>(<b>mut</b> bytes: vector&lt;u8&gt;): vector&lt;u8&gt; {
    vector::reverse(&<b>mut</b> bytes);
    bytes
}
</code></pre>



</details>

<a name="bridge_message_peel_u64_be"></a>

## Function `peel_u64_be`



<pre><code><b>fun</b> <a href="../bridge/message.md#bridge_message_peel_u64_be">peel_u64_be</a>(bcs: &<b>mut</b> <a href="../sui/bcs.md#sui_bcs_BCS">sui::bcs::BCS</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../bridge/message.md#bridge_message_peel_u64_be">peel_u64_be</a>(bcs: &<b>mut</b> BCS): u64 {
    <b>let</b> (<b>mut</b> value, <b>mut</b> i) = (0u64, 64u8);
    <b>while</b> (i &gt; 0) {
        i = i - 8;
        <b>let</b> byte = (bcs::peel_u8(bcs) <b>as</b> u64);
        value = value + (byte &lt;&lt; i);
    };
    value
}
</code></pre>



</details>
