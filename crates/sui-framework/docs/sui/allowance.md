---
title: Module `sui::allowance`
---

Native allowances: delegated, bounded, revocable spending from an
address's live balance (no escrow).

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
-  [Function `fixed_window`](#sui_allowance_fixed_window)
-  [Function `calendar_window`](#sui_allowance_calendar_window)
-  [Function `monthly`](#sui_allowance_monthly)
-  [Function `quarterly`](#sui_allowance_quarterly)
-  [Function `yearly`](#sui_allowance_yearly)
-  [Function `new`](#sui_allowance_new)
-  [Function `new_for_app`](#sui_allowance_new_for_app)
-  [Function `spend_balance`](#sui_allowance_spend_balance)
-  [Function `spend_balance_as_app`](#sui_allowance_spend_balance_as_app)
-  [Function `revoke`](#sui_allowance_revoke)
-  [Function `rotate_spender`](#sui_allowance_rotate_spender)
-  [Function `assert_app`](#sui_allowance_assert_app)
-  [Function `consume`](#sui_allowance_consume)
-  [Function `elapsed_windows`](#sui_allowance_elapsed_windows)
-  [Function `civil_from_ms`](#sui_allowance_civil_from_ms)
-  [Function `days_in_month`](#sui_allowance_days_in_month)
-  [Function `is_leap_year`](#sui_allowance_is_leap_year)
-  [Function `share_new`](#sui_allowance_share_new)


<pre><code><b>use</b> <a href="../std/address.md#std_address">std::address</a>;
<b>use</b> <a href="../std/ascii.md#std_ascii">std::ascii</a>;
<b>use</b> <a href="../std/bcs.md#std_bcs">std::bcs</a>;
<b>use</b> <a href="../std/internal.md#std_internal">std::internal</a>;
<b>use</b> <a href="../std/option.md#std_option">std::option</a>;
<b>use</b> <a href="../std/string.md#std_string">std::string</a>;
<b>use</b> <a href="../std/type_name.md#std_type_name">std::type_name</a>;
<b>use</b> <a href="../std/u128.md#std_u128">std::u128</a>;
<b>use</b> <a href="../std/u64.md#std_u64">std::u64</a>;
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
 When set, spends need the app's <code><a href="../sui/allowance.md#sui_allowance_Permit">Permit</a></code> on top of the spender's
 signature, and only the app's module can rotate the spender.
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
 Cumulative spend, compared against <code>lifetime_cap</code>.
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
 Off-chain label, at most 128 bytes; not consulted by any check.
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

At most <code>limit</code> per window. Windows only roll forward, so boundaries never
drift: <code>FixedWindow</code> periods tile absolute time from the Unix epoch, while
<code>CalendarWindow</code> windows follow the anniversary of the first charge.


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

<dt>
Variant <code>CalendarWindow</code>
</dt>
<dd>
</dd>

<dl>
<dt>
<code>months: u8</code>
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
<code>first_charge_ms: u64</code>
</dt>
<dd>
 Anniversary anchor; stamped by the first successful charge (0 until then).
</dd>
</dl>


<dl>
<dt>
<code>window: u64</code>
</dt>
<dd>
 Windows are numbered from the anchor (0 = first). This is the one
 <code>spent</code> accumulated in; a spend landing in a later one resets it.
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
<b>const</b> <a href="../sui/allowance.md#sui_allowance_EBadRateLimit">EBadRateLimit</a>: vector&lt;u8&gt; = b"Rate limit needs a positive period and limit";
</code></pre>



<a name="sui_allowance_ENotStarted"></a>



<pre><code>#[error]
<b>const</b> <a href="../sui/allowance.md#sui_allowance_ENotStarted">ENotStarted</a>: vector&lt;u8&gt; = b"<a href="../sui/allowance.md#sui_allowance_Allowance">Allowance</a> is not active yet; it <b>has</b> a future start timestamp";
</code></pre>



<a name="sui_allowance_EHasApp"></a>



<pre><code>#[error]
<b>const</b> <a href="../sui/allowance.md#sui_allowance_EHasApp">EHasApp</a>: vector&lt;u8&gt; = b"App-bound <a href="../sui/allowance.md#sui_allowance">allowance</a>: spend through `<a href="../sui/allowance.md#sui_allowance_spend_balance_as_app">spend_balance_as_app</a>`";
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



<a name="sui_allowance_ENotEnabled"></a>



<pre><code>#[error]
<b>const</b> <a href="../sui/allowance.md#sui_allowance_ENotEnabled">ENotEnabled</a>: vector&lt;u8&gt; = b"Allowances are not enabled";
</code></pre>



<a name="sui_allowance_MAX_NAME_LENGTH"></a>



<pre><code><b>const</b> <a href="../sui/allowance.md#sui_allowance_MAX_NAME_LENGTH">MAX_NAME_LENGTH</a>: u64 = 128;
</code></pre>



<a name="sui_allowance_MS_PER_DAY"></a>



<pre><code><b>const</b> <a href="../sui/allowance.md#sui_allowance_MS_PER_DAY">MS_PER_DAY</a>: u64 = 86400000;
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

<a name="sui_allowance_fixed_window"></a>

## Function `fixed_window`

At most <code>limit</code> per <code>period_ms</code>, windows aligned to whole periods from the
Unix epoch (a 24h period resets at midnight UTC).


<pre><code><b>public</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_fixed_window">fixed_window</a>(period_ms: u64, limit: u256): <a href="../sui/allowance.md#sui_allowance_RateLimit">sui::allowance::RateLimit</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_fixed_window">fixed_window</a>(period_ms: u64, limit: u256): <a href="../sui/allowance.md#sui_allowance_RateLimit">RateLimit</a> {
    // A zero period resets the window on every spend; a zero limit spends nothing.
    <b>assert</b>!(period_ms &gt; 0 && limit &gt; 0, <a href="../sui/allowance.md#sui_allowance_EBadRateLimit">EBadRateLimit</a>);
    RateLimit::FixedWindow { period_ms, limit, spent: 0, window_start_ms: 0 }
}
</code></pre>



</details>

<a name="sui_allowance_calendar_window"></a>

## Function `calendar_window`

At most <code>limit</code> per <code>months</code> civil (UTC) months, anchored at the first
successful charge: windows renew on its day-of-month at 00:00 UTC, clamped
to shorter months (a Jan 31 anchor renews Feb 28, then Mar 31).


<pre><code><b>public</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_calendar_window">calendar_window</a>(months: u8, limit: u256): <a href="../sui/allowance.md#sui_allowance_RateLimit">sui::allowance::RateLimit</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_calendar_window">calendar_window</a>(months: u8, limit: u256): <a href="../sui/allowance.md#sui_allowance_RateLimit">RateLimit</a> {
    <b>assert</b>!(months &gt; 0 && limit &gt; 0, <a href="../sui/allowance.md#sui_allowance_EBadRateLimit">EBadRateLimit</a>);
    RateLimit::CalendarWindow { months, limit, spent: 0, first_charge_ms: 0, window: 0 }
}
</code></pre>



</details>

<a name="sui_allowance_monthly"></a>

## Function `monthly`

At most <code>limit</code> per month, from the first charge.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_monthly">monthly</a>(limit: u256): <a href="../sui/allowance.md#sui_allowance_RateLimit">sui::allowance::RateLimit</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_monthly">monthly</a>(limit: u256): <a href="../sui/allowance.md#sui_allowance_RateLimit">RateLimit</a> { <a href="../sui/allowance.md#sui_allowance_calendar_window">calendar_window</a>(1, limit) }
</code></pre>



</details>

<a name="sui_allowance_quarterly"></a>

## Function `quarterly`

At most <code>limit</code> per quarter, from the first charge.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_quarterly">quarterly</a>(limit: u256): <a href="../sui/allowance.md#sui_allowance_RateLimit">sui::allowance::RateLimit</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_quarterly">quarterly</a>(limit: u256): <a href="../sui/allowance.md#sui_allowance_RateLimit">RateLimit</a> { <a href="../sui/allowance.md#sui_allowance_calendar_window">calendar_window</a>(3, limit) }
</code></pre>



</details>

<a name="sui_allowance_yearly"></a>

## Function `yearly`

At most <code>limit</code> per year, from the first charge.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_yearly">yearly</a>(limit: u256): <a href="../sui/allowance.md#sui_allowance_RateLimit">sui::allowance::RateLimit</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_yearly">yearly</a>(limit: u256): <a href="../sui/allowance.md#sui_allowance_RateLimit">RateLimit</a> { <a href="../sui/allowance.md#sui_allowance_calendar_window">calendar_window</a>(12, limit) }
</code></pre>



</details>

<a name="sui_allowance_new"></a>

## Function `new`



<pre><code><b>entry</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_new">new</a>&lt;T&gt;(name: <a href="../std/string.md#std_string_String">std::string::String</a>, spender: <b>address</b>, lifetime_cap: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;u256&gt;, start_timestamp_ms: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;u64&gt;, expiration_timestamp_ms: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;u64&gt;, rate_limit: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;<a href="../sui/allowance.md#sui_allowance_RateLimit">sui::allowance::RateLimit</a>&gt;, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>entry</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_new">new</a>&lt;T&gt;(
    name: String,
    spender: <b>address</b>,
    lifetime_cap: Option&lt;u256&gt;,
    start_timestamp_ms: Option&lt;u64&gt;,
    expiration_timestamp_ms: Option&lt;u64&gt;,
    rate_limit: Option&lt;<a href="../sui/allowance.md#sui_allowance_RateLimit">RateLimit</a>&gt;,
    ctx: &<b>mut</b> TxContext,
) {
    <a href="../sui/allowance.md#sui_allowance_share_new">share_new</a>&lt;T&gt;(
        name,
        spender,
        option::none(),
        lifetime_cap,
        start_timestamp_ms,
        expiration_timestamp_ms,
        rate_limit,
        ctx,
    );
}
</code></pre>



</details>

<a name="sui_allowance_new_for_app"></a>

## Function `new_for_app`

Like <code><a href="../sui/allowance.md#sui_allowance_new">new</a></code>, but also binds the controlling app <code>A</code> (see <code><a href="../sui/allowance.md#sui_allowance_Allowance">Allowance</a>.app</code>).


<pre><code><b>entry</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_new_for_app">new_for_app</a>&lt;T, A&gt;(name: <a href="../std/string.md#std_string_String">std::string::String</a>, spender: <b>address</b>, lifetime_cap: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;u256&gt;, start_timestamp_ms: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;u64&gt;, expiration_timestamp_ms: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;u64&gt;, rate_limit: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;<a href="../sui/allowance.md#sui_allowance_RateLimit">sui::allowance::RateLimit</a>&gt;, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>entry</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_new_for_app">new_for_app</a>&lt;T, A&gt;(
    name: String,
    spender: <b>address</b>,
    lifetime_cap: Option&lt;u256&gt;,
    start_timestamp_ms: Option&lt;u64&gt;,
    expiration_timestamp_ms: Option&lt;u64&gt;,
    rate_limit: Option&lt;<a href="../sui/allowance.md#sui_allowance_RateLimit">RateLimit</a>&gt;,
    ctx: &<b>mut</b> TxContext,
) {
    <a href="../sui/allowance.md#sui_allowance_share_new">share_new</a>&lt;T&gt;(
        name,
        spender,
        option::some(type_name::with_defining_ids&lt;A&gt;()),
        lifetime_cap,
        start_timestamp_ms,
        expiration_timestamp_ms,
        rate_limit,
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
    <b>assert</b>!(self.app.is_none(), <a href="../sui/allowance.md#sui_allowance_EHasApp">EHasApp</a>);
    <a href="../sui/balance.md#sui_balance_redeem_funds">balance::redeem_funds</a>(self.<a href="../sui/allowance.md#sui_allowance_consume">consume</a>(w, <a href="../sui/clock.md#sui_clock">clock</a>, ctx))
}
</code></pre>



</details>

<a name="sui_allowance_spend_balance_as_app"></a>

## Function `spend_balance_as_app`

App path: authorized by <code><a href="../sui/allowance.md#sui_allowance_Permit">Permit</a>&lt;A&gt;</code> (matching the allowance's <code>app</code>); the
tx must still come from the spender.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_spend_balance_as_app">spend_balance_as_app</a>&lt;C, A&gt;(self: &<b>mut</b> <a href="../sui/allowance.md#sui_allowance_Allowance">sui::allowance::Allowance</a>&lt;<a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;C&gt;&gt;, _: <a href="../sui/allowance.md#sui_allowance_Permit">sui::allowance::Permit</a>&lt;A&gt;, w: <a href="../sui/allowance.md#sui_allowance_AllowanceWithdrawal">sui::allowance::AllowanceWithdrawal</a>&lt;<a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;C&gt;&gt;, <a href="../sui/clock.md#sui_clock">clock</a>: &<a href="../sui/clock.md#sui_clock_Clock">sui::clock::Clock</a>, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;C&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_spend_balance_as_app">spend_balance_as_app</a>&lt;C, A&gt;(
    self: &<b>mut</b> <a href="../sui/allowance.md#sui_allowance_Allowance">Allowance</a>&lt;Balance&lt;C&gt;&gt;,
    _: <a href="../sui/allowance.md#sui_allowance_Permit">Permit</a>&lt;A&gt;,
    w: <a href="../sui/allowance.md#sui_allowance_AllowanceWithdrawal">AllowanceWithdrawal</a>&lt;Balance&lt;C&gt;&gt;,
    <a href="../sui/clock.md#sui_clock">clock</a>: &Clock,
    ctx: &TxContext,
): Balance&lt;C&gt; {
    self.<a href="../sui/allowance.md#sui_allowance_assert_app">assert_app</a>&lt;Balance&lt;C&gt;, A&gt;();
    <a href="../sui/balance.md#sui_balance_redeem_funds">balance::redeem_funds</a>(self.<a href="../sui/allowance.md#sui_allowance_consume">consume</a>(w, <a href="../sui/clock.md#sui_clock">clock</a>, ctx))
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

<a name="sui_allowance_assert_app"></a>

## Function `assert_app`



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

Policy checks + accounting shared by all spend paths. The spender gate
lives here; the app / no-app split stays with the callers.


<pre><code><b>fun</b> <a href="../sui/allowance.md#sui_allowance_consume">consume</a>&lt;T: store&gt;(self: &<b>mut</b> <a href="../sui/allowance.md#sui_allowance_Allowance">sui::allowance::Allowance</a>&lt;T&gt;, w: <a href="../sui/allowance.md#sui_allowance_AllowanceWithdrawal">sui::allowance::AllowanceWithdrawal</a>&lt;T&gt;, <a href="../sui/clock.md#sui_clock">clock</a>: &<a href="../sui/clock.md#sui_clock_Clock">sui::clock::Clock</a>, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <a href="../sui/funds_accumulator.md#sui_funds_accumulator_Withdrawal">sui::funds_accumulator::Withdrawal</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/allowance.md#sui_allowance_consume">consume</a>&lt;T: store&gt;(
    self: &<b>mut</b> <a href="../sui/allowance.md#sui_allowance_Allowance">Allowance</a>&lt;T&gt;,
    w: <a href="../sui/allowance.md#sui_allowance_AllowanceWithdrawal">AllowanceWithdrawal</a>&lt;T&gt;,
    <a href="../sui/clock.md#sui_clock">clock</a>: &Clock,
    ctx: &TxContext,
): Withdrawal&lt;T&gt; {
    <b>let</b> <a href="../sui/allowance.md#sui_allowance_AllowanceWithdrawal">AllowanceWithdrawal</a> { <a href="../sui/allowance.md#sui_allowance">allowance</a>, inner } = w;
    <b>assert</b>!(<a href="../sui/allowance.md#sui_allowance">allowance</a> == self.id.to_inner(), <a href="../sui/allowance.md#sui_allowance_EWrongAllowance">EWrongAllowance</a>);
    <b>assert</b>!(self.spender.contains(&ctx.sender()), <a href="../sui/allowance.md#sui_allowance_ENotSpender">ENotSpender</a>);
    // Signing already verified the funder; defense in depth against a core bug.
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
    self
        .rate_limit
        .do_mut!(
            |rl| match (rl) {
                RateLimit::FixedWindow { period_ms, limit, spent, window_start_ms } =&gt; {
                    // Roll forward by whole periods only, keeping the epoch grid.
                    <b>if</b> (now - *window_start_ms &gt;= *period_ms) {
                        <b>let</b> elapsed_periods = (now - *window_start_ms) / *period_ms;
                        *window_start_ms = *window_start_ms + elapsed_periods * *period_ms;
                        *spent = 0;
                    };
                    <b>assert</b>!(*spent + amount &lt;= *limit, <a href="../sui/allowance.md#sui_allowance_EExceedsRateLimit">EExceedsRateLimit</a>);
                    *spent = *spent + amount;
                },
                RateLimit::CalendarWindow { months, limit, spent, first_charge_ms, window } =&gt; {
                    <b>if</b> (*first_charge_ms == 0) {
                        // The first successful charge anchors the windows; an
                        // aborted spend unwinds the stamp.
                        *first_charge_ms = now;
                    } <b>else</b> {
                        <b>let</b> k = <a href="../sui/allowance.md#sui_allowance_elapsed_windows">elapsed_windows</a>(*first_charge_ms, now, *months);
                        <b>if</b> (k &gt; *window) {
                            *window = k;
                            *spent = 0;
                        };
                    };
                    <b>assert</b>!(*spent + amount &lt;= *limit, <a href="../sui/allowance.md#sui_allowance_EExceedsRateLimit">EExceedsRateLimit</a>);
                    *spent = *spent + amount;
                },
            },
        );
    inner
}
</code></pre>



</details>

<a name="sui_allowance_elapsed_windows"></a>

## Function `elapsed_windows`

How many whole <code>months</code>-month windows have elapsed since <code>anchor_ms</code> —
equivalently, the number of the window <code>now_ms</code> falls in (0 = first).


<pre><code><b>fun</b> <a href="../sui/allowance.md#sui_allowance_elapsed_windows">elapsed_windows</a>(anchor_ms: u64, now_ms: u64, months: u8): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/allowance.md#sui_allowance_elapsed_windows">elapsed_windows</a>(anchor_ms: u64, now_ms: u64, months: u8): u64 {
    <b>let</b> (anchor_year, anchor_month, anchor_day) = <a href="../sui/allowance.md#sui_allowance_civil_from_ms">civil_from_ms</a>(anchor_ms);
    <b>let</b> (year, month, day) = <a href="../sui/allowance.md#sui_allowance_civil_from_ms">civil_from_ms</a>(now_ms);
    <b>let</b> <b>mut</b> elapsed = (year * 12 + month) - (anchor_year * 12 + anchor_month);
    // A month only fully elapses once the anniversary day arrives, clamped to
    // month ends (a Jan 31 anchor renews Feb 28). Same-month spends have
    // day &gt;= anchor_day already; the elapsed &gt; 0 guard is underflow safety.
    <b>if</b> (elapsed &gt; 0 && day &lt; anchor_day.min(<a href="../sui/allowance.md#sui_allowance_days_in_month">days_in_month</a>(year, month))) {
        elapsed = elapsed - 1;
    };
    elapsed / (months <b>as</b> u64)
}
</code></pre>



</details>

<a name="sui_allowance_civil_from_ms"></a>

## Function `civil_from_ms`

Civil (year, month, day) in UTC — month and day 1-based, so the epoch is
<code>(1970, 1, 1)</code> — via Hinnant's <code>civil_from_days</code>; unsigned-only works
because chain timestamps are never pre-epoch.


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/allowance.md#sui_allowance_civil_from_ms">civil_from_ms</a>(timestamp_ms: u64): (u64, u64, u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/allowance.md#sui_allowance_civil_from_ms">civil_from_ms</a>(timestamp_ms: u64): (u64, u64, u64) {
    <b>let</b> z = timestamp_ms / <a href="../sui/allowance.md#sui_allowance_MS_PER_DAY">MS_PER_DAY</a> + 719468;
    <b>let</b> era = z / 146097;
    <b>let</b> doe = z % 146097;
    <b>let</b> yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    <b>let</b> doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    <b>let</b> mp = (5 * doy + 2) / 153;
    <b>let</b> day = doy - (153 * mp + 2) / 5 + 1;
    <b>let</b> (year, month) = <b>if</b> (mp &lt; 10) (era * 400 + yoe, mp + 3) <b>else</b> (era * 400 + yoe + 1, mp - 9);
    (year, month, day)
}
</code></pre>



</details>

<a name="sui_allowance_days_in_month"></a>

## Function `days_in_month`



<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/allowance.md#sui_allowance_days_in_month">days_in_month</a>(year: u64, month: u64): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/allowance.md#sui_allowance_days_in_month">days_in_month</a>(year: u64, month: u64): u64 {
    <b>if</b> (month == 2) {
        <b>if</b> (<a href="../sui/allowance.md#sui_allowance_is_leap_year">is_leap_year</a>(year)) 29 <b>else</b> 28
    } <b>else</b> <b>if</b> (month == 4 || month == 6 || month == 9 || month == 11) 30
    <b>else</b> 31
}
</code></pre>



</details>

<a name="sui_allowance_is_leap_year"></a>

## Function `is_leap_year`

Gregorian rule: every 4th year, except centuries not divisible by 400.


<pre><code><b>fun</b> <a href="../sui/allowance.md#sui_allowance_is_leap_year">is_leap_year</a>(year: u64): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/allowance.md#sui_allowance_is_leap_year">is_leap_year</a>(year: u64): bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
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
    <b>assert</b>!(<a href="../sui/protocol_config.md#sui_protocol_config_is_feature_enabled">sui::protocol_config::is_feature_enabled</a>(b"enable_allowances"), <a href="../sui/allowance.md#sui_allowance_ENotEnabled">ENotEnabled</a>);
    // we do not allow unlimited allowances (TODO: Do we?)
    <b>assert</b>!(lifetime_cap.is_some() || rate_limit.is_some(), <a href="../sui/allowance.md#sui_allowance_ENoLimit">ENoLimit</a>);
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
