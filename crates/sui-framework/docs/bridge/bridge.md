
---
title: Module `0xb::bridge`
---



-  [Resource `Bridge`](#0xb_bridge_Bridge)
-  [Struct `BridgeInner`](#0xb_bridge_BridgeInner)
-  [Struct `TokenBridgeEvent`](#0xb_bridge_TokenBridgeEvent)
-  [Struct `EmergencyOpEvent`](#0xb_bridge_EmergencyOpEvent)
-  [Struct `BridgeRecord`](#0xb_bridge_BridgeRecord)
-  [Struct `TokenTransferApproved`](#0xb_bridge_TokenTransferApproved)
-  [Struct `TokenTransferClaimed`](#0xb_bridge_TokenTransferClaimed)
-  [Struct `TokenTransferAlreadyApproved`](#0xb_bridge_TokenTransferAlreadyApproved)
-  [Struct `TokenTransferAlreadyClaimed`](#0xb_bridge_TokenTransferAlreadyClaimed)
-  [Constants](#@Constants_0)
-  [Function `create`](#0xb_bridge_create)
-  [Function `init_bridge_committee`](#0xb_bridge_init_bridge_committee)
-  [Function `committee_registration`](#0xb_bridge_committee_registration)
-  [Function `send_token`](#0xb_bridge_send_token)
-  [Function `approve_bridge_message`](#0xb_bridge_approve_bridge_message)
-  [Function `claim_token`](#0xb_bridge_claim_token)
-  [Function `claim_and_transfer_token`](#0xb_bridge_claim_and_transfer_token)
-  [Function `load_inner_mut`](#0xb_bridge_load_inner_mut)
-  [Function `load_inner`](#0xb_bridge_load_inner)
-  [Function `claim_token_internal`](#0xb_bridge_claim_token_internal)
-  [Function `execute_system_message`](#0xb_bridge_execute_system_message)
-  [Function `execute_emergency_op`](#0xb_bridge_execute_emergency_op)
-  [Function `execute_update_bridge_limit`](#0xb_bridge_execute_update_bridge_limit)
-  [Function `execute_update_asset_price`](#0xb_bridge_execute_update_asset_price)
-  [Function `get_current_seq_num_and_increment`](#0xb_bridge_get_current_seq_num_and_increment)
-  [Function `get_token_transfer_action_status`](#0xb_bridge_get_token_transfer_action_status)


<pre><code><b>use</b> <a href="../move-stdlib/option.md#0x1_option">0x1::option</a>;
<b>use</b> <a href="../sui-framework/address.md#0x2_address">0x2::address</a>;
<b>use</b> <a href="../sui-framework/balance.md#0x2_balance">0x2::balance</a>;
<b>use</b> <a href="../sui-framework/clock.md#0x2_clock">0x2::clock</a>;
<b>use</b> <a href="../sui-framework/coin.md#0x2_coin">0x2::coin</a>;
<b>use</b> <a href="../sui-framework/event.md#0x2_event">0x2::event</a>;
<b>use</b> <a href="../sui-framework/linked_table.md#0x2_linked_table">0x2::linked_table</a>;
<b>use</b> <a href="../sui-framework/object.md#0x2_object">0x2::object</a>;
<b>use</b> <a href="../sui-framework/transfer.md#0x2_transfer">0x2::transfer</a>;
<b>use</b> <a href="../sui-framework/tx_context.md#0x2_tx_context">0x2::tx_context</a>;
<b>use</b> <a href="../sui-framework/vec_map.md#0x2_vec_map">0x2::vec_map</a>;
<b>use</b> <a href="../sui-framework/versioned.md#0x2_versioned">0x2::versioned</a>;
<b>use</b> <a href="../sui-system/sui_system.md#0x3_sui_system">0x3::sui_system</a>;
<b>use</b> <a href="chain_ids.md#0xb_chain_ids">0xb::chain_ids</a>;
<b>use</b> <a href="committee.md#0xb_committee">0xb::committee</a>;
<b>use</b> <a href="limiter.md#0xb_limiter">0xb::limiter</a>;
<b>use</b> <a href="message.md#0xb_message">0xb::message</a>;
<b>use</b> <a href="message_types.md#0xb_message_types">0xb::message_types</a>;
<b>use</b> <a href="treasury.md#0xb_treasury">0xb::treasury</a>;
</code></pre>



<a name="0xb_bridge_Bridge"></a>

## Resource `Bridge`



<pre><code><b>struct</b> <a href="bridge.md#0xb_bridge_Bridge">Bridge</a> <b>has</b> key
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>id: <a href="../sui-framework/object.md#0x2_object_UID">object::UID</a></code>
</dt>
<dd>

</dd>
<dt>
<code>inner: <a href="../sui-framework/versioned.md#0x2_versioned_Versioned">versioned::Versioned</a></code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0xb_bridge_BridgeInner"></a>

## Struct `BridgeInner`



<pre><code><b>struct</b> <a href="bridge.md#0xb_bridge_BridgeInner">BridgeInner</a> <b>has</b> store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>bridge_version: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>message_version: u8</code>
</dt>
<dd>

</dd>
<dt>
<code>chain_id: u8</code>
</dt>
<dd>

</dd>
<dt>
<code>sequence_nums: <a href="../sui-framework/vec_map.md#0x2_vec_map_VecMap">vec_map::VecMap</a>&lt;u8, u64&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code><a href="committee.md#0xb_committee">committee</a>: <a href="committee.md#0xb_committee_BridgeCommittee">committee::BridgeCommittee</a></code>
</dt>
<dd>

</dd>
<dt>
<code><a href="treasury.md#0xb_treasury">treasury</a>: <a href="treasury.md#0xb_treasury_BridgeTreasury">treasury::BridgeTreasury</a></code>
</dt>
<dd>

</dd>
<dt>
<code>bridge_records: <a href="../sui-framework/linked_table.md#0x2_linked_table_LinkedTable">linked_table::LinkedTable</a>&lt;<a href="message.md#0xb_message_BridgeMessageKey">message::BridgeMessageKey</a>, <a href="bridge.md#0xb_bridge_BridgeRecord">bridge::BridgeRecord</a>&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code><a href="limiter.md#0xb_limiter">limiter</a>: <a href="limiter.md#0xb_limiter_TransferLimiter">limiter::TransferLimiter</a></code>
</dt>
<dd>

</dd>
<dt>
<code>paused: bool</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0xb_bridge_TokenBridgeEvent"></a>

## Struct `TokenBridgeEvent`



<pre><code><b>struct</b> <a href="bridge.md#0xb_bridge_TokenBridgeEvent">TokenBridgeEvent</a> <b>has</b> <b>copy</b>, drop
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

<a name="0xb_bridge_EmergencyOpEvent"></a>

## Struct `EmergencyOpEvent`



<pre><code><b>struct</b> <a href="bridge.md#0xb_bridge_EmergencyOpEvent">EmergencyOpEvent</a> <b>has</b> <b>copy</b>, drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>frozen: bool</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0xb_bridge_BridgeRecord"></a>

## Struct `BridgeRecord`



<pre><code><b>struct</b> <a href="bridge.md#0xb_bridge_BridgeRecord">BridgeRecord</a> <b>has</b> drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code><a href="message.md#0xb_message">message</a>: <a href="message.md#0xb_message_BridgeMessage">message::BridgeMessage</a></code>
</dt>
<dd>

</dd>
<dt>
<code>verified_signatures: <a href="../move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;&gt;&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>claimed: bool</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0xb_bridge_TokenTransferApproved"></a>

## Struct `TokenTransferApproved`



<pre><code><b>struct</b> <a href="bridge.md#0xb_bridge_TokenTransferApproved">TokenTransferApproved</a> <b>has</b> <b>copy</b>, drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>message_key: <a href="message.md#0xb_message_BridgeMessageKey">message::BridgeMessageKey</a></code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0xb_bridge_TokenTransferClaimed"></a>

## Struct `TokenTransferClaimed`



<pre><code><b>struct</b> <a href="bridge.md#0xb_bridge_TokenTransferClaimed">TokenTransferClaimed</a> <b>has</b> <b>copy</b>, drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>message_key: <a href="message.md#0xb_message_BridgeMessageKey">message::BridgeMessageKey</a></code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0xb_bridge_TokenTransferAlreadyApproved"></a>

## Struct `TokenTransferAlreadyApproved`



<pre><code><b>struct</b> <a href="bridge.md#0xb_bridge_TokenTransferAlreadyApproved">TokenTransferAlreadyApproved</a> <b>has</b> <b>copy</b>, drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>message_key: <a href="message.md#0xb_message_BridgeMessageKey">message::BridgeMessageKey</a></code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0xb_bridge_TokenTransferAlreadyClaimed"></a>

## Struct `TokenTransferAlreadyClaimed`



<pre><code><b>struct</b> <a href="bridge.md#0xb_bridge_TokenTransferAlreadyClaimed">TokenTransferAlreadyClaimed</a> <b>has</b> <b>copy</b>, drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>message_key: <a href="message.md#0xb_message_BridgeMessageKey">message::BridgeMessageKey</a></code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0xb_bridge_ENotSystemAddress"></a>



<pre><code><b>const</b> <a href="bridge.md#0xb_bridge_ENotSystemAddress">ENotSystemAddress</a>: u64 = 5;
</code></pre>



<a name="0xb_bridge_EWrongInnerVersion"></a>



<pre><code><b>const</b> <a href="bridge.md#0xb_bridge_EWrongInnerVersion">EWrongInnerVersion</a>: u64 = 7;
</code></pre>



<a name="0xb_bridge_CURRENT_VERSION"></a>



<pre><code><b>const</b> <a href="bridge.md#0xb_bridge_CURRENT_VERSION">CURRENT_VERSION</a>: u64 = 1;
</code></pre>



<a name="0xb_bridge_EBridgeAlreadyPaused"></a>



<pre><code><b>const</b> <a href="bridge.md#0xb_bridge_EBridgeAlreadyPaused">EBridgeAlreadyPaused</a>: u64 = 13;
</code></pre>



<a name="0xb_bridge_EBridgeNotPaused"></a>



<pre><code><b>const</b> <a href="bridge.md#0xb_bridge_EBridgeNotPaused">EBridgeNotPaused</a>: u64 = 14;
</code></pre>



<a name="0xb_bridge_EBridgeUnavailable"></a>



<pre><code><b>const</b> <a href="bridge.md#0xb_bridge_EBridgeUnavailable">EBridgeUnavailable</a>: u64 = 8;
</code></pre>



<a name="0xb_bridge_EInvariantSuiInitializedTokenTransferShouldNotBeClaimed"></a>



<pre><code><b>const</b> <a href="bridge.md#0xb_bridge_EInvariantSuiInitializedTokenTransferShouldNotBeClaimed">EInvariantSuiInitializedTokenTransferShouldNotBeClaimed</a>: u64 = 10;
</code></pre>



<a name="0xb_bridge_EMalformedMessageError"></a>



<pre><code><b>const</b> <a href="bridge.md#0xb_bridge_EMalformedMessageError">EMalformedMessageError</a>: u64 = 2;
</code></pre>



<a name="0xb_bridge_EMessageNotFoundInRecords"></a>



<pre><code><b>const</b> <a href="bridge.md#0xb_bridge_EMessageNotFoundInRecords">EMessageNotFoundInRecords</a>: u64 = 11;
</code></pre>



<a name="0xb_bridge_ETokenAlreadyClaimed"></a>



<pre><code><b>const</b> <a href="bridge.md#0xb_bridge_ETokenAlreadyClaimed">ETokenAlreadyClaimed</a>: u64 = 15;
</code></pre>



<a name="0xb_bridge_EUnauthorisedClaim"></a>



<pre><code><b>const</b> <a href="bridge.md#0xb_bridge_EUnauthorisedClaim">EUnauthorisedClaim</a>: u64 = 1;
</code></pre>



<a name="0xb_bridge_EUnexpectedChainID"></a>



<pre><code><b>const</b> <a href="bridge.md#0xb_bridge_EUnexpectedChainID">EUnexpectedChainID</a>: u64 = 4;
</code></pre>



<a name="0xb_bridge_EUnexpectedMessageType"></a>



<pre><code><b>const</b> <a href="bridge.md#0xb_bridge_EUnexpectedMessageType">EUnexpectedMessageType</a>: u64 = 0;
</code></pre>



<a name="0xb_bridge_EUnexpectedMessageVersion"></a>



<pre><code><b>const</b> <a href="bridge.md#0xb_bridge_EUnexpectedMessageVersion">EUnexpectedMessageVersion</a>: u64 = 12;
</code></pre>



<a name="0xb_bridge_EUnexpectedOperation"></a>



<pre><code><b>const</b> <a href="bridge.md#0xb_bridge_EUnexpectedOperation">EUnexpectedOperation</a>: u64 = 9;
</code></pre>



<a name="0xb_bridge_EUnexpectedSeqNum"></a>



<pre><code><b>const</b> <a href="bridge.md#0xb_bridge_EUnexpectedSeqNum">EUnexpectedSeqNum</a>: u64 = 6;
</code></pre>



<a name="0xb_bridge_EUnexpectedTokenType"></a>



<pre><code><b>const</b> <a href="bridge.md#0xb_bridge_EUnexpectedTokenType">EUnexpectedTokenType</a>: u64 = 3;
</code></pre>



<a name="0xb_bridge_MESSAGE_VERSION"></a>



<pre><code><b>const</b> <a href="bridge.md#0xb_bridge_MESSAGE_VERSION">MESSAGE_VERSION</a>: u8 = 1;
</code></pre>



<a name="0xb_bridge_TRANSFER_STATUS_APPROVED"></a>



<pre><code><b>const</b> <a href="bridge.md#0xb_bridge_TRANSFER_STATUS_APPROVED">TRANSFER_STATUS_APPROVED</a>: u8 = 1;
</code></pre>



<a name="0xb_bridge_TRANSFER_STATUS_CLAIMED"></a>



<pre><code><b>const</b> <a href="bridge.md#0xb_bridge_TRANSFER_STATUS_CLAIMED">TRANSFER_STATUS_CLAIMED</a>: u8 = 2;
</code></pre>



<a name="0xb_bridge_TRANSFER_STATUS_NOT_FOUND"></a>



<pre><code><b>const</b> <a href="bridge.md#0xb_bridge_TRANSFER_STATUS_NOT_FOUND">TRANSFER_STATUS_NOT_FOUND</a>: u8 = 3;
</code></pre>



<a name="0xb_bridge_TRANSFER_STATUS_PENDING"></a>



<pre><code><b>const</b> <a href="bridge.md#0xb_bridge_TRANSFER_STATUS_PENDING">TRANSFER_STATUS_PENDING</a>: u8 = 0;
</code></pre>



<a name="0xb_bridge_create"></a>

## Function `create`



<pre><code><b>fun</b> <a href="bridge.md#0xb_bridge_create">create</a>(id: <a href="../sui-framework/object.md#0x2_object_UID">object::UID</a>, chain_id: u8, ctx: &<b>mut</b> <a href="../sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="bridge.md#0xb_bridge_create">create</a>(id: UID, chain_id: u8, ctx: &<b>mut</b> TxContext) {
    <b>assert</b>!(<a href="../sui-framework/tx_context.md#0x2_tx_context_sender">tx_context::sender</a>(ctx) == @0x0, <a href="bridge.md#0xb_bridge_ENotSystemAddress">ENotSystemAddress</a>);
    <b>let</b> bridge_inner = <a href="bridge.md#0xb_bridge_BridgeInner">BridgeInner</a> {
        bridge_version: <a href="bridge.md#0xb_bridge_CURRENT_VERSION">CURRENT_VERSION</a>,
        message_version: <a href="bridge.md#0xb_bridge_MESSAGE_VERSION">MESSAGE_VERSION</a>,
        chain_id,
        sequence_nums: <a href="../sui-framework/vec_map.md#0x2_vec_map_empty">vec_map::empty</a>(),
        <a href="committee.md#0xb_committee">committee</a>: <a href="committee.md#0xb_committee_create">committee::create</a>(ctx),
        <a href="treasury.md#0xb_treasury">treasury</a>: <a href="treasury.md#0xb_treasury_create">treasury::create</a>(ctx),
        bridge_records: <a href="../sui-framework/linked_table.md#0x2_linked_table_new">linked_table::new</a>(ctx),
        <a href="limiter.md#0xb_limiter">limiter</a>: <a href="limiter.md#0xb_limiter_new">limiter::new</a>(),
        paused: <b>false</b>,
    };
    <b>let</b> <a href="bridge.md#0xb_bridge">bridge</a> = <a href="bridge.md#0xb_bridge_Bridge">Bridge</a> {
        id,
        inner: <a href="../sui-framework/versioned.md#0x2_versioned_create">versioned::create</a>(<a href="bridge.md#0xb_bridge_CURRENT_VERSION">CURRENT_VERSION</a>, bridge_inner, ctx)
    };
    <a href="../sui-framework/transfer.md#0x2_transfer_share_object">transfer::share_object</a>(<a href="bridge.md#0xb_bridge">bridge</a>);
}
</code></pre>



</details>

<a name="0xb_bridge_init_bridge_committee"></a>

## Function `init_bridge_committee`



<pre><code><b>fun</b> <a href="bridge.md#0xb_bridge_init_bridge_committee">init_bridge_committee</a>(self: &<b>mut</b> <a href="bridge.md#0xb_bridge_Bridge">bridge::Bridge</a>, active_validator_voting_power: <a href="../sui-framework/vec_map.md#0x2_vec_map_VecMap">vec_map::VecMap</a>&lt;<b>address</b>, u64&gt;, min_stake_participation_percentage: u64, ctx: &<a href="../sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="bridge.md#0xb_bridge_init_bridge_committee">init_bridge_committee</a>(
    self: &<b>mut</b> <a href="bridge.md#0xb_bridge_Bridge">Bridge</a>,
    active_validator_voting_power: VecMap&lt;<b>address</b>, u64&gt;,
    min_stake_participation_percentage: u64,
    ctx: &TxContext
) {
    <b>assert</b>!(<a href="../sui-framework/tx_context.md#0x2_tx_context_sender">tx_context::sender</a>(ctx) == @0x0, <a href="bridge.md#0xb_bridge_ENotSystemAddress">ENotSystemAddress</a>);
    <b>let</b> inner = <a href="bridge.md#0xb_bridge_load_inner_mut">load_inner_mut</a>(self);
    <b>if</b> (<a href="../sui-framework/vec_map.md#0x2_vec_map_is_empty">vec_map::is_empty</a>(<a href="committee.md#0xb_committee_committee_members">committee::committee_members</a>(&inner.<a href="committee.md#0xb_committee">committee</a>))) {
        <a href="committee.md#0xb_committee_try_create_next_committee">committee::try_create_next_committee</a>(
            &<b>mut</b> inner.<a href="committee.md#0xb_committee">committee</a>,
            active_validator_voting_power,
            min_stake_participation_percentage,
            ctx
        )
    }
}
</code></pre>



</details>

<a name="0xb_bridge_committee_registration"></a>

## Function `committee_registration`



<pre><code><b>public</b> <b>fun</b> <a href="bridge.md#0xb_bridge_committee_registration">committee_registration</a>(self: &<b>mut</b> <a href="bridge.md#0xb_bridge_Bridge">bridge::Bridge</a>, system_state: &<b>mut</b> <a href="../sui-system/sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, bridge_pubkey_bytes: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, http_rest_url: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, ctx: &<a href="../sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="bridge.md#0xb_bridge_committee_registration">committee_registration</a>(self: &<b>mut</b> <a href="bridge.md#0xb_bridge_Bridge">Bridge</a>,
                                  system_state: &<b>mut</b> SuiSystemState,
                                  bridge_pubkey_bytes: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
                                  http_rest_url: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
                                  ctx: &TxContext) {
    <b>let</b> inner = <a href="bridge.md#0xb_bridge_load_inner_mut">load_inner_mut</a>(self);
    <a href="committee.md#0xb_committee_register">committee::register</a>(&<b>mut</b> inner.<a href="committee.md#0xb_committee">committee</a>, system_state, bridge_pubkey_bytes, http_rest_url, ctx)
}
</code></pre>



</details>

<a name="0xb_bridge_send_token"></a>

## Function `send_token`



<pre><code><b>public</b> <b>fun</b> <a href="bridge.md#0xb_bridge_send_token">send_token</a>&lt;T&gt;(self: &<b>mut</b> <a href="bridge.md#0xb_bridge_Bridge">bridge::Bridge</a>, target_chain: u8, target_address: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, token: <a href="../sui-framework/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;, ctx: &<b>mut</b> <a href="../sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="bridge.md#0xb_bridge_send_token">send_token</a>&lt;T&gt;(
    self: &<b>mut</b> <a href="bridge.md#0xb_bridge_Bridge">Bridge</a>,
    target_chain: u8,
    target_address: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    token: Coin&lt;T&gt;,
    ctx: &<b>mut</b> TxContext
) {
    <b>let</b> inner = <a href="bridge.md#0xb_bridge_load_inner_mut">load_inner_mut</a>(self);
    <b>assert</b>!(!inner.paused, <a href="bridge.md#0xb_bridge_EBridgeUnavailable">EBridgeUnavailable</a>);
    <b>let</b> amount = <a href="../sui-framework/balance.md#0x2_balance_value">balance::value</a>(<a href="../sui-framework/coin.md#0x2_coin_balance">coin::balance</a>(&token));

    <b>let</b> bridge_seq_num = <a href="bridge.md#0xb_bridge_get_current_seq_num_and_increment">get_current_seq_num_and_increment</a>(inner, <a href="message_types.md#0xb_message_types_token">message_types::token</a>());
    <b>let</b> token_id = <a href="treasury.md#0xb_treasury_token_id">treasury::token_id</a>&lt;T&gt;();
    <b>let</b> token_amount = <a href="../sui-framework/balance.md#0x2_balance_value">balance::value</a>(<a href="../sui-framework/coin.md#0x2_coin_balance">coin::balance</a>(&token));

    // create <a href="bridge.md#0xb_bridge">bridge</a> <a href="message.md#0xb_message">message</a>
    <b>let</b> <a href="message.md#0xb_message">message</a> = <a href="message.md#0xb_message_create_token_bridge_message">message::create_token_bridge_message</a>(
        inner.chain_id,
        bridge_seq_num,
        address::to_bytes(<a href="../sui-framework/tx_context.md#0x2_tx_context_sender">tx_context::sender</a>(ctx)),
        target_chain,
        target_address,
        token_id,
        amount,
    );

    // burn / escrow token, unsupported coins will fail in this step
    <a href="treasury.md#0xb_treasury_burn">treasury::burn</a>(&<b>mut</b> inner.<a href="treasury.md#0xb_treasury">treasury</a>, token, ctx);

    // Store pending <a href="bridge.md#0xb_bridge">bridge</a> request
    <b>let</b> key = <a href="message.md#0xb_message_key">message::key</a>(&<a href="message.md#0xb_message">message</a>);
    <a href="../sui-framework/linked_table.md#0x2_linked_table_push_back">linked_table::push_back</a>(&<b>mut</b> inner.bridge_records, key, <a href="bridge.md#0xb_bridge_BridgeRecord">BridgeRecord</a> {
        <a href="message.md#0xb_message">message</a>,
        verified_signatures: none(),
        claimed: <b>false</b>,
    });

    // emit <a href="../sui-framework/event.md#0x2_event">event</a>
    emit(<a href="bridge.md#0xb_bridge_TokenBridgeEvent">TokenBridgeEvent</a> {
        message_type: <a href="message_types.md#0xb_message_types_token">message_types::token</a>(),
        seq_num: bridge_seq_num,
        source_chain: inner.chain_id,
        sender_address: address::to_bytes(<a href="../sui-framework/tx_context.md#0x2_tx_context_sender">tx_context::sender</a>(ctx)),
        target_chain,
        target_address,
        token_type: token_id,
        amount: token_amount,
    });
}
</code></pre>



</details>

<a name="0xb_bridge_approve_bridge_message"></a>

## Function `approve_bridge_message`



<pre><code><b>public</b> <b>fun</b> <a href="bridge.md#0xb_bridge_approve_bridge_message">approve_bridge_message</a>(self: &<b>mut</b> <a href="bridge.md#0xb_bridge_Bridge">bridge::Bridge</a>, <a href="message.md#0xb_message">message</a>: <a href="message.md#0xb_message_BridgeMessage">message::BridgeMessage</a>, signatures: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="bridge.md#0xb_bridge_approve_bridge_message">approve_bridge_message</a>(
    self: &<b>mut</b> <a href="bridge.md#0xb_bridge_Bridge">Bridge</a>,
    <a href="message.md#0xb_message">message</a>: BridgeMessage,
    signatures: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;&gt;,
) {
    // FIXME: need <b>to</b> check pause
    <b>let</b> inner = <a href="bridge.md#0xb_bridge_load_inner_mut">load_inner_mut</a>(self);
    <b>let</b> key = <a href="message.md#0xb_message_key">message::key</a>(&<a href="message.md#0xb_message">message</a>);
    // TODO: test this
    <b>assert</b>!(<a href="message.md#0xb_message_message_version">message::message_version</a>(&<a href="message.md#0xb_message">message</a>) == <a href="bridge.md#0xb_bridge_MESSAGE_VERSION">MESSAGE_VERSION</a>, <a href="bridge.md#0xb_bridge_EUnexpectedMessageVersion">EUnexpectedMessageVersion</a>);

    // retrieve pending <a href="message.md#0xb_message">message</a> <b>if</b> source chain is Sui, the initial <a href="message.md#0xb_message">message</a> must exist on chain.
    <b>if</b> (<a href="message.md#0xb_message_message_type">message::message_type</a>(&<a href="message.md#0xb_message">message</a>) == <a href="message_types.md#0xb_message_types_token">message_types::token</a>() && <a href="message.md#0xb_message_source_chain">message::source_chain</a>(
        &<a href="message.md#0xb_message">message</a>
    ) == inner.chain_id) {
        <b>let</b> record = <a href="../sui-framework/linked_table.md#0x2_linked_table_borrow_mut">linked_table::borrow_mut</a>(&<b>mut</b> inner.bridge_records, key);
        <b>assert</b>!(record.<a href="message.md#0xb_message">message</a> == <a href="message.md#0xb_message">message</a>, <a href="bridge.md#0xb_bridge_EMalformedMessageError">EMalformedMessageError</a>);
        <b>assert</b>!(!record.claimed, <a href="bridge.md#0xb_bridge_EInvariantSuiInitializedTokenTransferShouldNotBeClaimed">EInvariantSuiInitializedTokenTransferShouldNotBeClaimed</a>);

        // If record already <b>has</b> verified signatures, it means the <a href="message.md#0xb_message">message</a> <b>has</b> been approved.
        // Then we exit early.
        <b>if</b> (<a href="../move-stdlib/option.md#0x1_option_is_some">option::is_some</a>(&record.verified_signatures)) {
            emit(<a href="bridge.md#0xb_bridge_TokenTransferAlreadyApproved">TokenTransferAlreadyApproved</a> { message_key: key });
            <b>return</b>
        };
        // verify signatures
        <a href="committee.md#0xb_committee_verify_signatures">committee::verify_signatures</a>(&inner.<a href="committee.md#0xb_committee">committee</a>, <a href="message.md#0xb_message">message</a>, signatures);
        // Store approval
        record.verified_signatures = some(signatures)
    } <b>else</b> {
        // At this point, <b>if</b> this <a href="message.md#0xb_message">message</a> is in bridge_records, we know it's already approved
        // because we only add a <a href="message.md#0xb_message">message</a> <b>to</b> bridge_records after verifying the signatures.
        <b>if</b> (<a href="../sui-framework/linked_table.md#0x2_linked_table_contains">linked_table::contains</a>(&inner.bridge_records, key)) {
            emit(<a href="bridge.md#0xb_bridge_TokenTransferAlreadyApproved">TokenTransferAlreadyApproved</a> { message_key: key });
            <b>return</b>
        };
        // verify signatures
        <a href="committee.md#0xb_committee_verify_signatures">committee::verify_signatures</a>(&inner.<a href="committee.md#0xb_committee">committee</a>, <a href="message.md#0xb_message">message</a>, signatures);
        // Store <a href="message.md#0xb_message">message</a> and approval
        <a href="../sui-framework/linked_table.md#0x2_linked_table_push_back">linked_table::push_back</a>(&<b>mut</b> inner.bridge_records, key, <a href="bridge.md#0xb_bridge_BridgeRecord">BridgeRecord</a> {
            <a href="message.md#0xb_message">message</a>,
            verified_signatures: some(signatures),
            claimed: <b>false</b>
        });
    };
    emit(<a href="bridge.md#0xb_bridge_TokenTransferApproved">TokenTransferApproved</a> { message_key: key });
}
</code></pre>



</details>

<a name="0xb_bridge_claim_token"></a>

## Function `claim_token`



<pre><code><b>public</b> <b>fun</b> <a href="bridge.md#0xb_bridge_claim_token">claim_token</a>&lt;T&gt;(self: &<b>mut</b> <a href="bridge.md#0xb_bridge_Bridge">bridge::Bridge</a>, <a href="../sui-framework/clock.md#0x2_clock">clock</a>: &<a href="../sui-framework/clock.md#0x2_clock_Clock">clock::Clock</a>, source_chain: u8, bridge_seq_num: u64, ctx: &<b>mut</b> <a href="../sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="../sui-framework/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="bridge.md#0xb_bridge_claim_token">claim_token</a>&lt;T&gt;(self: &<b>mut</b> <a href="bridge.md#0xb_bridge_Bridge">Bridge</a>, <a href="../sui-framework/clock.md#0x2_clock">clock</a>: &Clock, source_chain: u8, bridge_seq_num: u64, ctx: &<b>mut</b> TxContext): Coin&lt;T&gt; {
    <b>let</b> (maybe_token, owner) = <a href="bridge.md#0xb_bridge_claim_token_internal">claim_token_internal</a>&lt;T&gt;(<a href="../sui-framework/clock.md#0x2_clock">clock</a>, self, source_chain, bridge_seq_num, ctx);
    // Only token owner can claim the token
    <b>assert</b>!(<a href="../sui-framework/tx_context.md#0x2_tx_context_sender">tx_context::sender</a>(ctx) == owner, <a href="bridge.md#0xb_bridge_EUnauthorisedClaim">EUnauthorisedClaim</a>);
    <b>assert</b>!(<a href="../move-stdlib/option.md#0x1_option_is_some">option::is_some</a>(&maybe_token), <a href="bridge.md#0xb_bridge_ETokenAlreadyClaimed">ETokenAlreadyClaimed</a>);
    <a href="../move-stdlib/option.md#0x1_option_destroy_some">option::destroy_some</a>(maybe_token)
}
</code></pre>



</details>

<a name="0xb_bridge_claim_and_transfer_token"></a>

## Function `claim_and_transfer_token`



<pre><code><b>public</b> <b>fun</b> <a href="bridge.md#0xb_bridge_claim_and_transfer_token">claim_and_transfer_token</a>&lt;T&gt;(self: &<b>mut</b> <a href="bridge.md#0xb_bridge_Bridge">bridge::Bridge</a>, <a href="../sui-framework/clock.md#0x2_clock">clock</a>: &<a href="../sui-framework/clock.md#0x2_clock_Clock">clock::Clock</a>, source_chain: u8, bridge_seq_num: u64, ctx: &<b>mut</b> <a href="../sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="bridge.md#0xb_bridge_claim_and_transfer_token">claim_and_transfer_token</a>&lt;T&gt;(
    self: &<b>mut</b> <a href="bridge.md#0xb_bridge_Bridge">Bridge</a>,
    <a href="../sui-framework/clock.md#0x2_clock">clock</a>: &Clock,
    source_chain: u8,
    bridge_seq_num: u64,
    ctx: &<b>mut</b> TxContext
) {
    <b>let</b> (token, owner) = <a href="bridge.md#0xb_bridge_claim_token_internal">claim_token_internal</a>&lt;T&gt;(<a href="../sui-framework/clock.md#0x2_clock">clock</a>, self, source_chain, bridge_seq_num, ctx);
    <b>if</b> (<a href="../move-stdlib/option.md#0x1_option_is_none">option::is_none</a>(&token)) {
        <a href="../move-stdlib/option.md#0x1_option_destroy_none">option::destroy_none</a>(token);
        <b>let</b> key = <a href="message.md#0xb_message_create_key">message::create_key</a>(source_chain, <a href="message_types.md#0xb_message_types_token">message_types::token</a>(), bridge_seq_num);
        emit(<a href="bridge.md#0xb_bridge_TokenTransferAlreadyClaimed">TokenTransferAlreadyClaimed</a> { message_key: key });
        <b>return</b>
    };
    <a href="../sui-framework/transfer.md#0x2_transfer_public_transfer">transfer::public_transfer</a>(<a href="../move-stdlib/option.md#0x1_option_destroy_some">option::destroy_some</a>(token), owner)
}
</code></pre>



</details>

<a name="0xb_bridge_load_inner_mut"></a>

## Function `load_inner_mut`



<pre><code><b>fun</b> <a href="bridge.md#0xb_bridge_load_inner_mut">load_inner_mut</a>(self: &<b>mut</b> <a href="bridge.md#0xb_bridge_Bridge">bridge::Bridge</a>): &<b>mut</b> <a href="bridge.md#0xb_bridge_BridgeInner">bridge::BridgeInner</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="bridge.md#0xb_bridge_load_inner_mut">load_inner_mut</a>(self: &<b>mut</b> <a href="bridge.md#0xb_bridge_Bridge">Bridge</a>): &<b>mut</b> <a href="bridge.md#0xb_bridge_BridgeInner">BridgeInner</a> {
    <b>let</b> version = <a href="../sui-framework/versioned.md#0x2_versioned_version">versioned::version</a>(&self.inner);
    // TODO: Replace this <b>with</b> a lazy <b>update</b> function when we add a new version of the inner <a href="../sui-framework/object.md#0x2_object">object</a>.
    <b>assert</b>!(version == <a href="bridge.md#0xb_bridge_CURRENT_VERSION">CURRENT_VERSION</a>, <a href="bridge.md#0xb_bridge_EWrongInnerVersion">EWrongInnerVersion</a>);
    <b>let</b> inner: &<b>mut</b> <a href="bridge.md#0xb_bridge_BridgeInner">BridgeInner</a> = <a href="../sui-framework/versioned.md#0x2_versioned_load_value_mut">versioned::load_value_mut</a>(&<b>mut</b> self.inner);
    <b>assert</b>!(inner.bridge_version == version, <a href="bridge.md#0xb_bridge_EWrongInnerVersion">EWrongInnerVersion</a>);
    inner
}
</code></pre>



</details>

<a name="0xb_bridge_load_inner"></a>

## Function `load_inner`



<pre><code><b>fun</b> <a href="bridge.md#0xb_bridge_load_inner">load_inner</a>(self: &<a href="bridge.md#0xb_bridge_Bridge">bridge::Bridge</a>): &<a href="bridge.md#0xb_bridge_BridgeInner">bridge::BridgeInner</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="bridge.md#0xb_bridge_load_inner">load_inner</a>(
    self: &<a href="bridge.md#0xb_bridge_Bridge">Bridge</a>,
): &<a href="bridge.md#0xb_bridge_BridgeInner">BridgeInner</a> {
    <b>let</b> version = <a href="../sui-framework/versioned.md#0x2_versioned_version">versioned::version</a>(&self.inner);

    // TODO: Replace this <b>with</b> a lazy <b>update</b> function when we add a new version of the inner <a href="../sui-framework/object.md#0x2_object">object</a>.
    <b>assert</b>!(version == <a href="bridge.md#0xb_bridge_CURRENT_VERSION">CURRENT_VERSION</a>, <a href="bridge.md#0xb_bridge_EWrongInnerVersion">EWrongInnerVersion</a>);
    <b>let</b> inner: &<a href="bridge.md#0xb_bridge_BridgeInner">BridgeInner</a> = <a href="../sui-framework/versioned.md#0x2_versioned_load_value">versioned::load_value</a>(&self.inner);
    <b>assert</b>!(inner.bridge_version == version, <a href="bridge.md#0xb_bridge_EWrongInnerVersion">EWrongInnerVersion</a>);
    inner
}
</code></pre>



</details>

<a name="0xb_bridge_claim_token_internal"></a>

## Function `claim_token_internal`



<pre><code><b>fun</b> <a href="bridge.md#0xb_bridge_claim_token_internal">claim_token_internal</a>&lt;T&gt;(<a href="../sui-framework/clock.md#0x2_clock">clock</a>: &<a href="../sui-framework/clock.md#0x2_clock_Clock">clock::Clock</a>, self: &<b>mut</b> <a href="bridge.md#0xb_bridge_Bridge">bridge::Bridge</a>, source_chain: u8, bridge_seq_num: u64, ctx: &<b>mut</b> <a href="../sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): (<a href="../move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;<a href="../sui-framework/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;&gt;, <b>address</b>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="bridge.md#0xb_bridge_claim_token_internal">claim_token_internal</a>&lt;T&gt;(
    <a href="../sui-framework/clock.md#0x2_clock">clock</a>: &Clock,
    self: &<b>mut</b> <a href="bridge.md#0xb_bridge_Bridge">Bridge</a>,
    source_chain: u8,
    bridge_seq_num: u64,
    ctx: &<b>mut</b> TxContext
): (Option&lt;Coin&lt;T&gt;&gt;, <b>address</b>) {
    <b>let</b> inner = <a href="bridge.md#0xb_bridge_load_inner_mut">load_inner_mut</a>(self);
    <b>assert</b>!(!inner.paused, <a href="bridge.md#0xb_bridge_EBridgeUnavailable">EBridgeUnavailable</a>);

    <b>let</b> key = <a href="message.md#0xb_message_create_key">message::create_key</a>(source_chain, <a href="message_types.md#0xb_message_types_token">message_types::token</a>(), bridge_seq_num);
    <b>assert</b>!(<a href="../sui-framework/linked_table.md#0x2_linked_table_contains">linked_table::contains</a>(&inner.bridge_records, key), <a href="bridge.md#0xb_bridge_EMessageNotFoundInRecords">EMessageNotFoundInRecords</a>);

    // retrieve approved <a href="bridge.md#0xb_bridge">bridge</a> <a href="message.md#0xb_message">message</a>
    <b>let</b> record = <a href="../sui-framework/linked_table.md#0x2_linked_table_borrow_mut">linked_table::borrow_mut</a>(&<b>mut</b> inner.bridge_records, key);
    // ensure this is a token <a href="bridge.md#0xb_bridge">bridge</a> <a href="message.md#0xb_message">message</a>
    <b>assert</b>!(<a href="message.md#0xb_message_message_type">message::message_type</a>(&record.<a href="message.md#0xb_message">message</a>) == <a href="message_types.md#0xb_message_types_token">message_types::token</a>(), <a href="bridge.md#0xb_bridge_EUnexpectedMessageType">EUnexpectedMessageType</a>);
    // Ensure it's signed
    <b>assert</b>!(<a href="../move-stdlib/option.md#0x1_option_is_some">option::is_some</a>(&record.verified_signatures), <a href="bridge.md#0xb_bridge_EUnauthorisedClaim">EUnauthorisedClaim</a>);

    // extract token <a href="message.md#0xb_message">message</a>
    <b>let</b> token_payload = <a href="message.md#0xb_message_extract_token_bridge_payload">message::extract_token_bridge_payload</a>(&record.<a href="message.md#0xb_message">message</a>);
    // get owner <b>address</b>
    <b>let</b> owner = address::from_bytes(<a href="message.md#0xb_message_token_target_address">message::token_target_address</a>(&token_payload));

    // If already claimed, exit early
    <b>if</b> (record.claimed) {
        <b>return</b> (<a href="../move-stdlib/option.md#0x1_option_none">option::none</a>(), owner)
    };

    <b>let</b> target_chain = <a href="message.md#0xb_message_token_target_chain">message::token_target_chain</a>(&token_payload);
    // ensure target chain matches self.chain_id
    <b>assert</b>!(target_chain == inner.chain_id, <a href="bridge.md#0xb_bridge_EUnexpectedChainID">EUnexpectedChainID</a>);

    // TODO: why do we check validity of the route here? what <b>if</b> inconsistency?
    // Ensure route is valid
    // TODO: add unit tests
    // `get_route` <b>abort</b> <b>if</b> route is invalid
    <b>let</b> route = <a href="chain_ids.md#0xb_chain_ids_get_route">chain_ids::get_route</a>(source_chain, target_chain);
    // get owner <b>address</b>
    <b>let</b> owner = address::from_bytes(<a href="message.md#0xb_message_token_target_address">message::token_target_address</a>(&token_payload));
    // check token type
    <b>assert</b>!(<a href="treasury.md#0xb_treasury_token_id">treasury::token_id</a>&lt;T&gt;() == <a href="message.md#0xb_message_token_type">message::token_type</a>(&token_payload), <a href="bridge.md#0xb_bridge_EUnexpectedTokenType">EUnexpectedTokenType</a>);
    <b>let</b> amount = <a href="message.md#0xb_message_token_amount">message::token_amount</a>(&token_payload);
    // Make sure <a href="../sui-framework/transfer.md#0x2_transfer">transfer</a> is within limit.
    <b>if</b> (!<a href="limiter.md#0xb_limiter_check_and_record_sending_transfer">limiter::check_and_record_sending_transfer</a>&lt;T&gt;(&<b>mut</b> inner.<a href="limiter.md#0xb_limiter">limiter</a>, <a href="../sui-framework/clock.md#0x2_clock">clock</a>, route, amount)) {
        <b>return</b> (<a href="../move-stdlib/option.md#0x1_option_none">option::none</a>(), owner)
    };
    // claim from <a href="treasury.md#0xb_treasury">treasury</a>
    <b>let</b> token = <a href="treasury.md#0xb_treasury_mint">treasury::mint</a>&lt;T&gt;(&<b>mut</b> inner.<a href="treasury.md#0xb_treasury">treasury</a>, amount, ctx);
    // Record changes
    record.claimed = <b>true</b>;
    emit(<a href="bridge.md#0xb_bridge_TokenTransferClaimed">TokenTransferClaimed</a> { message_key: key });
    (<a href="../move-stdlib/option.md#0x1_option_some">option::some</a>(token), owner)
}
</code></pre>



</details>

<a name="0xb_bridge_execute_system_message"></a>

## Function `execute_system_message`



<pre><code><b>public</b> <b>fun</b> <a href="bridge.md#0xb_bridge_execute_system_message">execute_system_message</a>(self: &<b>mut</b> <a href="bridge.md#0xb_bridge_Bridge">bridge::Bridge</a>, <a href="message.md#0xb_message">message</a>: <a href="message.md#0xb_message_BridgeMessage">message::BridgeMessage</a>, signatures: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="bridge.md#0xb_bridge_execute_system_message">execute_system_message</a>(
    self: &<b>mut</b> <a href="bridge.md#0xb_bridge_Bridge">Bridge</a>,
    <a href="message.md#0xb_message">message</a>: BridgeMessage,
    signatures: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;&gt;,
) {
    <b>let</b> message_type = <a href="message.md#0xb_message_message_type">message::message_type</a>(&<a href="message.md#0xb_message">message</a>);

    // TODO: test version mismatch
    <b>assert</b>!(<a href="message.md#0xb_message_message_version">message::message_version</a>(&<a href="message.md#0xb_message">message</a>) == <a href="bridge.md#0xb_bridge_MESSAGE_VERSION">MESSAGE_VERSION</a>, <a href="bridge.md#0xb_bridge_EUnexpectedMessageVersion">EUnexpectedMessageVersion</a>);
    <b>let</b> inner = <a href="bridge.md#0xb_bridge_load_inner_mut">load_inner_mut</a>(self);

    <b>assert</b>!(<a href="message.md#0xb_message_source_chain">message::source_chain</a>(&<a href="message.md#0xb_message">message</a>) == inner.chain_id, <a href="bridge.md#0xb_bridge_EUnexpectedChainID">EUnexpectedChainID</a>);

    // check system ops seq number and increment it
    <b>let</b> expected_seq_num = <a href="bridge.md#0xb_bridge_get_current_seq_num_and_increment">get_current_seq_num_and_increment</a>(inner, message_type);
    <b>assert</b>!(<a href="message.md#0xb_message_seq_num">message::seq_num</a>(&<a href="message.md#0xb_message">message</a>) == expected_seq_num, <a href="bridge.md#0xb_bridge_EUnexpectedSeqNum">EUnexpectedSeqNum</a>);

    <a href="committee.md#0xb_committee_verify_signatures">committee::verify_signatures</a>(&inner.<a href="committee.md#0xb_committee">committee</a>, <a href="message.md#0xb_message">message</a>, signatures);

    <b>if</b> (message_type == <a href="message_types.md#0xb_message_types_emergency_op">message_types::emergency_op</a>()) {
        <b>let</b> payload = <a href="message.md#0xb_message_extract_emergency_op_payload">message::extract_emergency_op_payload</a>(&<a href="message.md#0xb_message">message</a>);
        <a href="bridge.md#0xb_bridge_execute_emergency_op">execute_emergency_op</a>(inner, payload);
    } <b>else</b> <b>if</b> (message_type == <a href="message_types.md#0xb_message_types_committee_blocklist">message_types::committee_blocklist</a>()) {
        <b>let</b> payload = <a href="message.md#0xb_message_extract_blocklist_payload">message::extract_blocklist_payload</a>(&<a href="message.md#0xb_message">message</a>);
        <a href="committee.md#0xb_committee_execute_blocklist">committee::execute_blocklist</a>(&<b>mut</b> inner.<a href="committee.md#0xb_committee">committee</a>, payload);
    } <b>else</b> <b>if</b> (message_type == <a href="message_types.md#0xb_message_types_update_bridge_limit">message_types::update_bridge_limit</a>()) {
        <b>let</b> payload = <a href="message.md#0xb_message_extract_update_bridge_limit">message::extract_update_bridge_limit</a>(&<a href="message.md#0xb_message">message</a>);
        <a href="bridge.md#0xb_bridge_execute_update_bridge_limit">execute_update_bridge_limit</a>(inner, payload);
    } <b>else</b> <b>if</b> (message_type == <a href="message_types.md#0xb_message_types_update_asset_price">message_types::update_asset_price</a>()) {
        <b>let</b> payload = <a href="message.md#0xb_message_extract_update_asset_price">message::extract_update_asset_price</a>(&<a href="message.md#0xb_message">message</a>);
        <a href="bridge.md#0xb_bridge_execute_update_asset_price">execute_update_asset_price</a>(inner, payload);
    } <b>else</b> {
        <b>abort</b> <a href="bridge.md#0xb_bridge_EUnexpectedMessageType">EUnexpectedMessageType</a>
    };
}
</code></pre>



</details>

<a name="0xb_bridge_execute_emergency_op"></a>

## Function `execute_emergency_op`



<pre><code><b>fun</b> <a href="bridge.md#0xb_bridge_execute_emergency_op">execute_emergency_op</a>(inner: &<b>mut</b> <a href="bridge.md#0xb_bridge_BridgeInner">bridge::BridgeInner</a>, payload: <a href="message.md#0xb_message_EmergencyOp">message::EmergencyOp</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="bridge.md#0xb_bridge_execute_emergency_op">execute_emergency_op</a>(inner: &<b>mut</b> <a href="bridge.md#0xb_bridge_BridgeInner">BridgeInner</a>, payload: EmergencyOp) {
    <b>let</b> op = <a href="message.md#0xb_message_emergency_op_type">message::emergency_op_type</a>(&payload);
    <b>if</b> (op == <a href="message.md#0xb_message_emergency_op_pause">message::emergency_op_pause</a>()) {
        <b>assert</b>!(!inner.paused, <a href="bridge.md#0xb_bridge_EBridgeAlreadyPaused">EBridgeAlreadyPaused</a>);
        inner.paused = <b>true</b>;
        emit(<a href="bridge.md#0xb_bridge_EmergencyOpEvent">EmergencyOpEvent</a> { frozen: <b>true</b> });
    } <b>else</b> <b>if</b> (op == <a href="message.md#0xb_message_emergency_op_unpause">message::emergency_op_unpause</a>()) {
        <b>assert</b>!(inner.paused, <a href="bridge.md#0xb_bridge_EBridgeNotPaused">EBridgeNotPaused</a>);
        inner.paused = <b>false</b>;
        emit(<a href="bridge.md#0xb_bridge_EmergencyOpEvent">EmergencyOpEvent</a> { frozen: <b>false</b> });
    } <b>else</b> {
        <b>abort</b> <a href="bridge.md#0xb_bridge_EUnexpectedOperation">EUnexpectedOperation</a>
    };
}
</code></pre>



</details>

<a name="0xb_bridge_execute_update_bridge_limit"></a>

## Function `execute_update_bridge_limit`



<pre><code><b>fun</b> <a href="bridge.md#0xb_bridge_execute_update_bridge_limit">execute_update_bridge_limit</a>(inner: &<b>mut</b> <a href="bridge.md#0xb_bridge_BridgeInner">bridge::BridgeInner</a>, payload: <a href="message.md#0xb_message_UpdateBridgeLimit">message::UpdateBridgeLimit</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="bridge.md#0xb_bridge_execute_update_bridge_limit">execute_update_bridge_limit</a>(inner: &<b>mut</b> <a href="bridge.md#0xb_bridge_BridgeInner">BridgeInner</a>, payload: UpdateBridgeLimit) {
    <b>let</b> receiving_chain = <a href="message.md#0xb_message_update_bridge_limit_payload_receiving_chain">message::update_bridge_limit_payload_receiving_chain</a>(&payload);
    <b>assert</b>!(receiving_chain == inner.chain_id, <a href="bridge.md#0xb_bridge_EUnexpectedChainID">EUnexpectedChainID</a>);
    <b>let</b> route = <a href="chain_ids.md#0xb_chain_ids_get_route">chain_ids::get_route</a>(<a href="message.md#0xb_message_update_bridge_limit_payload_sending_chain">message::update_bridge_limit_payload_sending_chain</a>(&payload), receiving_chain);
    <a href="limiter.md#0xb_limiter_update_route_limit">limiter::update_route_limit</a>(&<b>mut</b> inner.<a href="limiter.md#0xb_limiter">limiter</a>, &route, <a href="message.md#0xb_message_update_bridge_limit_payload_limit">message::update_bridge_limit_payload_limit</a>(&payload))
}
</code></pre>



</details>

<a name="0xb_bridge_execute_update_asset_price"></a>

## Function `execute_update_asset_price`



<pre><code><b>fun</b> <a href="bridge.md#0xb_bridge_execute_update_asset_price">execute_update_asset_price</a>(inner: &<b>mut</b> <a href="bridge.md#0xb_bridge_BridgeInner">bridge::BridgeInner</a>, payload: <a href="message.md#0xb_message_UpdateAssetPrice">message::UpdateAssetPrice</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="bridge.md#0xb_bridge_execute_update_asset_price">execute_update_asset_price</a>(inner: &<b>mut</b> <a href="bridge.md#0xb_bridge_BridgeInner">BridgeInner</a>, payload: UpdateAssetPrice) {
    <a href="limiter.md#0xb_limiter_update_asset_notional_price">limiter::update_asset_notional_price</a>(&<b>mut</b> inner.<a href="limiter.md#0xb_limiter">limiter</a>, <a href="message.md#0xb_message_update_asset_price_payload_token_id">message::update_asset_price_payload_token_id</a>(&payload), <a href="message.md#0xb_message_update_asset_price_payload_new_price">message::update_asset_price_payload_new_price</a>(&payload))
}
</code></pre>



</details>

<a name="0xb_bridge_get_current_seq_num_and_increment"></a>

## Function `get_current_seq_num_and_increment`



<pre><code><b>fun</b> <a href="bridge.md#0xb_bridge_get_current_seq_num_and_increment">get_current_seq_num_and_increment</a>(self: &<b>mut</b> <a href="bridge.md#0xb_bridge_BridgeInner">bridge::BridgeInner</a>, msg_type: u8): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="bridge.md#0xb_bridge_get_current_seq_num_and_increment">get_current_seq_num_and_increment</a>(self: &<b>mut</b> <a href="bridge.md#0xb_bridge_BridgeInner">BridgeInner</a>, msg_type: u8): u64 {
    <b>if</b> (!<a href="../sui-framework/vec_map.md#0x2_vec_map_contains">vec_map::contains</a>(&self.sequence_nums, &msg_type)) {
        <a href="../sui-framework/vec_map.md#0x2_vec_map_insert">vec_map::insert</a>(&<b>mut</b> self.sequence_nums, msg_type, 1);
        <b>return</b> 0
    };
    <b>let</b> entry = <a href="../sui-framework/vec_map.md#0x2_vec_map_get_mut">vec_map::get_mut</a>(&<b>mut</b> self.sequence_nums, &msg_type);
    <b>let</b> seq_num = *entry;
    *entry = seq_num + 1;
    seq_num
}
</code></pre>



</details>

<a name="0xb_bridge_get_token_transfer_action_status"></a>

## Function `get_token_transfer_action_status`



<pre><code><b>public</b> <b>fun</b> <a href="bridge.md#0xb_bridge_get_token_transfer_action_status">get_token_transfer_action_status</a>(self: &<b>mut</b> <a href="bridge.md#0xb_bridge_Bridge">bridge::Bridge</a>, source_chain: u8, bridge_seq_num: u64): u8
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="bridge.md#0xb_bridge_get_token_transfer_action_status">get_token_transfer_action_status</a>(
    self: &<b>mut</b> <a href="bridge.md#0xb_bridge_Bridge">Bridge</a>,
    source_chain: u8,
    bridge_seq_num: u64,
): u8 {
    <b>let</b> inner = <a href="bridge.md#0xb_bridge_load_inner_mut">load_inner_mut</a>(self);
    <b>let</b> key = <a href="message.md#0xb_message_create_key">message::create_key</a>(source_chain, <a href="message_types.md#0xb_message_types_token">message_types::token</a>(), bridge_seq_num);
    <b>if</b> (!<a href="../sui-framework/linked_table.md#0x2_linked_table_contains">linked_table::contains</a>(&inner.bridge_records, key)) {
        <b>return</b> <a href="bridge.md#0xb_bridge_TRANSFER_STATUS_NOT_FOUND">TRANSFER_STATUS_NOT_FOUND</a>
    };
    <b>let</b> record = <a href="../sui-framework/linked_table.md#0x2_linked_table_borrow">linked_table::borrow</a>(&inner.bridge_records, key);
    <b>if</b> (record.claimed) {
        <b>return</b> <a href="bridge.md#0xb_bridge_TRANSFER_STATUS_CLAIMED">TRANSFER_STATUS_CLAIMED</a>
    };
    <b>if</b> (<a href="../move-stdlib/option.md#0x1_option_is_some">option::is_some</a>(&record.verified_signatures)) {
        <b>return</b> <a href="bridge.md#0xb_bridge_TRANSFER_STATUS_APPROVED">TRANSFER_STATUS_APPROVED</a>
    };
    <a href="bridge.md#0xb_bridge_TRANSFER_STATUS_PENDING">TRANSFER_STATUS_PENDING</a>
}
</code></pre>



</details>
