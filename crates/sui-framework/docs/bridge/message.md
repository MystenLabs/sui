
---
title: Module `0xb::message`
---



-  [Struct `BridgeMessage`](#0xb_message_BridgeMessage)
-  [Struct `BridgeMessageKey`](#0xb_message_BridgeMessageKey)
-  [Struct `TokenPayload`](#0xb_message_TokenPayload)
-  [Struct `EmergencyOp`](#0xb_message_EmergencyOp)
-  [Struct `Blocklist`](#0xb_message_Blocklist)
-  [Struct `UpdateBridgeLimit`](#0xb_message_UpdateBridgeLimit)
-  [Struct `UpdateAssetPrice`](#0xb_message_UpdateAssetPrice)
-  [Constants](#@Constants_0)
-  [Function `extract_token_bridge_payload`](#0xb_message_extract_token_bridge_payload)
-  [Function `extract_emergency_op_payload`](#0xb_message_extract_emergency_op_payload)
-  [Function `extract_blocklist_payload`](#0xb_message_extract_blocklist_payload)
-  [Function `extract_update_bridge_limit`](#0xb_message_extract_update_bridge_limit)
-  [Function `extract_update_asset_price`](#0xb_message_extract_update_asset_price)
-  [Function `serialize_message`](#0xb_message_serialize_message)
-  [Function `create_token_bridge_message`](#0xb_message_create_token_bridge_message)
-  [Function `create_emergency_op_message`](#0xb_message_create_emergency_op_message)
-  [Function `create_blocklist_message`](#0xb_message_create_blocklist_message)
-  [Function `create_update_bridge_limit_message`](#0xb_message_create_update_bridge_limit_message)
-  [Function `create_update_asset_price_message`](#0xb_message_create_update_asset_price_message)
-  [Function `create_key`](#0xb_message_create_key)
-  [Function `key`](#0xb_message_key)
-  [Function `message_version`](#0xb_message_message_version)
-  [Function `message_type`](#0xb_message_message_type)
-  [Function `seq_num`](#0xb_message_seq_num)
-  [Function `source_chain`](#0xb_message_source_chain)
-  [Function `token_target_chain`](#0xb_message_token_target_chain)
-  [Function `token_target_address`](#0xb_message_token_target_address)
-  [Function `token_type`](#0xb_message_token_type)
-  [Function `token_amount`](#0xb_message_token_amount)
-  [Function `emergency_op_type`](#0xb_message_emergency_op_type)
-  [Function `blocklist_type`](#0xb_message_blocklist_type)
-  [Function `blocklist_validator_addresses`](#0xb_message_blocklist_validator_addresses)
-  [Function `update_bridge_limit_payload_sending_chain`](#0xb_message_update_bridge_limit_payload_sending_chain)
-  [Function `update_bridge_limit_payload_receiving_chain`](#0xb_message_update_bridge_limit_payload_receiving_chain)
-  [Function `update_bridge_limit_payload_limit`](#0xb_message_update_bridge_limit_payload_limit)
-  [Function `update_asset_price_payload_token_id`](#0xb_message_update_asset_price_payload_token_id)
-  [Function `update_asset_price_payload_new_price`](#0xb_message_update_asset_price_payload_new_price)
-  [Function `emergency_op_pause`](#0xb_message_emergency_op_pause)
-  [Function `emergency_op_unpause`](#0xb_message_emergency_op_unpause)
-  [Function `required_voting_power`](#0xb_message_required_voting_power)
-  [Function `reverse_bytes`](#0xb_message_reverse_bytes)
-  [Function `peel_u64_be`](#0xb_message_peel_u64_be)


<pre><code><b>use</b> <a href="../move-stdlib/vector.md#0x1_vector">0x1::vector</a>;
<b>use</b> <a href="../sui-framework/bcs.md#0x2_bcs">0x2::bcs</a>;
<b>use</b> <a href="chain_ids.md#0xb_chain_ids">0xb::chain_ids</a>;
<b>use</b> <a href="message_types.md#0xb_message_types">0xb::message_types</a>;
<b>use</b> <a href="treasury.md#0xb_treasury">0xb::treasury</a>;
</code></pre>



<a name="0xb_message_BridgeMessage"></a>

## Struct `BridgeMessage`



<pre><code><b>struct</b> <a href="message.md#0xb_message_BridgeMessage">BridgeMessage</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>message_type: u8</code>
</dt>
<dd>

</dd>
<dt>
<code>message_version: u8</code>
</dt>
<dd>

</dd>
<dt>
<code>seq_num: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>source_chain: u8</code>
</dt>
<dd>

</dd>
<dt>
<code>payload: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0xb_message_BridgeMessageKey"></a>

## Struct `BridgeMessageKey`



<pre><code><b>struct</b> <a href="message.md#0xb_message_BridgeMessageKey">BridgeMessageKey</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>source_chain: u8</code>
</dt>
<dd>

</dd>
<dt>
<code>message_type: u8</code>
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

<a name="0xb_message_TokenPayload"></a>

## Struct `TokenPayload`



<pre><code><b>struct</b> <a href="message.md#0xb_message_TokenPayload">TokenPayload</a> <b>has</b> drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>sender_address: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>target_chain: u8</code>
</dt>
<dd>

</dd>
<dt>
<code>target_address: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>token_type: u8</code>
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

<a name="0xb_message_EmergencyOp"></a>

## Struct `EmergencyOp`



<pre><code><b>struct</b> <a href="message.md#0xb_message_EmergencyOp">EmergencyOp</a> <b>has</b> drop
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

<a name="0xb_message_Blocklist"></a>

## Struct `Blocklist`



<pre><code><b>struct</b> <a href="message.md#0xb_message_Blocklist">Blocklist</a> <b>has</b> drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>blocklist_type: u8</code>
</dt>
<dd>

</dd>
<dt>
<code>validator_eth_addresses: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;&gt;</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0xb_message_UpdateBridgeLimit"></a>

## Struct `UpdateBridgeLimit`



<pre><code><b>struct</b> <a href="message.md#0xb_message_UpdateBridgeLimit">UpdateBridgeLimit</a> <b>has</b> drop
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

<a name="0xb_message_UpdateAssetPrice"></a>

## Struct `UpdateAssetPrice`



<pre><code><b>struct</b> <a href="message.md#0xb_message_UpdateAssetPrice">UpdateAssetPrice</a> <b>has</b> drop
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

<a name="@Constants_0"></a>

## Constants


<a name="0xb_message_CURRENT_MESSAGE_VERSION"></a>



<pre><code><b>const</b> <a href="message.md#0xb_message_CURRENT_MESSAGE_VERSION">CURRENT_MESSAGE_VERSION</a>: u8 = 1;
</code></pre>



<a name="0xb_message_ECDSA_ADDRESS_LENGTH"></a>



<pre><code><b>const</b> <a href="message.md#0xb_message_ECDSA_ADDRESS_LENGTH">ECDSA_ADDRESS_LENGTH</a>: u64 = 20;
</code></pre>



<a name="0xb_message_EEmptyList"></a>



<pre><code><b>const</b> <a href="message.md#0xb_message_EEmptyList">EEmptyList</a>: u64 = 2;
</code></pre>



<a name="0xb_message_EInvalidAddressLength"></a>



<pre><code><b>const</b> <a href="message.md#0xb_message_EInvalidAddressLength">EInvalidAddressLength</a>: u64 = 1;
</code></pre>



<a name="0xb_message_EInvalidEmergencyOpType"></a>



<pre><code><b>const</b> <a href="message.md#0xb_message_EInvalidEmergencyOpType">EInvalidEmergencyOpType</a>: u64 = 4;
</code></pre>



<a name="0xb_message_EInvalidMessageType"></a>



<pre><code><b>const</b> <a href="message.md#0xb_message_EInvalidMessageType">EInvalidMessageType</a>: u64 = 3;
</code></pre>



<a name="0xb_message_ETrailingBytes"></a>



<pre><code><b>const</b> <a href="message.md#0xb_message_ETrailingBytes">ETrailingBytes</a>: u64 = 0;
</code></pre>



<a name="0xb_message_PAUSE"></a>



<pre><code><b>const</b> <a href="message.md#0xb_message_PAUSE">PAUSE</a>: u8 = 0;
</code></pre>



<a name="0xb_message_UNPAUSE"></a>



<pre><code><b>const</b> <a href="message.md#0xb_message_UNPAUSE">UNPAUSE</a>: u8 = 1;
</code></pre>



<a name="0xb_message_extract_token_bridge_payload"></a>

## Function `extract_token_bridge_payload`



<pre><code><b>public</b> <b>fun</b> <a href="message.md#0xb_message_extract_token_bridge_payload">extract_token_bridge_payload</a>(<a href="message.md#0xb_message">message</a>: &<a href="message.md#0xb_message_BridgeMessage">message::BridgeMessage</a>): <a href="message.md#0xb_message_TokenPayload">message::TokenPayload</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="message.md#0xb_message_extract_token_bridge_payload">extract_token_bridge_payload</a>(<a href="message.md#0xb_message">message</a>: &<a href="message.md#0xb_message_BridgeMessage">BridgeMessage</a>): <a href="message.md#0xb_message_TokenPayload">TokenPayload</a> {
    <b>let</b> <b>mut</b> <a href="../move-stdlib/bcs.md#0x1_bcs">bcs</a> = bcs::new(<a href="message.md#0xb_message">message</a>.payload);
    <b>let</b> sender_address = bcs::peel_vec_u8(&<b>mut</b> <a href="../move-stdlib/bcs.md#0x1_bcs">bcs</a>);
    <b>let</b> target_chain = bcs::peel_u8(&<b>mut</b> <a href="../move-stdlib/bcs.md#0x1_bcs">bcs</a>);
    // TODO: add test case for invalid chain id
    <a href="chain_ids.md#0xb_chain_ids_assert_valid_chain_id">chain_ids::assert_valid_chain_id</a>(target_chain);
    <b>let</b> target_address = bcs::peel_vec_u8(&<b>mut</b> <a href="../move-stdlib/bcs.md#0x1_bcs">bcs</a>);
    <b>let</b> token_type = bcs::peel_u8(&<b>mut</b> <a href="../move-stdlib/bcs.md#0x1_bcs">bcs</a>);
    <b>let</b> amount = <a href="message.md#0xb_message_peel_u64_be">peel_u64_be</a>(&<b>mut</b> <a href="../move-stdlib/bcs.md#0x1_bcs">bcs</a>);
    <b>assert</b>!(<a href="../move-stdlib/vector.md#0x1_vector_is_empty">vector::is_empty</a>(&bcs::into_remainder_bytes(<a href="../move-stdlib/bcs.md#0x1_bcs">bcs</a>)), <a href="message.md#0xb_message_ETrailingBytes">ETrailingBytes</a>);
    <a href="message.md#0xb_message_TokenPayload">TokenPayload</a> {
        sender_address,
        target_chain,
        target_address,
        token_type,
        amount
    }
}
</code></pre>



</details>

<a name="0xb_message_extract_emergency_op_payload"></a>

## Function `extract_emergency_op_payload`



<pre><code><b>public</b> <b>fun</b> <a href="message.md#0xb_message_extract_emergency_op_payload">extract_emergency_op_payload</a>(<a href="message.md#0xb_message">message</a>: &<a href="message.md#0xb_message_BridgeMessage">message::BridgeMessage</a>): <a href="message.md#0xb_message_EmergencyOp">message::EmergencyOp</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="message.md#0xb_message_extract_emergency_op_payload">extract_emergency_op_payload</a>(<a href="message.md#0xb_message">message</a>: &<a href="message.md#0xb_message_BridgeMessage">BridgeMessage</a>): <a href="message.md#0xb_message_EmergencyOp">EmergencyOp</a> {
    // emergency op payload is just a single byte
    <b>assert</b>!(<a href="../move-stdlib/vector.md#0x1_vector_length">vector::length</a>(&<a href="message.md#0xb_message">message</a>.payload) == 1, <a href="message.md#0xb_message_ETrailingBytes">ETrailingBytes</a>);
    <a href="message.md#0xb_message_EmergencyOp">EmergencyOp</a> {
        op_type: *<a href="../move-stdlib/vector.md#0x1_vector_borrow">vector::borrow</a>(&<a href="message.md#0xb_message">message</a>.payload, 0)
    }
}
</code></pre>



</details>

<a name="0xb_message_extract_blocklist_payload"></a>

## Function `extract_blocklist_payload`



<pre><code><b>public</b> <b>fun</b> <a href="message.md#0xb_message_extract_blocklist_payload">extract_blocklist_payload</a>(<a href="message.md#0xb_message">message</a>: &<a href="message.md#0xb_message_BridgeMessage">message::BridgeMessage</a>): <a href="message.md#0xb_message_Blocklist">message::Blocklist</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="message.md#0xb_message_extract_blocklist_payload">extract_blocklist_payload</a>(<a href="message.md#0xb_message">message</a>: &<a href="message.md#0xb_message_BridgeMessage">BridgeMessage</a>): <a href="message.md#0xb_message_Blocklist">Blocklist</a> {
    // blocklist payload should consist of one byte blocklist type, and list of 33 bytes ecdsa pub keys
    <b>let</b> <b>mut</b> <a href="../move-stdlib/bcs.md#0x1_bcs">bcs</a> = bcs::new(<a href="message.md#0xb_message">message</a>.payload);
    <b>let</b> blocklist_type = bcs::peel_u8(&<b>mut</b> <a href="../move-stdlib/bcs.md#0x1_bcs">bcs</a>);
    <b>let</b> <b>mut</b> address_count = bcs::peel_u8(&<b>mut</b> <a href="../move-stdlib/bcs.md#0x1_bcs">bcs</a>);
    // TODO: add test case for 0 value
    <b>assert</b>!(address_count != 0, <a href="message.md#0xb_message_EEmptyList">EEmptyList</a>);
    <b>let</b> <b>mut</b> validator_eth_addresses = <a href="../move-stdlib/vector.md#0x1_vector">vector</a>[];
    <b>while</b> (address_count &gt; 0) {
        <b>let</b> (<b>mut</b> <b>address</b>, <b>mut</b> i) = (<a href="../move-stdlib/vector.md#0x1_vector">vector</a>[], 0);
        <b>while</b> (i &lt; <a href="message.md#0xb_message_ECDSA_ADDRESS_LENGTH">ECDSA_ADDRESS_LENGTH</a>) {
            <a href="../move-stdlib/vector.md#0x1_vector_push_back">vector::push_back</a>(&<b>mut</b> <b>address</b>, bcs::peel_u8(&<b>mut</b> <a href="../move-stdlib/bcs.md#0x1_bcs">bcs</a>));
            i = i + 1;
        };
        <a href="../move-stdlib/vector.md#0x1_vector_push_back">vector::push_back</a>(&<b>mut</b> validator_eth_addresses, <b>address</b>);
        address_count = address_count - 1;
    };
    <b>assert</b>!(<a href="../move-stdlib/vector.md#0x1_vector_is_empty">vector::is_empty</a>(&bcs::into_remainder_bytes(<a href="../move-stdlib/bcs.md#0x1_bcs">bcs</a>)), <a href="message.md#0xb_message_ETrailingBytes">ETrailingBytes</a>);
    <a href="message.md#0xb_message_Blocklist">Blocklist</a> {
        blocklist_type,
        validator_eth_addresses
    }
}
</code></pre>



</details>

<a name="0xb_message_extract_update_bridge_limit"></a>

## Function `extract_update_bridge_limit`



<pre><code><b>public</b> <b>fun</b> <a href="message.md#0xb_message_extract_update_bridge_limit">extract_update_bridge_limit</a>(<a href="message.md#0xb_message">message</a>: &<a href="message.md#0xb_message_BridgeMessage">message::BridgeMessage</a>): <a href="message.md#0xb_message_UpdateBridgeLimit">message::UpdateBridgeLimit</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="message.md#0xb_message_extract_update_bridge_limit">extract_update_bridge_limit</a>(<a href="message.md#0xb_message">message</a>: &<a href="message.md#0xb_message_BridgeMessage">BridgeMessage</a>): <a href="message.md#0xb_message_UpdateBridgeLimit">UpdateBridgeLimit</a> {
    <b>let</b> <b>mut</b> <a href="../move-stdlib/bcs.md#0x1_bcs">bcs</a> = bcs::new(<a href="message.md#0xb_message">message</a>.payload);
    <b>let</b> sending_chain = bcs::peel_u8(&<b>mut</b> <a href="../move-stdlib/bcs.md#0x1_bcs">bcs</a>);
    // TODO: add test case for invalid chain id
    <a href="chain_ids.md#0xb_chain_ids_assert_valid_chain_id">chain_ids::assert_valid_chain_id</a>(sending_chain);
    <b>let</b> limit = <a href="message.md#0xb_message_peel_u64_be">peel_u64_be</a>(&<b>mut</b> <a href="../move-stdlib/bcs.md#0x1_bcs">bcs</a>);
    <b>assert</b>!(<a href="../move-stdlib/vector.md#0x1_vector_is_empty">vector::is_empty</a>(&bcs::into_remainder_bytes(<a href="../move-stdlib/bcs.md#0x1_bcs">bcs</a>)), <a href="message.md#0xb_message_ETrailingBytes">ETrailingBytes</a>);
    <a href="message.md#0xb_message_UpdateBridgeLimit">UpdateBridgeLimit</a> {
        receiving_chain: <a href="message.md#0xb_message">message</a>.source_chain,
        sending_chain,
        limit
    }
}
</code></pre>



</details>

<a name="0xb_message_extract_update_asset_price"></a>

## Function `extract_update_asset_price`



<pre><code><b>public</b> <b>fun</b> <a href="message.md#0xb_message_extract_update_asset_price">extract_update_asset_price</a>(<a href="message.md#0xb_message">message</a>: &<a href="message.md#0xb_message_BridgeMessage">message::BridgeMessage</a>): <a href="message.md#0xb_message_UpdateAssetPrice">message::UpdateAssetPrice</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="message.md#0xb_message_extract_update_asset_price">extract_update_asset_price</a>(<a href="message.md#0xb_message">message</a>: &<a href="message.md#0xb_message_BridgeMessage">BridgeMessage</a>): <a href="message.md#0xb_message_UpdateAssetPrice">UpdateAssetPrice</a> {
    <b>let</b> <b>mut</b> <a href="../move-stdlib/bcs.md#0x1_bcs">bcs</a> = bcs::new(<a href="message.md#0xb_message">message</a>.payload);
    <b>let</b> token_id = bcs::peel_u8(&<b>mut</b> <a href="../move-stdlib/bcs.md#0x1_bcs">bcs</a>);
    <b>let</b> new_price = <a href="message.md#0xb_message_peel_u64_be">peel_u64_be</a>(&<b>mut</b> <a href="../move-stdlib/bcs.md#0x1_bcs">bcs</a>);
    <b>assert</b>!(<a href="../move-stdlib/vector.md#0x1_vector_is_empty">vector::is_empty</a>(&bcs::into_remainder_bytes(<a href="../move-stdlib/bcs.md#0x1_bcs">bcs</a>)), <a href="message.md#0xb_message_ETrailingBytes">ETrailingBytes</a>);
    <a href="message.md#0xb_message_UpdateAssetPrice">UpdateAssetPrice</a> {
        token_id,
        new_price
    }
}
</code></pre>



</details>

<a name="0xb_message_serialize_message"></a>

## Function `serialize_message`



<pre><code><b>public</b> <b>fun</b> <a href="message.md#0xb_message_serialize_message">serialize_message</a>(<a href="message.md#0xb_message">message</a>: <a href="message.md#0xb_message_BridgeMessage">message::BridgeMessage</a>): <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="message.md#0xb_message_serialize_message">serialize_message</a>(<a href="message.md#0xb_message">message</a>: <a href="message.md#0xb_message_BridgeMessage">BridgeMessage</a>): <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt; {
    <b>let</b> <a href="message.md#0xb_message_BridgeMessage">BridgeMessage</a> {
        message_type,
        message_version,
        seq_num,
        source_chain,
        payload
    } = <a href="message.md#0xb_message">message</a>;

    <b>let</b> <b>mut</b> <a href="message.md#0xb_message">message</a> = <a href="../move-stdlib/vector.md#0x1_vector">vector</a>[];
    <a href="../move-stdlib/vector.md#0x1_vector_push_back">vector::push_back</a>(&<b>mut</b> <a href="message.md#0xb_message">message</a>, message_type);
    <a href="../move-stdlib/vector.md#0x1_vector_push_back">vector::push_back</a>(&<b>mut</b> <a href="message.md#0xb_message">message</a>, message_version);
    // <a href="../move-stdlib/bcs.md#0x1_bcs">bcs</a> serializes u64 <b>as</b> 8 bytes
    <a href="../move-stdlib/vector.md#0x1_vector_append">vector::append</a>(&<b>mut</b> <a href="message.md#0xb_message">message</a>, <a href="message.md#0xb_message_reverse_bytes">reverse_bytes</a>(<a href="../move-stdlib/bcs.md#0x1_bcs_to_bytes">bcs::to_bytes</a>(&seq_num)));
    <a href="../move-stdlib/vector.md#0x1_vector_push_back">vector::push_back</a>(&<b>mut</b> <a href="message.md#0xb_message">message</a>, source_chain);
    <a href="../move-stdlib/vector.md#0x1_vector_append">vector::append</a>(&<b>mut</b> <a href="message.md#0xb_message">message</a>, payload);
    <a href="message.md#0xb_message">message</a>
}
</code></pre>



</details>

<a name="0xb_message_create_token_bridge_message"></a>

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


<pre><code><b>public</b> <b>fun</b> <a href="message.md#0xb_message_create_token_bridge_message">create_token_bridge_message</a>(source_chain: u8, seq_num: u64, sender_address: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, target_chain: u8, target_address: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, token_type: u8, amount: u64): <a href="message.md#0xb_message_BridgeMessage">message::BridgeMessage</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="message.md#0xb_message_create_token_bridge_message">create_token_bridge_message</a>(
    source_chain: u8,
    seq_num: u64,
    sender_address: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    target_chain: u8,
    target_address: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    token_type: u8,
    amount: u64
): <a href="message.md#0xb_message_BridgeMessage">BridgeMessage</a> {
    // TODO: add test case for invalid chain id
    <a href="chain_ids.md#0xb_chain_ids_assert_valid_chain_id">chain_ids::assert_valid_chain_id</a>(source_chain);
    // TODO: add test case for invalid chain id
    <a href="chain_ids.md#0xb_chain_ids_assert_valid_chain_id">chain_ids::assert_valid_chain_id</a>(target_chain);
    <b>let</b> <b>mut</b> payload = <a href="../move-stdlib/vector.md#0x1_vector">vector</a>[];
    // sender <b>address</b> should be less than 255 bytes so can fit into u8
    <a href="../move-stdlib/vector.md#0x1_vector_push_back">vector::push_back</a>(&<b>mut</b> payload, (<a href="../move-stdlib/vector.md#0x1_vector_length">vector::length</a>(&sender_address) <b>as</b> u8));
    <a href="../move-stdlib/vector.md#0x1_vector_append">vector::append</a>(&<b>mut</b> payload, sender_address);
    <a href="../move-stdlib/vector.md#0x1_vector_push_back">vector::push_back</a>(&<b>mut</b> payload, target_chain);
    // target <b>address</b> should be less than 255 bytes so can fit into u8
    <a href="../move-stdlib/vector.md#0x1_vector_push_back">vector::push_back</a>(&<b>mut</b> payload, (<a href="../move-stdlib/vector.md#0x1_vector_length">vector::length</a>(&target_address) <b>as</b> u8));
    <a href="../move-stdlib/vector.md#0x1_vector_append">vector::append</a>(&<b>mut</b> payload, target_address);
    <a href="../move-stdlib/vector.md#0x1_vector_push_back">vector::push_back</a>(&<b>mut</b> payload, token_type);
    // <a href="../move-stdlib/bcs.md#0x1_bcs">bcs</a> serialzies u64 <b>as</b> 8 bytes
    <a href="../move-stdlib/vector.md#0x1_vector_append">vector::append</a>(&<b>mut</b> payload, <a href="message.md#0xb_message_reverse_bytes">reverse_bytes</a>(<a href="../move-stdlib/bcs.md#0x1_bcs_to_bytes">bcs::to_bytes</a>(&amount)));

    <a href="message.md#0xb_message_BridgeMessage">BridgeMessage</a> {
        message_type: <a href="message_types.md#0xb_message_types_token">message_types::token</a>(),
        message_version: <a href="message.md#0xb_message_CURRENT_MESSAGE_VERSION">CURRENT_MESSAGE_VERSION</a>,
        seq_num,
        source_chain,
        payload,
    }
}
</code></pre>



</details>

<a name="0xb_message_create_emergency_op_message"></a>

## Function `create_emergency_op_message`

Emergency Op Message Format:
[message_type: u8]
[version:u8]
[nonce:u64]
[chain_id: u8]
[op_type: u8]


<pre><code><b>public</b> <b>fun</b> <a href="message.md#0xb_message_create_emergency_op_message">create_emergency_op_message</a>(source_chain: u8, seq_num: u64, op_type: u8): <a href="message.md#0xb_message_BridgeMessage">message::BridgeMessage</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="message.md#0xb_message_create_emergency_op_message">create_emergency_op_message</a>(
    source_chain: u8,
    seq_num: u64,
    op_type: u8,
): <a href="message.md#0xb_message_BridgeMessage">BridgeMessage</a> {
    // TODO: add test case for invalid chain id
    <a href="chain_ids.md#0xb_chain_ids_assert_valid_chain_id">chain_ids::assert_valid_chain_id</a>(source_chain);
    <a href="message.md#0xb_message_BridgeMessage">BridgeMessage</a> {
        message_type: <a href="message_types.md#0xb_message_types_emergency_op">message_types::emergency_op</a>(),
        message_version: <a href="message.md#0xb_message_CURRENT_MESSAGE_VERSION">CURRENT_MESSAGE_VERSION</a>,
        seq_num,
        source_chain,
        payload: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>[op_type],
    }
}
</code></pre>



</details>

<a name="0xb_message_create_blocklist_message"></a>

## Function `create_blocklist_message`

Blocklist Message Format:
[message_type: u8]
[version:u8]
[nonce:u64]
[chain_id: u8]
[blocklist_type: u8]
[validator_length: u8]
[validator_ecdsa_addresses: byte[][]]


<pre><code><b>public</b> <b>fun</b> <a href="message.md#0xb_message_create_blocklist_message">create_blocklist_message</a>(source_chain: u8, seq_num: u64, blocklist_type: u8, validator_ecdsa_addresses: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;&gt;): <a href="message.md#0xb_message_BridgeMessage">message::BridgeMessage</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="message.md#0xb_message_create_blocklist_message">create_blocklist_message</a>(
    source_chain: u8,
    seq_num: u64,
    // 0: block, 1: unblock
    blocklist_type: u8,
    validator_ecdsa_addresses: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;&gt;,
): <a href="message.md#0xb_message_BridgeMessage">BridgeMessage</a> {
    // TODO: add test case for invalid chain id
    <a href="chain_ids.md#0xb_chain_ids_assert_valid_chain_id">chain_ids::assert_valid_chain_id</a>(source_chain);
    <b>let</b> address_length = (<a href="../move-stdlib/vector.md#0x1_vector_length">vector::length</a>(&validator_ecdsa_addresses) <b>as</b> u8);
    <b>let</b> <b>mut</b> payload = <a href="../move-stdlib/vector.md#0x1_vector">vector</a>[blocklist_type, address_length];
    <b>let</b> <b>mut</b> i = 0;

    <b>while</b> (i &lt; address_length) {
        <b>let</b> <b>address</b> = <a href="../move-stdlib/vector.md#0x1_vector_borrow">vector::borrow</a>(&validator_ecdsa_addresses, (i <b>as</b> u64));
        <b>assert</b>!(<a href="../move-stdlib/vector.md#0x1_vector_length">vector::length</a>(<b>address</b>) == <a href="message.md#0xb_message_ECDSA_ADDRESS_LENGTH">ECDSA_ADDRESS_LENGTH</a>, <a href="message.md#0xb_message_EInvalidAddressLength">EInvalidAddressLength</a>);
        <a href="../move-stdlib/vector.md#0x1_vector_append">vector::append</a>(&<b>mut</b> payload, *<b>address</b>);
        i = i + 1;
    };

    <a href="message.md#0xb_message_BridgeMessage">BridgeMessage</a> {
        message_type: <a href="message_types.md#0xb_message_types_committee_blocklist">message_types::committee_blocklist</a>(),
        message_version: <a href="message.md#0xb_message_CURRENT_MESSAGE_VERSION">CURRENT_MESSAGE_VERSION</a>,
        seq_num,
        source_chain,
        payload,
    }
}
</code></pre>



</details>

<a name="0xb_message_create_update_bridge_limit_message"></a>

## Function `create_update_bridge_limit_message`

Update bridge limit Message Format:
[message_type: u8]
[version:u8]
[nonce:u64]
[receiving_chain_id: u8]
[sending_chain_id: u8]
[new_limit: u64]


<pre><code><b>public</b> <b>fun</b> <a href="message.md#0xb_message_create_update_bridge_limit_message">create_update_bridge_limit_message</a>(receiving_chain: u8, seq_num: u64, sending_chain: u8, new_limit: u64): <a href="message.md#0xb_message_BridgeMessage">message::BridgeMessage</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="message.md#0xb_message_create_update_bridge_limit_message">create_update_bridge_limit_message</a>(
    receiving_chain: u8,
    seq_num: u64,
    sending_chain: u8,
    new_limit: u64,
): <a href="message.md#0xb_message_BridgeMessage">BridgeMessage</a> {
    // TODO: add test case for invalid chain id
    <a href="chain_ids.md#0xb_chain_ids_assert_valid_chain_id">chain_ids::assert_valid_chain_id</a>(receiving_chain);
    // TODO: add test case for invalid chain id
    <a href="chain_ids.md#0xb_chain_ids_assert_valid_chain_id">chain_ids::assert_valid_chain_id</a>(sending_chain);
    <b>let</b> <b>mut</b> payload = <a href="../move-stdlib/vector.md#0x1_vector">vector</a>[sending_chain];
    <a href="../move-stdlib/vector.md#0x1_vector_append">vector::append</a>(&<b>mut</b> payload, <a href="message.md#0xb_message_reverse_bytes">reverse_bytes</a>(<a href="../move-stdlib/bcs.md#0x1_bcs_to_bytes">bcs::to_bytes</a>(&new_limit)));
    <a href="message.md#0xb_message_BridgeMessage">BridgeMessage</a> {
        message_type: <a href="message_types.md#0xb_message_types_update_bridge_limit">message_types::update_bridge_limit</a>(),
        message_version: <a href="message.md#0xb_message_CURRENT_MESSAGE_VERSION">CURRENT_MESSAGE_VERSION</a>,
        seq_num,
        source_chain: receiving_chain,
        payload,
    }
}
</code></pre>



</details>

<a name="0xb_message_create_update_asset_price_message"></a>

## Function `create_update_asset_price_message`

Update asset price message
[message_type: u8]
[version:u8]
[nonce:u64]
[chain_id: u8]
[token_id: u8]
[new_price:u64]


<pre><code><b>public</b> <b>fun</b> <a href="message.md#0xb_message_create_update_asset_price_message">create_update_asset_price_message</a>&lt;T&gt;(source_chain: u8, seq_num: u64, new_price: u64): <a href="message.md#0xb_message_BridgeMessage">message::BridgeMessage</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="message.md#0xb_message_create_update_asset_price_message">create_update_asset_price_message</a>&lt;T&gt;(
    source_chain: u8,
    seq_num: u64,
    new_price: u64,
): <a href="message.md#0xb_message_BridgeMessage">BridgeMessage</a> {
    // TODO: add test case for invalid chain id
    <a href="chain_ids.md#0xb_chain_ids_assert_valid_chain_id">chain_ids::assert_valid_chain_id</a>(source_chain);
    <b>let</b> <b>mut</b> payload = <a href="../move-stdlib/vector.md#0x1_vector">vector</a>[<a href="treasury.md#0xb_treasury_token_id">treasury::token_id</a>&lt;T&gt;()];
    <a href="../move-stdlib/vector.md#0x1_vector_append">vector::append</a>(&<b>mut</b> payload, <a href="message.md#0xb_message_reverse_bytes">reverse_bytes</a>(<a href="../move-stdlib/bcs.md#0x1_bcs_to_bytes">bcs::to_bytes</a>(&new_price)));
    <a href="message.md#0xb_message_BridgeMessage">BridgeMessage</a> {
        message_type: <a href="message_types.md#0xb_message_types_update_asset_price">message_types::update_asset_price</a>(),
        message_version: <a href="message.md#0xb_message_CURRENT_MESSAGE_VERSION">CURRENT_MESSAGE_VERSION</a>,
        seq_num,
        source_chain,
        payload,
    }
}
</code></pre>



</details>

<a name="0xb_message_create_key"></a>

## Function `create_key`



<pre><code><b>public</b> <b>fun</b> <a href="message.md#0xb_message_create_key">create_key</a>(source_chain: u8, message_type: u8, bridge_seq_num: u64): <a href="message.md#0xb_message_BridgeMessageKey">message::BridgeMessageKey</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="message.md#0xb_message_create_key">create_key</a>(source_chain: u8, message_type: u8, bridge_seq_num: u64): <a href="message.md#0xb_message_BridgeMessageKey">BridgeMessageKey</a> {
    <a href="message.md#0xb_message_BridgeMessageKey">BridgeMessageKey</a> { source_chain, message_type, bridge_seq_num }
}
</code></pre>



</details>

<a name="0xb_message_key"></a>

## Function `key`



<pre><code><b>public</b> <b>fun</b> <a href="message.md#0xb_message_key">key</a>(self: &<a href="message.md#0xb_message_BridgeMessage">message::BridgeMessage</a>): <a href="message.md#0xb_message_BridgeMessageKey">message::BridgeMessageKey</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="message.md#0xb_message_key">key</a>(self: &<a href="message.md#0xb_message_BridgeMessage">BridgeMessage</a>): <a href="message.md#0xb_message_BridgeMessageKey">BridgeMessageKey</a> {
    <a href="message.md#0xb_message_create_key">create_key</a>(self.source_chain, self.message_type, self.seq_num)
}
</code></pre>



</details>

<a name="0xb_message_message_version"></a>

## Function `message_version`



<pre><code><b>public</b> <b>fun</b> <a href="message.md#0xb_message_message_version">message_version</a>(self: &<a href="message.md#0xb_message_BridgeMessage">message::BridgeMessage</a>): u8
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="message.md#0xb_message_message_version">message_version</a>(self: &<a href="message.md#0xb_message_BridgeMessage">BridgeMessage</a>): u8 {
    self.message_version
}
</code></pre>



</details>

<a name="0xb_message_message_type"></a>

## Function `message_type`



<pre><code><b>public</b> <b>fun</b> <a href="message.md#0xb_message_message_type">message_type</a>(self: &<a href="message.md#0xb_message_BridgeMessage">message::BridgeMessage</a>): u8
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="message.md#0xb_message_message_type">message_type</a>(self: &<a href="message.md#0xb_message_BridgeMessage">BridgeMessage</a>): u8 {
    self.message_type
}
</code></pre>



</details>

<a name="0xb_message_seq_num"></a>

## Function `seq_num`



<pre><code><b>public</b> <b>fun</b> <a href="message.md#0xb_message_seq_num">seq_num</a>(self: &<a href="message.md#0xb_message_BridgeMessage">message::BridgeMessage</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="message.md#0xb_message_seq_num">seq_num</a>(self: &<a href="message.md#0xb_message_BridgeMessage">BridgeMessage</a>): u64 {
    self.seq_num
}
</code></pre>



</details>

<a name="0xb_message_source_chain"></a>

## Function `source_chain`



<pre><code><b>public</b> <b>fun</b> <a href="message.md#0xb_message_source_chain">source_chain</a>(self: &<a href="message.md#0xb_message_BridgeMessage">message::BridgeMessage</a>): u8
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="message.md#0xb_message_source_chain">source_chain</a>(self: &<a href="message.md#0xb_message_BridgeMessage">BridgeMessage</a>): u8 {
    self.source_chain
}
</code></pre>



</details>

<a name="0xb_message_token_target_chain"></a>

## Function `token_target_chain`



<pre><code><b>public</b> <b>fun</b> <a href="message.md#0xb_message_token_target_chain">token_target_chain</a>(self: &<a href="message.md#0xb_message_TokenPayload">message::TokenPayload</a>): u8
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="message.md#0xb_message_token_target_chain">token_target_chain</a>(self: &<a href="message.md#0xb_message_TokenPayload">TokenPayload</a>): u8 {
    self.target_chain
}
</code></pre>



</details>

<a name="0xb_message_token_target_address"></a>

## Function `token_target_address`



<pre><code><b>public</b> <b>fun</b> <a href="message.md#0xb_message_token_target_address">token_target_address</a>(self: &<a href="message.md#0xb_message_TokenPayload">message::TokenPayload</a>): <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="message.md#0xb_message_token_target_address">token_target_address</a>(self: &<a href="message.md#0xb_message_TokenPayload">TokenPayload</a>): <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt; {
    self.target_address
}
</code></pre>



</details>

<a name="0xb_message_token_type"></a>

## Function `token_type`



<pre><code><b>public</b> <b>fun</b> <a href="message.md#0xb_message_token_type">token_type</a>(self: &<a href="message.md#0xb_message_TokenPayload">message::TokenPayload</a>): u8
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="message.md#0xb_message_token_type">token_type</a>(self: &<a href="message.md#0xb_message_TokenPayload">TokenPayload</a>): u8 {
    self.token_type
}
</code></pre>



</details>

<a name="0xb_message_token_amount"></a>

## Function `token_amount`



<pre><code><b>public</b> <b>fun</b> <a href="message.md#0xb_message_token_amount">token_amount</a>(self: &<a href="message.md#0xb_message_TokenPayload">message::TokenPayload</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="message.md#0xb_message_token_amount">token_amount</a>(self: &<a href="message.md#0xb_message_TokenPayload">TokenPayload</a>): u64 {
    self.amount
}
</code></pre>



</details>

<a name="0xb_message_emergency_op_type"></a>

## Function `emergency_op_type`



<pre><code><b>public</b> <b>fun</b> <a href="message.md#0xb_message_emergency_op_type">emergency_op_type</a>(self: &<a href="message.md#0xb_message_EmergencyOp">message::EmergencyOp</a>): u8
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="message.md#0xb_message_emergency_op_type">emergency_op_type</a>(self: &<a href="message.md#0xb_message_EmergencyOp">EmergencyOp</a>): u8 {
    self.op_type
}
</code></pre>



</details>

<a name="0xb_message_blocklist_type"></a>

## Function `blocklist_type`



<pre><code><b>public</b> <b>fun</b> <a href="message.md#0xb_message_blocklist_type">blocklist_type</a>(self: &<a href="message.md#0xb_message_Blocklist">message::Blocklist</a>): u8
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="message.md#0xb_message_blocklist_type">blocklist_type</a>(self: &<a href="message.md#0xb_message_Blocklist">Blocklist</a>): u8 {
    self.blocklist_type
}
</code></pre>



</details>

<a name="0xb_message_blocklist_validator_addresses"></a>

## Function `blocklist_validator_addresses`



<pre><code><b>public</b> <b>fun</b> <a href="message.md#0xb_message_blocklist_validator_addresses">blocklist_validator_addresses</a>(self: &<a href="message.md#0xb_message_Blocklist">message::Blocklist</a>): &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="message.md#0xb_message_blocklist_validator_addresses">blocklist_validator_addresses</a>(self: &<a href="message.md#0xb_message_Blocklist">Blocklist</a>): &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;&gt; {
    &self.validator_eth_addresses
}
</code></pre>



</details>

<a name="0xb_message_update_bridge_limit_payload_sending_chain"></a>

## Function `update_bridge_limit_payload_sending_chain`



<pre><code><b>public</b> <b>fun</b> <a href="message.md#0xb_message_update_bridge_limit_payload_sending_chain">update_bridge_limit_payload_sending_chain</a>(self: &<a href="message.md#0xb_message_UpdateBridgeLimit">message::UpdateBridgeLimit</a>): u8
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="message.md#0xb_message_update_bridge_limit_payload_sending_chain">update_bridge_limit_payload_sending_chain</a>(self: &<a href="message.md#0xb_message_UpdateBridgeLimit">UpdateBridgeLimit</a>): u8 {
    self.sending_chain
}
</code></pre>



</details>

<a name="0xb_message_update_bridge_limit_payload_receiving_chain"></a>

## Function `update_bridge_limit_payload_receiving_chain`



<pre><code><b>public</b> <b>fun</b> <a href="message.md#0xb_message_update_bridge_limit_payload_receiving_chain">update_bridge_limit_payload_receiving_chain</a>(self: &<a href="message.md#0xb_message_UpdateBridgeLimit">message::UpdateBridgeLimit</a>): u8
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="message.md#0xb_message_update_bridge_limit_payload_receiving_chain">update_bridge_limit_payload_receiving_chain</a>(self: &<a href="message.md#0xb_message_UpdateBridgeLimit">UpdateBridgeLimit</a>): u8 {
    self.receiving_chain
}
</code></pre>



</details>

<a name="0xb_message_update_bridge_limit_payload_limit"></a>

## Function `update_bridge_limit_payload_limit`



<pre><code><b>public</b> <b>fun</b> <a href="message.md#0xb_message_update_bridge_limit_payload_limit">update_bridge_limit_payload_limit</a>(self: &<a href="message.md#0xb_message_UpdateBridgeLimit">message::UpdateBridgeLimit</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="message.md#0xb_message_update_bridge_limit_payload_limit">update_bridge_limit_payload_limit</a>(self: &<a href="message.md#0xb_message_UpdateBridgeLimit">UpdateBridgeLimit</a>): u64 {
    self.limit
}
</code></pre>



</details>

<a name="0xb_message_update_asset_price_payload_token_id"></a>

## Function `update_asset_price_payload_token_id`



<pre><code><b>public</b> <b>fun</b> <a href="message.md#0xb_message_update_asset_price_payload_token_id">update_asset_price_payload_token_id</a>(self: &<a href="message.md#0xb_message_UpdateAssetPrice">message::UpdateAssetPrice</a>): u8
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="message.md#0xb_message_update_asset_price_payload_token_id">update_asset_price_payload_token_id</a>(self: &<a href="message.md#0xb_message_UpdateAssetPrice">UpdateAssetPrice</a>): u8 {
    self.token_id
}
</code></pre>



</details>

<a name="0xb_message_update_asset_price_payload_new_price"></a>

## Function `update_asset_price_payload_new_price`



<pre><code><b>public</b> <b>fun</b> <a href="message.md#0xb_message_update_asset_price_payload_new_price">update_asset_price_payload_new_price</a>(self: &<a href="message.md#0xb_message_UpdateAssetPrice">message::UpdateAssetPrice</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="message.md#0xb_message_update_asset_price_payload_new_price">update_asset_price_payload_new_price</a>(self: &<a href="message.md#0xb_message_UpdateAssetPrice">UpdateAssetPrice</a>): u64 {
    self.new_price
}
</code></pre>



</details>

<a name="0xb_message_emergency_op_pause"></a>

## Function `emergency_op_pause`



<pre><code><b>public</b> <b>fun</b> <a href="message.md#0xb_message_emergency_op_pause">emergency_op_pause</a>(): u8
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="message.md#0xb_message_emergency_op_pause">emergency_op_pause</a>(): u8 {
    <a href="message.md#0xb_message_PAUSE">PAUSE</a>
}
</code></pre>



</details>

<a name="0xb_message_emergency_op_unpause"></a>

## Function `emergency_op_unpause`



<pre><code><b>public</b> <b>fun</b> <a href="message.md#0xb_message_emergency_op_unpause">emergency_op_unpause</a>(): u8
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="message.md#0xb_message_emergency_op_unpause">emergency_op_unpause</a>(): u8 {
    <a href="message.md#0xb_message_UNPAUSE">UNPAUSE</a>
}
</code></pre>



</details>

<a name="0xb_message_required_voting_power"></a>

## Function `required_voting_power`

Return the required signature threshold for the message, values are voting power in the scale of 10000


<pre><code><b>public</b> <b>fun</b> <a href="message.md#0xb_message_required_voting_power">required_voting_power</a>(self: &<a href="message.md#0xb_message_BridgeMessage">message::BridgeMessage</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="message.md#0xb_message_required_voting_power">required_voting_power</a>(self: &<a href="message.md#0xb_message_BridgeMessage">BridgeMessage</a>): u64 {
    <b>let</b> message_type = <a href="message.md#0xb_message_message_type">message_type</a>(self);

    <b>if</b> (message_type == <a href="message_types.md#0xb_message_types_token">message_types::token</a>()) {
        3334
    } <b>else</b> <b>if</b> (message_type == <a href="message_types.md#0xb_message_types_emergency_op">message_types::emergency_op</a>()) {
        <b>let</b> payload = <a href="message.md#0xb_message_extract_emergency_op_payload">extract_emergency_op_payload</a>(self);
        <b>if</b> (payload.op_type == <a href="message.md#0xb_message_PAUSE">PAUSE</a>) {
            450
        } <b>else</b> <b>if</b> (payload.op_type == <a href="message.md#0xb_message_UNPAUSE">UNPAUSE</a>) {
            5001
        } <b>else</b> {
            <b>abort</b> <a href="message.md#0xb_message_EInvalidEmergencyOpType">EInvalidEmergencyOpType</a>
        }
    } <b>else</b> <b>if</b> (message_type == <a href="message_types.md#0xb_message_types_committee_blocklist">message_types::committee_blocklist</a>()) {
        5001
    } <b>else</b> <b>if</b> (message_type == <a href="message_types.md#0xb_message_types_update_asset_price">message_types::update_asset_price</a>()) {
        5001
    } <b>else</b> <b>if</b> (message_type == <a href="message_types.md#0xb_message_types_update_bridge_limit">message_types::update_bridge_limit</a>()) {
        5001
    } <b>else</b> {
        <b>abort</b> <a href="message.md#0xb_message_EInvalidMessageType">EInvalidMessageType</a>
    }
}
</code></pre>



</details>

<a name="0xb_message_reverse_bytes"></a>

## Function `reverse_bytes`



<pre><code><b>fun</b> <a href="message.md#0xb_message_reverse_bytes">reverse_bytes</a>(bytes: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;): <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="message.md#0xb_message_reverse_bytes">reverse_bytes</a>(<b>mut</b> bytes: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;): <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt; {
    <a href="../move-stdlib/vector.md#0x1_vector_reverse">vector::reverse</a>(&<b>mut</b> bytes);
    bytes
}
</code></pre>



</details>

<a name="0xb_message_peel_u64_be"></a>

## Function `peel_u64_be`



<pre><code><b>fun</b> <a href="message.md#0xb_message_peel_u64_be">peel_u64_be</a>(<a href="../move-stdlib/bcs.md#0x1_bcs">bcs</a>: &<b>mut</b> bcs::BCS): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="message.md#0xb_message_peel_u64_be">peel_u64_be</a>(<a href="../move-stdlib/bcs.md#0x1_bcs">bcs</a>: &<b>mut</b> BCS): u64 {
    <b>let</b> (<b>mut</b> value, <b>mut</b> i) = (0u64, 64u8);
    <b>while</b> (i &gt; 0) {
        i = i - 8;
        <b>let</b> byte = (bcs::peel_u8(<a href="../move-stdlib/bcs.md#0x1_bcs">bcs</a>) <b>as</b> u64);
        value = value + (byte &lt;&lt; i);
    };
    value
}
</code></pre>



</details>
