
<a name="0xb_committee"></a>

# Module `0xb::committee`



-  [Struct `BridgeCommittee`](#0xb_committee_BridgeCommittee)
-  [Struct `CommitteeMember`](#0xb_committee_CommitteeMember)
-  [Constants](#@Constants_0)
-  [Function `create`](#0xb_committee_create)
-  [Function `verify_signatures`](#0xb_committee_verify_signatures)


<pre><code><b>use</b> <a href="dependencies/move-stdlib/vector.md#0x1_vector">0x1::vector</a>;
<b>use</b> <a href="dependencies/sui-framework/address.md#0x2_address">0x2::address</a>;
<b>use</b> <a href="dependencies/sui-framework/ecdsa_k1.md#0x2_ecdsa_k1">0x2::ecdsa_k1</a>;
<b>use</b> <a href="dependencies/sui-framework/hex.md#0x2_hex">0x2::hex</a>;
<b>use</b> <a href="dependencies/sui-framework/tx_context.md#0x2_tx_context">0x2::tx_context</a>;
<b>use</b> <a href="dependencies/sui-framework/vec_map.md#0x2_vec_map">0x2::vec_map</a>;
<b>use</b> <a href="dependencies/sui-framework/vec_set.md#0x2_vec_set">0x2::vec_set</a>;
<b>use</b> <a href="message.md#0xb_message">0xb::message</a>;
<b>use</b> <a href="message_types.md#0xb_message_types">0xb::message_types</a>;
</code></pre>



<a name="0xb_committee_BridgeCommittee"></a>

## Struct `BridgeCommittee`



<pre><code><b>struct</b> <a href="committee.md#0xb_committee_BridgeCommittee">BridgeCommittee</a> <b>has</b> store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>members: <a href="dependencies/sui-framework/vec_map.md#0x2_vec_map_VecMap">vec_map::VecMap</a>&lt;<a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, <a href="committee.md#0xb_committee_CommitteeMember">committee::CommitteeMember</a>&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>thresholds: <a href="dependencies/sui-framework/vec_map.md#0x2_vec_map_VecMap">vec_map::VecMap</a>&lt;u8, u64&gt;</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0xb_committee_CommitteeMember"></a>

## Struct `CommitteeMember`



<pre><code><b>struct</b> <a href="committee.md#0xb_committee_CommitteeMember">CommitteeMember</a> <b>has</b> drop, store
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
<code>bridge_pubkey_bytes: <a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;</code>
</dt>
<dd>
 The public key bytes of the bridge key
</dd>
<dt>
<code>voting_power: u64</code>
</dt>
<dd>
 Voting power
</dd>
<dt>
<code>http_rest_url: <a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;</code>
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

<a name="@Constants_0"></a>

## Constants


<a name="0xb_committee_ENotSystemAddress"></a>



<pre><code><b>const</b> <a href="committee.md#0xb_committee_ENotSystemAddress">ENotSystemAddress</a>: u64 = 3;
</code></pre>



<a name="0xb_committee_EInvalidSignature"></a>



<pre><code><b>const</b> <a href="committee.md#0xb_committee_EInvalidSignature">EInvalidSignature</a>: u64 = 2;
</code></pre>



<a name="0xb_committee_EDuplicatedSignature"></a>



<pre><code><b>const</b> <a href="committee.md#0xb_committee_EDuplicatedSignature">EDuplicatedSignature</a>: u64 = 1;
</code></pre>



<a name="0xb_committee_ESignatureBelowThreshold"></a>



<pre><code><b>const</b> <a href="committee.md#0xb_committee_ESignatureBelowThreshold">ESignatureBelowThreshold</a>: u64 = 0;
</code></pre>



<a name="0xb_committee_SUI_MESSAGE_PREFIX"></a>



<pre><code><b>const</b> <a href="committee.md#0xb_committee_SUI_MESSAGE_PREFIX">SUI_MESSAGE_PREFIX</a>: <a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt; = [83, 85, 73, 95, 66, 82, 73, 68, 71, 69, 95, 77, 69, 83, 83, 65, 71, 69];
</code></pre>



<a name="0xb_committee_create"></a>

## Function `create`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="committee.md#0xb_committee_create">create</a>(ctx: &<a href="dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="committee.md#0xb_committee_BridgeCommittee">committee::BridgeCommittee</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="committee.md#0xb_committee_create">create</a>(ctx: &TxContext): <a href="committee.md#0xb_committee_BridgeCommittee">BridgeCommittee</a> {
    <b>assert</b>!(<a href="dependencies/sui-framework/tx_context.md#0x2_tx_context_sender">tx_context::sender</a>(ctx) == @0x0, <a href="committee.md#0xb_committee_ENotSystemAddress">ENotSystemAddress</a>);
    // Hardcoded genesis <a href="committee.md#0xb_committee">committee</a>
    // TODO: change this <b>to</b> real committe members
    <b>let</b> members = <a href="dependencies/sui-framework/vec_map.md#0x2_vec_map_empty">vec_map::empty</a>&lt;<a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, <a href="committee.md#0xb_committee_CommitteeMember">CommitteeMember</a>&gt;();

    <b>let</b> bridge_pubkey_bytes = <a href="dependencies/sui-framework/hex.md#0x2_hex_decode">hex::decode</a>(b"029bef8d556d80e43ae7e0becb3a7e6838b95defe45896ed6075bb9035d06c9964");
    <a href="dependencies/sui-framework/vec_map.md#0x2_vec_map_insert">vec_map::insert</a>(&<b>mut</b> members, bridge_pubkey_bytes, <a href="committee.md#0xb_committee_CommitteeMember">CommitteeMember</a> {
        sui_address: address::from_u256(1),
        bridge_pubkey_bytes,
        voting_power: 10,
        http_rest_url: b"https://127.0.0.1:9191",
        blocklisted: <b>false</b>
    });

    <b>let</b> bridge_pubkey_bytes = <a href="dependencies/sui-framework/hex.md#0x2_hex_decode">hex::decode</a>(b"033e99a541db69bd32040dfe5037fbf5210dafa8151a71e21c5204b05d95ce0a62");
    <a href="dependencies/sui-framework/vec_map.md#0x2_vec_map_insert">vec_map::insert</a>(&<b>mut</b> members, bridge_pubkey_bytes, <a href="committee.md#0xb_committee_CommitteeMember">CommitteeMember</a> {
        sui_address: address::from_u256(2),
        bridge_pubkey_bytes,
        voting_power: 10,
        http_rest_url: b"https://127.0.0.1:9192",
        blocklisted: <b>false</b>
    });

    <b>let</b> thresholds = <a href="dependencies/sui-framework/vec_map.md#0x2_vec_map_empty">vec_map::empty</a>();
    <a href="dependencies/sui-framework/vec_map.md#0x2_vec_map_insert">vec_map::insert</a>(&<b>mut</b> thresholds, <a href="message_types.md#0xb_message_types_token">message_types::token</a>(), 10);
    <a href="committee.md#0xb_committee_BridgeCommittee">BridgeCommittee</a> { members, thresholds }
}
</code></pre>



</details>

<a name="0xb_committee_verify_signatures"></a>

## Function `verify_signatures`



<pre><code><b>public</b> <b>fun</b> <a href="committee.md#0xb_committee_verify_signatures">verify_signatures</a>(self: &<a href="committee.md#0xb_committee_BridgeCommittee">committee::BridgeCommittee</a>, <a href="message.md#0xb_message">message</a>: <a href="message.md#0xb_message_BridgeMessage">message::BridgeMessage</a>, signatures: <a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="committee.md#0xb_committee_verify_signatures">verify_signatures</a>(
    self: &<a href="committee.md#0xb_committee_BridgeCommittee">BridgeCommittee</a>,
    <a href="message.md#0xb_message">message</a>: BridgeMessage,
    signatures: <a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;&gt;,
) {
    <b>let</b> (i, signature_counts) = (0, <a href="dependencies/move-stdlib/vector.md#0x1_vector_length">vector::length</a>(&signatures));
    <b>let</b> seen_pub_key = <a href="dependencies/sui-framework/vec_set.md#0x2_vec_set_empty">vec_set::empty</a>&lt;<a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;&gt;();
    <b>let</b> required_threshold = *<a href="dependencies/sui-framework/vec_map.md#0x2_vec_map_get">vec_map::get</a>(&self.thresholds, &<a href="message.md#0xb_message_message_type">message::message_type</a>(&<a href="message.md#0xb_message">message</a>));

    // add prefix <b>to</b> the <a href="message.md#0xb_message">message</a> bytes
    <b>let</b> message_bytes = <a href="committee.md#0xb_committee_SUI_MESSAGE_PREFIX">SUI_MESSAGE_PREFIX</a>;
    <a href="dependencies/move-stdlib/vector.md#0x1_vector_append">vector::append</a>(&<b>mut</b> message_bytes, <a href="message.md#0xb_message_serialize_message">message::serialize_message</a>(<a href="message.md#0xb_message">message</a>));

    <b>let</b> threshold = 0;
    <b>while</b> (i &lt; signature_counts) {
        <b>let</b> signature = <a href="dependencies/move-stdlib/vector.md#0x1_vector_borrow">vector::borrow</a>(&signatures, i);
        <b>let</b> pubkey = <a href="dependencies/sui-framework/ecdsa_k1.md#0x2_ecdsa_k1_secp256k1_ecrecover">ecdsa_k1::secp256k1_ecrecover</a>(signature, &message_bytes, 0);
        // check duplicate
        <b>assert</b>!(!<a href="dependencies/sui-framework/vec_set.md#0x2_vec_set_contains">vec_set::contains</a>(&seen_pub_key, &pubkey), <a href="committee.md#0xb_committee_EDuplicatedSignature">EDuplicatedSignature</a>);
        // make sure pub key is part of the <a href="committee.md#0xb_committee">committee</a>
        <b>assert</b>!(<a href="dependencies/sui-framework/vec_map.md#0x2_vec_map_contains">vec_map::contains</a>(&self.members, &pubkey), <a href="committee.md#0xb_committee_EInvalidSignature">EInvalidSignature</a>);
        // get <a href="committee.md#0xb_committee">committee</a> signature weight and check pubkey is part of the <a href="committee.md#0xb_committee">committee</a>
        <b>let</b> member = <a href="dependencies/sui-framework/vec_map.md#0x2_vec_map_get">vec_map::get</a>(&self.members, &pubkey);
        <b>if</b> (!member.blocklisted) {
            threshold = threshold + member.voting_power;
        };
        i = i + 1;
        <a href="dependencies/sui-framework/vec_set.md#0x2_vec_set_insert">vec_set::insert</a>(&<b>mut</b> seen_pub_key, pubkey);
    };
    <b>assert</b>!(threshold &gt;= required_threshold, <a href="committee.md#0xb_committee_ESignatureBelowThreshold">ESignatureBelowThreshold</a>);
}
</code></pre>



</details>
