---
title: Module `sui::guardrails`
---

Guardrails let an account bound how much of each balance its key alone can move per epoch.
The rules of an account are its <code><a href="../sui/guardrails.md#sui_guardrails_Policy">Policy</a></code>, a shared object whose ID is derived from the owner
address. Execution takes the sender's policy as an implicit mutable input, checks what left the
sender's coins and address balance against the limits, and records the spend in the policy. A
transaction sponsored by the policy's guardian is exempt.


-  [Struct `GuardrailsRegistry`](#sui_guardrails_GuardrailsRegistry)
-  [Struct `PolicyKey`](#sui_guardrails_PolicyKey)
-  [Struct `Policy`](#sui_guardrails_Policy)
-  [Constants](#@Constants_0)
-  [Function `create`](#sui_guardrails_create)
-  [Function `enable`](#sui_guardrails_enable)
-  [Function `re_enable`](#sui_guardrails_re_enable)
-  [Function `cancel`](#sui_guardrails_cancel)
-  [Function `disable`](#sui_guardrails_disable)
-  [Function `set_limit`](#sui_guardrails_set_limit)
-  [Function `remove_limit`](#sui_guardrails_remove_limit)
-  [Function `set_guardian`](#sui_guardrails_set_guardian)
-  [Function `request_disable`](#sui_guardrails_request_disable)
-  [Function `cancel_disable`](#sui_guardrails_cancel_disable)
-  [Function `is_active`](#sui_guardrails_is_active)
-  [Function `policy_address`](#sui_guardrails_policy_address)
-  [Function `assert_authorized`](#sui_guardrails_assert_authorized)


<pre><code><b>use</b> <a href="../std/address.md#std_address">std::address</a>;
<b>use</b> <a href="../std/ascii.md#std_ascii">std::ascii</a>;
<b>use</b> <a href="../std/bcs.md#std_bcs">std::bcs</a>;
<b>use</b> <a href="../std/internal.md#std_internal">std::internal</a>;
<b>use</b> <a href="../std/option.md#std_option">std::option</a>;
<b>use</b> <a href="../std/string.md#std_string">std::string</a>;
<b>use</b> <a href="../std/type_name.md#std_type_name">std::type_name</a>;
<b>use</b> <a href="../std/u128.md#std_u128">std::u128</a>;
<b>use</b> <a href="../std/vector.md#std_vector">std::vector</a>;
<b>use</b> <a href="../sui/accumulator.md#sui_accumulator">sui::accumulator</a>;
<b>use</b> <a href="../sui/accumulator_settlement.md#sui_accumulator_settlement">sui::accumulator_settlement</a>;
<b>use</b> <a href="../sui/address.md#sui_address">sui::address</a>;
<b>use</b> <a href="../sui/bag.md#sui_bag">sui::bag</a>;
<b>use</b> <a href="../sui/balance.md#sui_balance">sui::balance</a>;
<b>use</b> <a href="../sui/bcs.md#sui_bcs">sui::bcs</a>;
<b>use</b> <a href="../sui/coin.md#sui_coin">sui::coin</a>;
<b>use</b> <a href="../sui/config.md#sui_config">sui::config</a>;
<b>use</b> <a href="../sui/deny_list.md#sui_deny_list">sui::deny_list</a>;
<b>use</b> <a href="../sui/derived_object.md#sui_derived_object">sui::derived_object</a>;
<b>use</b> <a href="../sui/dynamic_field.md#sui_dynamic_field">sui::dynamic_field</a>;
<b>use</b> <a href="../sui/dynamic_object_field.md#sui_dynamic_object_field">sui::dynamic_object_field</a>;
<b>use</b> <a href="../sui/event.md#sui_event">sui::event</a>;
<b>use</b> <a href="../sui/funds_accumulator.md#sui_funds_accumulator">sui::funds_accumulator</a>;
<b>use</b> <a href="../sui/hash.md#sui_hash">sui::hash</a>;
<b>use</b> <a href="../sui/hex.md#sui_hex">sui::hex</a>;
<b>use</b> <a href="../sui/object.md#sui_object">sui::object</a>;
<b>use</b> <a href="../sui/party.md#sui_party">sui::party</a>;
<b>use</b> <a href="../sui/protocol_config.md#sui_protocol_config">sui::protocol_config</a>;
<b>use</b> <a href="../sui/sui.md#sui_sui">sui::sui</a>;
<b>use</b> <a href="../sui/table.md#sui_table">sui::table</a>;
<b>use</b> <a href="../sui/transfer.md#sui_transfer">sui::transfer</a>;
<b>use</b> <a href="../sui/tx_context.md#sui_tx_context">sui::tx_context</a>;
<b>use</b> <a href="../sui/types.md#sui_types">sui::types</a>;
<b>use</b> <a href="../sui/url.md#sui_url">sui::url</a>;
<b>use</b> <a href="../sui/vec_map.md#sui_vec_map">sui::vec_map</a>;
<b>use</b> <a href="../sui/vec_set.md#sui_vec_set">sui::vec_set</a>;
</code></pre>



<a name="sui_guardrails_GuardrailsRegistry"></a>

## Struct `GuardrailsRegistry`

The registry is a system object created at genesis. It serves only as the parent that policy
IDs derive from.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/guardrails.md#sui_guardrails_GuardrailsRegistry">GuardrailsRegistry</a> <b>has</b> key
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>id: <a href="../sui/object.md#sui_object_UID">sui::object::UID</a></code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="sui_guardrails_PolicyKey"></a>

## Struct `PolicyKey`

Derivation key: the policy of <code>owner</code> has the ID derived from the registry and
<code><a href="../sui/guardrails.md#sui_guardrails_PolicyKey">PolicyKey</a>(owner)</code>.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/guardrails.md#sui_guardrails_PolicyKey">PolicyKey</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>0: <b>address</b></code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="sui_guardrails_Policy"></a>

## Struct `Policy`

Each account has at most one policy, which is shared. <code>sui_types::guardrails::Policy</code> mirrors
its field layout.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/guardrails.md#sui_guardrails_Policy">Policy</a> <b>has</b> key
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>id: <a href="../sui/object.md#sui_object_UID">sui::object::UID</a></code>
</dt>
<dd>
</dd>
<dt>
<code>owner: <b>address</b></code>
</dt>
<dd>
</dd>
<dt>
<code>guardian: <b>address</b></code>
</dt>
<dd>
</dd>
<dt>
<code>active_from: u64</code>
</dt>
<dd>
 First epoch in which the policy is enforced. <code><a href="../sui/guardrails.md#sui_guardrails_NEVER">NEVER</a></code> once cancelled.
</dd>
<dt>
<code>active_until: u64</code>
</dt>
<dd>
 First epoch in which the policy is no longer enforced. <code><a href="../sui/guardrails.md#sui_guardrails_NEVER">NEVER</a></code> while no end is set.
</dd>
<dt>
<code>limits: <a href="../sui/vec_map.md#sui_vec_map_VecMap">sui::vec_map::VecMap</a>&lt;<a href="../std/type_name.md#std_type_name_TypeName">std::type_name::TypeName</a>, u64&gt;</code>
</dt>
<dd>
 Most of each coin type that may leave the account per epoch. A type without an entry
 cannot leave at all.
</dd>
<dt>
<code>recovery_delay_epochs: u64</code>
</dt>
<dd>
 Epochs between <code><a href="../sui/guardrails.md#sui_guardrails_request_disable">request_disable</a></code> and the policy becoming inactive.
</dd>
<dt>
<code>spent_epoch: u64</code>
</dt>
<dd>
 The epoch <code>spent</code> belongs to. Written by execution, never by this module.
</dd>
<dt>
<code>spent: <a href="../sui/vec_map.md#sui_vec_map_VecMap">sui::vec_map::VecMap</a>&lt;<a href="../std/type_name.md#std_type_name_TypeName">std::type_name::TypeName</a>, u64&gt;</code>
</dt>
<dd>
 What left the account in <code>spent_epoch</code>, by coin type. Written by execution.
</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="sui_guardrails_ENotSystemAddress"></a>



<pre><code>#[error]
<b>const</b> <a href="../sui/guardrails.md#sui_guardrails_ENotSystemAddress">ENotSystemAddress</a>: vector&lt;u8&gt; = b"Only the system can <a href="../sui/guardrails.md#sui_guardrails_create">create</a> the <a href="../sui/guardrails.md#sui_guardrails">guardrails</a> registry.";
</code></pre>



<a name="sui_guardrails_ENotOwner"></a>



<pre><code>#[error]
<b>const</b> <a href="../sui/guardrails.md#sui_guardrails_ENotOwner">ENotOwner</a>: vector&lt;u8&gt; = b"Only the owner of a policy can change it.";
</code></pre>



<a name="sui_guardrails_ENotAuthorized"></a>



<pre><code>#[error]
<b>const</b> <a href="../sui/guardrails.md#sui_guardrails_ENotAuthorized">ENotAuthorized</a>: vector&lt;u8&gt; = b"Changing an active policy requires the guardian to sponsor the transaction.";
</code></pre>



<a name="sui_guardrails_EGuardianIsOwner"></a>



<pre><code>#[error]
<b>const</b> <a href="../sui/guardrails.md#sui_guardrails_EGuardianIsOwner">EGuardianIsOwner</a>: vector&lt;u8&gt; = b"The guardian must differ from the owner.";
</code></pre>



<a name="sui_guardrails_ERecoveryDelayOutOfRange"></a>



<pre><code>#[error]
<b>const</b> <a href="../sui/guardrails.md#sui_guardrails_ERecoveryDelayOutOfRange">ERecoveryDelayOutOfRange</a>: vector&lt;u8&gt; = b"The recovery delay is outside the allowed range.";
</code></pre>



<a name="sui_guardrails_ETooManyLimits"></a>



<pre><code>#[error]
<b>const</b> <a href="../sui/guardrails.md#sui_guardrails_ETooManyLimits">ETooManyLimits</a>: vector&lt;u8&gt; = b"The policy limits too many <a href="../sui/coin.md#sui_coin">coin</a> <a href="../sui/types.md#sui_types">types</a>.";
</code></pre>



<a name="sui_guardrails_EAlreadyActive"></a>



<pre><code>#[error]
<b>const</b> <a href="../sui/guardrails.md#sui_guardrails_EAlreadyActive">EAlreadyActive</a>: vector&lt;u8&gt; = b"The policy is pending or active.";
</code></pre>



<a name="sui_guardrails_ENotPending"></a>



<pre><code>#[error]
<b>const</b> <a href="../sui/guardrails.md#sui_guardrails_ENotPending">ENotPending</a>: vector&lt;u8&gt; = b"The policy is not pending.";
</code></pre>



<a name="sui_guardrails_ENotActive"></a>



<pre><code>#[error]
<b>const</b> <a href="../sui/guardrails.md#sui_guardrails_ENotActive">ENotActive</a>: vector&lt;u8&gt; = b"The policy is not active.";
</code></pre>



<a name="sui_guardrails_EDisablePending"></a>



<pre><code>#[error]
<b>const</b> <a href="../sui/guardrails.md#sui_guardrails_EDisablePending">EDisablePending</a>: vector&lt;u8&gt; = b"The policy is already set to become inactive.";
</code></pre>



<a name="sui_guardrails_ENoPendingDisable"></a>



<pre><code>#[error]
<b>const</b> <a href="../sui/guardrails.md#sui_guardrails_ENoPendingDisable">ENoPendingDisable</a>: vector&lt;u8&gt; = b"The policy is not set to become inactive.";
</code></pre>



<a name="sui_guardrails_ENotGuardian"></a>



<pre><code>#[error]
<b>const</b> <a href="../sui/guardrails.md#sui_guardrails_ENotGuardian">ENotGuardian</a>: vector&lt;u8&gt; = b"Only a transaction sponsored by the guardian can do this.";
</code></pre>



<a name="sui_guardrails_MAX_LIMITED_TYPES"></a>

A policy limits at most this many coin types. The bound keeps the object small, since every
transaction of the account rewrites it.


<pre><code><b>const</b> <a href="../sui/guardrails.md#sui_guardrails_MAX_LIMITED_TYPES">MAX_LIMITED_TYPES</a>: u64 = 32;
</code></pre>



<a name="sui_guardrails_MIN_RECOVERY_DELAY_EPOCHS"></a>

A policy cannot choose a shorter recovery delay than this.


<pre><code><b>const</b> <a href="../sui/guardrails.md#sui_guardrails_MIN_RECOVERY_DELAY_EPOCHS">MIN_RECOVERY_DELAY_EPOCHS</a>: u64 = 14;
</code></pre>



<a name="sui_guardrails_MAX_RECOVERY_DELAY_EPOCHS"></a>

A policy cannot choose a longer recovery delay than this. The bound keeps <code><a href="../sui/guardrails.md#sui_guardrails_request_disable">request_disable</a></code>
from overflowing.


<pre><code><b>const</b> <a href="../sui/guardrails.md#sui_guardrails_MAX_RECOVERY_DELAY_EPOCHS">MAX_RECOVERY_DELAY_EPOCHS</a>: u64 = 365;
</code></pre>



<a name="sui_guardrails_NEVER"></a>



<pre><code><b>const</b> <a href="../sui/guardrails.md#sui_guardrails_NEVER">NEVER</a>: u64 = 18446744073709551615;
</code></pre>



<a name="sui_guardrails_create"></a>

## Function `create`

Create and share the <code><a href="../sui/guardrails.md#sui_guardrails_GuardrailsRegistry">GuardrailsRegistry</a></code>. Called once, by genesis.


<pre><code><b>fun</b> <a href="../sui/guardrails.md#sui_guardrails_create">create</a>(ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/guardrails.md#sui_guardrails_create">create</a>(ctx: &TxContext) {
    <b>assert</b>!(ctx.sender() == @0x0, <a href="../sui/guardrails.md#sui_guardrails_ENotSystemAddress">ENotSystemAddress</a>);
    <a href="../sui/transfer.md#sui_transfer_share_object">transfer::share_object</a>(<a href="../sui/guardrails.md#sui_guardrails_GuardrailsRegistry">GuardrailsRegistry</a> { id: <a href="../sui/object.md#sui_object_guardrails_registry">object::guardrails_registry</a>() });
}
</code></pre>



</details>

<a name="sui_guardrails_enable"></a>

## Function `enable`

Creates the sender's policy. It is enforced from the next epoch on; until then the sender can
configure or <code><a href="../sui/guardrails.md#sui_guardrails_cancel">cancel</a></code> it with the key alone.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/guardrails.md#sui_guardrails_enable">enable</a>(registry: &<b>mut</b> <a href="../sui/guardrails.md#sui_guardrails_GuardrailsRegistry">sui::guardrails::GuardrailsRegistry</a>, guardian: <b>address</b>, sui_limit: u64, recovery_delay_epochs: u64, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/guardrails.md#sui_guardrails_enable">enable</a>(
    registry: &<b>mut</b> <a href="../sui/guardrails.md#sui_guardrails_GuardrailsRegistry">GuardrailsRegistry</a>,
    guardian: <b>address</b>,
    sui_limit: u64,
    recovery_delay_epochs: u64,
    ctx: &TxContext,
) {
    <b>let</b> owner = ctx.sender();
    <b>assert</b>!(guardian != owner, <a href="../sui/guardrails.md#sui_guardrails_EGuardianIsOwner">EGuardianIsOwner</a>);
    <b>assert</b>!(
        recovery_delay_epochs &gt;= <a href="../sui/guardrails.md#sui_guardrails_MIN_RECOVERY_DELAY_EPOCHS">MIN_RECOVERY_DELAY_EPOCHS</a> &&
        recovery_delay_epochs &lt;= <a href="../sui/guardrails.md#sui_guardrails_MAX_RECOVERY_DELAY_EPOCHS">MAX_RECOVERY_DELAY_EPOCHS</a>,
        <a href="../sui/guardrails.md#sui_guardrails_ERecoveryDelayOutOfRange">ERecoveryDelayOutOfRange</a>,
    );
    <b>let</b> <b>mut</b> limits = <a href="../sui/vec_map.md#sui_vec_map_empty">vec_map::empty</a>();
    limits.insert(type_name::with_defining_ids&lt;SUI&gt;(), sui_limit);
    <a href="../sui/transfer.md#sui_transfer_share_object">transfer::share_object</a>(<a href="../sui/guardrails.md#sui_guardrails_Policy">Policy</a> {
        id: <a href="../sui/derived_object.md#sui_derived_object_claim">derived_object::claim</a>(&<b>mut</b> registry.id, <a href="../sui/guardrails.md#sui_guardrails_PolicyKey">PolicyKey</a>(owner)),
        owner,
        guardian,
        active_from: ctx.epoch() + 1,
        active_until: <a href="../sui/guardrails.md#sui_guardrails_NEVER">NEVER</a>,
        limits,
        recovery_delay_epochs,
        spent_epoch: 0,
        spent: <a href="../sui/vec_map.md#sui_vec_map_empty">vec_map::empty</a>(),
    });
}
</code></pre>



</details>

<a name="sui_guardrails_re_enable"></a>

## Function `re_enable`

Enables a policy that was cancelled or has become inactive, keeping its limits.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/guardrails.md#sui_guardrails_re_enable">re_enable</a>(policy: &<b>mut</b> <a href="../sui/guardrails.md#sui_guardrails_Policy">sui::guardrails::Policy</a>, guardian: <b>address</b>, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/guardrails.md#sui_guardrails_re_enable">re_enable</a>(policy: &<b>mut</b> <a href="../sui/guardrails.md#sui_guardrails_Policy">Policy</a>, guardian: <b>address</b>, ctx: &TxContext) {
    <b>let</b> epoch = ctx.epoch();
    policy.<a href="../sui/guardrails.md#sui_guardrails_assert_authorized">assert_authorized</a>(ctx);
    <b>assert</b>!(guardian != policy.owner, <a href="../sui/guardrails.md#sui_guardrails_EGuardianIsOwner">EGuardianIsOwner</a>);
    <b>assert</b>!(policy.active_from == <a href="../sui/guardrails.md#sui_guardrails_NEVER">NEVER</a> || policy.active_until &lt;= epoch, <a href="../sui/guardrails.md#sui_guardrails_EAlreadyActive">EAlreadyActive</a>);
    policy.guardian = guardian;
    policy.active_from = epoch + 1;
    policy.active_until = <a href="../sui/guardrails.md#sui_guardrails_NEVER">NEVER</a>;
}
</code></pre>



</details>

<a name="sui_guardrails_cancel"></a>

## Function `cancel`

Cancels a policy before it becomes active.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/guardrails.md#sui_guardrails_cancel">cancel</a>(policy: &<b>mut</b> <a href="../sui/guardrails.md#sui_guardrails_Policy">sui::guardrails::Policy</a>, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/guardrails.md#sui_guardrails_cancel">cancel</a>(policy: &<b>mut</b> <a href="../sui/guardrails.md#sui_guardrails_Policy">Policy</a>, ctx: &TxContext) {
    policy.<a href="../sui/guardrails.md#sui_guardrails_assert_authorized">assert_authorized</a>(ctx);
    <b>assert</b>!(policy.active_from != <a href="../sui/guardrails.md#sui_guardrails_NEVER">NEVER</a> && policy.active_from &gt; ctx.epoch(), <a href="../sui/guardrails.md#sui_guardrails_ENotPending">ENotPending</a>);
    policy.active_from = <a href="../sui/guardrails.md#sui_guardrails_NEVER">NEVER</a>;
}
</code></pre>



</details>

<a name="sui_guardrails_disable"></a>

## Function `disable`

Ends an active policy at the next epoch. The guardian must sponsor the transaction.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/guardrails.md#sui_guardrails_disable">disable</a>(policy: &<b>mut</b> <a href="../sui/guardrails.md#sui_guardrails_Policy">sui::guardrails::Policy</a>, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/guardrails.md#sui_guardrails_disable">disable</a>(policy: &<b>mut</b> <a href="../sui/guardrails.md#sui_guardrails_Policy">Policy</a>, ctx: &TxContext) {
    policy.<a href="../sui/guardrails.md#sui_guardrails_assert_authorized">assert_authorized</a>(ctx);
    <b>assert</b>!(policy.<a href="../sui/guardrails.md#sui_guardrails_is_active">is_active</a>(ctx.epoch()), <a href="../sui/guardrails.md#sui_guardrails_ENotActive">ENotActive</a>);
    policy.active_until = ctx.epoch() + 1;
}
</code></pre>



</details>

<a name="sui_guardrails_set_limit"></a>

## Function `set_limit`

Sets the per-epoch limit of coin type <code>T</code>.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/guardrails.md#sui_guardrails_set_limit">set_limit</a>&lt;T&gt;(policy: &<b>mut</b> <a href="../sui/guardrails.md#sui_guardrails_Policy">sui::guardrails::Policy</a>, limit: u64, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/guardrails.md#sui_guardrails_set_limit">set_limit</a>&lt;T&gt;(policy: &<b>mut</b> <a href="../sui/guardrails.md#sui_guardrails_Policy">Policy</a>, limit: u64, ctx: &TxContext) {
    policy.<a href="../sui/guardrails.md#sui_guardrails_assert_authorized">assert_authorized</a>(ctx);
    <b>let</b> coin_type = type_name::with_defining_ids&lt;T&gt;();
    <b>if</b> (policy.limits.contains(&coin_type)) {
        *policy.limits.get_mut(&coin_type) = limit;
    } <b>else</b> {
        <b>assert</b>!(policy.limits.length() &lt; <a href="../sui/guardrails.md#sui_guardrails_MAX_LIMITED_TYPES">MAX_LIMITED_TYPES</a>, <a href="../sui/guardrails.md#sui_guardrails_ETooManyLimits">ETooManyLimits</a>);
        policy.limits.insert(coin_type, limit);
    }
}
</code></pre>



</details>

<a name="sui_guardrails_remove_limit"></a>

## Function `remove_limit`

Removes the limit of coin type <code>T</code>, after which <code>T</code> cannot leave the account at all.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/guardrails.md#sui_guardrails_remove_limit">remove_limit</a>&lt;T&gt;(policy: &<b>mut</b> <a href="../sui/guardrails.md#sui_guardrails_Policy">sui::guardrails::Policy</a>, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/guardrails.md#sui_guardrails_remove_limit">remove_limit</a>&lt;T&gt;(policy: &<b>mut</b> <a href="../sui/guardrails.md#sui_guardrails_Policy">Policy</a>, ctx: &TxContext) {
    policy.<a href="../sui/guardrails.md#sui_guardrails_assert_authorized">assert_authorized</a>(ctx);
    <b>let</b> (_, _) = policy.limits.remove(&type_name::with_defining_ids&lt;T&gt;());
}
</code></pre>



</details>

<a name="sui_guardrails_set_guardian"></a>

## Function `set_guardian`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/guardrails.md#sui_guardrails_set_guardian">set_guardian</a>(policy: &<b>mut</b> <a href="../sui/guardrails.md#sui_guardrails_Policy">sui::guardrails::Policy</a>, guardian: <b>address</b>, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/guardrails.md#sui_guardrails_set_guardian">set_guardian</a>(policy: &<b>mut</b> <a href="../sui/guardrails.md#sui_guardrails_Policy">Policy</a>, guardian: <b>address</b>, ctx: &TxContext) {
    policy.<a href="../sui/guardrails.md#sui_guardrails_assert_authorized">assert_authorized</a>(ctx);
    <b>assert</b>!(guardian != policy.owner, <a href="../sui/guardrails.md#sui_guardrails_EGuardianIsOwner">EGuardianIsOwner</a>);
    policy.guardian = guardian;
}
</code></pre>



</details>

<a name="sui_guardrails_request_disable"></a>

## Function `request_disable`

Ends an active policy after its recovery delay, with the key alone. This is the way out for
an owner who lost the guardian. Only the guardian can undo it, so a stolen key that requests
it cannot also hold it open, and cannot push an owner's own request further out.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/guardrails.md#sui_guardrails_request_disable">request_disable</a>(policy: &<b>mut</b> <a href="../sui/guardrails.md#sui_guardrails_Policy">sui::guardrails::Policy</a>, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/guardrails.md#sui_guardrails_request_disable">request_disable</a>(policy: &<b>mut</b> <a href="../sui/guardrails.md#sui_guardrails_Policy">Policy</a>, ctx: &TxContext) {
    <b>assert</b>!(policy.owner == ctx.sender(), <a href="../sui/guardrails.md#sui_guardrails_ENotOwner">ENotOwner</a>);
    <b>assert</b>!(policy.<a href="../sui/guardrails.md#sui_guardrails_is_active">is_active</a>(ctx.epoch()), <a href="../sui/guardrails.md#sui_guardrails_ENotActive">ENotActive</a>);
    <b>assert</b>!(policy.active_until == <a href="../sui/guardrails.md#sui_guardrails_NEVER">NEVER</a>, <a href="../sui/guardrails.md#sui_guardrails_EDisablePending">EDisablePending</a>);
    policy.active_until = ctx.epoch() + policy.recovery_delay_epochs;
}
</code></pre>



</details>

<a name="sui_guardrails_cancel_disable"></a>

## Function `cancel_disable`

Keeps an active policy that <code><a href="../sui/guardrails.md#sui_guardrails_disable">disable</a></code> or <code><a href="../sui/guardrails.md#sui_guardrails_request_disable">request_disable</a></code> set to end. The guardian must
sponsor the transaction.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/guardrails.md#sui_guardrails_cancel_disable">cancel_disable</a>(policy: &<b>mut</b> <a href="../sui/guardrails.md#sui_guardrails_Policy">sui::guardrails::Policy</a>, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/guardrails.md#sui_guardrails_cancel_disable">cancel_disable</a>(policy: &<b>mut</b> <a href="../sui/guardrails.md#sui_guardrails_Policy">Policy</a>, ctx: &TxContext) {
    <b>assert</b>!(policy.owner == ctx.sender(), <a href="../sui/guardrails.md#sui_guardrails_ENotOwner">ENotOwner</a>);
    <b>assert</b>!(ctx.sponsor() == option::some(policy.guardian), <a href="../sui/guardrails.md#sui_guardrails_ENotGuardian">ENotGuardian</a>);
    <b>assert</b>!(policy.active_until != <a href="../sui/guardrails.md#sui_guardrails_NEVER">NEVER</a> && policy.active_until &gt; ctx.epoch(), <a href="../sui/guardrails.md#sui_guardrails_ENoPendingDisable">ENoPendingDisable</a>);
    policy.active_until = <a href="../sui/guardrails.md#sui_guardrails_NEVER">NEVER</a>;
}
</code></pre>



</details>

<a name="sui_guardrails_is_active"></a>

## Function `is_active`



<pre><code><b>fun</b> <a href="../sui/guardrails.md#sui_guardrails_is_active">is_active</a>(policy: &<a href="../sui/guardrails.md#sui_guardrails_Policy">sui::guardrails::Policy</a>, epoch: u64): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/guardrails.md#sui_guardrails_is_active">is_active</a>(policy: &<a href="../sui/guardrails.md#sui_guardrails_Policy">Policy</a>, epoch: u64): bool {
    policy.active_from &lt;= epoch && epoch &lt; policy.active_until
}
</code></pre>



</details>

<a name="sui_guardrails_policy_address"></a>

## Function `policy_address`

Returns the policy ID of <code>owner</code>, whether or not the policy exists.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/guardrails.md#sui_guardrails_policy_address">policy_address</a>(registry: &<a href="../sui/guardrails.md#sui_guardrails_GuardrailsRegistry">sui::guardrails::GuardrailsRegistry</a>, owner: <b>address</b>): <b>address</b>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/guardrails.md#sui_guardrails_policy_address">policy_address</a>(registry: &<a href="../sui/guardrails.md#sui_guardrails_GuardrailsRegistry">GuardrailsRegistry</a>, owner: <b>address</b>): <b>address</b> {
    <a href="../sui/derived_object.md#sui_derived_object_derive_address">derived_object::derive_address</a>(<a href="../sui/object.md#sui_object_id">object::id</a>(registry), <a href="../sui/guardrails.md#sui_guardrails_PolicyKey">PolicyKey</a>(owner))
}
</code></pre>



</details>

<a name="sui_guardrails_assert_authorized"></a>

## Function `assert_authorized`

Every change comes from the owner, and a change to an active policy needs the guardian as the
transaction's sponsor.


<pre><code><b>fun</b> <a href="../sui/guardrails.md#sui_guardrails_assert_authorized">assert_authorized</a>(policy: &<a href="../sui/guardrails.md#sui_guardrails_Policy">sui::guardrails::Policy</a>, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/guardrails.md#sui_guardrails_assert_authorized">assert_authorized</a>(policy: &<a href="../sui/guardrails.md#sui_guardrails_Policy">Policy</a>, ctx: &TxContext) {
    <b>assert</b>!(policy.owner == ctx.sender(), <a href="../sui/guardrails.md#sui_guardrails_ENotOwner">ENotOwner</a>);
    <b>assert</b>!(
        !policy.<a href="../sui/guardrails.md#sui_guardrails_is_active">is_active</a>(ctx.epoch()) || ctx.sponsor() == option::some(policy.guardian),
        <a href="../sui/guardrails.md#sui_guardrails_ENotAuthorized">ENotAuthorized</a>,
    );
}
</code></pre>



</details>
