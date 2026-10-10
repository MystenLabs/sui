---
title: Module `sui::account_policy`
---

Opt-in account policies that bound what a transaction signed by the account key alone may do.

Policies are dynamic fields of the singleton <code><a href="../sui/account_policy.md#sui_account_policy_AccountPolicyRegistry">AccountPolicyRegistry</a></code>, keyed by owner address.
Every transaction implicitly reads the registry at the version consensus assigned to it, so
execution can look up the sender's policy without the transaction declaring it. While a policy
is active, execution enforces a gas budget cap, a per-transaction outflow limit for each coin
type, a package allowlist, and that the owner's objects stay with the owner unless they go to a
listed recipient or are taken by a package with custody permission. A transaction co-signed by
the guardian is exempt.

A policy only takes effect <code><a href="../sui/account_policy.md#sui_account_policy_ACTIVATION_DELAY_EPOCHS">ACTIVATION_DELAY_EPOCHS</a></code> after it is enabled. Until then the owner
can change or cancel it with the key alone, so an attacker holding the key cannot lock the
owner out by enabling a policy with their own guardian. Once active, every change needs the
guardian's co-signature.


-  [Struct `AccountPolicyRegistry`](#sui_account_policy_AccountPolicyRegistry)
-  [Struct `PolicyKey`](#sui_account_policy_PolicyKey)
-  [Struct `PackagePermission`](#sui_account_policy_PackagePermission)
-  [Struct `AccountPolicy`](#sui_account_policy_AccountPolicy)
-  [Constants](#@Constants_0)
-  [Function `create`](#sui_account_policy_create)
-  [Function `enable`](#sui_account_policy_enable)
-  [Function `cancel`](#sui_account_policy_cancel)
-  [Function `disable`](#sui_account_policy_disable)
-  [Function `set_guardian`](#sui_account_policy_set_guardian)
-  [Function `set_gas_budget_cap`](#sui_account_policy_set_gas_budget_cap)
-  [Function `set_coin_limit`](#sui_account_policy_set_coin_limit)
-  [Function `add_recipient`](#sui_account_policy_add_recipient)
-  [Function `remove_recipient`](#sui_account_policy_remove_recipient)
-  [Function `set_package`](#sui_account_policy_set_package)
-  [Function `remove_package`](#sui_account_policy_remove_package)
-  [Function `exists`](#sui_account_policy_exists)
-  [Function `activation_epoch`](#sui_account_policy_activation_epoch)
-  [Function `policy_mut`](#sui_account_policy_policy_mut)
-  [Function `authorize`](#sui_account_policy_authorize)


<pre><code><b>use</b> <a href="../std/ascii.md#std_ascii">std::ascii</a>;
<b>use</b> <a href="../std/bcs.md#std_bcs">std::bcs</a>;
<b>use</b> <a href="../std/option.md#std_option">std::option</a>;
<b>use</b> <a href="../std/string.md#std_string">std::string</a>;
<b>use</b> <a href="../std/vector.md#std_vector">std::vector</a>;
<b>use</b> <a href="../sui/address.md#sui_address">sui::address</a>;
<b>use</b> <a href="../sui/dynamic_field.md#sui_dynamic_field">sui::dynamic_field</a>;
<b>use</b> <a href="../sui/hex.md#sui_hex">sui::hex</a>;
<b>use</b> <a href="../sui/object.md#sui_object">sui::object</a>;
<b>use</b> <a href="../sui/party.md#sui_party">sui::party</a>;
<b>use</b> <a href="../sui/transfer.md#sui_transfer">sui::transfer</a>;
<b>use</b> <a href="../sui/tx_context.md#sui_tx_context">sui::tx_context</a>;
<b>use</b> <a href="../sui/vec_map.md#sui_vec_map">sui::vec_map</a>;
<b>use</b> <a href="../sui/vec_set.md#sui_vec_set">sui::vec_set</a>;
</code></pre>



<a name="sui_account_policy_AccountPolicyRegistry"></a>

## Struct `AccountPolicyRegistry`

Singleton shared object holding every account policy as a dynamic field.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/account_policy.md#sui_account_policy_AccountPolicyRegistry">AccountPolicyRegistry</a> <b>has</b> key
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

<a name="sui_account_policy_PolicyKey"></a>

## Struct `PolicyKey`

Dynamic field key of an owner's policy.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/account_policy.md#sui_account_policy_PolicyKey">PolicyKey</a> <b>has</b> <b>copy</b>, drop, store
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

<a name="sui_account_policy_PackagePermission"></a>

## Struct `PackagePermission`

What a listed package may do with the owner's objects. Field layout is mirrored by
<code>sui_types::account_policy::PackagePermission</code>.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/account_policy.md#sui_account_policy_PackagePermission">PackagePermission</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>custody: bool</code>
</dt>
<dd>
 The package may delete, wrap, or give away the owner's objects.
</dd>
<dt>
<code>custody_types: vector&lt;<a href="../std/ascii.md#std_ascii_String">std::ascii::String</a>&gt;</code>
</dt>
<dd>
 Object types custody is limited to, as type strings; empty means any type.
</dd>
</dl>


</details>

<a name="sui_account_policy_AccountPolicy"></a>

## Struct `AccountPolicy`

The policy itself. Field layout is mirrored by <code>sui_types::account_policy::AccountPolicy</code>.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/account_policy.md#sui_account_policy_AccountPolicy">AccountPolicy</a> <b>has</b> store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
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
<code>gas_budget_cap: u64</code>
</dt>
<dd>
</dd>
<dt>
<code><a href="../sui/account_policy.md#sui_account_policy_activation_epoch">activation_epoch</a>: u64</code>
</dt>
<dd>
 First epoch in which the policy is enforced; <code><a href="../sui/account_policy.md#sui_account_policy_DISABLED">DISABLED</a></code> if cancelled or disabled.
</dd>
<dt>
<code>coin_limits: <a href="../sui/vec_map.md#sui_vec_map_VecMap">sui::vec_map::VecMap</a>&lt;<a href="../std/ascii.md#std_ascii_String">std::ascii::String</a>, u64&gt;</code>
</dt>
<dd>
 Per-transaction net outflow limit (in the coin's smallest unit) by coin type string.
 Types without an entry may not flow out at all.
</dd>
<dt>
<code>recipients: <a href="../sui/vec_set.md#sui_vec_set_VecSet">sui::vec_set::VecSet</a>&lt;<b>address</b>&gt;</code>
</dt>
<dd>
 Addresses (or object IDs) that coins and objects may be sent to without limit.
</dd>
<dt>
<code>packages: <a href="../sui/vec_map.md#sui_vec_map_VecMap">sui::vec_map::VecMap</a>&lt;<a href="../sui/object.md#sui_object_ID">sui::object::ID</a>, <a href="../sui/account_policy.md#sui_account_policy_PackagePermission">sui::account_policy::PackagePermission</a>&gt;</code>
</dt>
<dd>
 Packages that may be called, by original package ID. The system package is always
 callable.
</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="sui_account_policy_ACTIVATION_DELAY_EPOCHS"></a>



<pre><code><b>const</b> <a href="../sui/account_policy.md#sui_account_policy_ACTIVATION_DELAY_EPOCHS">ACTIVATION_DELAY_EPOCHS</a>: u64 = 1;
</code></pre>



<a name="sui_account_policy_DISABLED"></a>

<code><a href="../sui/account_policy.md#sui_account_policy_activation_epoch">activation_epoch</a></code> of a cancelled or disabled policy.


<pre><code><b>const</b> <a href="../sui/account_policy.md#sui_account_policy_DISABLED">DISABLED</a>: u64 = 18446744073709551615;
</code></pre>



<a name="sui_account_policy_ENotSystemAddress"></a>



<pre><code>#[error]
<b>const</b> <a href="../sui/account_policy.md#sui_account_policy_ENotSystemAddress">ENotSystemAddress</a>: vector&lt;u8&gt; = b"Only the system can <a href="../sui/account_policy.md#sui_account_policy_create">create</a> the account policy registry.";
</code></pre>



<a name="sui_account_policy_EAlreadyActive"></a>



<pre><code>#[error]
<b>const</b> <a href="../sui/account_policy.md#sui_account_policy_EAlreadyActive">EAlreadyActive</a>: vector&lt;u8&gt; = b"Only a pending policy can be cancelled with the key alone.";
</code></pre>



<a name="sui_account_policy_ENotCoSignedByGuardian"></a>



<pre><code>#[error]
<b>const</b> <a href="../sui/account_policy.md#sui_account_policy_ENotCoSignedByGuardian">ENotCoSignedByGuardian</a>: vector&lt;u8&gt; = b"The guardian must co-sign this transaction.";
</code></pre>



<a name="sui_account_policy_create"></a>

## Function `create`

Create and share the <code><a href="../sui/account_policy.md#sui_account_policy_AccountPolicyRegistry">AccountPolicyRegistry</a></code>. Called exactly once, by genesis.


<pre><code><b>fun</b> <a href="../sui/account_policy.md#sui_account_policy_create">create</a>(ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/account_policy.md#sui_account_policy_create">create</a>(ctx: &TxContext) {
    <b>assert</b>!(ctx.sender() == @0x0, <a href="../sui/account_policy.md#sui_account_policy_ENotSystemAddress">ENotSystemAddress</a>);
    <a href="../sui/transfer.md#sui_transfer_share_object">transfer::share_object</a>(<a href="../sui/account_policy.md#sui_account_policy_AccountPolicyRegistry">AccountPolicyRegistry</a> { id: <a href="../sui/object.md#sui_object_account_policy_registry">object::account_policy_registry</a>() });
}
</code></pre>



</details>

<a name="sui_account_policy_enable"></a>

## Function `enable`

Opt the sender in with an empty rule set. The policy is enforced from
<code><a href="../sui/account_policy.md#sui_account_policy_ACTIVATION_DELAY_EPOCHS">ACTIVATION_DELAY_EPOCHS</a></code> epochs from now; configure it before then with the <code>set_*</code> calls.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/account_policy.md#sui_account_policy_enable">enable</a>(registry: &<b>mut</b> <a href="../sui/account_policy.md#sui_account_policy_AccountPolicyRegistry">sui::account_policy::AccountPolicyRegistry</a>, guardian: <b>address</b>, gas_budget_cap: u64, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/account_policy.md#sui_account_policy_enable">enable</a>(
    registry: &<b>mut</b> <a href="../sui/account_policy.md#sui_account_policy_AccountPolicyRegistry">AccountPolicyRegistry</a>,
    guardian: <b>address</b>,
    gas_budget_cap: u64,
    ctx: &TxContext,
) {
    <b>let</b> owner = ctx.sender();
    df::add(
        &<b>mut</b> registry.id,
        <a href="../sui/account_policy.md#sui_account_policy_PolicyKey">PolicyKey</a>(owner),
        <a href="../sui/account_policy.md#sui_account_policy_AccountPolicy">AccountPolicy</a> {
            owner,
            guardian,
            gas_budget_cap,
            <a href="../sui/account_policy.md#sui_account_policy_activation_epoch">activation_epoch</a>: ctx.epoch() + <a href="../sui/account_policy.md#sui_account_policy_ACTIVATION_DELAY_EPOCHS">ACTIVATION_DELAY_EPOCHS</a>,
            coin_limits: <a href="../sui/vec_map.md#sui_vec_map_empty">vec_map::empty</a>(),
            recipients: <a href="../sui/vec_set.md#sui_vec_set_empty">vec_set::empty</a>(),
            packages: <a href="../sui/vec_map.md#sui_vec_map_empty">vec_map::empty</a>(),
        },
    );
}
</code></pre>



</details>

<a name="sui_account_policy_cancel"></a>

## Function `cancel`

Cancel the sender's policy before it becomes active. Needs only the owner's key, so an
attacker who enabled a policy on a stolen key cannot lock the owner out.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/account_policy.md#sui_account_policy_cancel">cancel</a>(registry: &<b>mut</b> <a href="../sui/account_policy.md#sui_account_policy_AccountPolicyRegistry">sui::account_policy::AccountPolicyRegistry</a>, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/account_policy.md#sui_account_policy_cancel">cancel</a>(registry: &<b>mut</b> <a href="../sui/account_policy.md#sui_account_policy_AccountPolicyRegistry">AccountPolicyRegistry</a>, ctx: &TxContext) {
    <b>let</b> policy = registry.<a href="../sui/account_policy.md#sui_account_policy_policy_mut">policy_mut</a>(ctx.sender());
    <b>assert</b>!(ctx.epoch() &lt; policy.<a href="../sui/account_policy.md#sui_account_policy_activation_epoch">activation_epoch</a>, <a href="../sui/account_policy.md#sui_account_policy_EAlreadyActive">EAlreadyActive</a>);
    policy.<a href="../sui/account_policy.md#sui_account_policy_activation_epoch">activation_epoch</a> = <a href="../sui/account_policy.md#sui_account_policy_DISABLED">DISABLED</a>;
}
</code></pre>



</details>

<a name="sui_account_policy_disable"></a>

## Function `disable`

Stop enforcing the sender's active policy. The guardian must co-sign.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/account_policy.md#sui_account_policy_disable">disable</a>(registry: &<b>mut</b> <a href="../sui/account_policy.md#sui_account_policy_AccountPolicyRegistry">sui::account_policy::AccountPolicyRegistry</a>, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/account_policy.md#sui_account_policy_disable">disable</a>(registry: &<b>mut</b> <a href="../sui/account_policy.md#sui_account_policy_AccountPolicyRegistry">AccountPolicyRegistry</a>, ctx: &TxContext) {
    <b>let</b> policy = registry.<a href="../sui/account_policy.md#sui_account_policy_policy_mut">policy_mut</a>(ctx.sender());
    policy.<a href="../sui/account_policy.md#sui_account_policy_authorize">authorize</a>(ctx);
    policy.<a href="../sui/account_policy.md#sui_account_policy_activation_epoch">activation_epoch</a> = <a href="../sui/account_policy.md#sui_account_policy_DISABLED">DISABLED</a>;
}
</code></pre>



</details>

<a name="sui_account_policy_set_guardian"></a>

## Function `set_guardian`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/account_policy.md#sui_account_policy_set_guardian">set_guardian</a>(registry: &<b>mut</b> <a href="../sui/account_policy.md#sui_account_policy_AccountPolicyRegistry">sui::account_policy::AccountPolicyRegistry</a>, guardian: <b>address</b>, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/account_policy.md#sui_account_policy_set_guardian">set_guardian</a>(registry: &<b>mut</b> <a href="../sui/account_policy.md#sui_account_policy_AccountPolicyRegistry">AccountPolicyRegistry</a>, guardian: <b>address</b>, ctx: &TxContext) {
    <b>let</b> policy = registry.<a href="../sui/account_policy.md#sui_account_policy_policy_mut">policy_mut</a>(ctx.sender());
    policy.<a href="../sui/account_policy.md#sui_account_policy_authorize">authorize</a>(ctx);
    policy.guardian = guardian;
}
</code></pre>



</details>

<a name="sui_account_policy_set_gas_budget_cap"></a>

## Function `set_gas_budget_cap`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/account_policy.md#sui_account_policy_set_gas_budget_cap">set_gas_budget_cap</a>(registry: &<b>mut</b> <a href="../sui/account_policy.md#sui_account_policy_AccountPolicyRegistry">sui::account_policy::AccountPolicyRegistry</a>, gas_budget_cap: u64, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/account_policy.md#sui_account_policy_set_gas_budget_cap">set_gas_budget_cap</a>(
    registry: &<b>mut</b> <a href="../sui/account_policy.md#sui_account_policy_AccountPolicyRegistry">AccountPolicyRegistry</a>,
    gas_budget_cap: u64,
    ctx: &TxContext,
) {
    <b>let</b> policy = registry.<a href="../sui/account_policy.md#sui_account_policy_policy_mut">policy_mut</a>(ctx.sender());
    policy.<a href="../sui/account_policy.md#sui_account_policy_authorize">authorize</a>(ctx);
    policy.gas_budget_cap = gas_budget_cap;
}
</code></pre>



</details>

<a name="sui_account_policy_set_coin_limit"></a>

## Function `set_coin_limit`

Set the per-transaction outflow limit of <code>coin_type</code> (e.g. <code><a href="../sui/sui.md#sui_sui_SUI">0x2::sui::SUI</a></code>).


<pre><code><b>public</b> <b>fun</b> <a href="../sui/account_policy.md#sui_account_policy_set_coin_limit">set_coin_limit</a>(registry: &<b>mut</b> <a href="../sui/account_policy.md#sui_account_policy_AccountPolicyRegistry">sui::account_policy::AccountPolicyRegistry</a>, coin_type: <a href="../std/ascii.md#std_ascii_String">std::ascii::String</a>, limit: u64, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/account_policy.md#sui_account_policy_set_coin_limit">set_coin_limit</a>(
    registry: &<b>mut</b> <a href="../sui/account_policy.md#sui_account_policy_AccountPolicyRegistry">AccountPolicyRegistry</a>,
    coin_type: String,
    limit: u64,
    ctx: &TxContext,
) {
    <b>let</b> policy = registry.<a href="../sui/account_policy.md#sui_account_policy_policy_mut">policy_mut</a>(ctx.sender());
    policy.<a href="../sui/account_policy.md#sui_account_policy_authorize">authorize</a>(ctx);
    <b>if</b> (policy.coin_limits.contains(&coin_type)) {
        *policy.coin_limits.get_mut(&coin_type) = limit;
    } <b>else</b> {
        policy.coin_limits.insert(coin_type, limit);
    }
}
</code></pre>



</details>

<a name="sui_account_policy_add_recipient"></a>

## Function `add_recipient`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/account_policy.md#sui_account_policy_add_recipient">add_recipient</a>(registry: &<b>mut</b> <a href="../sui/account_policy.md#sui_account_policy_AccountPolicyRegistry">sui::account_policy::AccountPolicyRegistry</a>, recipient: <b>address</b>, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/account_policy.md#sui_account_policy_add_recipient">add_recipient</a>(registry: &<b>mut</b> <a href="../sui/account_policy.md#sui_account_policy_AccountPolicyRegistry">AccountPolicyRegistry</a>, recipient: <b>address</b>, ctx: &TxContext) {
    <b>let</b> policy = registry.<a href="../sui/account_policy.md#sui_account_policy_policy_mut">policy_mut</a>(ctx.sender());
    policy.<a href="../sui/account_policy.md#sui_account_policy_authorize">authorize</a>(ctx);
    <b>if</b> (!policy.recipients.contains(&recipient)) {
        policy.recipients.insert(recipient);
    };
}
</code></pre>



</details>

<a name="sui_account_policy_remove_recipient"></a>

## Function `remove_recipient`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/account_policy.md#sui_account_policy_remove_recipient">remove_recipient</a>(registry: &<b>mut</b> <a href="../sui/account_policy.md#sui_account_policy_AccountPolicyRegistry">sui::account_policy::AccountPolicyRegistry</a>, recipient: <b>address</b>, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/account_policy.md#sui_account_policy_remove_recipient">remove_recipient</a>(
    registry: &<b>mut</b> <a href="../sui/account_policy.md#sui_account_policy_AccountPolicyRegistry">AccountPolicyRegistry</a>,
    recipient: <b>address</b>,
    ctx: &TxContext,
) {
    <b>let</b> policy = registry.<a href="../sui/account_policy.md#sui_account_policy_policy_mut">policy_mut</a>(ctx.sender());
    policy.<a href="../sui/account_policy.md#sui_account_policy_authorize">authorize</a>(ctx);
    <b>if</b> (policy.recipients.contains(&recipient)) {
        policy.recipients.remove(&recipient);
    };
}
</code></pre>



</details>

<a name="sui_account_policy_set_package"></a>

## Function `set_package`

Allow calling <code><a href="../sui/package.md#sui_package">package</a></code> (its original ID), optionally with custody of the owner's objects.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/account_policy.md#sui_account_policy_set_package">set_package</a>(registry: &<b>mut</b> <a href="../sui/account_policy.md#sui_account_policy_AccountPolicyRegistry">sui::account_policy::AccountPolicyRegistry</a>, <a href="../sui/package.md#sui_package">package</a>: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a>, custody: bool, custody_types: vector&lt;<a href="../std/ascii.md#std_ascii_String">std::ascii::String</a>&gt;, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/account_policy.md#sui_account_policy_set_package">set_package</a>(
    registry: &<b>mut</b> <a href="../sui/account_policy.md#sui_account_policy_AccountPolicyRegistry">AccountPolicyRegistry</a>,
    <a href="../sui/package.md#sui_package">package</a>: ID,
    custody: bool,
    custody_types: vector&lt;String&gt;,
    ctx: &TxContext,
) {
    <b>let</b> policy = registry.<a href="../sui/account_policy.md#sui_account_policy_policy_mut">policy_mut</a>(ctx.sender());
    policy.<a href="../sui/account_policy.md#sui_account_policy_authorize">authorize</a>(ctx);
    <b>let</b> permission = <a href="../sui/account_policy.md#sui_account_policy_PackagePermission">PackagePermission</a> { custody, custody_types };
    <b>if</b> (policy.packages.contains(&<a href="../sui/package.md#sui_package">package</a>)) {
        *policy.packages.get_mut(&<a href="../sui/package.md#sui_package">package</a>) = permission;
    } <b>else</b> {
        policy.packages.insert(<a href="../sui/package.md#sui_package">package</a>, permission);
    }
}
</code></pre>



</details>

<a name="sui_account_policy_remove_package"></a>

## Function `remove_package`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/account_policy.md#sui_account_policy_remove_package">remove_package</a>(registry: &<b>mut</b> <a href="../sui/account_policy.md#sui_account_policy_AccountPolicyRegistry">sui::account_policy::AccountPolicyRegistry</a>, <a href="../sui/package.md#sui_package">package</a>: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a>, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/account_policy.md#sui_account_policy_remove_package">remove_package</a>(registry: &<b>mut</b> <a href="../sui/account_policy.md#sui_account_policy_AccountPolicyRegistry">AccountPolicyRegistry</a>, <a href="../sui/package.md#sui_package">package</a>: ID, ctx: &TxContext) {
    <b>let</b> policy = registry.<a href="../sui/account_policy.md#sui_account_policy_policy_mut">policy_mut</a>(ctx.sender());
    policy.<a href="../sui/account_policy.md#sui_account_policy_authorize">authorize</a>(ctx);
    <b>if</b> (policy.packages.contains(&<a href="../sui/package.md#sui_package">package</a>)) {
        policy.packages.remove(&<a href="../sui/package.md#sui_package">package</a>);
    };
}
</code></pre>



</details>

<a name="sui_account_policy_exists"></a>

## Function `exists`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/account_policy.md#sui_account_policy_exists">exists</a>(registry: &<a href="../sui/account_policy.md#sui_account_policy_AccountPolicyRegistry">sui::account_policy::AccountPolicyRegistry</a>, owner: <b>address</b>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/account_policy.md#sui_account_policy_exists">exists</a>(registry: &<a href="../sui/account_policy.md#sui_account_policy_AccountPolicyRegistry">AccountPolicyRegistry</a>, owner: <b>address</b>): bool {
    df::exists(&registry.id, <a href="../sui/account_policy.md#sui_account_policy_PolicyKey">PolicyKey</a>(owner))
}
</code></pre>



</details>

<a name="sui_account_policy_activation_epoch"></a>

## Function `activation_epoch`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/account_policy.md#sui_account_policy_activation_epoch">activation_epoch</a>(registry: &<a href="../sui/account_policy.md#sui_account_policy_AccountPolicyRegistry">sui::account_policy::AccountPolicyRegistry</a>, owner: <b>address</b>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/account_policy.md#sui_account_policy_activation_epoch">activation_epoch</a>(registry: &<a href="../sui/account_policy.md#sui_account_policy_AccountPolicyRegistry">AccountPolicyRegistry</a>, owner: <b>address</b>): u64 {
    df::borrow&lt;<a href="../sui/account_policy.md#sui_account_policy_PolicyKey">PolicyKey</a>, <a href="../sui/account_policy.md#sui_account_policy_AccountPolicy">AccountPolicy</a>&gt;(&registry.id, <a href="../sui/account_policy.md#sui_account_policy_PolicyKey">PolicyKey</a>(owner)).<a href="../sui/account_policy.md#sui_account_policy_activation_epoch">activation_epoch</a>
}
</code></pre>



</details>

<a name="sui_account_policy_policy_mut"></a>

## Function `policy_mut`



<pre><code><b>fun</b> <a href="../sui/account_policy.md#sui_account_policy_policy_mut">policy_mut</a>(registry: &<b>mut</b> <a href="../sui/account_policy.md#sui_account_policy_AccountPolicyRegistry">sui::account_policy::AccountPolicyRegistry</a>, owner: <b>address</b>): &<b>mut</b> <a href="../sui/account_policy.md#sui_account_policy_AccountPolicy">sui::account_policy::AccountPolicy</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/account_policy.md#sui_account_policy_policy_mut">policy_mut</a>(registry: &<b>mut</b> <a href="../sui/account_policy.md#sui_account_policy_AccountPolicyRegistry">AccountPolicyRegistry</a>, owner: <b>address</b>): &<b>mut</b> <a href="../sui/account_policy.md#sui_account_policy_AccountPolicy">AccountPolicy</a> {
    df::borrow_mut(&<b>mut</b> registry.id, <a href="../sui/account_policy.md#sui_account_policy_PolicyKey">PolicyKey</a>(owner))
}
</code></pre>



</details>

<a name="sui_account_policy_authorize"></a>

## Function `authorize`

A pending policy is the owner's to shape; an active one changes only with the guardian.


<pre><code><b>fun</b> <a href="../sui/account_policy.md#sui_account_policy_authorize">authorize</a>(policy: &<a href="../sui/account_policy.md#sui_account_policy_AccountPolicy">sui::account_policy::AccountPolicy</a>, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/account_policy.md#sui_account_policy_authorize">authorize</a>(policy: &<a href="../sui/account_policy.md#sui_account_policy_AccountPolicy">AccountPolicy</a>, ctx: &TxContext) {
    <b>if</b> (ctx.epoch() &gt;= policy.<a href="../sui/account_policy.md#sui_account_policy_activation_epoch">activation_epoch</a>) {
        <b>assert</b>!(ctx.co_signers().contains(&policy.guardian), <a href="../sui/account_policy.md#sui_account_policy_ENotCoSignedByGuardian">ENotCoSignedByGuardian</a>);
    }
}
</code></pre>



</details>
