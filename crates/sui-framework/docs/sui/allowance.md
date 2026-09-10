---
title: Module `sui::allowance`
---

Native allowances enable delegated, bounded, revocable spending from an address's live balance.

A transaction declares its funding source as a (funder, allowance) pair. Signing verifies that
source against the shared <code><a href="../sui/allowance.md#sui_allowance_Allowance">Allowance</a></code> and hands the transaction an <code><a href="../sui/allowance.md#sui_allowance_AllowanceWithdrawal">AllowanceWithdrawal</a></code>.

All policy checks (rate limits, lifetime caps, expiry) are enforced by this module.


-  [Struct `AllowanceWithdrawal`](#sui_allowance_AllowanceWithdrawal)
-  [Struct `Allowance`](#sui_allowance_Allowance)
-  [Struct `Settings`](#sui_allowance_Settings)
-  [Struct `AllowanceCap`](#sui_allowance_AllowanceCap)
-  [Struct `AllowanceProposal`](#sui_allowance_AllowanceProposal)
-  [Struct `SpendPermit`](#sui_allowance_SpendPermit)
-  [Struct `SettingsPermit`](#sui_allowance_SettingsPermit)
-  [Enum `RateLimit`](#sui_allowance_RateLimit)
-  [Enum `Window`](#sui_allowance_Window)
-  [Constants](#@Constants_0)
-  [Function `spend_permit`](#sui_allowance_spend_permit)
-  [Function `settings_permit`](#sui_allowance_settings_permit)
-  [Function `periodic_rate_limit`](#sui_allowance_periodic_rate_limit)
-  [Function `calendar_rate_limit`](#sui_allowance_calendar_rate_limit)
-  [Function `monthly_rate_limit`](#sui_allowance_monthly_rate_limit)
-  [Function `quarterly_rate_limit`](#sui_allowance_quarterly_rate_limit)
-  [Function `yearly_rate_limit`](#sui_allowance_yearly_rate_limit)
-  [Function `new`](#sui_allowance_new)
-  [Function `propose_for_app`](#sui_allowance_propose_for_app)
-  [Function `issue`](#sui_allowance_issue)
-  [Function `balance_spend`](#sui_allowance_balance_spend)
-  [Function `app_balance_spend`](#sui_allowance_app_balance_spend)
-  [Function `revoke`](#sui_allowance_revoke)
-  [Function `rotate_spender`](#sui_allowance_rotate_spender)
-  [Function `allowance_settings`](#sui_allowance_allowance_settings)
-  [Function `allowance_current_spend`](#sui_allowance_allowance_current_spend)
-  [Function `allowance_cap_allowance`](#sui_allowance_allowance_cap_allowance)
-  [Function `allowance_proposal_settings`](#sui_allowance_allowance_proposal_settings)
-  [Function `funder`](#sui_allowance_funder)
-  [Function `spender`](#sui_allowance_spender)
-  [Function `app`](#sui_allowance_app)
-  [Function `lifetime_cap`](#sui_allowance_lifetime_cap)
-  [Function `start_timestamp_ms`](#sui_allowance_start_timestamp_ms)
-  [Function `expiration_timestamp_ms`](#sui_allowance_expiration_timestamp_ms)
-  [Function `rate_limit`](#sui_allowance_rate_limit)
-  [Function `name`](#sui_allowance_name)
-  [Function `rate_limit_limit`](#sui_allowance_rate_limit_limit)
-  [Function `rate_limit_spent`](#sui_allowance_rate_limit_spent)
-  [Function `rate_limit_window`](#sui_allowance_rate_limit_window)
-  [Function `civil_from_ms`](#sui_allowance_civil_from_ms)
-  [Function `days_in_month`](#sui_allowance_days_in_month)
-  [Function `assert_app`](#sui_allowance_assert_app)
-  [Function `consume`](#sui_allowance_consume)
-  [Function `new_rate_limit`](#sui_allowance_new_rate_limit)
-  [Function `charge`](#sui_allowance_charge)
-  [Function `index_at`](#sui_allowance_index_at)
-  [Function `elapsed_windows`](#sui_allowance_elapsed_windows)
-  [Function `is_leap_year`](#sui_allowance_is_leap_year)
-  [Function `new_settings`](#sui_allowance_new_settings)
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

Created via a PTB Argument.

The inner <code>Withdrawal</code> is contained within this module and cannot be accessed directly.
An allowance's limits can never be charged without the funds actually moving.


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
<code>is_sponsor: bool</code>
</dt>
<dd>
 Today is always false. Opens the door for future <code>ctx.sponsor()</code> based allowances.
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

Enables withdrawing <code>T</code> from the funder's balance, within this allowance's limits.
Always kept as a shared object.


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
<code>settings: <a href="../sui/allowance.md#sui_allowance_Settings">sui::allowance::Settings</a></code>
</dt>
<dd>
</dd>
<dt>
<code>current_spend: u256</code>
</dt>
<dd>
 Total cumulative spend from this allowance.
</dd>
</dl>


</details>

<a name="sui_allowance_Settings"></a>

## Struct `Settings`

Configuration of the Allowance, held by <code><a href="../sui/allowance.md#sui_allowance_Allowance">Allowance</a></code> and <code><a href="../sui/allowance.md#sui_allowance_AllowanceProposal">AllowanceProposal</a></code>


<pre><code><b>public</b> <b>struct</b> <a href="../sui/allowance.md#sui_allowance_Settings">Settings</a> <b>has</b> drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code><a href="../sui/allowance.md#sui_allowance_funder">funder</a>: <b>address</b></code>
</dt>
<dd>
 The address whose balance is debited by spends against this allowance.
</dd>
<dt>
<code><a href="../sui/allowance.md#sui_allowance_spender">spender</a>: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;<b>address</b>&gt;</code>
</dt>
<dd>
 The spender of the allowance.
 While it is currently always set, in the future this may become optional
 to allow for keyless app-bound withdrawals.
</dd>
<dt>
<code><a href="../sui/allowance.md#sui_allowance_app">app</a>: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;<a href="../std/type_name.md#std_type_name_TypeName">std::type_name::TypeName</a>&gt;</code>
</dt>
<dd>
 When set, requires the app's <code><a href="../sui/allowance.md#sui_allowance_SpendPermit">SpendPermit</a></code> to spend, and only that app
 can rotate the spender or issue the allowance in the first place.
</dd>
<dt>
<code><a href="../sui/allowance.md#sui_allowance_lifetime_cap">lifetime_cap</a>: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;u256&gt;</code>
</dt>
<dd>
 An optional lifetime cap on withdrawals using this allowance. Inclusive.
 Amounts are <code>u256</code>, matching <code>Withdrawal.limit</code>.
</dd>
<dt>
<code><a href="../sui/allowance.md#sui_allowance_start_timestamp_ms">start_timestamp_ms</a>: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;u64&gt;</code>
</dt>
<dd>
 Optional activation time, in milliseconds. Inclusive.
</dd>
<dt>
<code><a href="../sui/allowance.md#sui_allowance_expiration_timestamp_ms">expiration_timestamp_ms</a>: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;u64&gt;</code>
</dt>
<dd>
 Optional expiration time, in milliseconds. Exclusive.
</dd>
<dt>
<code><a href="../sui/allowance.md#sui_allowance_rate_limit">rate_limit</a>: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;<a href="../sui/allowance.md#sui_allowance_RateLimit">sui::allowance::RateLimit</a>&gt;</code>
</dt>
<dd>
 An optional recurring limit, applied on top of <code><a href="../sui/allowance.md#sui_allowance_lifetime_cap">lifetime_cap</a></code>. At least one of the two
 must be set.
</dd>
<dt>
<code><a href="../sui/allowance.md#sui_allowance_name">name</a>: <a href="../std/string.md#std_string_String">std::string::String</a></code>
</dt>
<dd>
 A label for off-chain use, never read by any check. At most 128 bytes.
</dd>
</dl>


</details>

<a name="sui_allowance_AllowanceCap"></a>

## Struct `AllowanceCap`

Revocation for an allowance, sent to the funder at issuance (soulbound).
Also used for discoverability (funder -> allowances).

Created with the allowance and destroyed with it by <code><a href="../sui/allowance.md#sui_allowance_revoke">revoke</a></code>.


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

<a name="sui_allowance_AllowanceProposal"></a>

## Struct `AllowanceProposal`

A proposal that can only be issued by app <code>A</code>.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/allowance.md#sui_allowance_AllowanceProposal">AllowanceProposal</a>&lt;<b>phantom</b> T&gt; <b>has</b> drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>0: <a href="../sui/allowance.md#sui_allowance_Settings">sui::allowance::Settings</a></code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="sui_allowance_SpendPermit"></a>

## Struct `SpendPermit`

A <code><a href="../sui/allowance.md#sui_allowance_SpendPermit">SpendPermit</a>&lt;A&gt;</code> authorizes a single spend against an allowance bound to
<code>A</code>. It is issued from an <code>internal::Permit&lt;A&gt;</code>, allowing the module that
defines <code>A</code> to gate every withdrawal on its own logic.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/allowance.md#sui_allowance_SpendPermit">SpendPermit</a>&lt;<b>phantom</b> A&gt; <b>has</b> drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
</dl>


</details>

<a name="sui_allowance_SettingsPermit"></a>

## Struct `SettingsPermit`

A <code><a href="../sui/allowance.md#sui_allowance_SettingsPermit">SettingsPermit</a>&lt;A&gt;</code> authorizes changing the configuration of an allowance bound to <code>A</code>:
issuing one, or rotating its spender. It is issued from an <code>internal::Permit&lt;A&gt;</code>.

Kept distinct from <code><a href="../sui/allowance.md#sui_allowance_SpendPermit">SpendPermit</a></code> so an app can hand out the right to spend without also
handing out the right to reconfigure, and vice versa.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/allowance.md#sui_allowance_SettingsPermit">SettingsPermit</a>&lt;<b>phantom</b> A&gt; <b>has</b> drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
</dl>


</details>

<a name="sui_allowance_RateLimit"></a>

## Enum `RateLimit`

A recurring cap on withdrawals, applied on top of any lifetime cap.

An enum so other mechanics, like sliding windows, can be added as variants.


<pre><code><b>public</b> <b>enum</b> <a href="../sui/allowance.md#sui_allowance_RateLimit">RateLimit</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Variants</summary>


<dl>
<dt>
Variant <code>Windowed</code>
</dt>
<dd>
 At most <code>limit</code> per window. Windows only roll forward.
</dd>

<dl>
<dt>
<code>limit: u256</code>
</dt>
<dd>
 The most that can be spent within a window. Inclusive.
</dd>
</dl>


<dl>
<dt>
<code>spent: u256</code>
</dt>
<dd>
 Amount spent so far within the current window.
</dd>
</dl>


<dl>
<dt>
<code>anchor_ms: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;u64&gt;</code>
</dt>
<dd>
 Start of the first window, stamped by the first successful charge.
</dd>
</dl>


<dl>
<dt>
<code>index: u64</code>
</dt>
<dd>
 Which window <code>spent</code> is accumulated in, numbered from the anchor (0 = first).
 A spend landing in a later one resets <code>spent</code>.
</dd>
</dl>


<dl>
<dt>
<code>window: <a href="../sui/allowance.md#sui_allowance_Window">sui::allowance::Window</a></code>
</dt>
<dd>
 The defining period of time for this rate limit.
</dd>
</dl>

</dl>


</details>

<a name="sui_allowance_Window"></a>

## Enum `Window`

The defining time period for a <code>RateLimit::Windowed</code>.


<pre><code><b>public</b> <b>enum</b> <a href="../sui/allowance.md#sui_allowance_Window">Window</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Variants</summary>


<dl>
<dt>
Variant <code>PeriodicMs</code>
</dt>
<dd>
 Windows of exactly this many milliseconds.
</dd>

<dl>
<dt>
<code>0: u64</code>
</dt>
<dd>
</dd>
</dl>

<dt>
Variant <code>CalendarMonths</code>
</dt>
<dd>
 Windows of this many civil (UTC) months.
</dd>

<dl>
<dt>
<code>0: u8</code>
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
<b>const</b> <a href="../sui/allowance.md#sui_allowance_ENotSpender">ENotSpender</a>: vector&lt;u8&gt; = b"Transaction sender is not this <a href="../sui/allowance.md#sui_allowance">allowance</a>'s <a href="../sui/allowance.md#sui_allowance_spender">spender</a>";
</code></pre>



<a name="sui_allowance_EWrongApp"></a>



<pre><code>#[error]
<b>const</b> <a href="../sui/allowance.md#sui_allowance_EWrongApp">EWrongApp</a>: vector&lt;u8&gt; = b"<a href="../sui/allowance.md#sui_allowance_Allowance">Allowance</a> is not bound to this <a href="../sui/allowance.md#sui_allowance_app">app</a>";
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
<b>const</b> <a href="../sui/allowance.md#sui_allowance_EHasApp">EHasApp</a>: vector&lt;u8&gt; = b"App-bound <a href="../sui/allowance.md#sui_allowance">allowance</a>: spend through `<a href="../sui/allowance.md#sui_allowance_app_balance_spend">app_balance_spend</a>`";
</code></pre>



<a name="sui_allowance_EWrongFunder"></a>



<pre><code>#[error]
<b>const</b> <a href="../sui/allowance.md#sui_allowance_EWrongFunder">EWrongFunder</a>: vector&lt;u8&gt; = b"Withdrawal debits a different <b>address</b> than this <a href="../sui/allowance.md#sui_allowance">allowance</a>'s <a href="../sui/allowance.md#sui_allowance_funder">funder</a>";
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



<a name="sui_allowance_ESponsorWithdrawalNotEnabled"></a>



<pre><code>#[error]
<b>const</b> <a href="../sui/allowance.md#sui_allowance_ESponsorWithdrawalNotEnabled">ESponsorWithdrawalNotEnabled</a>: vector&lt;u8&gt; = b"Sponsor <a href="../sui/allowance.md#sui_allowance">allowance</a> withdrawals are not enabled";
</code></pre>



<a name="sui_allowance_MAX_NAME_LENGTH"></a>



<pre><code><b>const</b> <a href="../sui/allowance.md#sui_allowance_MAX_NAME_LENGTH">MAX_NAME_LENGTH</a>: u64 = 128;
</code></pre>



<a name="sui_allowance_MS_PER_DAY"></a>



<pre><code><b>const</b> <a href="../sui/allowance.md#sui_allowance_MS_PER_DAY">MS_PER_DAY</a>: u64 = 86400000;
</code></pre>



<a name="sui_allowance_spend_permit"></a>

## Function `spend_permit`

Issues a <code><a href="../sui/allowance.md#sui_allowance_SpendPermit">SpendPermit</a>&lt;A&gt;</code> from the privileged <code>internal::Permit&lt;A&gt;</code>.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_spend_permit">spend_permit</a>&lt;A&gt;(_: <a href="../std/internal.md#std_internal_Permit">std::internal::Permit</a>&lt;A&gt;): <a href="../sui/allowance.md#sui_allowance_SpendPermit">sui::allowance::SpendPermit</a>&lt;A&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_spend_permit">spend_permit</a>&lt;A&gt;(_: internal::Permit&lt;A&gt;): <a href="../sui/allowance.md#sui_allowance_SpendPermit">SpendPermit</a>&lt;A&gt; {
    <a href="../sui/allowance.md#sui_allowance_SpendPermit">SpendPermit</a>()
}
</code></pre>



</details>

<a name="sui_allowance_settings_permit"></a>

## Function `settings_permit`

Issues a <code><a href="../sui/allowance.md#sui_allowance_SettingsPermit">SettingsPermit</a>&lt;A&gt;</code> from the privileged <code>internal::Permit&lt;A&gt;</code>.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_settings_permit">settings_permit</a>&lt;A&gt;(_: <a href="../std/internal.md#std_internal_Permit">std::internal::Permit</a>&lt;A&gt;): <a href="../sui/allowance.md#sui_allowance_SettingsPermit">sui::allowance::SettingsPermit</a>&lt;A&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_settings_permit">settings_permit</a>&lt;A&gt;(_: internal::Permit&lt;A&gt;): <a href="../sui/allowance.md#sui_allowance_SettingsPermit">SettingsPermit</a>&lt;A&gt; {
    <a href="../sui/allowance.md#sui_allowance_SettingsPermit">SettingsPermit</a>()
}
</code></pre>



</details>

<a name="sui_allowance_periodic_rate_limit"></a>

## Function `periodic_rate_limit`

At most <code>limit</code> per <code>period_ms</code>, counted from the first charge.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_periodic_rate_limit">periodic_rate_limit</a>(period_ms: u64, limit: u256): <a href="../sui/allowance.md#sui_allowance_RateLimit">sui::allowance::RateLimit</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_periodic_rate_limit">periodic_rate_limit</a>(period_ms: u64, limit: u256): <a href="../sui/allowance.md#sui_allowance_RateLimit">RateLimit</a> {
    // A zero period resets the window on every spend; a zero limit spends nothing.
    <b>assert</b>!(period_ms &gt; 0 && limit &gt; 0, <a href="../sui/allowance.md#sui_allowance_EBadRateLimit">EBadRateLimit</a>);
    <a href="../sui/allowance.md#sui_allowance_new_rate_limit">new_rate_limit</a>(limit, Window::PeriodicMs(period_ms))
}
</code></pre>



</details>

<a name="sui_allowance_calendar_rate_limit"></a>

## Function `calendar_rate_limit`

At most <code>limit</code> per <code>months</code> civil (UTC) months, counted from the first charge. Windows renew
on the anchor's day-of-month at 00:00 UTC, clamped to shorter months (a Jan 31 anchor renews
Feb 28, then Mar 31).


<pre><code><b>public</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_calendar_rate_limit">calendar_rate_limit</a>(months: u8, limit: u256): <a href="../sui/allowance.md#sui_allowance_RateLimit">sui::allowance::RateLimit</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_calendar_rate_limit">calendar_rate_limit</a>(months: u8, limit: u256): <a href="../sui/allowance.md#sui_allowance_RateLimit">RateLimit</a> {
    <b>assert</b>!(months &gt; 0 && limit &gt; 0, <a href="../sui/allowance.md#sui_allowance_EBadRateLimit">EBadRateLimit</a>);
    <a href="../sui/allowance.md#sui_allowance_new_rate_limit">new_rate_limit</a>(limit, Window::CalendarMonths(months))
}
</code></pre>



</details>

<a name="sui_allowance_monthly_rate_limit"></a>

## Function `monthly_rate_limit`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_monthly_rate_limit">monthly_rate_limit</a>(limit: u256): <a href="../sui/allowance.md#sui_allowance_RateLimit">sui::allowance::RateLimit</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_monthly_rate_limit">monthly_rate_limit</a>(limit: u256): <a href="../sui/allowance.md#sui_allowance_RateLimit">RateLimit</a> { <a href="../sui/allowance.md#sui_allowance_calendar_rate_limit">calendar_rate_limit</a>(1, limit) }
</code></pre>



</details>

<a name="sui_allowance_quarterly_rate_limit"></a>

## Function `quarterly_rate_limit`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_quarterly_rate_limit">quarterly_rate_limit</a>(limit: u256): <a href="../sui/allowance.md#sui_allowance_RateLimit">sui::allowance::RateLimit</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_quarterly_rate_limit">quarterly_rate_limit</a>(limit: u256): <a href="../sui/allowance.md#sui_allowance_RateLimit">RateLimit</a> { <a href="../sui/allowance.md#sui_allowance_calendar_rate_limit">calendar_rate_limit</a>(3, limit) }
</code></pre>



</details>

<a name="sui_allowance_yearly_rate_limit"></a>

## Function `yearly_rate_limit`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_yearly_rate_limit">yearly_rate_limit</a>(limit: u256): <a href="../sui/allowance.md#sui_allowance_RateLimit">sui::allowance::RateLimit</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_yearly_rate_limit">yearly_rate_limit</a>(limit: u256): <a href="../sui/allowance.md#sui_allowance_RateLimit">RateLimit</a> { <a href="../sui/allowance.md#sui_allowance_calendar_rate_limit">calendar_rate_limit</a>(12, limit) }
</code></pre>



</details>

<a name="sui_allowance_new"></a>

## Function `new`

Issues an allowance funded by the sender, sharing it and sending its <code><a href="../sui/allowance.md#sui_allowance_AllowanceCap">AllowanceCap</a></code> to the
sender. Creation is <code><b>entry</b></code> so contracts cannot create allowances implicitly.


<pre><code><b>entry</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_new">new</a>&lt;T&gt;(<a href="../sui/allowance.md#sui_allowance_name">name</a>: <a href="../std/string.md#std_string_String">std::string::String</a>, <a href="../sui/allowance.md#sui_allowance_spender">spender</a>: <b>address</b>, <a href="../sui/allowance.md#sui_allowance_lifetime_cap">lifetime_cap</a>: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;u256&gt;, <a href="../sui/allowance.md#sui_allowance_start_timestamp_ms">start_timestamp_ms</a>: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;u64&gt;, <a href="../sui/allowance.md#sui_allowance_expiration_timestamp_ms">expiration_timestamp_ms</a>: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;u64&gt;, <a href="../sui/allowance.md#sui_allowance_rate_limit">rate_limit</a>: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;<a href="../sui/allowance.md#sui_allowance_RateLimit">sui::allowance::RateLimit</a>&gt;, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>entry</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_new">new</a>&lt;T&gt;(
    <a href="../sui/allowance.md#sui_allowance_name">name</a>: String,
    <a href="../sui/allowance.md#sui_allowance_spender">spender</a>: <b>address</b>,
    <a href="../sui/allowance.md#sui_allowance_lifetime_cap">lifetime_cap</a>: Option&lt;u256&gt;,
    <a href="../sui/allowance.md#sui_allowance_start_timestamp_ms">start_timestamp_ms</a>: Option&lt;u64&gt;,
    <a href="../sui/allowance.md#sui_allowance_expiration_timestamp_ms">expiration_timestamp_ms</a>: Option&lt;u64&gt;,
    <a href="../sui/allowance.md#sui_allowance_rate_limit">rate_limit</a>: Option&lt;<a href="../sui/allowance.md#sui_allowance_RateLimit">RateLimit</a>&gt;,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> settings = <a href="../sui/allowance.md#sui_allowance_new_settings">new_settings</a>(
        ctx.sender(),
        <a href="../sui/allowance.md#sui_allowance_spender">spender</a>,
        option::none(),
        <a href="../sui/allowance.md#sui_allowance_lifetime_cap">lifetime_cap</a>,
        <a href="../sui/allowance.md#sui_allowance_start_timestamp_ms">start_timestamp_ms</a>,
        <a href="../sui/allowance.md#sui_allowance_expiration_timestamp_ms">expiration_timestamp_ms</a>,
        <a href="../sui/allowance.md#sui_allowance_rate_limit">rate_limit</a>,
        <a href="../sui/allowance.md#sui_allowance_name">name</a>,
    );
    <a href="../sui/allowance.md#sui_allowance_share_new">share_new</a>&lt;T&gt;(settings, ctx);
}
</code></pre>



</details>

<a name="sui_allowance_propose_for_app"></a>

## Function `propose_for_app`

Returns an <code><a href="../sui/allowance.md#sui_allowance_AllowanceProposal">AllowanceProposal</a></code> for an allowance bound to the controlling app <code>A</code>, funded by
the sender.

Unlike <code><a href="../sui/allowance.md#sui_allowance_new">new</a></code>, this creates no allowance on its own. <code>A</code>'s module must accept the proposal via
<code><a href="../sui/allowance.md#sui_allowance_issue">issue</a></code>, giving the app a say in every allowance that names it.


<pre><code><b>entry</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_propose_for_app">propose_for_app</a>&lt;T, A&gt;(<a href="../sui/allowance.md#sui_allowance_name">name</a>: <a href="../std/string.md#std_string_String">std::string::String</a>, <a href="../sui/allowance.md#sui_allowance_spender">spender</a>: <b>address</b>, <a href="../sui/allowance.md#sui_allowance_lifetime_cap">lifetime_cap</a>: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;u256&gt;, <a href="../sui/allowance.md#sui_allowance_start_timestamp_ms">start_timestamp_ms</a>: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;u64&gt;, <a href="../sui/allowance.md#sui_allowance_expiration_timestamp_ms">expiration_timestamp_ms</a>: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;u64&gt;, <a href="../sui/allowance.md#sui_allowance_rate_limit">rate_limit</a>: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;<a href="../sui/allowance.md#sui_allowance_RateLimit">sui::allowance::RateLimit</a>&gt;, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <a href="../sui/allowance.md#sui_allowance_AllowanceProposal">sui::allowance::AllowanceProposal</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>entry</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_propose_for_app">propose_for_app</a>&lt;T, A&gt;(
    <a href="../sui/allowance.md#sui_allowance_name">name</a>: String,
    <a href="../sui/allowance.md#sui_allowance_spender">spender</a>: <b>address</b>,
    <a href="../sui/allowance.md#sui_allowance_lifetime_cap">lifetime_cap</a>: Option&lt;u256&gt;,
    <a href="../sui/allowance.md#sui_allowance_start_timestamp_ms">start_timestamp_ms</a>: Option&lt;u64&gt;,
    <a href="../sui/allowance.md#sui_allowance_expiration_timestamp_ms">expiration_timestamp_ms</a>: Option&lt;u64&gt;,
    <a href="../sui/allowance.md#sui_allowance_rate_limit">rate_limit</a>: Option&lt;<a href="../sui/allowance.md#sui_allowance_RateLimit">RateLimit</a>&gt;,
    ctx: &TxContext,
): <a href="../sui/allowance.md#sui_allowance_AllowanceProposal">AllowanceProposal</a>&lt;T&gt; {
    <b>let</b> settings = <a href="../sui/allowance.md#sui_allowance_new_settings">new_settings</a>(
        ctx.sender(),
        <a href="../sui/allowance.md#sui_allowance_spender">spender</a>,
        option::some(type_name::with_defining_ids&lt;A&gt;()),
        <a href="../sui/allowance.md#sui_allowance_lifetime_cap">lifetime_cap</a>,
        <a href="../sui/allowance.md#sui_allowance_start_timestamp_ms">start_timestamp_ms</a>,
        <a href="../sui/allowance.md#sui_allowance_expiration_timestamp_ms">expiration_timestamp_ms</a>,
        <a href="../sui/allowance.md#sui_allowance_rate_limit">rate_limit</a>,
        <a href="../sui/allowance.md#sui_allowance_name">name</a>,
    );
    <a href="../sui/allowance.md#sui_allowance_AllowanceProposal">AllowanceProposal</a>(settings)
}
</code></pre>



</details>

<a name="sui_allowance_issue"></a>

## Function `issue`

Issues the proposed allowance on <code>A</code>'s behalf, creating and sharing it.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_issue">issue</a>&lt;T, A&gt;(proposal: <a href="../sui/allowance.md#sui_allowance_AllowanceProposal">sui::allowance::AllowanceProposal</a>&lt;T&gt;, _: <a href="../sui/allowance.md#sui_allowance_SettingsPermit">sui::allowance::SettingsPermit</a>&lt;A&gt;, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_issue">issue</a>&lt;T, A&gt;(proposal: <a href="../sui/allowance.md#sui_allowance_AllowanceProposal">AllowanceProposal</a>&lt;T&gt;, _: <a href="../sui/allowance.md#sui_allowance_SettingsPermit">SettingsPermit</a>&lt;A&gt;, ctx: &<b>mut</b> TxContext) {
    <b>let</b> <a href="../sui/allowance.md#sui_allowance_AllowanceProposal">AllowanceProposal</a>(settings) = proposal;
    <b>assert</b>!(settings.<a href="../sui/allowance.md#sui_allowance_app">app</a>.contains(&type_name::with_defining_ids&lt;A&gt;()), <a href="../sui/allowance.md#sui_allowance_EWrongApp">EWrongApp</a>);
    <a href="../sui/allowance.md#sui_allowance_share_new">share_new</a>&lt;T&gt;(settings, ctx);
}
</code></pre>



</details>

<a name="sui_allowance_balance_spend"></a>

## Function `balance_spend`

Signer path: the tx sender must be the spender.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_balance_spend">balance_spend</a>&lt;C&gt;(self: &<b>mut</b> <a href="../sui/allowance.md#sui_allowance_Allowance">sui::allowance::Allowance</a>&lt;<a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;C&gt;&gt;, w: <a href="../sui/allowance.md#sui_allowance_AllowanceWithdrawal">sui::allowance::AllowanceWithdrawal</a>&lt;<a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;C&gt;&gt;, <a href="../sui/clock.md#sui_clock">clock</a>: &<a href="../sui/clock.md#sui_clock_Clock">sui::clock::Clock</a>, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;C&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_balance_spend">balance_spend</a>&lt;C&gt;(
    self: &<b>mut</b> <a href="../sui/allowance.md#sui_allowance_Allowance">Allowance</a>&lt;Balance&lt;C&gt;&gt;,
    w: <a href="../sui/allowance.md#sui_allowance_AllowanceWithdrawal">AllowanceWithdrawal</a>&lt;Balance&lt;C&gt;&gt;,
    <a href="../sui/clock.md#sui_clock">clock</a>: &Clock,
    ctx: &TxContext,
): Balance&lt;C&gt; {
    <b>assert</b>!(self.settings.<a href="../sui/allowance.md#sui_allowance_app">app</a>.is_none(), <a href="../sui/allowance.md#sui_allowance_EHasApp">EHasApp</a>);
    <a href="../sui/balance.md#sui_balance_redeem_funds">balance::redeem_funds</a>(self.<a href="../sui/allowance.md#sui_allowance_consume">consume</a>(w, <a href="../sui/clock.md#sui_clock">clock</a>, ctx))
}
</code></pre>



</details>

<a name="sui_allowance_app_balance_spend"></a>

## Function `app_balance_spend`

App path: requires a <code><a href="../sui/allowance.md#sui_allowance_SpendPermit">SpendPermit</a>&lt;A&gt;</code> matching the allowance's <code><a href="../sui/allowance.md#sui_allowance_app">app</a></code>. The tx must still come
from the spender.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_app_balance_spend">app_balance_spend</a>&lt;C, A&gt;(self: &<b>mut</b> <a href="../sui/allowance.md#sui_allowance_Allowance">sui::allowance::Allowance</a>&lt;<a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;C&gt;&gt;, _: <a href="../sui/allowance.md#sui_allowance_SpendPermit">sui::allowance::SpendPermit</a>&lt;A&gt;, w: <a href="../sui/allowance.md#sui_allowance_AllowanceWithdrawal">sui::allowance::AllowanceWithdrawal</a>&lt;<a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;C&gt;&gt;, <a href="../sui/clock.md#sui_clock">clock</a>: &<a href="../sui/clock.md#sui_clock_Clock">sui::clock::Clock</a>, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;C&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_app_balance_spend">app_balance_spend</a>&lt;C, A&gt;(
    self: &<b>mut</b> <a href="../sui/allowance.md#sui_allowance_Allowance">Allowance</a>&lt;Balance&lt;C&gt;&gt;,
    _: <a href="../sui/allowance.md#sui_allowance_SpendPermit">SpendPermit</a>&lt;A&gt;,
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

Revokes an allowance, removing the ability to spend.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_revoke">revoke</a>&lt;T&gt;(self: <a href="../sui/allowance.md#sui_allowance_AllowanceCap">sui::allowance::AllowanceCap</a>&lt;T&gt;, <a href="../sui/allowance.md#sui_allowance">allowance</a>: <a href="../sui/allowance.md#sui_allowance_Allowance">sui::allowance::Allowance</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_revoke">revoke</a>&lt;T&gt;(self: <a href="../sui/allowance.md#sui_allowance_AllowanceCap">AllowanceCap</a>&lt;T&gt;, <a href="../sui/allowance.md#sui_allowance">allowance</a>: <a href="../sui/allowance.md#sui_allowance_Allowance">Allowance</a>&lt;T&gt;) {
    <b>let</b> <a href="../sui/allowance.md#sui_allowance_AllowanceCap">AllowanceCap</a> { id: cap_id, <a href="../sui/allowance.md#sui_allowance">allowance</a>: cap_allowance } = self;
    <b>assert</b>!(cap_allowance == <a href="../sui/allowance.md#sui_allowance">allowance</a>.id.to_inner(), <a href="../sui/allowance.md#sui_allowance_EWrongCap">EWrongCap</a>);
    <b>let</b> <a href="../sui/allowance.md#sui_allowance_Allowance">Allowance</a> { id, .. } = <a href="../sui/allowance.md#sui_allowance">allowance</a>;
    id.delete();
    cap_id.delete();
}
</code></pre>



</details>

<a name="sui_allowance_rotate_spender"></a>

## Function `rotate_spender`

Rotates the spender key without the funder reissuing the allowance.

App-only: for an app-bound allowance the app dictates who the spender is. Non-app allowances
rotate through address aliases instead.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_rotate_spender">rotate_spender</a>&lt;T, A&gt;(self: &<b>mut</b> <a href="../sui/allowance.md#sui_allowance_Allowance">sui::allowance::Allowance</a>&lt;T&gt;, _: <a href="../sui/allowance.md#sui_allowance_SettingsPermit">sui::allowance::SettingsPermit</a>&lt;A&gt;, new_spender: <b>address</b>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_rotate_spender">rotate_spender</a>&lt;T, A&gt;(
    self: &<b>mut</b> <a href="../sui/allowance.md#sui_allowance_Allowance">Allowance</a>&lt;T&gt;,
    _: <a href="../sui/allowance.md#sui_allowance_SettingsPermit">SettingsPermit</a>&lt;A&gt;,
    new_spender: <b>address</b>,
) {
    self.<a href="../sui/allowance.md#sui_allowance_assert_app">assert_app</a>&lt;T, A&gt;();
    self.settings.<a href="../sui/allowance.md#sui_allowance_spender">spender</a> = option::some(new_spender);
}
</code></pre>



</details>

<a name="sui_allowance_allowance_settings"></a>

## Function `allowance_settings`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_allowance_settings">allowance_settings</a>&lt;T&gt;(self: &<a href="../sui/allowance.md#sui_allowance_Allowance">sui::allowance::Allowance</a>&lt;T&gt;): &<a href="../sui/allowance.md#sui_allowance_Settings">sui::allowance::Settings</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_allowance_settings">allowance_settings</a>&lt;T&gt;(self: &<a href="../sui/allowance.md#sui_allowance_Allowance">Allowance</a>&lt;T&gt;): &<a href="../sui/allowance.md#sui_allowance_Settings">Settings</a> { &self.settings }
</code></pre>



</details>

<a name="sui_allowance_allowance_current_spend"></a>

## Function `allowance_current_spend`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_allowance_current_spend">allowance_current_spend</a>&lt;T&gt;(self: &<a href="../sui/allowance.md#sui_allowance_Allowance">sui::allowance::Allowance</a>&lt;T&gt;): u256
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_allowance_current_spend">allowance_current_spend</a>&lt;T&gt;(self: &<a href="../sui/allowance.md#sui_allowance_Allowance">Allowance</a>&lt;T&gt;): u256 { self.current_spend }
</code></pre>



</details>

<a name="sui_allowance_allowance_cap_allowance"></a>

## Function `allowance_cap_allowance`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_allowance_cap_allowance">allowance_cap_allowance</a>&lt;T&gt;(self: &<a href="../sui/allowance.md#sui_allowance_AllowanceCap">sui::allowance::AllowanceCap</a>&lt;T&gt;): <a href="../sui/object.md#sui_object_ID">sui::object::ID</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_allowance_cap_allowance">allowance_cap_allowance</a>&lt;T&gt;(self: &<a href="../sui/allowance.md#sui_allowance_AllowanceCap">AllowanceCap</a>&lt;T&gt;): ID { self.<a href="../sui/allowance.md#sui_allowance">allowance</a> }
</code></pre>



</details>

<a name="sui_allowance_allowance_proposal_settings"></a>

## Function `allowance_proposal_settings`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_allowance_proposal_settings">allowance_proposal_settings</a>&lt;T&gt;(self: &<a href="../sui/allowance.md#sui_allowance_AllowanceProposal">sui::allowance::AllowanceProposal</a>&lt;T&gt;): &<a href="../sui/allowance.md#sui_allowance_Settings">sui::allowance::Settings</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_allowance_proposal_settings">allowance_proposal_settings</a>&lt;T&gt;(self: &<a href="../sui/allowance.md#sui_allowance_AllowanceProposal">AllowanceProposal</a>&lt;T&gt;): &<a href="../sui/allowance.md#sui_allowance_Settings">Settings</a> { &self.0 }
</code></pre>



</details>

<a name="sui_allowance_funder"></a>

## Function `funder`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_funder">funder</a>(self: &<a href="../sui/allowance.md#sui_allowance_Settings">sui::allowance::Settings</a>): <b>address</b>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_funder">funder</a>(self: &<a href="../sui/allowance.md#sui_allowance_Settings">Settings</a>): <b>address</b> { self.<a href="../sui/allowance.md#sui_allowance_funder">funder</a> }
</code></pre>



</details>

<a name="sui_allowance_spender"></a>

## Function `spender`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_spender">spender</a>(self: &<a href="../sui/allowance.md#sui_allowance_Settings">sui::allowance::Settings</a>): <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;<b>address</b>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_spender">spender</a>(self: &<a href="../sui/allowance.md#sui_allowance_Settings">Settings</a>): Option&lt;<b>address</b>&gt; { self.<a href="../sui/allowance.md#sui_allowance_spender">spender</a> }
</code></pre>



</details>

<a name="sui_allowance_app"></a>

## Function `app`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_app">app</a>(self: &<a href="../sui/allowance.md#sui_allowance_Settings">sui::allowance::Settings</a>): <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;<a href="../std/type_name.md#std_type_name_TypeName">std::type_name::TypeName</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_app">app</a>(self: &<a href="../sui/allowance.md#sui_allowance_Settings">Settings</a>): Option&lt;TypeName&gt; { self.<a href="../sui/allowance.md#sui_allowance_app">app</a> }
</code></pre>



</details>

<a name="sui_allowance_lifetime_cap"></a>

## Function `lifetime_cap`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_lifetime_cap">lifetime_cap</a>(self: &<a href="../sui/allowance.md#sui_allowance_Settings">sui::allowance::Settings</a>): <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;u256&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_lifetime_cap">lifetime_cap</a>(self: &<a href="../sui/allowance.md#sui_allowance_Settings">Settings</a>): Option&lt;u256&gt; { self.<a href="../sui/allowance.md#sui_allowance_lifetime_cap">lifetime_cap</a> }
</code></pre>



</details>

<a name="sui_allowance_start_timestamp_ms"></a>

## Function `start_timestamp_ms`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_start_timestamp_ms">start_timestamp_ms</a>(self: &<a href="../sui/allowance.md#sui_allowance_Settings">sui::allowance::Settings</a>): <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;u64&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_start_timestamp_ms">start_timestamp_ms</a>(self: &<a href="../sui/allowance.md#sui_allowance_Settings">Settings</a>): Option&lt;u64&gt; { self.<a href="../sui/allowance.md#sui_allowance_start_timestamp_ms">start_timestamp_ms</a> }
</code></pre>



</details>

<a name="sui_allowance_expiration_timestamp_ms"></a>

## Function `expiration_timestamp_ms`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_expiration_timestamp_ms">expiration_timestamp_ms</a>(self: &<a href="../sui/allowance.md#sui_allowance_Settings">sui::allowance::Settings</a>): <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;u64&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_expiration_timestamp_ms">expiration_timestamp_ms</a>(self: &<a href="../sui/allowance.md#sui_allowance_Settings">Settings</a>): Option&lt;u64&gt; {
    self.<a href="../sui/allowance.md#sui_allowance_expiration_timestamp_ms">expiration_timestamp_ms</a>
}
</code></pre>



</details>

<a name="sui_allowance_rate_limit"></a>

## Function `rate_limit`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_rate_limit">rate_limit</a>(self: &<a href="../sui/allowance.md#sui_allowance_Settings">sui::allowance::Settings</a>): <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;<a href="../sui/allowance.md#sui_allowance_RateLimit">sui::allowance::RateLimit</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_rate_limit">rate_limit</a>(self: &<a href="../sui/allowance.md#sui_allowance_Settings">Settings</a>): Option&lt;<a href="../sui/allowance.md#sui_allowance_RateLimit">RateLimit</a>&gt; { self.<a href="../sui/allowance.md#sui_allowance_rate_limit">rate_limit</a> }
</code></pre>



</details>

<a name="sui_allowance_name"></a>

## Function `name`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_name">name</a>(self: &<a href="../sui/allowance.md#sui_allowance_Settings">sui::allowance::Settings</a>): &<a href="../std/string.md#std_string_String">std::string::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_name">name</a>(self: &<a href="../sui/allowance.md#sui_allowance_Settings">Settings</a>): &String { &self.<a href="../sui/allowance.md#sui_allowance_name">name</a> }
</code></pre>



</details>

<a name="sui_allowance_rate_limit_limit"></a>

## Function `rate_limit_limit`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_rate_limit_limit">rate_limit_limit</a>(self: &<a href="../sui/allowance.md#sui_allowance_RateLimit">sui::allowance::RateLimit</a>): u256
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_rate_limit_limit">rate_limit_limit</a>(self: &<a href="../sui/allowance.md#sui_allowance_RateLimit">RateLimit</a>): u256 {
    match (self) {
        RateLimit::Windowed { limit, .. } =&gt; *limit,
    }
}
</code></pre>



</details>

<a name="sui_allowance_rate_limit_spent"></a>

## Function `rate_limit_spent`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_rate_limit_spent">rate_limit_spent</a>(self: &<a href="../sui/allowance.md#sui_allowance_RateLimit">sui::allowance::RateLimit</a>): u256
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_rate_limit_spent">rate_limit_spent</a>(self: &<a href="../sui/allowance.md#sui_allowance_RateLimit">RateLimit</a>): u256 {
    match (self) {
        RateLimit::Windowed { spent, .. } =&gt; *spent,
    }
}
</code></pre>



</details>

<a name="sui_allowance_rate_limit_window"></a>

## Function `rate_limit_window`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_rate_limit_window">rate_limit_window</a>(self: &<a href="../sui/allowance.md#sui_allowance_RateLimit">sui::allowance::RateLimit</a>): <a href="../sui/allowance.md#sui_allowance_Window">sui::allowance::Window</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/allowance.md#sui_allowance_rate_limit_window">rate_limit_window</a>(self: &<a href="../sui/allowance.md#sui_allowance_RateLimit">RateLimit</a>): <a href="../sui/allowance.md#sui_allowance_Window">Window</a> {
    match (self) {
        RateLimit::Windowed { window, .. } =&gt; *window,
    }
}
</code></pre>



</details>

<a name="sui_allowance_civil_from_ms"></a>

## Function `civil_from_ms`

Civil (year, month, day) in UTC, month and day 1-based. This is Howard Hinnant's
<code>civil_from_days</code>, where the derivation of every constant here is documented:
https://howardhinnant.github.io/date_algorithms.html#civil_from_days

The unsigned-only form of the algorithm. Chain timestamps are never pre-epoch.


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

Companion to <code><a href="../sui/allowance.md#sui_allowance_civil_from_ms">civil_from_ms</a></code>, following the same reference:
https://howardhinnant.github.io/date_algorithms.html#last_day_of_month


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/allowance.md#sui_allowance_days_in_month">days_in_month</a>(year: u64, month: u64): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/allowance.md#sui_allowance_days_in_month">days_in_month</a>(year: u64, month: u64): u64 {
    match (month) {
        2 <b>if</b> (<a href="../sui/allowance.md#sui_allowance_is_leap_year">is_leap_year</a>(year)) =&gt; 29,
        2 =&gt; 28,
        4 | 6 | 9 | 11 =&gt; 30,
        _ =&gt; 31,
    }
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
    <b>let</b> <a href="../sui/allowance.md#sui_allowance_app">app</a> = type_name::with_defining_ids&lt;A&gt;();
    <b>assert</b>!(self.settings.<a href="../sui/allowance.md#sui_allowance_app">app</a>.contains(&<a href="../sui/allowance.md#sui_allowance_app">app</a>), <a href="../sui/allowance.md#sui_allowance_EWrongApp">EWrongApp</a>);
}
</code></pre>



</details>

<a name="sui_allowance_consume"></a>

## Function `consume`

Central logic for policy checks and accounting, including the check of the
spender. NB: Any app checks must be done beforehand by the caller.


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
    <b>let</b> <a href="../sui/allowance.md#sui_allowance_AllowanceWithdrawal">AllowanceWithdrawal</a> { <a href="../sui/allowance.md#sui_allowance">allowance</a>, is_sponsor, inner } = w;
    <b>assert</b>!(!is_sponsor, <a href="../sui/allowance.md#sui_allowance_ESponsorWithdrawalNotEnabled">ESponsorWithdrawalNotEnabled</a>);
    <b>assert</b>!(<a href="../sui/allowance.md#sui_allowance">allowance</a> == self.id.to_inner(), <a href="../sui/allowance.md#sui_allowance_EWrongAllowance">EWrongAllowance</a>);
    <b>assert</b>!(self.settings.<a href="../sui/allowance.md#sui_allowance_spender">spender</a>.contains(&ctx.sender()), <a href="../sui/allowance.md#sui_allowance_ENotSpender">ENotSpender</a>);
    // Defense-in-depth check <b>as</b> this should already be verified at signing.
    <b>assert</b>!(inner.owner() == self.settings.<a href="../sui/allowance.md#sui_allowance_funder">funder</a>, <a href="../sui/allowance.md#sui_allowance_EWrongFunder">EWrongFunder</a>);
    <b>let</b> amount = inner.limit();
    <b>let</b> now = <a href="../sui/clock.md#sui_clock">clock</a>.timestamp_ms();
    self.settings.<a href="../sui/allowance.md#sui_allowance_start_timestamp_ms">start_timestamp_ms</a>.do_ref!(|<a href="../sui/allowance.md#sui_allowance_start_timestamp_ms">start_timestamp_ms</a>| {
        <b>assert</b>!(now &gt;= *<a href="../sui/allowance.md#sui_allowance_start_timestamp_ms">start_timestamp_ms</a>, <a href="../sui/allowance.md#sui_allowance_ENotStarted">ENotStarted</a>);
    });
    self.settings.<a href="../sui/allowance.md#sui_allowance_expiration_timestamp_ms">expiration_timestamp_ms</a>.do_ref!(|<a href="../sui/allowance.md#sui_allowance_expiration_timestamp_ms">expiration_timestamp_ms</a>| {
        <b>assert</b>!(now &lt; *<a href="../sui/allowance.md#sui_allowance_expiration_timestamp_ms">expiration_timestamp_ms</a>, <a href="../sui/allowance.md#sui_allowance_EExpired">EExpired</a>);
    });
    self.current_spend = self.current_spend + amount;
    self.settings.<a href="../sui/allowance.md#sui_allowance_lifetime_cap">lifetime_cap</a>.do_ref!(|<a href="../sui/allowance.md#sui_allowance_lifetime_cap">lifetime_cap</a>| {
        <b>assert</b>!(self.current_spend &lt;= *<a href="../sui/allowance.md#sui_allowance_lifetime_cap">lifetime_cap</a>, <a href="../sui/allowance.md#sui_allowance_EExceedsLifetimeCap">EExceedsLifetimeCap</a>);
    });
    self.settings.<a href="../sui/allowance.md#sui_allowance_rate_limit">rate_limit</a>.do_mut!(|<a href="../sui/allowance.md#sui_allowance_rate_limit">rate_limit</a>| <a href="../sui/allowance.md#sui_allowance_rate_limit">rate_limit</a>.<a href="../sui/allowance.md#sui_allowance_charge">charge</a>(amount, now));
    inner
}
</code></pre>



</details>

<a name="sui_allowance_new_rate_limit"></a>

## Function `new_rate_limit`



<pre><code><b>fun</b> <a href="../sui/allowance.md#sui_allowance_new_rate_limit">new_rate_limit</a>(limit: u256, window: <a href="../sui/allowance.md#sui_allowance_Window">sui::allowance::Window</a>): <a href="../sui/allowance.md#sui_allowance_RateLimit">sui::allowance::RateLimit</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/allowance.md#sui_allowance_new_rate_limit">new_rate_limit</a>(limit: u256, window: <a href="../sui/allowance.md#sui_allowance_Window">Window</a>): <a href="../sui/allowance.md#sui_allowance_RateLimit">RateLimit</a> {
    RateLimit::Windowed { limit, spent: 0, anchor_ms: option::none(), index: 0, window }
}
</code></pre>



</details>

<a name="sui_allowance_charge"></a>

## Function `charge`

Records <code>amount</code> against the limit, aborting if it does not fit.


<pre><code><b>fun</b> <a href="../sui/allowance.md#sui_allowance_charge">charge</a>(self: &<b>mut</b> <a href="../sui/allowance.md#sui_allowance_RateLimit">sui::allowance::RateLimit</a>, amount: u256, now_ms: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/allowance.md#sui_allowance_charge">charge</a>(self: &<b>mut</b> <a href="../sui/allowance.md#sui_allowance_RateLimit">RateLimit</a>, amount: u256, now_ms: u64) {
    match (self) {
        RateLimit::Windowed { limit, spent, anchor_ms, index, window } =&gt; {
            <b>if</b> (anchor_ms.is_some()) {
                <b>let</b> current = window.<a href="../sui/allowance.md#sui_allowance_index_at">index_at</a>(*anchor_ms.<a href="../sui/borrow.md#sui_borrow">borrow</a>(), now_ms);
                <b>if</b> (current &gt; *index) {
                    *index = current;
                    *spent = 0;
                };
            } <b>else</b> {
                // The first successful <a href="../sui/allowance.md#sui_allowance_charge">charge</a> anchors the windows.
                *anchor_ms = option::some(now_ms);
            };
            *spent = *spent + amount;
            <b>assert</b>!(*spent &lt;= *limit, <a href="../sui/allowance.md#sui_allowance_EExceedsRateLimit">EExceedsRateLimit</a>);
        },
    }
}
</code></pre>



</details>

<a name="sui_allowance_index_at"></a>

## Function `index_at`

Which window <code>now_ms</code> falls in, numbered from <code>anchor_ms</code> (0 = first).


<pre><code><b>fun</b> <a href="../sui/allowance.md#sui_allowance_index_at">index_at</a>(self: &<a href="../sui/allowance.md#sui_allowance_Window">sui::allowance::Window</a>, anchor_ms: u64, now_ms: u64): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/allowance.md#sui_allowance_index_at">index_at</a>(self: &<a href="../sui/allowance.md#sui_allowance_Window">Window</a>, anchor_ms: u64, now_ms: u64): u64 {
    match (self) {
        Window::PeriodicMs(period_ms) =&gt; (now_ms - anchor_ms) / *period_ms,
        Window::CalendarMonths(months) =&gt; <a href="../sui/allowance.md#sui_allowance_elapsed_windows">elapsed_windows</a>(anchor_ms, now_ms, *months),
    }
}
</code></pre>



</details>

<a name="sui_allowance_elapsed_windows"></a>

## Function `elapsed_windows`

Which <code>months</code>-month window <code>now_ms</code> falls in, counting from <code>anchor_ms</code>
(0 = the window the anchor itself is in).


<pre><code><b>fun</b> <a href="../sui/allowance.md#sui_allowance_elapsed_windows">elapsed_windows</a>(anchor_ms: u64, now_ms: u64, months: u8): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/allowance.md#sui_allowance_elapsed_windows">elapsed_windows</a>(anchor_ms: u64, now_ms: u64, months: u8): u64 {
    <b>let</b> (anchor_year, anchor_month, anchor_day) = <a href="../sui/allowance.md#sui_allowance_civil_from_ms">civil_from_ms</a>(anchor_ms);
    <b>let</b> (year, month, day) = <a href="../sui/allowance.md#sui_allowance_civil_from_ms">civil_from_ms</a>(now_ms);
    <b>let</b> <b>mut</b> elapsed = (year * 12 + month) - (anchor_year * 12 + anchor_month);
    // A month only fully elapses once the anniversary day arrives, clamped to month ends.
    <b>if</b> (elapsed &gt; 0 && day &lt; anchor_day.min(<a href="../sui/allowance.md#sui_allowance_days_in_month">days_in_month</a>(year, month))) {
        elapsed = elapsed - 1;
    };
    elapsed / (months <b>as</b> u64)
}
</code></pre>



</details>

<a name="sui_allowance_is_leap_year"></a>

## Function `is_leap_year`



<pre><code><b>fun</b> <a href="../sui/allowance.md#sui_allowance_is_leap_year">is_leap_year</a>(year: u64): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/allowance.md#sui_allowance_is_leap_year">is_leap_year</a>(year: u64): bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}
</code></pre>



</details>

<a name="sui_allowance_new_settings"></a>

## Function `new_settings`

Builds and validates settings. Rejects allowances that are unbounded in amount or in time.


<pre><code><b>fun</b> <a href="../sui/allowance.md#sui_allowance_new_settings">new_settings</a>(<a href="../sui/allowance.md#sui_allowance_funder">funder</a>: <b>address</b>, <a href="../sui/allowance.md#sui_allowance_spender">spender</a>: <b>address</b>, <a href="../sui/allowance.md#sui_allowance_app">app</a>: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;<a href="../std/type_name.md#std_type_name_TypeName">std::type_name::TypeName</a>&gt;, <a href="../sui/allowance.md#sui_allowance_lifetime_cap">lifetime_cap</a>: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;u256&gt;, <a href="../sui/allowance.md#sui_allowance_start_timestamp_ms">start_timestamp_ms</a>: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;u64&gt;, <a href="../sui/allowance.md#sui_allowance_expiration_timestamp_ms">expiration_timestamp_ms</a>: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;u64&gt;, <a href="../sui/allowance.md#sui_allowance_rate_limit">rate_limit</a>: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;<a href="../sui/allowance.md#sui_allowance_RateLimit">sui::allowance::RateLimit</a>&gt;, <a href="../sui/allowance.md#sui_allowance_name">name</a>: <a href="../std/string.md#std_string_String">std::string::String</a>): <a href="../sui/allowance.md#sui_allowance_Settings">sui::allowance::Settings</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/allowance.md#sui_allowance_new_settings">new_settings</a>(
    <a href="../sui/allowance.md#sui_allowance_funder">funder</a>: <b>address</b>,
    <a href="../sui/allowance.md#sui_allowance_spender">spender</a>: <b>address</b>,
    <a href="../sui/allowance.md#sui_allowance_app">app</a>: Option&lt;TypeName&gt;,
    <a href="../sui/allowance.md#sui_allowance_lifetime_cap">lifetime_cap</a>: Option&lt;u256&gt;,
    <a href="../sui/allowance.md#sui_allowance_start_timestamp_ms">start_timestamp_ms</a>: Option&lt;u64&gt;,
    <a href="../sui/allowance.md#sui_allowance_expiration_timestamp_ms">expiration_timestamp_ms</a>: Option&lt;u64&gt;,
    <a href="../sui/allowance.md#sui_allowance_rate_limit">rate_limit</a>: Option&lt;<a href="../sui/allowance.md#sui_allowance_RateLimit">RateLimit</a>&gt;,
    <a href="../sui/allowance.md#sui_allowance_name">name</a>: String,
): <a href="../sui/allowance.md#sui_allowance_Settings">Settings</a> {
    <b>assert</b>!(<a href="../sui/protocol_config.md#sui_protocol_config_is_feature_enabled">sui::protocol_config::is_feature_enabled</a>("enable_allowances"), <a href="../sui/allowance.md#sui_allowance_ENotEnabled">ENotEnabled</a>);
    <b>assert</b>!(<a href="../sui/allowance.md#sui_allowance_lifetime_cap">lifetime_cap</a>.is_some() || <a href="../sui/allowance.md#sui_allowance_rate_limit">rate_limit</a>.is_some(), <a href="../sui/allowance.md#sui_allowance_ENoLimit">ENoLimit</a>);
    <b>assert</b>!(<a href="../sui/allowance.md#sui_allowance_expiration_timestamp_ms">expiration_timestamp_ms</a>.is_some() || <a href="../sui/allowance.md#sui_allowance_rate_limit">rate_limit</a>.is_some(), <a href="../sui/allowance.md#sui_allowance_ENoExpiration">ENoExpiration</a>);
    <b>assert</b>!(<a href="../sui/allowance.md#sui_allowance_name">name</a>.length() &lt;= <a href="../sui/allowance.md#sui_allowance_MAX_NAME_LENGTH">MAX_NAME_LENGTH</a>, <a href="../sui/allowance.md#sui_allowance_ENameTooLong">ENameTooLong</a>);
    <a href="../sui/allowance.md#sui_allowance_lifetime_cap">lifetime_cap</a>.do_ref!(|cap| <b>assert</b>!(*cap &gt; 0, <a href="../sui/allowance.md#sui_allowance_EZeroLifetimeCap">EZeroLifetimeCap</a>));
    <b>if</b> (<a href="../sui/allowance.md#sui_allowance_start_timestamp_ms">start_timestamp_ms</a>.is_some() && <a href="../sui/allowance.md#sui_allowance_expiration_timestamp_ms">expiration_timestamp_ms</a>.is_some()) {
        <b>assert</b>!(*<a href="../sui/allowance.md#sui_allowance_start_timestamp_ms">start_timestamp_ms</a>.<a href="../sui/borrow.md#sui_borrow">borrow</a>() &lt; *<a href="../sui/allowance.md#sui_allowance_expiration_timestamp_ms">expiration_timestamp_ms</a>.<a href="../sui/borrow.md#sui_borrow">borrow</a>(), <a href="../sui/allowance.md#sui_allowance_EBadTimeWindow">EBadTimeWindow</a>);
    };
    <a href="../sui/allowance.md#sui_allowance_Settings">Settings</a> {
        <a href="../sui/allowance.md#sui_allowance_funder">funder</a>,
        <a href="../sui/allowance.md#sui_allowance_spender">spender</a>: option::some(<a href="../sui/allowance.md#sui_allowance_spender">spender</a>),
        <a href="../sui/allowance.md#sui_allowance_app">app</a>,
        <a href="../sui/allowance.md#sui_allowance_lifetime_cap">lifetime_cap</a>,
        <a href="../sui/allowance.md#sui_allowance_start_timestamp_ms">start_timestamp_ms</a>,
        <a href="../sui/allowance.md#sui_allowance_expiration_timestamp_ms">expiration_timestamp_ms</a>,
        <a href="../sui/allowance.md#sui_allowance_rate_limit">rate_limit</a>,
        <a href="../sui/allowance.md#sui_allowance_name">name</a>,
    }
}
</code></pre>



</details>

<a name="sui_allowance_share_new"></a>

## Function `share_new`



<pre><code><b>fun</b> <a href="../sui/allowance.md#sui_allowance_share_new">share_new</a>&lt;T&gt;(settings: <a href="../sui/allowance.md#sui_allowance_Settings">sui::allowance::Settings</a>, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/allowance.md#sui_allowance_share_new">share_new</a>&lt;T&gt;(settings: <a href="../sui/allowance.md#sui_allowance_Settings">Settings</a>, ctx: &<b>mut</b> TxContext) {
    <b>let</b> <a href="../sui/allowance.md#sui_allowance_funder">funder</a> = settings.<a href="../sui/allowance.md#sui_allowance_funder">funder</a>;
    <b>let</b> <a href="../sui/allowance.md#sui_allowance">allowance</a> = <a href="../sui/allowance.md#sui_allowance_Allowance">Allowance</a>&lt;T&gt; {
        id: <a href="../sui/object.md#sui_object_new">object::new</a>(ctx),
        settings,
        current_spend: 0,
    };
    <b>let</b> cap = <a href="../sui/allowance.md#sui_allowance_AllowanceCap">AllowanceCap</a>&lt;T&gt; {
        id: <a href="../sui/object.md#sui_object_new">object::new</a>(ctx),
        <a href="../sui/allowance.md#sui_allowance">allowance</a>: <a href="../sui/allowance.md#sui_allowance">allowance</a>.id.to_inner(),
    };
    <a href="../sui/transfer.md#sui_transfer_transfer">transfer::transfer</a>(cap, <a href="../sui/allowance.md#sui_allowance_funder">funder</a>);
    <a href="../sui/transfer.md#sui_transfer_share_object">transfer::share_object</a>(<a href="../sui/allowance.md#sui_allowance">allowance</a>);
}
</code></pre>



</details>
