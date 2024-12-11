---
title: Module `0xb::bridge`
---



-  [Resource `Bridge`](#0xb_bridge_Bridge)
-  [Struct `BridgeInner`](#0xb_bridge_BridgeInner)
-  [Struct `TokenDepositedEvent`](#0xb_bridge_TokenDepositedEvent)
-  [Struct `EmergencyOpEvent`](#0xb_bridge_EmergencyOpEvent)
-  [Struct `BridgeRecord`](#0xb_bridge_BridgeRecord)
-  [Struct `TokenTransferApproved`](#0xb_bridge_TokenTransferApproved)
-  [Struct `TokenTransferClaimed`](#0xb_bridge_TokenTransferClaimed)
-  [Struct `TokenTransferAlreadyApproved`](#0xb_bridge_TokenTransferAlreadyApproved)
-  [Struct `TokenTransferAlreadyClaimed`](#0xb_bridge_TokenTransferAlreadyClaimed)
-  [Struct `TokenTransferLimitExceed`](#0xb_bridge_TokenTransferLimitExceed)
-  [Constants](#@Constants_0)
-  [Function `create`](#0xb_bridge_create)
-  [Function `init_bridge_committee`](#0xb_bridge_init_bridge_committee)
-  [Function `committee_registration`](#0xb_bridge_committee_registration)
-  [Function `update_node_url`](#0xb_bridge_update_node_url)
-  [Function `register_foreign_token`](#0xb_bridge_register_foreign_token)
-  [Function `send_token`](#0xb_bridge_send_token)
-  [Function `approve_token_transfer`](#0xb_bridge_approve_token_transfer)
-  [Function `claim_token`](#0xb_bridge_claim_token)
-  [Function `claim_and_transfer_token`](#0xb_bridge_claim_and_transfer_token)
-  [Function `execute_system_message`](#0xb_bridge_execute_system_message)
-  [Function `get_token_transfer_action_status`](#0xb_bridge_get_token_transfer_action_status)
-  [Function `get_token_transfer_action_signatures`](#0xb_bridge_get_token_transfer_action_signatures)
-  [Function `load_inner`](#0xb_bridge_load_inner)
-  [Function `load_inner_mut`](#0xb_bridge_load_inner_mut)
-  [Function `claim_token_internal`](#0xb_bridge_claim_token_internal)
-  [Function `execute_emergency_op`](#0xb_bridge_execute_emergency_op)
-  [Function `execute_update_bridge_limit`](#0xb_bridge_execute_update_bridge_limit)
-  [Function `execute_update_asset_price`](#0xb_bridge_execute_update_asset_price)
-  [Function `execute_add_tokens_on_sui`](#0xb_bridge_execute_add_tokens_on_sui)
-  [Function `get_current_seq_num_and_increment`](#0xb_bridge_get_current_seq_num_and_increment)
-  [Function `get_parsed_token_transfer_message`](#0xb_bridge_get_parsed_token_transfer_message)


<pre><code><b>use</b> <a href="../move-stdlib/ascii.md#0x1_ascii">0x1::ascii</a>;
<b>use</b> <a href="../move-stdlib/option.md#0x1_option">0x1::option</a>;
<b>use</b> <a href="../sui-framework/address.md#0x2_address">0x2::address</a>;
<b>use</b> <a href="../sui-framework/balance.md#0x2_balance">0x2::balance</a>;
<b>use</b> <a href="../sui-framework/clock.md#0x2_clock">0x2::clock</a>;
<b>use</b> <a href="../sui-framework/coin.md#0x2_coin">0x2::coin</a>;
<b>use</b> <a href="../sui-framework/event.md#0x2_event">0x2::event</a>;
<b>use</b> <a href="../sui-framework/linked_table.md#0x2_linked_table">0x2::linked_table</a>;
<b>use</b> <a href="../sui-framework/object.md#0x2_object">0x2::object</a>;
<b>use</b> <a href="../sui-framework/package.md#0x2_package">0x2::package</a>;
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
<code>bridge_version: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
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
<code>sequence_nums: <a href="../sui-framework/vec_map.md#0x2_vec_map_VecMap">vec_map::VecMap</a>&lt;u8, <a href="../move-stdlib/u64.md#0x1_u64">u64</a>&gt;</code>
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
<code>token_transfer_records: <a href="../sui-framework/linked_table.md#0x2_linked_table_LinkedTable">linked_table::LinkedTable</a>&lt;<a href="message.md#0xb_message_BridgeMessageKey">message::BridgeMessageKey</a>, <a href="bridge.md#0xb_bridge_BridgeRecord">bridge::BridgeRecord</a>&gt;</code>
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

<a name="0xb_bridge_TokenDepositedEvent"></a>

## Struct `TokenDepositedEvent`



<pre><code><b>struct</b> <a href="bridge.md#0xb_bridge_TokenDepositedEvent">TokenDepositedEvent</a> <b>has</b> <b>copy</b>, drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>seq_num: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
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
<code>amount: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
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

<a name="0xb_bridge_TokenTransferLimitExceed"></a>

## Struct `TokenTransferLimitExceed`



<pre><code><b>struct</b> <a href="bridge.md#0xb_bridge_TokenTransferLimitExceed">TokenTransferLimitExceed</a> <b>has</b> <b>copy</b>, drop
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



<pre><code><b>const</b> <a href="bridge.md#0xb_bridge_ENotSystemAddress">ENotSystemAddress</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 5;
</code></pre>



<a name="0xb_bridge_EWrongInnerVersion"></a>



<pre><code><b>const</b> <a href="bridge.md#0xb_bridge_EWrongInnerVersion">EWrongInnerVersion</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 7;
</code></pre>



<a name="0xb_bridge_CURRENT_VERSION"></a>



<pre><code><b>const</b> <a href="bridge.md#0xb_bridge_CURRENT_VERSION">CURRENT_VERSION</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 1;
</code></pre>



<a name="0xb_bridge_EInvalidBridgeRoute"></a>



<pre><code><b>const</b> <a href="bridge.md#0xb_bridge_EInvalidBridgeRoute">EInvalidBridgeRoute</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 16;
</code></pre>



<a name="0xb_bridge_EMustBeTokenMessage"></a>



<pre><code><b>const</b> <a href="bridge.md#0xb_bridge_EMustBeTokenMessage">EMustBeTokenMessage</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 17;
</code></pre>



<a name="0xb_bridge_EBridgeAlreadyPaused"></a>



<pre><code><b>const</b> <a href="bridge.md#0xb_bridge_EBridgeAlreadyPaused">EBridgeAlreadyPaused</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 13;
</code></pre>



<a name="0xb_bridge_EBridgeNotPaused"></a>



<pre><code><b>const</b> <a href="bridge.md#0xb_bridge_EBridgeNotPaused">EBridgeNotPaused</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 14;
</code></pre>



<a name="0xb_bridge_EBridgeUnavailable"></a>



<pre><code><b>const</b> <a href="bridge.md#0xb_bridge_EBridgeUnavailable">EBridgeUnavailable</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 8;
</code></pre>



<a name="0xb_bridge_EInvalidEvmAddress"></a>



<pre><code><b>const</b> <a href="bridge.md#0xb_bridge_EInvalidEvmAddress">EInvalidEvmAddress</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 18;
</code></pre>



<a name="0xb_bridge_EInvariantSuiInitializedTokenTransferShouldNotBeClaimed"></a>



<pre><code><b>const</b> <a href="bridge.md#0xb_bridge_EInvariantSuiInitializedTokenTransferShouldNotBeClaimed">EInvariantSuiInitializedTokenTransferShouldNotBeClaimed</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 10;
</code></pre>



<a name="0xb_bridge_EMalformedMessageError"></a>



<pre><code><b>const</b> <a href="bridge.md#0xb_bridge_EMalformedMessageError">EMalformedMessageError</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 2;
</code></pre>



<a name="0xb_bridge_EMessageNotFoundInRecords"></a>



<pre><code><b>const</b> <a href="bridge.md#0xb_bridge_EMessageNotFoundInRecords">EMessageNotFoundInRecords</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 11;
</code></pre>



<a name="0xb_bridge_ETokenAlreadyClaimedOrHitLimit"></a>



<pre><code><b>const</b> <a href="bridge.md#0xb_bridge_ETokenAlreadyClaimedOrHitLimit">ETokenAlreadyClaimedOrHitLimit</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 15;
</code></pre>



<a name="0xb_bridge_ETokenValueIsZero"></a>



<pre><code><b>const</b> <a href="bridge.md#0xb_bridge_ETokenValueIsZero">ETokenValueIsZero</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 19;
</code></pre>



<a name="0xb_bridge_EUnauthorisedClaim"></a>



<pre><code><b>const</b> <a href="bridge.md#0xb_bridge_EUnauthorisedClaim">EUnauthorisedClaim</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 1;
</code></pre>



<a name="0xb_bridge_EUnexpectedChainID"></a>



<pre><code><b>const</b> <a href="bridge.md#0xb_bridge_EUnexpectedChainID">EUnexpectedChainID</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 4;
</code></pre>



<a name="0xb_bridge_EUnexpectedMessageType"></a>



<pre><code><b>const</b> <a href="bridge.md#0xb_bridge_EUnexpectedMessageType">EUnexpectedMessageType</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 0;
</code></pre>



<a name="0xb_bridge_EUnexpectedMessageVersion"></a>



<pre><code><b>const</b> <a href="bridge.md#0xb_bridge_EUnexpectedMessageVersion">EUnexpectedMessageVersion</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 12;
</code></pre>



<a name="0xb_bridge_EUnexpectedOperation"></a>



<pre><code><b>const</b> <a href="bridge.md#0xb_bridge_EUnexpectedOperation">EUnexpectedOperation</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 9;
</code></pre>



<a name="0xb_bridge_EUnexpectedSeqNum"></a>



<pre><code><b>const</b> <a href="bridge.md#0xb_bridge_EUnexpectedSeqNum">EUnexpectedSeqNum</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 6;
</code></pre>



<a name="0xb_bridge_EUnexpectedTokenType"></a>



<pre><code><b>const</b> <a href="bridge.md#0xb_bridge_EUnexpectedTokenType">EUnexpectedTokenType</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 3;
</code></pre>



<a name="0xb_bridge_EVM_ADDRESS_LENGTH"></a>



<pre><code><b>const</b> <a href="bridge.md#0xb_bridge_EVM_ADDRESS_LENGTH">EVM_ADDRESS_LENGTH</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 20;
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
    <b>assert</b>!(ctx.sender() == @0x0, <a href="bridge.md#0xb_bridge_ENotSystemAddress">ENotSystemAddress</a>);
    <b>let</b> bridge_inner = <a href="bridge.md#0xb_bridge_BridgeInner">BridgeInner</a> {
        bridge_version: <a href="bridge.md#0xb_bridge_CURRENT_VERSION">CURRENT_VERSION</a>,
        message_version: <a href="bridge.md#0xb_bridge_MESSAGE_VERSION">MESSAGE_VERSION</a>,
        chain_id,
        sequence_nums: <a href="../sui-framework/vec_map.md#0x2_vec_map_empty">vec_map::empty</a>(),
        <a href="committee.md#0xb_committee">committee</a>: <a href="committee.md#0xb_committee_create">committee::create</a>(ctx),
        <a href="treasury.md#0xb_treasury">treasury</a>: <a href="treasury.md#0xb_treasury_create">treasury::create</a>(ctx),
        token_transfer_records: <a href="../sui-framework/linked_table.md#0x2_linked_table_new">linked_table::new</a>(ctx),
        <a href="limiter.md#0xb_limiter">limiter</a>: <a href="limiter.md#0xb_limiter_new">limiter::new</a>(),
        paused: <b>false</b>,
    };
    <b>let</b> <a href="bridge.md#0xb_bridge">bridge</a> = <a href="bridge.md#0xb_bridge_Bridge">Bridge</a> {
        id,
        inner: <a href="../sui-framework/versioned.md#0x2_versioned_create">versioned::create</a>(<a href="bridge.md#0xb_bridge_CURRENT_VERSION">CURRENT_VERSION</a>, bridge_inner, ctx),
    };
    <a href="../sui-framework/transfer.md#0x2_transfer_share_object">transfer::share_object</a>(<a href="bridge.md#0xb_bridge">bridge</a>);
}
</code></pre>



</details>

<a name="0xb_bridge_init_bridge_committee"></a>

## Function `init_bridge_committee`



<pre><code><b>fun</b> <a href="bridge.md#0xb_bridge_init_bridge_committee">init_bridge_committee</a>(<a href="bridge.md#0xb_bridge">bridge</a>: &<b>mut</b> <a href="bridge.md#0xb_bridge_Bridge">bridge::Bridge</a>, active_validator_voting_power: <a href="../sui-framework/vec_map.md#0x2_vec_map_VecMap">vec_map::VecMap</a>&lt;<b>address</b>, <a href="../move-stdlib/u64.md#0x1_u64">u64</a>&gt;, min_stake_participation_percentage: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>, ctx: &<a href="../sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="bridge.md#0xb_bridge_init_bridge_committee">init_bridge_committee</a>(
    <a href="bridge.md#0xb_bridge">bridge</a>: &<b>mut</b> <a href="bridge.md#0xb_bridge_Bridge">Bridge</a>,
    active_validator_voting_power: VecMap&lt;<b>address</b>, <a href="../move-stdlib/u64.md#0x1_u64">u64</a>&gt;,
    min_stake_participation_percentage: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>,
    ctx: &TxContext
) {
    <b>assert</b>!(ctx.sender() == @0x0, <a href="bridge.md#0xb_bridge_ENotSystemAddress">ENotSystemAddress</a>);
    <b>let</b> inner = <a href="bridge.md#0xb_bridge_load_inner_mut">load_inner_mut</a>(<a href="bridge.md#0xb_bridge">bridge</a>);
    <b>if</b> (inner.<a href="committee.md#0xb_committee">committee</a>.committee_members().is_empty()) {
        inner.<a href="committee.md#0xb_committee">committee</a>.try_create_next_committee(
            active_validator_voting_power,
            min_stake_participation_percentage,
            ctx,
        )
    }
}
</code></pre>



</details>

<a name="0xb_bridge_committee_registration"></a>

## Function `committee_registration`



<pre><code><b>public</b> <b>fun</b> <a href="bridge.md#0xb_bridge_committee_registration">committee_registration</a>(<a href="bridge.md#0xb_bridge">bridge</a>: &<b>mut</b> <a href="bridge.md#0xb_bridge_Bridge">bridge::Bridge</a>, system_state: &<b>mut</b> <a href="../sui-system/sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, bridge_pubkey_bytes: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, http_rest_url: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, ctx: &<a href="../sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="bridge.md#0xb_bridge_committee_registration">committee_registration</a>(
    <a href="bridge.md#0xb_bridge">bridge</a>: &<b>mut</b> <a href="bridge.md#0xb_bridge_Bridge">Bridge</a>,
    system_state: &<b>mut</b> SuiSystemState,
    bridge_pubkey_bytes: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    http_rest_url: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    ctx: &TxContext
) {
    <a href="bridge.md#0xb_bridge_load_inner_mut">load_inner_mut</a>(<a href="bridge.md#0xb_bridge">bridge</a>)
        .<a href="committee.md#0xb_committee">committee</a>
        .register(system_state, bridge_pubkey_bytes, http_rest_url, ctx);
}
</code></pre>



</details>

<a name="0xb_bridge_update_node_url"></a>

## Function `update_node_url`



<pre><code><b>public</b> <b>fun</b> <a href="bridge.md#0xb_bridge_update_node_url">update_node_url</a>(<a href="bridge.md#0xb_bridge">bridge</a>: &<b>mut</b> <a href="bridge.md#0xb_bridge_Bridge">bridge::Bridge</a>, new_url: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, ctx: &<a href="../sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="bridge.md#0xb_bridge_update_node_url">update_node_url</a>(<a href="bridge.md#0xb_bridge">bridge</a>: &<b>mut</b> <a href="bridge.md#0xb_bridge_Bridge">Bridge</a>, new_url: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, ctx: &TxContext) {
    <a href="bridge.md#0xb_bridge_load_inner_mut">load_inner_mut</a>(<a href="bridge.md#0xb_bridge">bridge</a>).<a href="committee.md#0xb_committee">committee</a>.<a href="bridge.md#0xb_bridge_update_node_url">update_node_url</a>(new_url, ctx);
}
</code></pre>



</details>

<a name="0xb_bridge_register_foreign_token"></a>

## Function `register_foreign_token`



<pre><code><b>public</b> <b>fun</b> <a href="bridge.md#0xb_bridge_register_foreign_token">register_foreign_token</a>&lt;T&gt;(<a href="bridge.md#0xb_bridge">bridge</a>: &<b>mut</b> <a href="bridge.md#0xb_bridge_Bridge">bridge::Bridge</a>, tc: <a href="../sui-framework/coin.md#0x2_coin_TreasuryCap">coin::TreasuryCap</a>&lt;T&gt;, uc: <a href="../sui-framework/package.md#0x2_package_UpgradeCap">package::UpgradeCap</a>, metadata: &<a href="../sui-framework/coin.md#0x2_coin_CoinMetadata">coin::CoinMetadata</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="bridge.md#0xb_bridge_register_foreign_token">register_foreign_token</a>&lt;T&gt;(
    <a href="bridge.md#0xb_bridge">bridge</a>: &<b>mut</b> <a href="bridge.md#0xb_bridge_Bridge">Bridge</a>,
    tc: TreasuryCap&lt;T&gt;,
    uc: UpgradeCap,
    metadata: &CoinMetadata&lt;T&gt;,
) {
    <a href="bridge.md#0xb_bridge_load_inner_mut">load_inner_mut</a>(<a href="bridge.md#0xb_bridge">bridge</a>)
        .<a href="treasury.md#0xb_treasury">treasury</a>
        .<a href="bridge.md#0xb_bridge_register_foreign_token">register_foreign_token</a>&lt;T&gt;(tc, uc, metadata)
}
</code></pre>



</details>

<a name="0xb_bridge_send_token"></a>

## Function `send_token`



<pre><code><b>public</b> <b>fun</b> <a href="bridge.md#0xb_bridge_send_token">send_token</a>&lt;T&gt;(<a href="bridge.md#0xb_bridge">bridge</a>: &<b>mut</b> <a href="bridge.md#0xb_bridge_Bridge">bridge::Bridge</a>, target_chain: u8, target_address: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, token: <a href="../sui-framework/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;, ctx: &<b>mut</b> <a href="../sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="bridge.md#0xb_bridge_send_token">send_token</a>&lt;T&gt;(
    <a href="bridge.md#0xb_bridge">bridge</a>: &<b>mut</b> <a href="bridge.md#0xb_bridge_Bridge">Bridge</a>,
    target_chain: u8,
    target_address: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    token: Coin&lt;T&gt;,
    ctx: &<b>mut</b> TxContext
) {
    <b>let</b> inner = <a href="bridge.md#0xb_bridge_load_inner_mut">load_inner_mut</a>(<a href="bridge.md#0xb_bridge">bridge</a>);
    <b>assert</b>!(!inner.paused, <a href="bridge.md#0xb_bridge_EBridgeUnavailable">EBridgeUnavailable</a>);
    <b>assert</b>!(<a href="chain_ids.md#0xb_chain_ids_is_valid_route">chain_ids::is_valid_route</a>(inner.chain_id, target_chain), <a href="bridge.md#0xb_bridge_EInvalidBridgeRoute">EInvalidBridgeRoute</a>);
    <b>assert</b>!(target_address.length() == <a href="bridge.md#0xb_bridge_EVM_ADDRESS_LENGTH">EVM_ADDRESS_LENGTH</a>, <a href="bridge.md#0xb_bridge_EInvalidEvmAddress">EInvalidEvmAddress</a>);

    <b>let</b> bridge_seq_num = inner.<a href="bridge.md#0xb_bridge_get_current_seq_num_and_increment">get_current_seq_num_and_increment</a>(<a href="message_types.md#0xb_message_types_token">message_types::token</a>());
    <b>let</b> token_id = inner.<a href="treasury.md#0xb_treasury">treasury</a>.token_id&lt;T&gt;();
    <b>let</b> token_amount = token.<a href="../sui-framework/balance.md#0x2_balance">balance</a>().value();
    <b>assert</b>!(token_amount &gt; 0, <a href="bridge.md#0xb_bridge_ETokenValueIsZero">ETokenValueIsZero</a>);

    // create <a href="bridge.md#0xb_bridge">bridge</a> <a href="message.md#0xb_message">message</a>
    <b>let</b> <a href="message.md#0xb_message">message</a> = <a href="message.md#0xb_message_create_token_bridge_message">message::create_token_bridge_message</a>(
        inner.chain_id,
        bridge_seq_num,
        address::to_bytes(ctx.sender()),
        target_chain,
        target_address,
        token_id,
        token_amount,
    );

    // burn / escrow token, unsupported coins will fail in this step
    inner.<a href="treasury.md#0xb_treasury">treasury</a>.burn(token);

    // Store pending <a href="bridge.md#0xb_bridge">bridge</a> request
    inner.token_transfer_records.push_back(
        <a href="message.md#0xb_message">message</a>.key(),
        <a href="bridge.md#0xb_bridge_BridgeRecord">BridgeRecord</a> {
            <a href="message.md#0xb_message">message</a>,
            verified_signatures: <a href="../move-stdlib/option.md#0x1_option_none">option::none</a>(),
            claimed: <b>false</b>,
        },
    );

    // emit <a href="../sui-framework/event.md#0x2_event">event</a>
    emit(
        <a href="bridge.md#0xb_bridge_TokenDepositedEvent">TokenDepositedEvent</a> {
            seq_num: bridge_seq_num,
            source_chain: inner.chain_id,
            sender_address: address::to_bytes(ctx.sender()),
            target_chain,
            target_address,
            token_type: token_id,
            amount: token_amount,
        },
    );
}
</code></pre>



</details>

<a name="0xb_bridge_approve_token_transfer"></a>

## Function `approve_token_transfer`



<pre><code><b>public</b> <b>fun</b> <a href="bridge.md#0xb_bridge_approve_token_transfer">approve_token_transfer</a>(<a href="bridge.md#0xb_bridge">bridge</a>: &<b>mut</b> <a href="bridge.md#0xb_bridge_Bridge">bridge::Bridge</a>, <a href="message.md#0xb_message">message</a>: <a href="message.md#0xb_message_BridgeMessage">message::BridgeMessage</a>, signatures: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="bridge.md#0xb_bridge_approve_token_transfer">approve_token_transfer</a>(
    <a href="bridge.md#0xb_bridge">bridge</a>: &<b>mut</b> <a href="bridge.md#0xb_bridge_Bridge">Bridge</a>,
    <a href="message.md#0xb_message">message</a>: BridgeMessage,
    signatures: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;&gt;,
) {
    <b>let</b> inner = <a href="bridge.md#0xb_bridge_load_inner_mut">load_inner_mut</a>(<a href="bridge.md#0xb_bridge">bridge</a>);
    <b>assert</b>!(!inner.paused, <a href="bridge.md#0xb_bridge_EBridgeUnavailable">EBridgeUnavailable</a>);
    // verify signatures
    inner.<a href="committee.md#0xb_committee">committee</a>.verify_signatures(<a href="message.md#0xb_message">message</a>, signatures);

    <b>assert</b>!(<a href="message.md#0xb_message">message</a>.message_type() == <a href="message_types.md#0xb_message_types_token">message_types::token</a>(), <a href="bridge.md#0xb_bridge_EMustBeTokenMessage">EMustBeTokenMessage</a>);
    <b>assert</b>!(<a href="message.md#0xb_message">message</a>.message_version() == <a href="bridge.md#0xb_bridge_MESSAGE_VERSION">MESSAGE_VERSION</a>, <a href="bridge.md#0xb_bridge_EUnexpectedMessageVersion">EUnexpectedMessageVersion</a>);
    <b>let</b> token_payload = <a href="message.md#0xb_message">message</a>.extract_token_bridge_payload();
    <b>let</b> target_chain = token_payload.token_target_chain();
    <b>assert</b>!(
        <a href="message.md#0xb_message">message</a>.source_chain() == inner.chain_id || target_chain == inner.chain_id,
        <a href="bridge.md#0xb_bridge_EUnexpectedChainID">EUnexpectedChainID</a>,
    );

    <b>let</b> message_key = <a href="message.md#0xb_message">message</a>.key();
    // retrieve pending <a href="message.md#0xb_message">message</a> <b>if</b> source chain is Sui, the initial <a href="message.md#0xb_message">message</a>
    // must exist on chain
    <b>if</b> (<a href="message.md#0xb_message">message</a>.source_chain() == inner.chain_id) {
        <b>let</b> record = &<b>mut</b> inner.token_transfer_records[message_key];

        <b>assert</b>!(record.<a href="message.md#0xb_message">message</a> == <a href="message.md#0xb_message">message</a>, <a href="bridge.md#0xb_bridge_EMalformedMessageError">EMalformedMessageError</a>);
        <b>assert</b>!(!record.claimed, <a href="bridge.md#0xb_bridge_EInvariantSuiInitializedTokenTransferShouldNotBeClaimed">EInvariantSuiInitializedTokenTransferShouldNotBeClaimed</a>);

        // If record already <b>has</b> verified signatures, it means the <a href="message.md#0xb_message">message</a> <b>has</b> been approved
        // Then we exit early.
        <b>if</b> (record.verified_signatures.is_some()) {
            emit(<a href="bridge.md#0xb_bridge_TokenTransferAlreadyApproved">TokenTransferAlreadyApproved</a> { message_key });
            <b>return</b>
        };
        // Store approval
        record.verified_signatures = <a href="../move-stdlib/option.md#0x1_option_some">option::some</a>(signatures)
    } <b>else</b> {
        // At this point, <b>if</b> this <a href="message.md#0xb_message">message</a> is in token_transfer_records, we know
        // it's already approved because we only add a <a href="message.md#0xb_message">message</a> <b>to</b> token_transfer_records
        // after verifying the signatures
        <b>if</b> (inner.token_transfer_records.contains(message_key)) {
            emit(<a href="bridge.md#0xb_bridge_TokenTransferAlreadyApproved">TokenTransferAlreadyApproved</a> { message_key });
            <b>return</b>
        };
        // Store <a href="message.md#0xb_message">message</a> and approval
        inner.token_transfer_records.push_back(
            message_key,
            <a href="bridge.md#0xb_bridge_BridgeRecord">BridgeRecord</a> {
                <a href="message.md#0xb_message">message</a>,
                verified_signatures: <a href="../move-stdlib/option.md#0x1_option_some">option::some</a>(signatures),
                claimed: <b>false</b>
            },
        );
    };

    emit(<a href="bridge.md#0xb_bridge_TokenTransferApproved">TokenTransferApproved</a> { message_key });
}
</code></pre>



</details>

<a name="0xb_bridge_claim_token"></a>

## Function `claim_token`



<pre><code><b>public</b> <b>fun</b> <a href="bridge.md#0xb_bridge_claim_token">claim_token</a>&lt;T&gt;(<a href="bridge.md#0xb_bridge">bridge</a>: &<b>mut</b> <a href="bridge.md#0xb_bridge_Bridge">bridge::Bridge</a>, <a href="../sui-framework/clock.md#0x2_clock">clock</a>: &<a href="../sui-framework/clock.md#0x2_clock_Clock">clock::Clock</a>, source_chain: u8, bridge_seq_num: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>, ctx: &<b>mut</b> <a href="../sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="../sui-framework/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="bridge.md#0xb_bridge_claim_token">claim_token</a>&lt;T&gt;(
    <a href="bridge.md#0xb_bridge">bridge</a>: &<b>mut</b> <a href="bridge.md#0xb_bridge_Bridge">Bridge</a>,
    <a href="../sui-framework/clock.md#0x2_clock">clock</a>: &Clock,
    source_chain: u8,
    bridge_seq_num: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>,
    ctx: &<b>mut</b> TxContext,
): Coin&lt;T&gt; {
    <b>let</b> (maybe_token, owner) = <a href="bridge.md#0xb_bridge">bridge</a>.<a href="bridge.md#0xb_bridge_claim_token_internal">claim_token_internal</a>&lt;T&gt;(
        <a href="../sui-framework/clock.md#0x2_clock">clock</a>,
        source_chain,
        bridge_seq_num,
        ctx,
    );
    // Only token owner can claim the token
    <b>assert</b>!(ctx.sender() == owner, <a href="bridge.md#0xb_bridge_EUnauthorisedClaim">EUnauthorisedClaim</a>);
    <b>assert</b>!(maybe_token.is_some(), <a href="bridge.md#0xb_bridge_ETokenAlreadyClaimedOrHitLimit">ETokenAlreadyClaimedOrHitLimit</a>);
    maybe_token.destroy_some()
}
</code></pre>



</details>

<a name="0xb_bridge_claim_and_transfer_token"></a>

## Function `claim_and_transfer_token`



<pre><code><b>public</b> <b>fun</b> <a href="bridge.md#0xb_bridge_claim_and_transfer_token">claim_and_transfer_token</a>&lt;T&gt;(<a href="bridge.md#0xb_bridge">bridge</a>: &<b>mut</b> <a href="bridge.md#0xb_bridge_Bridge">bridge::Bridge</a>, <a href="../sui-framework/clock.md#0x2_clock">clock</a>: &<a href="../sui-framework/clock.md#0x2_clock_Clock">clock::Clock</a>, source_chain: u8, bridge_seq_num: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>, ctx: &<b>mut</b> <a href="../sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="bridge.md#0xb_bridge_claim_and_transfer_token">claim_and_transfer_token</a>&lt;T&gt;(
    <a href="bridge.md#0xb_bridge">bridge</a>: &<b>mut</b> <a href="bridge.md#0xb_bridge_Bridge">Bridge</a>,
    <a href="../sui-framework/clock.md#0x2_clock">clock</a>: &Clock,
    source_chain: u8,
    bridge_seq_num: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> (token, owner) = <a href="bridge.md#0xb_bridge">bridge</a>.<a href="bridge.md#0xb_bridge_claim_token_internal">claim_token_internal</a>&lt;T&gt;(<a href="../sui-framework/clock.md#0x2_clock">clock</a>, source_chain, bridge_seq_num, ctx);
    <b>if</b> (token.is_some()) {
        <a href="../sui-framework/transfer.md#0x2_transfer_public_transfer">transfer::public_transfer</a>(token.destroy_some(), owner)
    } <b>else</b> {
        token.destroy_none();
    };
}
</code></pre>



</details>

<a name="0xb_bridge_execute_system_message"></a>

## Function `execute_system_message`



<pre><code><b>public</b> <b>fun</b> <a href="bridge.md#0xb_bridge_execute_system_message">execute_system_message</a>(<a href="bridge.md#0xb_bridge">bridge</a>: &<b>mut</b> <a href="bridge.md#0xb_bridge_Bridge">bridge::Bridge</a>, <a href="message.md#0xb_message">message</a>: <a href="message.md#0xb_message_BridgeMessage">message::BridgeMessage</a>, signatures: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="bridge.md#0xb_bridge_execute_system_message">execute_system_message</a>(
    <a href="bridge.md#0xb_bridge">bridge</a>: &<b>mut</b> <a href="bridge.md#0xb_bridge_Bridge">Bridge</a>,
    <a href="message.md#0xb_message">message</a>: BridgeMessage,
    signatures: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;&gt;,
) {
    <b>let</b> message_type = <a href="message.md#0xb_message">message</a>.message_type();

    // TODO: test version mismatch
    <b>assert</b>!(<a href="message.md#0xb_message">message</a>.message_version() == <a href="bridge.md#0xb_bridge_MESSAGE_VERSION">MESSAGE_VERSION</a>, <a href="bridge.md#0xb_bridge_EUnexpectedMessageVersion">EUnexpectedMessageVersion</a>);
    <b>let</b> inner = <a href="bridge.md#0xb_bridge_load_inner_mut">load_inner_mut</a>(<a href="bridge.md#0xb_bridge">bridge</a>);

    <b>assert</b>!(<a href="message.md#0xb_message">message</a>.source_chain() == inner.chain_id, <a href="bridge.md#0xb_bridge_EUnexpectedChainID">EUnexpectedChainID</a>);

    // check system ops seq number and increment it
    <b>let</b> expected_seq_num = inner.<a href="bridge.md#0xb_bridge_get_current_seq_num_and_increment">get_current_seq_num_and_increment</a>(message_type);
    <b>assert</b>!(<a href="message.md#0xb_message">message</a>.seq_num() == expected_seq_num, <a href="bridge.md#0xb_bridge_EUnexpectedSeqNum">EUnexpectedSeqNum</a>);

    inner.<a href="committee.md#0xb_committee">committee</a>.verify_signatures(<a href="message.md#0xb_message">message</a>, signatures);

    <b>if</b> (message_type == <a href="message_types.md#0xb_message_types_emergency_op">message_types::emergency_op</a>()) {
        <b>let</b> payload = <a href="message.md#0xb_message">message</a>.extract_emergency_op_payload();
        inner.<a href="bridge.md#0xb_bridge_execute_emergency_op">execute_emergency_op</a>(payload);
    } <b>else</b> <b>if</b> (message_type == <a href="message_types.md#0xb_message_types_committee_blocklist">message_types::committee_blocklist</a>()) {
        <b>let</b> payload = <a href="message.md#0xb_message">message</a>.extract_blocklist_payload();
        inner.<a href="committee.md#0xb_committee">committee</a>.execute_blocklist(payload);
    } <b>else</b> <b>if</b> (message_type == <a href="message_types.md#0xb_message_types_update_bridge_limit">message_types::update_bridge_limit</a>()) {
        <b>let</b> payload = <a href="message.md#0xb_message">message</a>.extract_update_bridge_limit();
        inner.<a href="bridge.md#0xb_bridge_execute_update_bridge_limit">execute_update_bridge_limit</a>(payload);
    } <b>else</b> <b>if</b> (message_type == <a href="message_types.md#0xb_message_types_update_asset_price">message_types::update_asset_price</a>()) {
        <b>let</b> payload = <a href="message.md#0xb_message">message</a>.extract_update_asset_price();
        inner.<a href="bridge.md#0xb_bridge_execute_update_asset_price">execute_update_asset_price</a>(payload);
    } <b>else</b> <b>if</b> (message_type == <a href="message_types.md#0xb_message_types_add_tokens_on_sui">message_types::add_tokens_on_sui</a>()) {
        <b>let</b> payload = <a href="message.md#0xb_message">message</a>.extract_add_tokens_on_sui();
        inner.<a href="bridge.md#0xb_bridge_execute_add_tokens_on_sui">execute_add_tokens_on_sui</a>(payload);
    } <b>else</b> {
        <b>abort</b> <a href="bridge.md#0xb_bridge_EUnexpectedMessageType">EUnexpectedMessageType</a>
    };
}
</code></pre>



</details>

<a name="0xb_bridge_get_token_transfer_action_status"></a>

## Function `get_token_transfer_action_status`



<pre><code><b>fun</b> <a href="bridge.md#0xb_bridge_get_token_transfer_action_status">get_token_transfer_action_status</a>(<a href="bridge.md#0xb_bridge">bridge</a>: &<a href="bridge.md#0xb_bridge_Bridge">bridge::Bridge</a>, source_chain: u8, bridge_seq_num: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>): u8
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="bridge.md#0xb_bridge_get_token_transfer_action_status">get_token_transfer_action_status</a>(
    <a href="bridge.md#0xb_bridge">bridge</a>: &<a href="bridge.md#0xb_bridge_Bridge">Bridge</a>,
    source_chain: u8,
    bridge_seq_num: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>,
): u8 {
    <b>let</b> inner = <a href="bridge.md#0xb_bridge_load_inner">load_inner</a>(<a href="bridge.md#0xb_bridge">bridge</a>);
    <b>let</b> key = <a href="message.md#0xb_message_create_key">message::create_key</a>(
        source_chain,
        <a href="message_types.md#0xb_message_types_token">message_types::token</a>(),
        bridge_seq_num
    );

    <b>if</b> (!inner.token_transfer_records.contains(key)) {
        <b>return</b> <a href="bridge.md#0xb_bridge_TRANSFER_STATUS_NOT_FOUND">TRANSFER_STATUS_NOT_FOUND</a>
    };

    <b>let</b> record = &inner.token_transfer_records[key];
    <b>if</b> (record.claimed) {
        <b>return</b> <a href="bridge.md#0xb_bridge_TRANSFER_STATUS_CLAIMED">TRANSFER_STATUS_CLAIMED</a>
    };

    <b>if</b> (record.verified_signatures.is_some()) {
        <b>return</b> <a href="bridge.md#0xb_bridge_TRANSFER_STATUS_APPROVED">TRANSFER_STATUS_APPROVED</a>
    };

    <a href="bridge.md#0xb_bridge_TRANSFER_STATUS_PENDING">TRANSFER_STATUS_PENDING</a>
}
</code></pre>



</details>

<a name="0xb_bridge_get_token_transfer_action_signatures"></a>

## Function `get_token_transfer_action_signatures`



<pre><code><b>fun</b> <a href="bridge.md#0xb_bridge_get_token_transfer_action_signatures">get_token_transfer_action_signatures</a>(<a href="bridge.md#0xb_bridge">bridge</a>: &<a href="bridge.md#0xb_bridge_Bridge">bridge::Bridge</a>, source_chain: u8, bridge_seq_num: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>): <a href="../move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;&gt;&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="bridge.md#0xb_bridge_get_token_transfer_action_signatures">get_token_transfer_action_signatures</a>(
    <a href="bridge.md#0xb_bridge">bridge</a>: &<a href="bridge.md#0xb_bridge_Bridge">Bridge</a>,
    source_chain: u8,
    bridge_seq_num: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>,
): Option&lt;<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;&gt;&gt; {
    <b>let</b> inner = <a href="bridge.md#0xb_bridge_load_inner">load_inner</a>(<a href="bridge.md#0xb_bridge">bridge</a>);
    <b>let</b> key = <a href="message.md#0xb_message_create_key">message::create_key</a>(
        source_chain,
        <a href="message_types.md#0xb_message_types_token">message_types::token</a>(),
        bridge_seq_num
    );

    <b>if</b> (!inner.token_transfer_records.contains(key)) {
        <b>return</b> <a href="../move-stdlib/option.md#0x1_option_none">option::none</a>()
    };

    <b>let</b> record = &inner.token_transfer_records[key];
    record.verified_signatures
}
</code></pre>



</details>

<a name="0xb_bridge_load_inner"></a>

## Function `load_inner`



<pre><code><b>fun</b> <a href="bridge.md#0xb_bridge_load_inner">load_inner</a>(<a href="bridge.md#0xb_bridge">bridge</a>: &<a href="bridge.md#0xb_bridge_Bridge">bridge::Bridge</a>): &<a href="bridge.md#0xb_bridge_BridgeInner">bridge::BridgeInner</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="bridge.md#0xb_bridge_load_inner">load_inner</a>(
    <a href="bridge.md#0xb_bridge">bridge</a>: &<a href="bridge.md#0xb_bridge_Bridge">Bridge</a>,
): &<a href="bridge.md#0xb_bridge_BridgeInner">BridgeInner</a> {
    <b>let</b> version = <a href="bridge.md#0xb_bridge">bridge</a>.inner.version();

    // TODO: Replace this <b>with</b> a lazy <b>update</b> function when we add a new version of the inner <a href="../sui-framework/object.md#0x2_object">object</a>.
    <b>assert</b>!(version == <a href="bridge.md#0xb_bridge_CURRENT_VERSION">CURRENT_VERSION</a>, <a href="bridge.md#0xb_bridge_EWrongInnerVersion">EWrongInnerVersion</a>);
    <b>let</b> inner: &<a href="bridge.md#0xb_bridge_BridgeInner">BridgeInner</a> = <a href="bridge.md#0xb_bridge">bridge</a>.inner.load_value();
    <b>assert</b>!(inner.bridge_version == version, <a href="bridge.md#0xb_bridge_EWrongInnerVersion">EWrongInnerVersion</a>);
    inner
}
</code></pre>



</details>

<a name="0xb_bridge_load_inner_mut"></a>

## Function `load_inner_mut`



<pre><code><b>fun</b> <a href="bridge.md#0xb_bridge_load_inner_mut">load_inner_mut</a>(<a href="bridge.md#0xb_bridge">bridge</a>: &<b>mut</b> <a href="bridge.md#0xb_bridge_Bridge">bridge::Bridge</a>): &<b>mut</b> <a href="bridge.md#0xb_bridge_BridgeInner">bridge::BridgeInner</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="bridge.md#0xb_bridge_load_inner_mut">load_inner_mut</a>(<a href="bridge.md#0xb_bridge">bridge</a>: &<b>mut</b> <a href="bridge.md#0xb_bridge_Bridge">Bridge</a>): &<b>mut</b> <a href="bridge.md#0xb_bridge_BridgeInner">BridgeInner</a> {
    <b>let</b> version = <a href="bridge.md#0xb_bridge">bridge</a>.inner.version();
    // TODO: Replace this <b>with</b> a lazy <b>update</b> function when we add a new version of the inner <a href="../sui-framework/object.md#0x2_object">object</a>.
    <b>assert</b>!(version == <a href="bridge.md#0xb_bridge_CURRENT_VERSION">CURRENT_VERSION</a>, <a href="bridge.md#0xb_bridge_EWrongInnerVersion">EWrongInnerVersion</a>);
    <b>let</b> inner: &<b>mut</b> <a href="bridge.md#0xb_bridge_BridgeInner">BridgeInner</a> = <a href="bridge.md#0xb_bridge">bridge</a>.inner.load_value_mut();
    <b>assert</b>!(inner.bridge_version == version, <a href="bridge.md#0xb_bridge_EWrongInnerVersion">EWrongInnerVersion</a>);
    inner
}
</code></pre>



</details>

<a name="0xb_bridge_claim_token_internal"></a>

## Function `claim_token_internal`



<pre><code><b>fun</b> <a href="bridge.md#0xb_bridge_claim_token_internal">claim_token_internal</a>&lt;T&gt;(<a href="bridge.md#0xb_bridge">bridge</a>: &<b>mut</b> <a href="bridge.md#0xb_bridge_Bridge">bridge::Bridge</a>, <a href="../sui-framework/clock.md#0x2_clock">clock</a>: &<a href="../sui-framework/clock.md#0x2_clock_Clock">clock::Clock</a>, source_chain: u8, bridge_seq_num: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>, ctx: &<b>mut</b> <a href="../sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): (<a href="../move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;<a href="../sui-framework/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;&gt;, <b>address</b>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="bridge.md#0xb_bridge_claim_token_internal">claim_token_internal</a>&lt;T&gt;(
    <a href="bridge.md#0xb_bridge">bridge</a>: &<b>mut</b> <a href="bridge.md#0xb_bridge_Bridge">Bridge</a>,
    <a href="../sui-framework/clock.md#0x2_clock">clock</a>: &Clock,
    source_chain: u8,
    bridge_seq_num: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>,
    ctx: &<b>mut</b> TxContext,
): (Option&lt;Coin&lt;T&gt;&gt;, <b>address</b>) {
    <b>let</b> inner = <a href="bridge.md#0xb_bridge_load_inner_mut">load_inner_mut</a>(<a href="bridge.md#0xb_bridge">bridge</a>);
    <b>assert</b>!(!inner.paused, <a href="bridge.md#0xb_bridge_EBridgeUnavailable">EBridgeUnavailable</a>);

    <b>let</b> key = <a href="message.md#0xb_message_create_key">message::create_key</a>(source_chain, <a href="message_types.md#0xb_message_types_token">message_types::token</a>(), bridge_seq_num);
    <b>assert</b>!(inner.token_transfer_records.contains(key), <a href="bridge.md#0xb_bridge_EMessageNotFoundInRecords">EMessageNotFoundInRecords</a>);

    // retrieve approved <a href="bridge.md#0xb_bridge">bridge</a> <a href="message.md#0xb_message">message</a>
    <b>let</b> record = &<b>mut</b> inner.token_transfer_records[key];
    // ensure this is a token <a href="bridge.md#0xb_bridge">bridge</a> <a href="message.md#0xb_message">message</a>
    <b>assert</b>!(
        &record.<a href="message.md#0xb_message">message</a>.message_type() == <a href="message_types.md#0xb_message_types_token">message_types::token</a>(),
        <a href="bridge.md#0xb_bridge_EUnexpectedMessageType">EUnexpectedMessageType</a>,
    );
    // Ensure it's signed
    <b>assert</b>!(record.verified_signatures.is_some(), <a href="bridge.md#0xb_bridge_EUnauthorisedClaim">EUnauthorisedClaim</a>);

    // extract token <a href="message.md#0xb_message">message</a>
    <b>let</b> token_payload = record.<a href="message.md#0xb_message">message</a>.extract_token_bridge_payload();
    // get owner <b>address</b>
    <b>let</b> owner = address::from_bytes(token_payload.token_target_address());

    // If already claimed, exit early
    <b>if</b> (record.claimed) {
        emit(<a href="bridge.md#0xb_bridge_TokenTransferAlreadyClaimed">TokenTransferAlreadyClaimed</a> { message_key: key });
        <b>return</b> (<a href="../move-stdlib/option.md#0x1_option_none">option::none</a>(), owner)
    };

    <b>let</b> target_chain = token_payload.token_target_chain();
    // ensure target chain matches <a href="bridge.md#0xb_bridge">bridge</a>.chain_id
    <b>assert</b>!(target_chain == inner.chain_id, <a href="bridge.md#0xb_bridge_EUnexpectedChainID">EUnexpectedChainID</a>);

    // TODO: why do we check validity of the route here? what <b>if</b> inconsistency?
    // Ensure route is valid
    // TODO: add unit tests
    // `get_route` <b>abort</b> <b>if</b> route is invalid
    <b>let</b> route = <a href="chain_ids.md#0xb_chain_ids_get_route">chain_ids::get_route</a>(source_chain, target_chain);
    // check token type
    <b>assert</b>!(
        <a href="treasury.md#0xb_treasury_token_id">treasury::token_id</a>&lt;T&gt;(&inner.<a href="treasury.md#0xb_treasury">treasury</a>) == token_payload.token_type(),
        <a href="bridge.md#0xb_bridge_EUnexpectedTokenType">EUnexpectedTokenType</a>,
    );

    <b>let</b> amount = token_payload.token_amount();
    // Make sure <a href="../sui-framework/transfer.md#0x2_transfer">transfer</a> is within limit.
    <b>if</b> (!inner
        .<a href="limiter.md#0xb_limiter">limiter</a>
        .check_and_record_sending_transfer&lt;T&gt;(
        &inner.<a href="treasury.md#0xb_treasury">treasury</a>,
        <a href="../sui-framework/clock.md#0x2_clock">clock</a>,
        route,
        amount,
    )
    ) {
        emit(<a href="bridge.md#0xb_bridge_TokenTransferLimitExceed">TokenTransferLimitExceed</a> { message_key: key });
        <b>return</b> (<a href="../move-stdlib/option.md#0x1_option_none">option::none</a>(), owner)
    };

    // claim from <a href="treasury.md#0xb_treasury">treasury</a>
    <b>let</b> token = inner.<a href="treasury.md#0xb_treasury">treasury</a>.mint&lt;T&gt;(amount, ctx);

    // Record changes
    record.claimed = <b>true</b>;
    emit(<a href="bridge.md#0xb_bridge_TokenTransferClaimed">TokenTransferClaimed</a> { message_key: key });

    (<a href="../move-stdlib/option.md#0x1_option_some">option::some</a>(token), owner)
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
    <b>let</b> op = payload.emergency_op_type();
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
    <b>let</b> receiving_chain = payload.update_bridge_limit_payload_receiving_chain();
    <b>assert</b>!(receiving_chain == inner.chain_id, <a href="bridge.md#0xb_bridge_EUnexpectedChainID">EUnexpectedChainID</a>);
    <b>let</b> route = <a href="chain_ids.md#0xb_chain_ids_get_route">chain_ids::get_route</a>(
        payload.update_bridge_limit_payload_sending_chain(),
        receiving_chain
    );

    inner.<a href="limiter.md#0xb_limiter">limiter</a>.update_route_limit(
        &route,
        payload.update_bridge_limit_payload_limit()
    )
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
    inner.<a href="treasury.md#0xb_treasury">treasury</a>.update_asset_notional_price(
        payload.update_asset_price_payload_token_id(),
        payload.update_asset_price_payload_new_price()
    )
}
</code></pre>



</details>

<a name="0xb_bridge_execute_add_tokens_on_sui"></a>

## Function `execute_add_tokens_on_sui`



<pre><code><b>fun</b> <a href="bridge.md#0xb_bridge_execute_add_tokens_on_sui">execute_add_tokens_on_sui</a>(inner: &<b>mut</b> <a href="bridge.md#0xb_bridge_BridgeInner">bridge::BridgeInner</a>, payload: <a href="message.md#0xb_message_AddTokenOnSui">message::AddTokenOnSui</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="bridge.md#0xb_bridge_execute_add_tokens_on_sui">execute_add_tokens_on_sui</a>(inner: &<b>mut</b> <a href="bridge.md#0xb_bridge_BridgeInner">BridgeInner</a>, payload: AddTokenOnSui) {
    // FIXME: <b>assert</b> native_token <b>to</b> be <b>false</b> and add test
    <b>let</b> native_token = payload.is_native();
    <b>let</b> <b>mut</b> token_ids = payload.token_ids();
    <b>let</b> <b>mut</b> token_type_names = payload.token_type_names();
    <b>let</b> <b>mut</b> token_prices = payload.token_prices();

    // Make sure token data is consistent
    <b>assert</b>!(token_ids.length() == token_type_names.length(), <a href="bridge.md#0xb_bridge_EMalformedMessageError">EMalformedMessageError</a>);
    <b>assert</b>!(token_ids.length() == token_prices.length(), <a href="bridge.md#0xb_bridge_EMalformedMessageError">EMalformedMessageError</a>);

    <b>while</b> (token_ids.length() &gt; 0) {
        <b>let</b> token_id = token_ids.pop_back();
        <b>let</b> token_type_name = token_type_names.pop_back();
        <b>let</b> token_price = token_prices.pop_back();
        inner.<a href="treasury.md#0xb_treasury">treasury</a>.add_new_token(token_type_name, token_id, native_token, token_price)
    }
}
</code></pre>



</details>

<a name="0xb_bridge_get_current_seq_num_and_increment"></a>

## Function `get_current_seq_num_and_increment`



<pre><code><b>fun</b> <a href="bridge.md#0xb_bridge_get_current_seq_num_and_increment">get_current_seq_num_and_increment</a>(<a href="bridge.md#0xb_bridge">bridge</a>: &<b>mut</b> <a href="bridge.md#0xb_bridge_BridgeInner">bridge::BridgeInner</a>, msg_type: u8): <a href="../move-stdlib/u64.md#0x1_u64">u64</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="bridge.md#0xb_bridge_get_current_seq_num_and_increment">get_current_seq_num_and_increment</a>(<a href="bridge.md#0xb_bridge">bridge</a>: &<b>mut</b> <a href="bridge.md#0xb_bridge_BridgeInner">BridgeInner</a>, msg_type: u8): <a href="../move-stdlib/u64.md#0x1_u64">u64</a> {
    <b>if</b> (!<a href="bridge.md#0xb_bridge">bridge</a>.sequence_nums.contains(&msg_type)) {
        <a href="bridge.md#0xb_bridge">bridge</a>.sequence_nums.insert(msg_type, 1);
        <b>return</b> 0
    };

    <b>let</b> entry = &<b>mut</b> <a href="bridge.md#0xb_bridge">bridge</a>.sequence_nums[&msg_type];
    <b>let</b> seq_num = *entry;
    *entry = seq_num + 1;
    seq_num
}
</code></pre>



</details>

<a name="0xb_bridge_get_parsed_token_transfer_message"></a>

## Function `get_parsed_token_transfer_message`



<pre><code><b>fun</b> <a href="bridge.md#0xb_bridge_get_parsed_token_transfer_message">get_parsed_token_transfer_message</a>(<a href="bridge.md#0xb_bridge">bridge</a>: &<a href="bridge.md#0xb_bridge_Bridge">bridge::Bridge</a>, source_chain: u8, bridge_seq_num: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>): <a href="../move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;<a href="message.md#0xb_message_ParsedTokenTransferMessage">message::ParsedTokenTransferMessage</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="bridge.md#0xb_bridge_get_parsed_token_transfer_message">get_parsed_token_transfer_message</a>(
    <a href="bridge.md#0xb_bridge">bridge</a>: &<a href="bridge.md#0xb_bridge_Bridge">Bridge</a>,
    source_chain: u8,
    bridge_seq_num: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>,
): Option&lt;ParsedTokenTransferMessage&gt; {
    <b>let</b> inner = <a href="bridge.md#0xb_bridge_load_inner">load_inner</a>(<a href="bridge.md#0xb_bridge">bridge</a>);
    <b>let</b> key = <a href="message.md#0xb_message_create_key">message::create_key</a>(
        source_chain,
        <a href="message_types.md#0xb_message_types_token">message_types::token</a>(),
        bridge_seq_num
    );

    <b>if</b> (!inner.token_transfer_records.contains(key)) {
        <b>return</b> <a href="../move-stdlib/option.md#0x1_option_none">option::none</a>()
    };

    <b>let</b> record = &inner.token_transfer_records[key];
    <b>let</b> <a href="message.md#0xb_message">message</a> = &record.<a href="message.md#0xb_message">message</a>;
    <a href="../move-stdlib/option.md#0x1_option_some">option::some</a>(to_parsed_token_transfer_message(<a href="message.md#0xb_message">message</a>))
}
</code></pre>



</details>
