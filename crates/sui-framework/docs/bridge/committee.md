---
title: Module `0xb::committee`
---



-  [Struct `BlocklistValidatorEvent`](#0xb_committee_BlocklistValidatorEvent)
-  [Struct `BridgeCommittee`](#0xb_committee_BridgeCommittee)
-  [Struct `CommitteeUpdateEvent`](#0xb_committee_CommitteeUpdateEvent)
-  [Struct `CommitteeMemberUrlUpdateEvent`](#0xb_committee_CommitteeMemberUrlUpdateEvent)
-  [Struct `CommitteeMember`](#0xb_committee_CommitteeMember)
-  [Struct `CommitteeMemberRegistration`](#0xb_committee_CommitteeMemberRegistration)
-  [Constants](#@Constants_0)
-  [Function `verify_signatures`](#0xb_committee_verify_signatures)
-  [Function `create`](#0xb_committee_create)
-  [Function `register`](#0xb_committee_register)
-  [Function `try_create_next_committee`](#0xb_committee_try_create_next_committee)
-  [Function `execute_blocklist`](#0xb_committee_execute_blocklist)
-  [Function `committee_members`](#0xb_committee_committee_members)
-  [Function `update_node_url`](#0xb_committee_update_node_url)
-  [Function `check_uniqueness_bridge_keys`](#0xb_committee_check_uniqueness_bridge_keys)


<pre><code><b>use</b> <a href="../move-stdlib/option.md#0x1_option">0x1::option</a>;
<b>use</b> <a href="../move-stdlib/vector.md#0x1_vector">0x1::vector</a>;
<b>use</b> <a href="../sui-framework/ecdsa_k1.md#0x2_ecdsa_k1">0x2::ecdsa_k1</a>;
<b>use</b> <a href="../sui-framework/event.md#0x2_event">0x2::event</a>;
<b>use</b> <a href="../sui-framework/tx_context.md#0x2_tx_context">0x2::tx_context</a>;
<b>use</b> <a href="../sui-framework/vec_map.md#0x2_vec_map">0x2::vec_map</a>;
<b>use</b> <a href="../sui-framework/vec_set.md#0x2_vec_set">0x2::vec_set</a>;
<b>use</b> <a href="../sui-system/sui_system.md#0x3_sui_system">0x3::sui_system</a>;
<b>use</b> <a href="crypto.md#0xb_crypto">0xb::crypto</a>;
<b>use</b> <a href="message.md#0xb_message">0xb::message</a>;
</code></pre>



<a name="0xb_committee_BlocklistValidatorEvent"></a>

## Struct `BlocklistValidatorEvent`



<pre><code><b>struct</b> <a href="committee.md#0xb_committee_BlocklistValidatorEvent">BlocklistValidatorEvent</a> <b>has</b> <b>copy</b>, drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>blocklisted: bool</code>
</dt>
<dd>

</dd>
<dt>
<code>public_keys: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;&gt;</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0xb_committee_BridgeCommittee"></a>

## Struct `BridgeCommittee`



<pre><code><b>struct</b> <a href="committee.md#0xb_committee_BridgeCommittee">BridgeCommittee</a> <b>has</b> store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>members: <a href="../sui-framework/vec_map.md#0x2_vec_map_VecMap">vec_map::VecMap</a>&lt;<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, <a href="committee.md#0xb_committee_CommitteeMember">committee::CommitteeMember</a>&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>member_registrations: <a href="../sui-framework/vec_map.md#0x2_vec_map_VecMap">vec_map::VecMap</a>&lt;<b>address</b>, <a href="committee.md#0xb_committee_CommitteeMemberRegistration">committee::CommitteeMemberRegistration</a>&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>last_committee_update_epoch: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0xb_committee_CommitteeUpdateEvent"></a>

## Struct `CommitteeUpdateEvent`



<pre><code><b>struct</b> <a href="committee.md#0xb_committee_CommitteeUpdateEvent">CommitteeUpdateEvent</a> <b>has</b> <b>copy</b>, drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>members: <a href="../sui-framework/vec_map.md#0x2_vec_map_VecMap">vec_map::VecMap</a>&lt;<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, <a href="committee.md#0xb_committee_CommitteeMember">committee::CommitteeMember</a>&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>stake_participation_percentage: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0xb_committee_CommitteeMemberUrlUpdateEvent"></a>

## Struct `CommitteeMemberUrlUpdateEvent`



<pre><code><b>struct</b> <a href="committee.md#0xb_committee_CommitteeMemberUrlUpdateEvent">CommitteeMemberUrlUpdateEvent</a> <b>has</b> <b>copy</b>, drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>member: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>new_url: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0xb_committee_CommitteeMember"></a>

## Struct `CommitteeMember`



<pre><code><b>struct</b> <a href="committee.md#0xb_committee_CommitteeMember">CommitteeMember</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>sui_address: <b>address</b></code>
</dt>
<dd>
 The Sui Address of the validator
</dd>
<dt>
<code>bridge_pubkey_bytes: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;</code>
</dt>
<dd>
 The public key bytes of the bridge key
</dd>
<dt>
<code><a href="../sui-system/voting_power.md#0x3_voting_power">voting_power</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>
 Voting power, values are voting power in the scale of 10000.
</dd>
<dt>
<code>http_rest_url: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;</code>
</dt>
<dd>
 The HTTP REST URL the member's node listens to
 it looks like b'https://127.0.0.1:9191'
</dd>
<dt>
<code>blocklisted: bool</code>
</dt>
<dd>
 If this member is blocklisted
</dd>
</dl>


</details>

<a name="0xb_committee_CommitteeMemberRegistration"></a>

## Struct `CommitteeMemberRegistration`



<pre><code><b>struct</b> <a href="committee.md#0xb_committee_CommitteeMemberRegistration">CommitteeMemberRegistration</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>sui_address: <b>address</b></code>
</dt>
<dd>
 The Sui Address of the validator
</dd>
<dt>
<code>bridge_pubkey_bytes: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;</code>
</dt>
<dd>
 The public key bytes of the bridge key
</dd>
<dt>
<code>http_rest_url: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;</code>
</dt>
<dd>
 The HTTP REST URL the member's node listens to
 it looks like b'https://127.0.0.1:9191'
</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0xb_committee_ENotSystemAddress"></a>



<pre><code><b>const</b> <a href="committee.md#0xb_committee_ENotSystemAddress">ENotSystemAddress</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 3;
</code></pre>



<a name="0xb_committee_EInvalidSignature"></a>



<pre><code><b>const</b> <a href="committee.md#0xb_committee_EInvalidSignature">EInvalidSignature</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 2;
</code></pre>



<a name="0xb_committee_ECDSA_COMPRESSED_PUBKEY_LENGTH"></a>



<pre><code><b>const</b> <a href="committee.md#0xb_committee_ECDSA_COMPRESSED_PUBKEY_LENGTH">ECDSA_COMPRESSED_PUBKEY_LENGTH</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 33;
</code></pre>



<a name="0xb_committee_ECommitteeAlreadyInitiated"></a>



<pre><code><b>const</b> <a href="committee.md#0xb_committee_ECommitteeAlreadyInitiated">ECommitteeAlreadyInitiated</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 7;
</code></pre>



<a name="0xb_committee_EDuplicatePubkey"></a>



<pre><code><b>const</b> <a href="committee.md#0xb_committee_EDuplicatePubkey">EDuplicatePubkey</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 8;
</code></pre>



<a name="0xb_committee_EDuplicatedSignature"></a>



<pre><code><b>const</b> <a href="committee.md#0xb_committee_EDuplicatedSignature">EDuplicatedSignature</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 1;
</code></pre>



<a name="0xb_committee_EInvalidPubkeyLength"></a>



<pre><code><b>const</b> <a href="committee.md#0xb_committee_EInvalidPubkeyLength">EInvalidPubkeyLength</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 6;
</code></pre>



<a name="0xb_committee_ESenderIsNotInBridgeCommittee"></a>



<pre><code><b>const</b> <a href="committee.md#0xb_committee_ESenderIsNotInBridgeCommittee">ESenderIsNotInBridgeCommittee</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 9;
</code></pre>



<a name="0xb_committee_ESenderNotActiveValidator"></a>



<pre><code><b>const</b> <a href="committee.md#0xb_committee_ESenderNotActiveValidator">ESenderNotActiveValidator</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 5;
</code></pre>



<a name="0xb_committee_ESignatureBelowThreshold"></a>



<pre><code><b>const</b> <a href="committee.md#0xb_committee_ESignatureBelowThreshold">ESignatureBelowThreshold</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 0;
</code></pre>



<a name="0xb_committee_EValidatorBlocklistContainsUnknownKey"></a>



<pre><code><b>const</b> <a href="committee.md#0xb_committee_EValidatorBlocklistContainsUnknownKey">EValidatorBlocklistContainsUnknownKey</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 4;
</code></pre>



<a name="0xb_committee_SUI_MESSAGE_PREFIX"></a>



<pre><code><b>const</b> <a href="committee.md#0xb_committee_SUI_MESSAGE_PREFIX">SUI_MESSAGE_PREFIX</a>: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt; = [83, 85, 73, 95, 66, 82, 73, 68, 71, 69, 95, 77, 69, 83, 83, 65, 71, 69];
</code></pre>



<a name="0xb_committee_verify_signatures"></a>

## Function `verify_signatures`



<pre><code><b>public</b> <b>fun</b> <a href="committee.md#0xb_committee_verify_signatures">verify_signatures</a>(self: &<a href="committee.md#0xb_committee_BridgeCommittee">committee::BridgeCommittee</a>, <a href="message.md#0xb_message">message</a>: <a href="message.md#0xb_message_BridgeMessage">message::BridgeMessage</a>, signatures: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="committee.md#0xb_committee_verify_signatures">verify_signatures</a>(
    self: &<a href="committee.md#0xb_committee_BridgeCommittee">BridgeCommittee</a>,
    <a href="message.md#0xb_message">message</a>: BridgeMessage,
    signatures: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;&gt;,
) {
    <b>let</b> (<b>mut</b> i, signature_counts) = (0, <a href="../move-stdlib/vector.md#0x1_vector_length">vector::length</a>(&signatures));
    <b>let</b> <b>mut</b> seen_pub_key = <a href="../sui-framework/vec_set.md#0x2_vec_set_empty">vec_set::empty</a>&lt;<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;&gt;();
    <b>let</b> required_voting_power = <a href="message.md#0xb_message">message</a>.required_voting_power();
    // add prefix <b>to</b> the <a href="message.md#0xb_message">message</a> bytes
    <b>let</b> <b>mut</b> message_bytes = <a href="committee.md#0xb_committee_SUI_MESSAGE_PREFIX">SUI_MESSAGE_PREFIX</a>;
    message_bytes.append(<a href="message.md#0xb_message">message</a>.serialize_message());

    <b>let</b> <b>mut</b> threshold = 0;
    <b>while</b> (i &lt; signature_counts) {
        <b>let</b> pubkey = <a href="../sui-framework/ecdsa_k1.md#0x2_ecdsa_k1_secp256k1_ecrecover">ecdsa_k1::secp256k1_ecrecover</a>(&signatures[i], &message_bytes, 0);

        // check duplicate
        // and make sure pub key is part of the <a href="committee.md#0xb_committee">committee</a>
        <b>assert</b>!(!seen_pub_key.contains(&pubkey), <a href="committee.md#0xb_committee_EDuplicatedSignature">EDuplicatedSignature</a>);
        <b>assert</b>!(self.members.contains(&pubkey), <a href="committee.md#0xb_committee_EInvalidSignature">EInvalidSignature</a>);

        // get <a href="committee.md#0xb_committee">committee</a> signature weight and check pubkey is part of the <a href="committee.md#0xb_committee">committee</a>
        <b>let</b> member = &self.members[&pubkey];
        <b>if</b> (!member.blocklisted) {
            threshold = threshold + member.<a href="../sui-system/voting_power.md#0x3_voting_power">voting_power</a>;
        };
        seen_pub_key.insert(pubkey);
        i = i + 1;
    };

    <b>assert</b>!(threshold &gt;= required_voting_power, <a href="committee.md#0xb_committee_ESignatureBelowThreshold">ESignatureBelowThreshold</a>);
}
</code></pre>



</details>

<a name="0xb_committee_create"></a>

## Function `create`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="committee.md#0xb_committee_create">create</a>(ctx: &<a href="../sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="committee.md#0xb_committee_BridgeCommittee">committee::BridgeCommittee</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui-framework/package.md#0x2_package">package</a>) <b>fun</b> <a href="committee.md#0xb_committee_create">create</a>(ctx: &TxContext): <a href="committee.md#0xb_committee_BridgeCommittee">BridgeCommittee</a> {
    <b>assert</b>!(<a href="../sui-framework/tx_context.md#0x2_tx_context_sender">tx_context::sender</a>(ctx) == @0x0, <a href="committee.md#0xb_committee_ENotSystemAddress">ENotSystemAddress</a>);
    <a href="committee.md#0xb_committee_BridgeCommittee">BridgeCommittee</a> {
        members: <a href="../sui-framework/vec_map.md#0x2_vec_map_empty">vec_map::empty</a>(),
        member_registrations: <a href="../sui-framework/vec_map.md#0x2_vec_map_empty">vec_map::empty</a>(),
        last_committee_update_epoch: 0,
    }
}
</code></pre>



</details>

<a name="0xb_committee_register"></a>

## Function `register`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="committee.md#0xb_committee_register">register</a>(self: &<b>mut</b> <a href="committee.md#0xb_committee_BridgeCommittee">committee::BridgeCommittee</a>, system_state: &<b>mut</b> <a href="../sui-system/sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, bridge_pubkey_bytes: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, http_rest_url: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, ctx: &<a href="../sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui-framework/package.md#0x2_package">package</a>) <b>fun</b> <a href="committee.md#0xb_committee_register">register</a>(
    self: &<b>mut</b> <a href="committee.md#0xb_committee_BridgeCommittee">BridgeCommittee</a>,
    system_state: &<b>mut</b> SuiSystemState,
    bridge_pubkey_bytes: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    http_rest_url: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    ctx: &TxContext
) {
    // We disallow registration after <a href="committee.md#0xb_committee">committee</a> initiated in v1
    <b>assert</b>!(self.members.is_empty(), <a href="committee.md#0xb_committee_ECommitteeAlreadyInitiated">ECommitteeAlreadyInitiated</a>);
    // Ensure pubkey is valid
    <b>assert</b>!(bridge_pubkey_bytes.length() == <a href="committee.md#0xb_committee_ECDSA_COMPRESSED_PUBKEY_LENGTH">ECDSA_COMPRESSED_PUBKEY_LENGTH</a>, <a href="committee.md#0xb_committee_EInvalidPubkeyLength">EInvalidPubkeyLength</a>);
    // sender must be the same sender that created the <a href="../sui-system/validator.md#0x3_validator">validator</a> <a href="../sui-framework/object.md#0x2_object">object</a>, this is <b>to</b> prevent DDoS from non-<a href="../sui-system/validator.md#0x3_validator">validator</a> actor.
    <b>let</b> sender = ctx.sender();
    <b>let</b> validators = system_state.active_validator_addresses();

    <b>assert</b>!(validators.contains(&sender), <a href="committee.md#0xb_committee_ESenderNotActiveValidator">ESenderNotActiveValidator</a>);
    // Sender is active <a href="../sui-system/validator.md#0x3_validator">validator</a>, record the registration

    // In case <a href="../sui-system/validator.md#0x3_validator">validator</a> need <b>to</b> <b>update</b> the info
    <b>let</b> registration = <b>if</b> (self.member_registrations.contains(&sender)) {
        <b>let</b> registration = &<b>mut</b> self.member_registrations[&sender];
        registration.http_rest_url = http_rest_url;
        registration.bridge_pubkey_bytes = bridge_pubkey_bytes;
        *registration
    } <b>else</b> {
        <b>let</b> registration = <a href="committee.md#0xb_committee_CommitteeMemberRegistration">CommitteeMemberRegistration</a> {
            sui_address: sender,
            bridge_pubkey_bytes,
            http_rest_url,
        };
        self.member_registrations.insert(sender, registration);
        registration
    };

    // check uniqueness of the <a href="bridge.md#0xb_bridge">bridge</a> pubkey.
    // `try_create_next_committee` will <b>abort</b> <b>if</b> bridge_pubkey_bytes are not unique and
    // that will fail the end of epoch transaction (possibly "forever", well, we
    // need <b>to</b> deploy proper <a href="../sui-system/validator.md#0x3_validator">validator</a> changes <b>to</b> stop end of epoch from failing).
    <a href="committee.md#0xb_committee_check_uniqueness_bridge_keys">check_uniqueness_bridge_keys</a>(self, bridge_pubkey_bytes);

    emit(registration)
}
</code></pre>



</details>

<a name="0xb_committee_try_create_next_committee"></a>

## Function `try_create_next_committee`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="committee.md#0xb_committee_try_create_next_committee">try_create_next_committee</a>(self: &<b>mut</b> <a href="committee.md#0xb_committee_BridgeCommittee">committee::BridgeCommittee</a>, active_validator_voting_power: <a href="../sui-framework/vec_map.md#0x2_vec_map_VecMap">vec_map::VecMap</a>&lt;<b>address</b>, <a href="../move-stdlib/u64.md#0x1_u64">u64</a>&gt;, min_stake_participation_percentage: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>, ctx: &<a href="../sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui-framework/package.md#0x2_package">package</a>) <b>fun</b> <a href="committee.md#0xb_committee_try_create_next_committee">try_create_next_committee</a>(
    self: &<b>mut</b> <a href="committee.md#0xb_committee_BridgeCommittee">BridgeCommittee</a>,
    active_validator_voting_power: VecMap&lt;<b>address</b>, <a href="../move-stdlib/u64.md#0x1_u64">u64</a>&gt;,
    min_stake_participation_percentage: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>,
    ctx: &TxContext
) {
    <b>let</b> <b>mut</b> i = 0;
    <b>let</b> <b>mut</b> new_members = <a href="../sui-framework/vec_map.md#0x2_vec_map_empty">vec_map::empty</a>();
    <b>let</b> <b>mut</b> stake_participation_percentage = 0;

    <b>while</b> (i &lt; self.member_registrations.size()) {
        // retrieve registration
        <b>let</b> (_, registration) = self.member_registrations.get_entry_by_idx(i);
        // Find <a href="../sui-system/validator.md#0x3_validator">validator</a> stake amount from system state

        // Process registration <b>if</b> it's active <a href="../sui-system/validator.md#0x3_validator">validator</a>
        <b>let</b> <a href="../sui-system/voting_power.md#0x3_voting_power">voting_power</a> = active_validator_voting_power.try_get(&registration.sui_address);
        <b>if</b> (<a href="../sui-system/voting_power.md#0x3_voting_power">voting_power</a>.is_some()) {
            <b>let</b> <a href="../sui-system/voting_power.md#0x3_voting_power">voting_power</a> = <a href="../sui-system/voting_power.md#0x3_voting_power">voting_power</a>.destroy_some();
            stake_participation_percentage = stake_participation_percentage + <a href="../sui-system/voting_power.md#0x3_voting_power">voting_power</a>;

            <b>let</b> member = <a href="committee.md#0xb_committee_CommitteeMember">CommitteeMember</a> {
                sui_address: registration.sui_address,
                bridge_pubkey_bytes: registration.bridge_pubkey_bytes,
                <a href="../sui-system/voting_power.md#0x3_voting_power">voting_power</a>: (<a href="../sui-system/voting_power.md#0x3_voting_power">voting_power</a> <b>as</b> <a href="../move-stdlib/u64.md#0x1_u64">u64</a>),
                http_rest_url: registration.http_rest_url,
                blocklisted: <b>false</b>,
            };

            new_members.insert(registration.bridge_pubkey_bytes, member)
        };

        i = i + 1;
    };

    // Make sure the new <a href="committee.md#0xb_committee">committee</a> represent enough stakes, percentage are accurate <b>to</b> 2DP
    <b>if</b> (stake_participation_percentage &gt;= min_stake_participation_percentage) {
        // Clear registrations
        self.member_registrations = <a href="../sui-framework/vec_map.md#0x2_vec_map_empty">vec_map::empty</a>();
        // Store new <a href="committee.md#0xb_committee">committee</a> info
        self.members = new_members;
        self.last_committee_update_epoch = ctx.epoch();

        emit(<a href="committee.md#0xb_committee_CommitteeUpdateEvent">CommitteeUpdateEvent</a> {
            members: new_members,
            stake_participation_percentage
        })
    }
}
</code></pre>



</details>

<a name="0xb_committee_execute_blocklist"></a>

## Function `execute_blocklist`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="committee.md#0xb_committee_execute_blocklist">execute_blocklist</a>(self: &<b>mut</b> <a href="committee.md#0xb_committee_BridgeCommittee">committee::BridgeCommittee</a>, blocklist: <a href="message.md#0xb_message_Blocklist">message::Blocklist</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui-framework/package.md#0x2_package">package</a>) <b>fun</b> <a href="committee.md#0xb_committee_execute_blocklist">execute_blocklist</a>(self: &<b>mut</b> <a href="committee.md#0xb_committee_BridgeCommittee">BridgeCommittee</a>, blocklist: Blocklist) {
    <b>let</b> blocklisted = blocklist.blocklist_type() != 1;
    <b>let</b> eth_addresses = blocklist.blocklist_validator_addresses();
    <b>let</b> list_len = eth_addresses.length();
    <b>let</b> <b>mut</b> list_idx = 0;
    <b>let</b> <b>mut</b> member_idx = 0;
    <b>let</b> <b>mut</b> pub_keys = <a href="../move-stdlib/vector.md#0x1_vector">vector</a>[];

    <b>while</b> (list_idx &lt; list_len) {
        <b>let</b> target_address = &eth_addresses[list_idx];
        <b>let</b> <b>mut</b> found = <b>false</b>;

        <b>while</b> (member_idx &lt; self.members.size()) {
            <b>let</b> (pub_key, member) = self.members.get_entry_by_idx_mut(member_idx);
            <b>let</b> eth_address = <a href="crypto.md#0xb_crypto_ecdsa_pub_key_to_eth_address">crypto::ecdsa_pub_key_to_eth_address</a>(pub_key);

            <b>if</b> (*target_address == eth_address) {
                member.blocklisted = blocklisted;
                pub_keys.push_back(*pub_key);
                found = <b>true</b>;
                member_idx = 0;
                <b>break</b>
            };

            member_idx = member_idx + 1;
        };

        <b>assert</b>!(found, <a href="committee.md#0xb_committee_EValidatorBlocklistContainsUnknownKey">EValidatorBlocklistContainsUnknownKey</a>);
        list_idx = list_idx + 1;
    };

    emit(<a href="committee.md#0xb_committee_BlocklistValidatorEvent">BlocklistValidatorEvent</a> {
        blocklisted,
        public_keys: pub_keys,
    })
}
</code></pre>



</details>

<a name="0xb_committee_committee_members"></a>

## Function `committee_members`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="committee.md#0xb_committee_committee_members">committee_members</a>(self: &<a href="committee.md#0xb_committee_BridgeCommittee">committee::BridgeCommittee</a>): &<a href="../sui-framework/vec_map.md#0x2_vec_map_VecMap">vec_map::VecMap</a>&lt;<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, <a href="committee.md#0xb_committee_CommitteeMember">committee::CommitteeMember</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui-framework/package.md#0x2_package">package</a>) <b>fun</b> <a href="committee.md#0xb_committee_committee_members">committee_members</a>(
    self: &<a href="committee.md#0xb_committee_BridgeCommittee">BridgeCommittee</a>,
): &VecMap&lt;<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, <a href="committee.md#0xb_committee_CommitteeMember">CommitteeMember</a>&gt; {
    &self.members
}
</code></pre>



</details>

<a name="0xb_committee_update_node_url"></a>

## Function `update_node_url`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="committee.md#0xb_committee_update_node_url">update_node_url</a>(self: &<b>mut</b> <a href="committee.md#0xb_committee_BridgeCommittee">committee::BridgeCommittee</a>, new_url: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, ctx: &<a href="../sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui-framework/package.md#0x2_package">package</a>) <b>fun</b> <a href="committee.md#0xb_committee_update_node_url">update_node_url</a>(self: &<b>mut</b> <a href="committee.md#0xb_committee_BridgeCommittee">BridgeCommittee</a>, new_url: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, ctx: &TxContext) {
    <b>let</b> <b>mut</b> idx = 0;
    <b>while</b> (idx &lt; self.members.size()) {
        <b>let</b> (_, member) = self.members.get_entry_by_idx_mut(idx);
        <b>if</b> (member.sui_address == ctx.sender()) {
            member.http_rest_url = new_url;
            emit (<a href="committee.md#0xb_committee_CommitteeMemberUrlUpdateEvent">CommitteeMemberUrlUpdateEvent</a> {
                member: member.bridge_pubkey_bytes,
                new_url
            });
            <b>return</b>
        };
        idx = idx + 1;
    };
    <b>abort</b> <a href="committee.md#0xb_committee_ESenderIsNotInBridgeCommittee">ESenderIsNotInBridgeCommittee</a>
}
</code></pre>



</details>

<a name="0xb_committee_check_uniqueness_bridge_keys"></a>

## Function `check_uniqueness_bridge_keys`



<pre><code><b>fun</b> <a href="committee.md#0xb_committee_check_uniqueness_bridge_keys">check_uniqueness_bridge_keys</a>(self: &<a href="committee.md#0xb_committee_BridgeCommittee">committee::BridgeCommittee</a>, bridge_pubkey_bytes: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="committee.md#0xb_committee_check_uniqueness_bridge_keys">check_uniqueness_bridge_keys</a>(self: &<a href="committee.md#0xb_committee_BridgeCommittee">BridgeCommittee</a>, bridge_pubkey_bytes: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;) {
    <b>let</b> <b>mut</b> count = self.member_registrations.size();
    // bridge_pubkey_bytes must be found once and once only
    <b>let</b> <b>mut</b> bridge_key_found = <b>false</b>;
    <b>while</b> (count &gt; 0) {
        count = count - 1;
        <b>let</b> (_, registration) = self.member_registrations.get_entry_by_idx(count);
        <b>if</b> (registration.bridge_pubkey_bytes == bridge_pubkey_bytes) {
            <b>assert</b>!(!bridge_key_found, <a href="committee.md#0xb_committee_EDuplicatePubkey">EDuplicatePubkey</a>);
            bridge_key_found = <b>true</b>; // bridge_pubkey_bytes found, we must not have another one
        }
    };
}
</code></pre>



</details>
