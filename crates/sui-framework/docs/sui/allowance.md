---
title: Module `sui::allowance`
---

SAMPLE / API SKETCH: native allowances. Delegated, bounded, revocable
spending from an address's live balance (no escrow).

The core verifies a tx's declared (funder, allowance) source at signing and
hands the PTB an <code><a href="../sui/allowance.md#sui_allowance_AllowanceWithdrawal">AllowanceWithdrawal</a></code>; the spend paths enforce policy and
redeem in one step, so limits are never consumed without funds moving.


-  [Struct `AllowanceWithdrawal`](#sui_allowance_AllowanceWithdrawal)
-  [Struct `Allowance`](#sui_allowance_Allowance)
-  [Struct `AllowanceCap`](#sui_allowance_AllowanceCap)
-  [Struct `Permit`](#sui_allowance_Permit)
-  [Enum `RateLimit`](#sui_allowance_RateLimit)
-  [Constants](#@Constants_0)
-  [Function `permit`](#sui_allowance_permit)
-  [Function `new`](#sui_allowance_new)
-  [Function `new_for_app`](#sui_allowance_new_for_app)
-  [Function `spend_balance`](#sui_allowance_spend_balance)
-  [Function `spend_balance_as_app`](#sui_allowance_spend_balance_as_app)
-  [Function `revoke`](#sui_allowance_revoke)
-  [Function `rotate_spender`](#sui_allowance_rotate_spender)
-  [Function `assert_signer`](#sui_allowance_assert_signer)
-  [Function `assert_app`](#sui_allowance_assert_app)
-  [Function `consume`](#sui_allowance_consume)
-  [Function `build_rate_limit`](#sui_allowance_build_rate_limit)
-  [Function `share_new`](#sui_allowance_share_new)


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
<b>use</b> <a href="../sui/address.md#sui_address">sui::address</a>;
<b>use</b> <a href="../sui/balance.md#sui_balance">sui::balance</a>;
<b>use</b> <a href="../sui/clock.md#sui_clock">sui::clock</a>;
<b>use</b> <a href="../sui/dynamic_field.md#sui_dynamic_field">sui::dynamic_field</a>;
<b>use</b> <a href="../sui/funds_accumulator.md#sui_funds_accumulator">sui::funds_accumulator</a>;
<b>use</b> <a href="../sui/hex.md#sui_hex">sui::hex</a>;
<b>use</b> <a href="../sui/object.md#sui_object">sui::object</a>;
<b>use</b> <a href="../sui/party.md#sui_party">sui::party</a>;
<b>use</b> <a href="../sui/protocol_config.md#sui_protocol_config">sui::protocol_config</a>;
<b>use</b> <a href="../sui/transfer.md#sui_transfer">sui::transfer</a>;
<b>use</b> <a href="../sui/tx_context.md#sui_tx_context">sui::tx_context</a>;
<b>use</b> <a href="../sui/vec_map.md#sui_vec_map">sui::vec_map</a>;
</code></pre>



<a name="sui_allowance_AllowanceWithdrawal"></a>

## Struct `AllowanceWithdrawal`

Created by the core for a declared allowance source. Only the bound
allowance's spend paths can unpack it. Dropping it is fine: funds only
move on redemption.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/allowance.md#sui_allowance_AllowanceWithdrawal">AllowanceWithdrawal</a>&lt;<b>phantom</b> T: store&gt; <b>has</b> drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code><a href="../sui/allowance.md#sui_allowance">allowance</a>: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a></code>
</dt>
<dd>
</dd>
<dt>
<code>inner: <a href="../sui/funds_accumulator.md#sui_funds_accumulator_Withdrawal">sui::funds_accumulator::Withdrawal</a>&lt;T&gt;</code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="sui_allowance_Allowance"></a>

## Struct `Allowance`

Delegated authority to withdraw <code>T</code> from <code>funder</code>'s balance, within limits.
A shared object (discoverable + revocable); the spending tx references it by id.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/allowance.md#sui_allowance_Allowance">Allowance</a>&lt;<b>phantom</b> T&gt; <b>has</b> key
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
<code>funder: <b>address</b></code>
</dt>
<dd>
</dd>
<dt>
<code>spender: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;<b>address</b>&gt;</code>
</dt>
<dd>
 Always <code>Some</code> in the first release.
 <code>Option</code> so app-bound allowances can later go keyless.
</dd>
<dt>
<code>app: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;<a href="../std/type_name.md#std_type_name_TypeName">std::type_name::TypeName</a>&gt;</code>
</dt>
<dd>
 When set, only the app's module can spend and rotate; the signer path
 is disabled and <code>spender</code> is just the sign-time gate.
</dd>
<dt>
<code>lifetime_cap: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;u256&gt;</code>
</dt>
<dd>
 <code>None</code> = no lifetime total; at least one of cap / rate limit must be
 set. Amounts are <code>u256</code> (matching <code>Withdrawal.limit</code>); times are ms.
</dd>
<dt>
<code>current_spend: u256</code>
</dt>
<dd>
 The total spend, to date, of this allowance. Gets bumped on every spend.
</dd>
<dt>
<code>start_timestamp_ms: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;u64&gt;</code>
</dt>
<dd>
 Inclusive activation time; <code>None</code> = active on issue.
</dd>
<dt>
<code>expiration_timestamp_ms: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;u64&gt;</code>
</dt>
<dd>
</dd>
<dt>
<code>rate_limit: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;<a href="../sui/allowance.md#sui_allowance_RateLimit">sui::allowance::RateLimit</a>&gt;</code>
</dt>
<dd>
</dd>
<dt>
<code>name: <a href="../std/string.md#std_string_String">std::string::String</a></code>
</dt>
<dd>
 custom label, at most 128 bytes; only present for off-chain consumption
 (adding a short desc or label for an allowance, not consulted by any check)
</dd>
</dl>


</details>

<a name="sui_allowance_AllowanceCap"></a>

## Struct `AllowanceCap`

Revocation for an allowance, sent to the funder at issuance (key-only, non-transferrable).
Also used for discoverability (funder -> allowances)


<pre><code><b>public</b> <b>struct</b> <a href="../sui/allowance.md#sui_allowance_AllowanceCap">AllowanceCap</a>&lt;<b>phantom</b> T&gt; <b>has</b> key
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
<code><a href="../sui/allowance.md#sui_allowance">allowance</a>: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a></code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="sui_allowance_Permit"></a>

## Struct `Permit`

App authorization for the <code>_as_app</code> endpoints. A separate type so the
allowance API has its own authorization type instead of <code>internal::Permit</code>.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/allowance.md#sui_allowance_Permit">Permit</a>&lt;<b>phantom</b> A&gt; <b>has</b> drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
</dl>


</details>

<a name="sui_allowance_RateLimit"></a>

## Enum `RateLimit`

A tumbling cap: at most <code>limit</code> per <code>period_ms</code>, the window restarting at
the first spend after it elapses. An enum to leave layout room for future
kinds (public because the compiler does not support internal enums yet).


<pre><code><b>public</b> <b>enum</b> <a href="../sui/allowance.md#sui_allowance_RateLimit">RateLimit</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Variants</summary>


<dl>
<dt>
Variant <code>FixedWindow</code>
</dt>
<dd>
</dd>

<dl>
<dt>
<code>period_ms: u64</code>
</dt>
<dd>
</dd>
</dl>


<dl>
<dt>
<code>limit: u256</code>
</dt>
<dd>
</dd>
</dl>


<dl>
<dt>
<code>spent: u256</code>
</dt>
<dd>
</dd>
</dl>


<dl>
<dt>
<code>window_start_ms: u64</code>
</dt>
<dd>
</dd>
</dl>

</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="sui_allowance_ENotSpender"></a>



<pre><code>#[error]
<b>const</b> <a href="../sui/allowance.md#sui_allowance_ENotSpender">ENotSpender</a>: vector&lt;u8&gt; = b"Transaction sender is not this <a href="../sui/allowance.md#sui_allowance">allowance</a>'s spender";
</code></pre>



<a name="sui_allowance_EWrongApp"></a>



<pre><code>#[error]
<b>const</b> <a href="../sui/allowance.md#sui_allowance_EWrongApp">EWrongApp</a>: vector&lt;u8&gt; = b"<a href="../sui/allowance.md#sui_allowance_Permit">Permit</a> type does not match the <a href="../sui/allowance.md#sui_allowance">allowance</a>'s app";
</code></pre>



<a name="sui_allowance_ENoApp"></a>



<pre><code>#[error]
<b>const</b> <a href="../sui/allowance.md#sui_allowance_ENoApp">ENoApp</a>: vector&lt;u8&gt; = b"<a href="../sui/allowance.md#sui_allowance_Allowance">Allowance</a> <b>has</b> no app, so it <b>has</b> no app-authorized spend or rotate";
</code></pre>



<a name="sui_allowance_EExpired"></a>



<pre><code>#[error]
<b>const</b> <a href="../sui/allowance.md#sui_allowance_EExpired">EExpired</a>: vector&lt;u8&gt; = b"<a href="../sui/allowance.md#sui_allowance_Allowance">Allowance</a> <b>has</b> expired";
</code></pre>



<a name="sui_allowance_EExceedsLifetimeCap"></a>



<pre><code>#[error]
<b>const</b> <a href="../sui/allowance.md#sui_allowance_EExceedsLifetimeCap">EExceedsLifetimeCap</a>: vector&lt;u8&gt; = b"Spend would exceed the lifetime cap";
</code></pre>



<a name="sui_allowance_EExceedsRateLimit"></a>



<pre><code>#[error]
<b>const</b> <a href="../sui/allowance.md#sui_allowance_EExceedsRateLimit">EExceedsRateLimit</a>: vector&lt;u8&gt; = b"Spend would exceed the current rate-limit window";
</code></pre>



<a name="sui_allowance_ENoLimit"></a>



<pre><code>#[error]
<b>const</b> <a href="../sui/allowance.md#sui_allowance_ENoLimit">ENoLimit</a>: vector&lt;u8&gt; = b"<a href="../sui/allowance.md#sui_allowance_Allowance">Allowance</a> must have a lifetime cap or a rate limit";
</code></pre>



<a name="sui_allowance_EWrongAllowance"></a>



<pre><code>#[error]
<b>const</b> <a href="../sui/allowance.md#sui_allowance_EWrongAllowance">EWrongAllowance</a>: vector&lt;u8&gt; = b"Withdrawal was issued <b>for</b> a different <a href="../sui/allowance.md#sui_allowance">allowance</a>";
</code></pre>



<a name="sui_allowance_EBadRateLimit"></a>



<pre><code>#[error]
<b>const</b> <a href="../sui/allowance.md#sui_allowance_EBadRateLimit">EBadRateLimit</a>: vector&lt;u8&gt; = b"Rate limit needs a positive period and amount, both set or neither";
</code></pre>



<a name="sui_allowance_ENotStarted"></a>



<pre><code>#[error]
<b>const</b> <a href="../sui/allowance.md#sui_allowance_ENotStarted">ENotStarted</a>: vector&lt;u8&gt; = b"<a href="../sui/allowance.md#sui_allowance_Allowance">Allowance</a> is not active yet; it <b>has</b> a future start timestamp";
</code></pre>



<a name="sui_allowance_EHasApp"></a>



<pre><code>#[error]
<b>const</b> <a href="../sui/allowance.md#sui_allowance_EHasApp">EHasApp</a>: vector&lt;u8&gt; = b"App-controlled <a href="../sui/allowance.md#sui_allowance">allowance</a>: spending must go through `spend_as_app`";
</code></pre>



<a name="sui_allowance_EWrongFunder"></a>



<pre><code>#[error]
<b>const</b> <a href="../sui/allowance.md#sui_allowance_EWrongFunder">EWrongFunder</a>: vector&lt;u8&gt; = b"Withdrawal debits a different <b>address</b> than this <a href="../sui/allowance.md#sui_allowance">allowance</a>'s funder";
</code></pre>



<a name="sui_allowance_EWrongCap"></a>



<pre><code>#[error]
<b>const</b> <a href="../sui/allowance.md#sui_allowance_EWrongCap">EWrongCap</a>: vector&lt;u8&gt; = b"Cap does not match this <a href="../sui/allowance.md#sui_allowance">allowance</a>";
</code></pre>



<a name="sui_allowance_ENameTooLong"></a>



<pre><code>#[error]
<b>const</b> <a href="../sui/allowance.md#sui_allowance_ENameTooLong">ENameTooLong</a>: vector&lt;u8&gt; = b"Name exceeds the 128-byte limit";
</code></pre>



<a name="sui_allowance_EZeroLifetimeCap"></a>



<pre><code>#[error]
<b>const</b> <a href="../sui/allowance.md#sui_allowance_EZeroLifetimeCap">EZeroLifetimeCap</a>: vector&lt;u8&gt; = b"Lifetime cap must be greater than zero";
</code></pre>



<a name="sui_allowance_EBadTimeWindow"></a>



<pre><code>#[error]
<b>const</b> <a href="../sui/allowance.md#sui_allowance_EBadTimeWindow">EBadTimeWindow</a>: vector&lt;u8&gt; = b"Expiration must be after the start time";
</code></pre>



<a name="sui_allowance_ENoExpiration"></a>



<pre><code>#[error]
<b>const</b> <a href="../sui/allowance.md#sui_allowance_ENoExpiration">ENoExpiration</a>: vector&lt;u8&gt; = b"<a href="../sui/allowance.md#sui_allowance_Allowance">Allowance</a> must have an expiration or a rate limit";
</code></pre>



<a name="sui_allowance_MAX_NAME_LENGTH"></a>



<pre><code><b>const</b> <a href="../sui/allowance.md#sui_allowance_MAX_NAME_LENGTH">MAX_NAME_LENGTH</a>: u64 = 128;
</code></pre>



<a name="sui_allowance_permit"></a>

## Function `permit`

Only <code>A</code>'s module can create <code>internal::Permit&lt;A&gt;</code>, so only it can build this.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_permit">permit</a>&lt;A&gt;(_: <a href="../std/internal.md#std_internal_Permit">std::internal::Permit</a>&lt;A&gt;): <a href="../sui/allowance.md#sui_allowance_Permit">sui::allowance::Permit</a>&lt;A&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_permit">permit</a>&lt;A&gt;(_: internal::Permit&lt;A&gt;): <a href="../sui/allowance.md#sui_allowance_Permit">Permit</a>&lt;A&gt; {
    <a href="../sui/allowance.md#sui_allowance_Permit">Permit</a>()
}
</code></pre>



</details>

<a name="sui_allowance_new"></a>

## Function `new`



<pre><code><b>entry</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_new">new</a>&lt;T&gt;(name: <a href="../std/string.md#std_string_String">std::string::String</a>, spender: <b>address</b>, lifetime_cap: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;u256&gt;, start_timestamp_ms: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;u64&gt;, expiration_timestamp_ms: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;u64&gt;, rate_period_ms: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;u64&gt;, rate_amount: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;u256&gt;, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>entry</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_new">new</a>&lt;T&gt;(
    name: String,
    spender: <b>address</b>,
    lifetime_cap: Option&lt;u256&gt;,
    start_timestamp_ms: Option&lt;u64&gt;,
    expiration_timestamp_ms: Option&lt;u64&gt;,
    rate_period_ms: Option&lt;u64&gt;,
    rate_amount: Option&lt;u256&gt;,
    ctx: &<b>mut</b> TxContext,
) {
    <a href="../sui/allowance.md#sui_allowance_share_new">share_new</a>&lt;T&gt;(
        name,
        spender,
        option::none(),
        lifetime_cap,
        start_timestamp_ms,
        expiration_timestamp_ms,
        <a href="../sui/allowance.md#sui_allowance_build_rate_limit">build_rate_limit</a>(rate_period_ms, rate_amount),
        ctx,
    );
}
</code></pre>



</details>

<a name="sui_allowance_new_for_app"></a>

## Function `new_for_app`

Like <code><a href="../sui/allowance.md#sui_allowance_new">new</a></code>, but also binds the controlling app <code>A</code> (see <code><a href="../sui/allowance.md#sui_allowance_Allowance">Allowance</a>.app</code>).


<pre><code><b>entry</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_new_for_app">new_for_app</a>&lt;T, A&gt;(name: <a href="../std/string.md#std_string_String">std::string::String</a>, spender: <b>address</b>, lifetime_cap: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;u256&gt;, start_timestamp_ms: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;u64&gt;, expiration_timestamp_ms: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;u64&gt;, rate_period_ms: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;u64&gt;, rate_amount: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;u256&gt;, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>entry</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_new_for_app">new_for_app</a>&lt;T, A&gt;(
    name: String,
    spender: <b>address</b>,
    lifetime_cap: Option&lt;u256&gt;,
    start_timestamp_ms: Option&lt;u64&gt;,
    expiration_timestamp_ms: Option&lt;u64&gt;,
    rate_period_ms: Option&lt;u64&gt;,
    rate_amount: Option&lt;u256&gt;,
    ctx: &<b>mut</b> TxContext,
) {
    <a href="../sui/allowance.md#sui_allowance_share_new">share_new</a>&lt;T&gt;(
        name,
        spender,
        option::some(type_name::with_defining_ids&lt;A&gt;()),
        lifetime_cap,
        start_timestamp_ms,
        expiration_timestamp_ms,
        <a href="../sui/allowance.md#sui_allowance_build_rate_limit">build_rate_limit</a>(rate_period_ms, rate_amount),
        ctx,
    );
}
</code></pre>



</details>

<a name="sui_allowance_spend_balance"></a>

## Function `spend_balance`

Signer path: the tx sender must be the spender. (A non-balance spend would
require access to <code>funds_accumulator::Permit&lt;T&gt;</code>, so <code>Balance</code>-only for now.)


<pre><code><b>public</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_spend_balance">spend_balance</a>&lt;C&gt;(self: &<b>mut</b> <a href="../sui/allowance.md#sui_allowance_Allowance">sui::allowance::Allowance</a>&lt;<a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;C&gt;&gt;, w: <a href="../sui/allowance.md#sui_allowance_AllowanceWithdrawal">sui::allowance::AllowanceWithdrawal</a>&lt;<a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;C&gt;&gt;, <a href="../sui/clock.md#sui_clock">clock</a>: &<a href="../sui/clock.md#sui_clock_Clock">sui::clock::Clock</a>, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;C&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_spend_balance">spend_balance</a>&lt;C&gt;(
    self: &<b>mut</b> <a href="../sui/allowance.md#sui_allowance_Allowance">Allowance</a>&lt;Balance&lt;C&gt;&gt;,
    w: <a href="../sui/allowance.md#sui_allowance_AllowanceWithdrawal">AllowanceWithdrawal</a>&lt;Balance&lt;C&gt;&gt;,
    <a href="../sui/clock.md#sui_clock">clock</a>: &Clock,
    ctx: &TxContext,
): Balance&lt;C&gt; {
    self.<a href="../sui/allowance.md#sui_allowance_assert_signer">assert_signer</a>(ctx);
    <a href="../sui/balance.md#sui_balance_redeem_funds">balance::redeem_funds</a>(self.<a href="../sui/allowance.md#sui_allowance_consume">consume</a>(w, <a href="../sui/clock.md#sui_clock">clock</a>))
}
</code></pre>



</details>

<a name="sui_allowance_spend_balance_as_app"></a>

## Function `spend_balance_as_app`

App path: authorized by <code><a href="../sui/allowance.md#sui_allowance_Permit">Permit</a>&lt;A&gt;</code> (matching the allowance's <code>app</code>), no
signer required.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_spend_balance_as_app">spend_balance_as_app</a>&lt;C, A&gt;(self: &<b>mut</b> <a href="../sui/allowance.md#sui_allowance_Allowance">sui::allowance::Allowance</a>&lt;<a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;C&gt;&gt;, _: <a href="../sui/allowance.md#sui_allowance_Permit">sui::allowance::Permit</a>&lt;A&gt;, w: <a href="../sui/allowance.md#sui_allowance_AllowanceWithdrawal">sui::allowance::AllowanceWithdrawal</a>&lt;<a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;C&gt;&gt;, <a href="../sui/clock.md#sui_clock">clock</a>: &<a href="../sui/clock.md#sui_clock_Clock">sui::clock::Clock</a>): <a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;C&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_spend_balance_as_app">spend_balance_as_app</a>&lt;C, A&gt;(
    self: &<b>mut</b> <a href="../sui/allowance.md#sui_allowance_Allowance">Allowance</a>&lt;Balance&lt;C&gt;&gt;,
    _: <a href="../sui/allowance.md#sui_allowance_Permit">Permit</a>&lt;A&gt;,
    w: <a href="../sui/allowance.md#sui_allowance_AllowanceWithdrawal">AllowanceWithdrawal</a>&lt;Balance&lt;C&gt;&gt;,
    <a href="../sui/clock.md#sui_clock">clock</a>: &Clock,
): Balance&lt;C&gt; {
    self.<a href="../sui/allowance.md#sui_allowance_assert_app">assert_app</a>&lt;Balance&lt;C&gt;, A&gt;();
    <a href="../sui/balance.md#sui_balance_redeem_funds">balance::redeem_funds</a>(self.<a href="../sui/allowance.md#sui_allowance_consume">consume</a>(w, <a href="../sui/clock.md#sui_clock">clock</a>))
}
</code></pre>



</details>

<a name="sui_allowance_revoke"></a>

## Function `revoke`

Possession of the matching cap is what authorizes revocation; no signer check.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_revoke">revoke</a>&lt;T&gt;(self: <a href="../sui/allowance.md#sui_allowance_Allowance">sui::allowance::Allowance</a>&lt;T&gt;, cap: <a href="../sui/allowance.md#sui_allowance_AllowanceCap">sui::allowance::AllowanceCap</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_revoke">revoke</a>&lt;T&gt;(self: <a href="../sui/allowance.md#sui_allowance_Allowance">Allowance</a>&lt;T&gt;, cap: <a href="../sui/allowance.md#sui_allowance_AllowanceCap">AllowanceCap</a>&lt;T&gt;) {
    <b>let</b> <a href="../sui/allowance.md#sui_allowance_AllowanceCap">AllowanceCap</a> { id: cap_id, <a href="../sui/allowance.md#sui_allowance">allowance</a> } = cap;
    <b>assert</b>!(<a href="../sui/allowance.md#sui_allowance">allowance</a> == self.id.to_inner(), <a href="../sui/allowance.md#sui_allowance_EWrongCap">EWrongCap</a>);
    <b>let</b> <a href="../sui/allowance.md#sui_allowance_Allowance">Allowance</a> {
        id,
        ..,
    } = self;
    id.delete();
    cap_id.delete();
}
</code></pre>



</details>

<a name="sui_allowance_rotate_spender"></a>

## Function `rotate_spender`

App-only: rotate the spender key without the funder reissuing.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_rotate_spender">rotate_spender</a>&lt;T, A&gt;(self: &<b>mut</b> <a href="../sui/allowance.md#sui_allowance_Allowance">sui::allowance::Allowance</a>&lt;T&gt;, _: <a href="../sui/allowance.md#sui_allowance_Permit">sui::allowance::Permit</a>&lt;A&gt;, new_spender: <b>address</b>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_rotate_spender">rotate_spender</a>&lt;T, A&gt;(self: &<b>mut</b> <a href="../sui/allowance.md#sui_allowance_Allowance">Allowance</a>&lt;T&gt;, _: <a href="../sui/allowance.md#sui_allowance_Permit">Permit</a>&lt;A&gt;, new_spender: <b>address</b>) {
    self.<a href="../sui/allowance.md#sui_allowance_assert_app">assert_app</a>&lt;T, A&gt;();
    self.spender = option::some(new_spender);
}
</code></pre>



</details>

<a name="sui_allowance_assert_signer"></a>

## Function `assert_signer`

Signer path: no controlling app, and the tx sender is the spender.


<pre><code><b>fun</b> <a href="../sui/allowance.md#sui_allowance_assert_signer">assert_signer</a>&lt;T&gt;(self: &<a href="../sui/allowance.md#sui_allowance_Allowance">sui::allowance::Allowance</a>&lt;T&gt;, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/allowance.md#sui_allowance_assert_signer">assert_signer</a>&lt;T&gt;(self: &<a href="../sui/allowance.md#sui_allowance_Allowance">Allowance</a>&lt;T&gt;, ctx: &TxContext) {
    <b>assert</b>!(self.app.is_none(), <a href="../sui/allowance.md#sui_allowance_EHasApp">EHasApp</a>);
    <b>assert</b>!(self.spender.contains(&ctx.sender()), <a href="../sui/allowance.md#sui_allowance_ENotSpender">ENotSpender</a>);
}
</code></pre>



</details>

<a name="sui_allowance_assert_app"></a>

## Function `assert_app`

App-path authorization: <code>A</code> matches the allowance's controlling app.


<pre><code><b>fun</b> <a href="../sui/allowance.md#sui_allowance_assert_app">assert_app</a>&lt;T, A&gt;(self: &<a href="../sui/allowance.md#sui_allowance_Allowance">sui::allowance::Allowance</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/allowance.md#sui_allowance_assert_app">assert_app</a>&lt;T, A&gt;(self: &<a href="../sui/allowance.md#sui_allowance_Allowance">Allowance</a>&lt;T&gt;) {
    <b>assert</b>!(self.app.is_some(), <a href="../sui/allowance.md#sui_allowance_ENoApp">ENoApp</a>);
    <b>assert</b>!(*self.app.<a href="../sui/borrow.md#sui_borrow">borrow</a>() == type_name::with_defining_ids&lt;A&gt;(), <a href="../sui/allowance.md#sui_allowance_EWrongApp">EWrongApp</a>);
}
</code></pre>



</details>

<a name="sui_allowance_consume"></a>

## Function `consume`

Policy checks + accounting shared by all spend paths; authorization is the
callers' responsibility.


<pre><code><b>fun</b> <a href="../sui/allowance.md#sui_allowance_consume">consume</a>&lt;T: store&gt;(self: &<b>mut</b> <a href="../sui/allowance.md#sui_allowance_Allowance">sui::allowance::Allowance</a>&lt;T&gt;, w: <a href="../sui/allowance.md#sui_allowance_AllowanceWithdrawal">sui::allowance::AllowanceWithdrawal</a>&lt;T&gt;, <a href="../sui/clock.md#sui_clock">clock</a>: &<a href="../sui/clock.md#sui_clock_Clock">sui::clock::Clock</a>): <a href="../sui/funds_accumulator.md#sui_funds_accumulator_Withdrawal">sui::funds_accumulator::Withdrawal</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/allowance.md#sui_allowance_consume">consume</a>&lt;T: store&gt;(
    self: &<b>mut</b> <a href="../sui/allowance.md#sui_allowance_Allowance">Allowance</a>&lt;T&gt;,
    w: <a href="../sui/allowance.md#sui_allowance_AllowanceWithdrawal">AllowanceWithdrawal</a>&lt;T&gt;,
    <a href="../sui/clock.md#sui_clock">clock</a>: &Clock,
): Withdrawal&lt;T&gt; {
    <b>let</b> <a href="../sui/allowance.md#sui_allowance_AllowanceWithdrawal">AllowanceWithdrawal</a> { <a href="../sui/allowance.md#sui_allowance">allowance</a>, inner } = w;
    <b>assert</b>!(<a href="../sui/allowance.md#sui_allowance">allowance</a> == self.id.to_inner(), <a href="../sui/allowance.md#sui_allowance_EWrongAllowance">EWrongAllowance</a>);
    // This can only happen <b>if</b> we have a bug in the core, so this is just <b>for</b> in-depth defense.
    <b>assert</b>!(inner.owner() == self.funder, <a href="../sui/allowance.md#sui_allowance_EWrongFunder">EWrongFunder</a>);
    <b>let</b> amount = inner.limit();
    <b>let</b> now = <a href="../sui/clock.md#sui_clock">clock</a>.timestamp_ms();
    self.start_timestamp_ms.do_ref!(|start_timestamp_ms| {
        <b>assert</b>!(now &gt;= *start_timestamp_ms, <a href="../sui/allowance.md#sui_allowance_ENotStarted">ENotStarted</a>);
    });
    self.expiration_timestamp_ms.do_ref!(|expiration_timestamp_ms| {
        <b>assert</b>!(now &lt;= *expiration_timestamp_ms, <a href="../sui/allowance.md#sui_allowance_EExpired">EExpired</a>);
    });
    self.lifetime_cap.do_ref!(|lifetime_cap| {
        <b>assert</b>!(self.current_spend + amount &lt;= *lifetime_cap, <a href="../sui/allowance.md#sui_allowance_EExceedsLifetimeCap">EExceedsLifetimeCap</a>);
    });
    self.current_spend = self.current_spend + amount;
    self.rate_limit.do_mut!(|rl| match (rl) {
        RateLimit::FixedWindow { period_ms, limit, spent, window_start_ms } =&gt; {
            // Tumbling window: reset once the period <b>has</b> elapsed.
            <b>if</b> (now &gt;= *window_start_ms + *period_ms) {
                *window_start_ms = now;
                *spent = 0;
            };
            <b>assert</b>!(*spent + amount &lt;= *limit, <a href="../sui/allowance.md#sui_allowance_EExceedsRateLimit">EExceedsRateLimit</a>);
            *spent = *spent + amount;
        },
    });
    inner
}
</code></pre>



</details>

<a name="sui_allowance_build_rate_limit"></a>

## Function `build_rate_limit`

Both <code>Some</code> (a limit) or both <code>None</code> (no limit); a mismatch aborts.


<pre><code><b>fun</b> <a href="../sui/allowance.md#sui_allowance_build_rate_limit">build_rate_limit</a>(period_ms: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;u64&gt;, amount: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;u256&gt;): <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;<a href="../sui/allowance.md#sui_allowance_RateLimit">sui::allowance::RateLimit</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/allowance.md#sui_allowance_build_rate_limit">build_rate_limit</a>(period_ms: Option&lt;u64&gt;, amount: Option&lt;u256&gt;): Option&lt;<a href="../sui/allowance.md#sui_allowance_RateLimit">RateLimit</a>&gt; {
    <b>assert</b>!(period_ms.is_some() == amount.is_some(), <a href="../sui/allowance.md#sui_allowance_EBadRateLimit">EBadRateLimit</a>);
    <b>if</b> (period_ms.is_none()) <b>return</b> option::none();
    <b>let</b> period_ms = *period_ms.<a href="../sui/borrow.md#sui_borrow">borrow</a>();
    <b>let</b> limit = *amount.<a href="../sui/borrow.md#sui_borrow">borrow</a>();
    // A zero period resets the window on every spend; a zero amount spends nothing.
    <b>assert</b>!(period_ms &gt; 0 && limit &gt; 0, <a href="../sui/allowance.md#sui_allowance_EBadRateLimit">EBadRateLimit</a>);
    option::some(RateLimit::FixedWindow {
        period_ms,
        limit,
        spent: 0,
        window_start_ms: 0,
    })
}
</code></pre>



</details>

<a name="sui_allowance_share_new"></a>

## Function `share_new`



<pre><code><b>fun</b> <a href="../sui/allowance.md#sui_allowance_share_new">share_new</a>&lt;T&gt;(name: <a href="../std/string.md#std_string_String">std::string::String</a>, spender: <b>address</b>, app: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;<a href="../std/type_name.md#std_type_name_TypeName">std::type_name::TypeName</a>&gt;, lifetime_cap: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;u256&gt;, start_timestamp_ms: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;u64&gt;, expiration_timestamp_ms: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;u64&gt;, rate_limit: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;<a href="../sui/allowance.md#sui_allowance_RateLimit">sui::allowance::RateLimit</a>&gt;, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/allowance.md#sui_allowance_share_new">share_new</a>&lt;T&gt;(
    name: String,
    spender: <b>address</b>,
    app: Option&lt;TypeName&gt;,
    lifetime_cap: Option&lt;u256&gt;,
    start_timestamp_ms: Option&lt;u64&gt;,
    expiration_timestamp_ms: Option&lt;u64&gt;,
    rate_limit: Option&lt;<a href="../sui/allowance.md#sui_allowance_RateLimit">RateLimit</a>&gt;,
    ctx: &<b>mut</b> TxContext,
) {
    // we do not allow unlimited allowances (TODO: Do we?)
    <b>assert</b>!(lifetime_cap.is_some() || rate_limit.is_some(), <a href="../sui/allowance.md#sui_allowance_ENoLimit">ENoLimit</a>);
    // Either a hard end date or bounded drain velocity.
    <b>assert</b>!(expiration_timestamp_ms.is_some() || rate_limit.is_some(), <a href="../sui/allowance.md#sui_allowance_ENoExpiration">ENoExpiration</a>);
    <b>assert</b>!(name.length() &lt;= <a href="../sui/allowance.md#sui_allowance_MAX_NAME_LENGTH">MAX_NAME_LENGTH</a>, <a href="../sui/allowance.md#sui_allowance_ENameTooLong">ENameTooLong</a>);
    lifetime_cap.do_ref!(|cap| <b>assert</b>!(*cap &gt; 0, <a href="../sui/allowance.md#sui_allowance_EZeroLifetimeCap">EZeroLifetimeCap</a>));
    <b>if</b> (start_timestamp_ms.is_some() && expiration_timestamp_ms.is_some()) {
        <b>assert</b>!(*start_timestamp_ms.<a href="../sui/borrow.md#sui_borrow">borrow</a>() &lt; *expiration_timestamp_ms.<a href="../sui/borrow.md#sui_borrow">borrow</a>(), <a href="../sui/allowance.md#sui_allowance_EBadTimeWindow">EBadTimeWindow</a>);
    };
    <b>let</b> <a href="../sui/allowance.md#sui_allowance">allowance</a> = <a href="../sui/allowance.md#sui_allowance_Allowance">Allowance</a>&lt;T&gt; {
        id: <a href="../sui/object.md#sui_object_new">object::new</a>(ctx),
        name,
        funder: ctx.sender(),
        spender: option::some(spender),
        app,
        lifetime_cap,
        current_spend: 0,
        start_timestamp_ms,
        expiration_timestamp_ms,
        rate_limit,
    };
    <b>let</b> cap = <a href="../sui/allowance.md#sui_allowance_AllowanceCap">AllowanceCap</a>&lt;T&gt; {
        id: <a href="../sui/object.md#sui_object_new">object::new</a>(ctx),
        <a href="../sui/allowance.md#sui_allowance">allowance</a>: <a href="../sui/allowance.md#sui_allowance">allowance</a>.id.to_inner(),
    };
    <a href="../sui/transfer.md#sui_transfer_transfer">transfer::transfer</a>(cap, ctx.sender());
    <a href="../sui/transfer.md#sui_transfer_share_object">transfer::share_object</a>(<a href="../sui/allowance.md#sui_allowance">allowance</a>);
}
</code></pre>



</details>
