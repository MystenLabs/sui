---
title: Module `sui::account_policy`
---

Opt-in account policies that bound what a transaction signed by the account key alone may do.

Policies are dynamic fields of the singleton <code><a href="../sui/account_policy.md#sui_account_policy_AccountPolicyRegistry">AccountPolicyRegistry</a></code>, keyed by owner address.
Every transaction implicitly reads the registry at the version consensus assigned to it, so
execution can look up the sender's policy without the transaction declaring it. While a policy
is active, execution enforces a per-transaction SUI outflow limit, a gas budget cap, a fixed
package allowlist, and that no non-coin object leaves the owner's ownership. The guardian
exempts individual transactions by approving their digests.

A policy only takes effect <code><a href="../sui/account_policy.md#sui_account_policy_ACTIVATION_DELAY_EPOCHS">ACTIVATION_DELAY_EPOCHS</a></code> after it is enabled, and the owner can
cancel it before then with the key alone, so an attacker holding the key cannot lock the
owner out by enabling a policy with their own guardian.


-  [Struct `AccountPolicyRegistry`](#sui_account_policy_AccountPolicyRegistry)
-  [Struct `PolicyKey`](#sui_account_policy_PolicyKey)
-  [Struct `AccountPolicy`](#sui_account_policy_AccountPolicy)
-  [Constants](#@Constants_0)
-  [Function `create`](#sui_account_policy_create)
-  [Function `enable`](#sui_account_policy_enable)
-  [Function `cancel`](#sui_account_policy_cancel)
-  [Function `approve`](#sui_account_policy_approve)
-  [Function `update`](#sui_account_policy_update)
-  [Function `disable`](#sui_account_policy_disable)
-  [Function `exists`](#sui_account_policy_exists)
-  [Function `activation_epoch`](#sui_account_policy_activation_epoch)
-  [Function `policy_mut`](#sui_account_policy_policy_mut)
-  [Function `assert_approved`](#sui_account_policy_assert_approved)


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
<code>sui_limit_per_tx: u64</code>
</dt>
<dd>
 Maximum net SUI (in MIST) that may leave the owner's coins and stake in one transaction.
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
<code>approved_digests: vector&lt;vector&lt;u8&gt;&gt;</code>
</dt>
<dd>
 Transaction digests the guardian has exempted from the policy.
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



<a name="sui_account_policy_ENotGuardian"></a>



<pre><code>#[error]
<b>const</b> <a href="../sui/account_policy.md#sui_account_policy_ENotGuardian">ENotGuardian</a>: vector&lt;u8&gt; = b"Only the policy guardian can do this.";
</code></pre>



<a name="sui_account_policy_EAlreadyActive"></a>



<pre><code>#[error]
<b>const</b> <a href="../sui/account_policy.md#sui_account_policy_EAlreadyActive">EAlreadyActive</a>: vector&lt;u8&gt; = b"An active policy can only be changed with guardian approval.";
</code></pre>



<a name="sui_account_policy_ENotApproved"></a>



<pre><code>#[error]
<b>const</b> <a href="../sui/account_policy.md#sui_account_policy_ENotApproved">ENotApproved</a>: vector&lt;u8&gt; = b"This transaction <b>has</b> not been approved by the guardian.";
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

Opt the sender in. The policy is enforced from <code><a href="../sui/account_policy.md#sui_account_policy_ACTIVATION_DELAY_EPOCHS">ACTIVATION_DELAY_EPOCHS</a></code> epochs from now.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/account_policy.md#sui_account_policy_enable">enable</a>(registry: &<b>mut</b> <a href="../sui/account_policy.md#sui_account_policy_AccountPolicyRegistry">sui::account_policy::AccountPolicyRegistry</a>, guardian: <b>address</b>, sui_limit_per_tx: u64, gas_budget_cap: u64, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/account_policy.md#sui_account_policy_enable">enable</a>(
    registry: &<b>mut</b> <a href="../sui/account_policy.md#sui_account_policy_AccountPolicyRegistry">AccountPolicyRegistry</a>,
    guardian: <b>address</b>,
    sui_limit_per_tx: u64,
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
            sui_limit_per_tx,
            gas_budget_cap,
            <a href="../sui/account_policy.md#sui_account_policy_activation_epoch">activation_epoch</a>: ctx.epoch() + <a href="../sui/account_policy.md#sui_account_policy_ACTIVATION_DELAY_EPOCHS">ACTIVATION_DELAY_EPOCHS</a>,
            approved_digests: vector[],
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

<a name="sui_account_policy_approve"></a>

## Function `approve`

Exempt the transaction with digest <code>digest</code> from <code>owner</code>'s policy. Guardian only.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/account_policy.md#sui_account_policy_approve">approve</a>(registry: &<b>mut</b> <a href="../sui/account_policy.md#sui_account_policy_AccountPolicyRegistry">sui::account_policy::AccountPolicyRegistry</a>, owner: <b>address</b>, digest: vector&lt;u8&gt;, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/account_policy.md#sui_account_policy_approve">approve</a>(
    registry: &<b>mut</b> <a href="../sui/account_policy.md#sui_account_policy_AccountPolicyRegistry">AccountPolicyRegistry</a>,
    owner: <b>address</b>,
    digest: vector&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> policy = registry.<a href="../sui/account_policy.md#sui_account_policy_policy_mut">policy_mut</a>(owner);
    <b>assert</b>!(ctx.sender() == policy.guardian, <a href="../sui/account_policy.md#sui_account_policy_ENotGuardian">ENotGuardian</a>);
    policy.approved_digests.push_back(digest);
}
</code></pre>



</details>

<a name="sui_account_policy_update"></a>

## Function `update`

Change the sender's policy. The calling transaction must itself be guardian-approved.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/account_policy.md#sui_account_policy_update">update</a>(registry: &<b>mut</b> <a href="../sui/account_policy.md#sui_account_policy_AccountPolicyRegistry">sui::account_policy::AccountPolicyRegistry</a>, guardian: <b>address</b>, sui_limit_per_tx: u64, gas_budget_cap: u64, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/account_policy.md#sui_account_policy_update">update</a>(
    registry: &<b>mut</b> <a href="../sui/account_policy.md#sui_account_policy_AccountPolicyRegistry">AccountPolicyRegistry</a>,
    guardian: <b>address</b>,
    sui_limit_per_tx: u64,
    gas_budget_cap: u64,
    ctx: &TxContext,
) {
    <b>let</b> policy = registry.<a href="../sui/account_policy.md#sui_account_policy_policy_mut">policy_mut</a>(ctx.sender());
    policy.<a href="../sui/account_policy.md#sui_account_policy_assert_approved">assert_approved</a>(ctx);
    policy.guardian = guardian;
    policy.sui_limit_per_tx = sui_limit_per_tx;
    policy.gas_budget_cap = gas_budget_cap;
}
</code></pre>



</details>

<a name="sui_account_policy_disable"></a>

## Function `disable`

Stop enforcing the sender's policy. The calling transaction must itself be guardian-approved.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/account_policy.md#sui_account_policy_disable">disable</a>(registry: &<b>mut</b> <a href="../sui/account_policy.md#sui_account_policy_AccountPolicyRegistry">sui::account_policy::AccountPolicyRegistry</a>, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/account_policy.md#sui_account_policy_disable">disable</a>(registry: &<b>mut</b> <a href="../sui/account_policy.md#sui_account_policy_AccountPolicyRegistry">AccountPolicyRegistry</a>, ctx: &TxContext) {
    <b>let</b> policy = registry.<a href="../sui/account_policy.md#sui_account_policy_policy_mut">policy_mut</a>(ctx.sender());
    policy.<a href="../sui/account_policy.md#sui_account_policy_assert_approved">assert_approved</a>(ctx);
    policy.<a href="../sui/account_policy.md#sui_account_policy_activation_epoch">activation_epoch</a> = <a href="../sui/account_policy.md#sui_account_policy_DISABLED">DISABLED</a>;
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

<a name="sui_account_policy_assert_approved"></a>

## Function `assert_approved`



<pre><code><b>fun</b> <a href="../sui/account_policy.md#sui_account_policy_assert_approved">assert_approved</a>(policy: &<a href="../sui/account_policy.md#sui_account_policy_AccountPolicy">sui::account_policy::AccountPolicy</a>, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/account_policy.md#sui_account_policy_assert_approved">assert_approved</a>(policy: &<a href="../sui/account_policy.md#sui_account_policy_AccountPolicy">AccountPolicy</a>, ctx: &TxContext) {
    <b>assert</b>!(policy.approved_digests.contains(ctx.digest()), <a href="../sui/account_policy.md#sui_account_policy_ENotApproved">ENotApproved</a>);
}
</code></pre>



</details>
