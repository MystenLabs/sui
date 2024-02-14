
<a name="0xb_committee"></a>

# Module `0xb::committee`



-  [Struct `BlocklistValidatorEvent`](#0xb_committee_BlocklistValidatorEvent)
-  [Struct `BridgeCommittee`](#0xb_committee_BridgeCommittee)
-  [Struct `CommitteeMember`](#0xb_committee_CommitteeMember)
-  [Struct `CommitteeMemberRegistration`](#0xb_committee_CommitteeMemberRegistration)
-  [Constants](#@Constants_0)
-  [Function `create`](#0xb_committee_create)
-  [Function `verify_signatures`](#0xb_committee_verify_signatures)
-  [Function `register`](#0xb_committee_register)
-  [Function `try_create_next_committee`](#0xb_committee_try_create_next_committee)
-  [Function `find_validator`](#0xb_committee_find_validator)
-  [Function `execute_blocklist`](#0xb_committee_execute_blocklist)


<pre><code><b>use</b> <a href="dependencies/move-stdlib/option.md#0x1_option">0x1::option</a>;
<b>use</b> <a href="dependencies/move-stdlib/vector.md#0x1_vector">0x1::vector</a>;
<b>use</b> <a href="dependencies/sui-framework/ecdsa_k1.md#0x2_ecdsa_k1">0x2::ecdsa_k1</a>;
<b>use</b> <a href="dependencies/sui-framework/event.md#0x2_event">0x2::event</a>;
<b>use</b> <a href="dependencies/sui-framework/tx_context.md#0x2_tx_context">0x2::tx_context</a>;
<b>use</b> <a href="dependencies/sui-framework/vec_map.md#0x2_vec_map">0x2::vec_map</a>;
<b>use</b> <a href="dependencies/sui-framework/vec_set.md#0x2_vec_set">0x2::vec_set</a>;
<b>use</b> <a href="dependencies/sui-system/sui_system.md#0x3_sui_system">0x3::sui_system</a>;
<b>use</b> <a href="dependencies/sui-system/validator.md#0x3_validator">0x3::validator</a>;
<b>use</b> <a href="dependencies/sui-system/validator_set.md#0x3_validator_set">0x3::validator_set</a>;
<b>use</b> <a href="crypto.md#0xb_crypto">0xb::crypto</a>;
<b>use</b> <a href="message.md#0xb_message">0xb::message</a>;
<b>use</b> <a href="message_types.md#0xb_message_types">0xb::message_types</a>;
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
<code>public_keys: <a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;&gt;</code>
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
<code>members: <a href="dependencies/sui-framework/vec_map.md#0x2_vec_map_VecMap">vec_map::VecMap</a>&lt;<a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, <a href="committee.md#0xb_committee_CommitteeMember">committee::CommitteeMember</a>&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>stake_thresholds_percentage: <a href="dependencies/sui-framework/vec_map.md#0x2_vec_map_VecMap">vec_map::VecMap</a>&lt;u8, u8&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>member_registration: <a href="dependencies/sui-framework/vec_map.md#0x2_vec_map_VecMap">vec_map::VecMap</a>&lt;<b>address</b>, <a href="committee.md#0xb_committee_CommitteeMemberRegistration">committee::CommitteeMemberRegistration</a>&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>total_member_stake: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>update_epoch: u64</code>
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
<code>stake_amount: u64</code>
</dt>
<dd>
 Voting power (stake amount)
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

<a name="0xb_committee_CommitteeMemberRegistration"></a>

## Struct `CommitteeMemberRegistration`



<pre><code><b>struct</b> <a href="committee.md#0xb_committee_CommitteeMemberRegistration">CommitteeMemberRegistration</a> <b>has</b> drop, store
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
<code>http_rest_url: <a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;</code>
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



<pre><code><b>const</b> <a href="committee.md#0xb_committee_ENotSystemAddress">ENotSystemAddress</a>: u64 = 3;
</code></pre>



<a name="0xb_committee_EInvalidSignature"></a>



<pre><code><b>const</b> <a href="committee.md#0xb_committee_EInvalidSignature">EInvalidSignature</a>: u64 = 2;
</code></pre>



<a name="0xb_committee_EDuplicatedSignature"></a>



<pre><code><b>const</b> <a href="committee.md#0xb_committee_EDuplicatedSignature">EDuplicatedSignature</a>: u64 = 1;
</code></pre>



<a name="0xb_committee_ESenderNotActiveValidator"></a>



<pre><code><b>const</b> <a href="committee.md#0xb_committee_ESenderNotActiveValidator">ESenderNotActiveValidator</a>: u64 = 5;
</code></pre>



<a name="0xb_committee_ESignatureBelowThreshold"></a>



<pre><code><b>const</b> <a href="committee.md#0xb_committee_ESignatureBelowThreshold">ESignatureBelowThreshold</a>: u64 = 0;
</code></pre>



<a name="0xb_committee_EValidatorBlocklistContainsUnknownKey"></a>



<pre><code><b>const</b> <a href="committee.md#0xb_committee_EValidatorBlocklistContainsUnknownKey">EValidatorBlocklistContainsUnknownKey</a>: u64 = 4;
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
    // Default signature threshold
    <b>let</b> thresholds = <a href="dependencies/sui-framework/vec_map.md#0x2_vec_map_empty">vec_map::empty</a>();
    <a href="dependencies/sui-framework/vec_map.md#0x2_vec_map_insert">vec_map::insert</a>(&<b>mut</b> thresholds, <a href="message_types.md#0xb_message_types_token">message_types::token</a>(), 50);
    <a href="dependencies/sui-framework/vec_map.md#0x2_vec_map_insert">vec_map::insert</a>(&<b>mut</b> thresholds, <a href="message_types.md#0xb_message_types_committee_blocklist">message_types::committee_blocklist</a>(), 50);
    <a href="dependencies/sui-framework/vec_map.md#0x2_vec_map_insert">vec_map::insert</a>(&<b>mut</b> thresholds, <a href="message_types.md#0xb_message_types_emergency_op">message_types::emergency_op</a>(), 50);
    <a href="dependencies/sui-framework/vec_map.md#0x2_vec_map_insert">vec_map::insert</a>(&<b>mut</b> thresholds, <a href="message_types.md#0xb_message_types_update_asset_price">message_types::update_asset_price</a>(), 50);
    <a href="dependencies/sui-framework/vec_map.md#0x2_vec_map_insert">vec_map::insert</a>(&<b>mut</b> thresholds, <a href="message_types.md#0xb_message_types_update_bridge_limit">message_types::update_bridge_limit</a>(), 50);
    <a href="committee.md#0xb_committee_BridgeCommittee">BridgeCommittee</a> {
        members: <a href="dependencies/sui-framework/vec_map.md#0x2_vec_map_empty">vec_map::empty</a>(),
        stake_thresholds_percentage: thresholds,
        member_registration: <a href="dependencies/sui-framework/vec_map.md#0x2_vec_map_empty">vec_map::empty</a>(),
        total_member_stake: 0,
        update_epoch: 0,
    }
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
    <b>let</b> required_stake_percentage = (*<a href="dependencies/sui-framework/vec_map.md#0x2_vec_map_get">vec_map::get</a>(
        &self.stake_thresholds_percentage,
        &<a href="message.md#0xb_message_message_type">message::message_type</a>(&<a href="message.md#0xb_message">message</a>)
    ) <b>as</b> u64);
    <b>let</b> required_stake = required_stake_percentage * self.total_member_stake / 100;

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
            threshold = threshold + member.stake_amount;
        };
        i = i + 1;
        <a href="dependencies/sui-framework/vec_set.md#0x2_vec_set_insert">vec_set::insert</a>(&<b>mut</b> seen_pub_key, pubkey);
    };
    <b>assert</b>!(threshold &gt;= required_stake, <a href="committee.md#0xb_committee_ESignatureBelowThreshold">ESignatureBelowThreshold</a>);
}
</code></pre>



</details>

<a name="0xb_committee_register"></a>

## Function `register`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="committee.md#0xb_committee_register">register</a>(self: &<b>mut</b> <a href="committee.md#0xb_committee_BridgeCommittee">committee::BridgeCommittee</a>, system_state: &<a href="dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, bridge_pubkey_bytes: <a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, http_rest_url: <a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, ctx: &<a href="dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="committee.md#0xb_committee_register">register</a>(
    self: &<b>mut</b> <a href="committee.md#0xb_committee_BridgeCommittee">BridgeCommittee</a>,
    system_state: &SuiSystemState,
    bridge_pubkey_bytes: <a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    http_rest_url: <a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    ctx: &TxContext
) {
    // sender must be the same sender that created the <a href="dependencies/sui-system/validator.md#0x3_validator">validator</a> <a href="dependencies/sui-framework/object.md#0x2_object">object</a>
    <b>let</b> sender = <a href="dependencies/sui-framework/tx_context.md#0x2_tx_context_sender">tx_context::sender</a>(ctx);
    <b>let</b> validators = <a href="dependencies/sui-system/sui_system.md#0x3_sui_system_validators">sui_system::validators</a>(system_state);
    <b>let</b> active_validators = <a href="dependencies/sui-system/validator_set.md#0x3_validator_set_active_validators">validator_set::active_validators</a>(validators);
    <b>let</b> validator_index = <a href="committee.md#0xb_committee_find_validator">find_validator</a>(active_validators, sender);

    <b>assert</b>!(<a href="dependencies/move-stdlib/option.md#0x1_option_is_some">option::is_some</a>(&validator_index), <a href="committee.md#0xb_committee_ESenderNotActiveValidator">ESenderNotActiveValidator</a>);
    // Sender is active <a href="dependencies/sui-system/validator.md#0x3_validator">validator</a>, record the registration

    // In case <a href="dependencies/sui-system/validator.md#0x3_validator">validator</a> need <b>to</b> <b>update</b> the info
    <b>if</b> (<a href="dependencies/sui-framework/vec_map.md#0x2_vec_map_contains">vec_map::contains</a>(&self.member_registration, &sender)) {
        <b>let</b> registration = <a href="dependencies/sui-framework/vec_map.md#0x2_vec_map_get_mut">vec_map::get_mut</a>(&<b>mut</b> self.member_registration, &sender);
        registration.http_rest_url = http_rest_url;
        registration.bridge_pubkey_bytes = bridge_pubkey_bytes;
    }<b>else</b> {
        <b>let</b> registration = <a href="committee.md#0xb_committee_CommitteeMemberRegistration">CommitteeMemberRegistration</a> {
            sui_address: sender,
            bridge_pubkey_bytes,
            http_rest_url,
        };
        <a href="dependencies/sui-framework/vec_map.md#0x2_vec_map_insert">vec_map::insert</a>(&<b>mut</b> self.member_registration, sender, registration);
    }
}
</code></pre>



</details>

<a name="0xb_committee_try_create_next_committee"></a>

## Function `try_create_next_committee`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="committee.md#0xb_committee_try_create_next_committee">try_create_next_committee</a>(self: &<b>mut</b> <a href="committee.md#0xb_committee_BridgeCommittee">committee::BridgeCommittee</a>, system_state: &<a href="dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, min_stake_participation_percentage: u8)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="committee.md#0xb_committee_try_create_next_committee">try_create_next_committee</a>(
    self: &<b>mut</b> <a href="committee.md#0xb_committee_BridgeCommittee">BridgeCommittee</a>,
    system_state: &SuiSystemState,
    min_stake_participation_percentage: u8,
) {
    <b>let</b> validators = <a href="dependencies/sui-system/sui_system.md#0x3_sui_system_validators">sui_system::validators</a>(system_state);
    <b>let</b> active_validators = <a href="dependencies/sui-system/validator_set.md#0x3_validator_set_active_validators">validator_set::active_validators</a>(validators);
    <b>let</b> total_member_stake = 0;
    <b>let</b> i = 0;

    <b>let</b> new_members = <a href="dependencies/sui-framework/vec_map.md#0x2_vec_map_empty">vec_map::empty</a>();

    <b>while</b> (i &lt; <a href="dependencies/sui-framework/vec_map.md#0x2_vec_map_size">vec_map::size</a>(&self.member_registration)) {
        // retrieve registration
        <b>let</b> (_, registration) = <a href="dependencies/sui-framework/vec_map.md#0x2_vec_map_get_entry_by_idx">vec_map::get_entry_by_idx</a>(&self.member_registration, i);
        // Find <a href="dependencies/sui-system/validator.md#0x3_validator">validator</a> info from system state
        <b>let</b> validator_index = <a href="committee.md#0xb_committee_find_validator">find_validator</a>(active_validators, registration.sui_address);
        // Process registration <b>if</b> it's active <a href="dependencies/sui-system/validator.md#0x3_validator">validator</a>
        <b>if</b> (<a href="dependencies/move-stdlib/option.md#0x1_option_is_some">option::is_some</a>(&validator_index)) {
            <b>let</b> index = <a href="dependencies/move-stdlib/option.md#0x1_option_destroy_some">option::destroy_some</a>(validator_index);
            <b>let</b> <a href="dependencies/sui-system/validator.md#0x3_validator">validator</a> = <a href="dependencies/move-stdlib/vector.md#0x1_vector_borrow">vector::borrow</a>(active_validators, index);
            <b>let</b> stake_amount = <a href="dependencies/sui-system/validator.md#0x3_validator_stake_amount">validator::stake_amount</a>(<a href="dependencies/sui-system/validator.md#0x3_validator">validator</a>);

            total_member_stake = total_member_stake + stake_amount;
            <b>let</b> member = <a href="committee.md#0xb_committee_CommitteeMember">CommitteeMember</a> {
                sui_address: registration.sui_address,
                bridge_pubkey_bytes: registration.bridge_pubkey_bytes,
                stake_amount,
                http_rest_url: registration.http_rest_url,
                blocklisted: <b>false</b>,
            };
            <a href="dependencies/sui-framework/vec_map.md#0x2_vec_map_insert">vec_map::insert</a>(&<b>mut</b> new_members, registration.bridge_pubkey_bytes, member)
        };
        i = i + 1;
    };

    // Make sure the new <a href="committee.md#0xb_committee">committee</a> represent enough stakes
    <b>let</b> stake_participation_percentage = ((total_member_stake * 100 / <a href="dependencies/sui-system/validator_set.md#0x3_validator_set_total_stake">validator_set::total_stake</a>(
        validators
    )) <b>as</b> u8);

    // Store new <a href="committee.md#0xb_committee">committee</a> info
    <b>if</b> (stake_participation_percentage &gt;= min_stake_participation_percentage) {
        self.total_member_stake = total_member_stake;
        // Clear registrations
        self.member_registration = <a href="dependencies/sui-framework/vec_map.md#0x2_vec_map_empty">vec_map::empty</a>();
        self.members = new_members
    }
}
</code></pre>



</details>

<a name="0xb_committee_find_validator"></a>

## Function `find_validator`



<pre><code><b>fun</b> <a href="committee.md#0xb_committee_find_validator">find_validator</a>(validators: &<a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="dependencies/sui-system/validator.md#0x3_validator_Validator">validator::Validator</a>&gt;, validator_address: <b>address</b>): <a href="dependencies/move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;u64&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="committee.md#0xb_committee_find_validator">find_validator</a>(validators: &<a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;Validator&gt;, validator_address: <b>address</b>): Option&lt;u64&gt; {
    <b>let</b> length = <a href="dependencies/move-stdlib/vector.md#0x1_vector_length">vector::length</a>(validators);
    <b>let</b> i = 0;
    <b>while</b> (i &lt; length) {
        <b>let</b> v = <a href="dependencies/move-stdlib/vector.md#0x1_vector_borrow">vector::borrow</a>(validators, i);
        <b>if</b> (<a href="dependencies/sui-system/validator.md#0x3_validator_sui_address">validator::sui_address</a>(v) == validator_address) {
            <b>return</b> <a href="dependencies/move-stdlib/option.md#0x1_option_some">option::some</a>(i)
        };
        i = i + 1;
    };
    <a href="dependencies/move-stdlib/option.md#0x1_option_none">option::none</a>()
}
</code></pre>



</details>

<a name="0xb_committee_execute_blocklist"></a>

## Function `execute_blocklist`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="committee.md#0xb_committee_execute_blocklist">execute_blocklist</a>(self: &<b>mut</b> <a href="committee.md#0xb_committee_BridgeCommittee">committee::BridgeCommittee</a>, blocklist: <a href="message.md#0xb_message_Blocklist">message::Blocklist</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="committee.md#0xb_committee_execute_blocklist">execute_blocklist</a>(self: &<b>mut</b> <a href="committee.md#0xb_committee_BridgeCommittee">BridgeCommittee</a>, blocklist: Blocklist) {
    <b>let</b> blocklisted = <a href="message.md#0xb_message_blocklist_type">message::blocklist_type</a>(&blocklist) != 1;
    <b>let</b> eth_addresses = <a href="message.md#0xb_message_blocklist_validator_addresses">message::blocklist_validator_addresses</a>(&blocklist);
    <b>let</b> list_len = <a href="dependencies/move-stdlib/vector.md#0x1_vector_length">vector::length</a>(eth_addresses);
    <b>let</b> list_idx = 0;
    <b>let</b> member_idx = 0;
    <b>let</b> pub_keys = <a href="dependencies/move-stdlib/vector.md#0x1_vector_empty">vector::empty</a>&lt;<a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;&gt;();
    <b>while</b> (list_idx &lt; list_len) {
        <b>let</b> target_address = <a href="dependencies/move-stdlib/vector.md#0x1_vector_borrow">vector::borrow</a>(eth_addresses, list_idx);
        <b>let</b> found = <b>false</b>;
        <b>while</b> (member_idx &lt; <a href="dependencies/sui-framework/vec_map.md#0x2_vec_map_size">vec_map::size</a>(&self.members)) {
            <b>let</b> (pub_key, member) = <a href="dependencies/sui-framework/vec_map.md#0x2_vec_map_get_entry_by_idx_mut">vec_map::get_entry_by_idx_mut</a>(&<b>mut</b> self.members, member_idx);
            <b>let</b> eth_address = <a href="crypto.md#0xb_crypto_ecdsa_pub_key_to_eth_address">crypto::ecdsa_pub_key_to_eth_address</a>(*pub_key);
            <b>if</b> (*target_address == eth_address) {
                member.blocklisted = blocklisted;
                <a href="dependencies/move-stdlib/vector.md#0x1_vector_push_back">vector::push_back</a>(&<b>mut</b> pub_keys, *pub_key);
                found = <b>true</b>;
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
