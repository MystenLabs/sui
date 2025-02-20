---
title: Module `sui::authenticator_state`
---



-  [Struct `AuthenticatorState`](#sui_authenticator_state_AuthenticatorState)
-  [Struct `AuthenticatorStateInner`](#sui_authenticator_state_AuthenticatorStateInner)
-  [Struct `JWK`](#sui_authenticator_state_JWK)
-  [Struct `JwkId`](#sui_authenticator_state_JwkId)
-  [Struct `ActiveJwk`](#sui_authenticator_state_ActiveJwk)
-  [Constants](#@Constants_0)
-  [Function `active_jwk_equal`](#sui_authenticator_state_active_jwk_equal)
-  [Function `jwk_equal`](#sui_authenticator_state_jwk_equal)
-  [Function `jwk_id_equal`](#sui_authenticator_state_jwk_id_equal)
-  [Function `string_bytes_lt`](#sui_authenticator_state_string_bytes_lt)
-  [Function `jwk_lt`](#sui_authenticator_state_jwk_lt)
-  [Function `create`](#sui_authenticator_state_create)
-  [Function `load_inner_mut`](#sui_authenticator_state_load_inner_mut)
-  [Function `load_inner`](#sui_authenticator_state_load_inner)
-  [Function `check_sorted`](#sui_authenticator_state_check_sorted)
-  [Function `update_authenticator_state`](#sui_authenticator_state_update_authenticator_state)
-  [Function `deduplicate`](#sui_authenticator_state_deduplicate)
-  [Function `expire_jwks`](#sui_authenticator_state_expire_jwks)
-  [Function `get_active_jwks`](#sui_authenticator_state_get_active_jwks)


<pre><code><b>use</b> <a href="../std/ascii.md#std_ascii">std::ascii</a>;
<b>use</b> <a href="../std/bcs.md#std_bcs">std::bcs</a>;
<b>use</b> <a href="../std/option.md#std_option">std::option</a>;
<b>use</b> <a href="../std/string.md#std_string">std::string</a>;
<b>use</b> <a href="../std/u64.md#std_u64">std::u64</a>;
<b>use</b> <a href="../std/vector.md#std_vector">std::vector</a>;
<b>use</b> <a href="../sui/address.md#sui_address">sui::address</a>;
<b>use</b> <a href="../sui/dynamic_field.md#sui_dynamic_field">sui::dynamic_field</a>;
<b>use</b> <a href="../sui/hex.md#sui_hex">sui::hex</a>;
<b>use</b> <a href="../sui/object.md#sui_object">sui::object</a>;
<b>use</b> <a href="../sui/transfer.md#sui_transfer">sui::transfer</a>;
<b>use</b> <a href="../sui/tx_context.md#sui_tx_context">sui::tx_context</a>;
</code></pre>



<a name="sui_authenticator_state_AuthenticatorState"></a>

## Struct `AuthenticatorState`

Singleton shared object which stores the global authenticator state.
The actual state is stored in a dynamic field of type AuthenticatorStateInner to support
future versions of the authenticator state.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/authenticator_state.md#sui_authenticator_state_AuthenticatorState">AuthenticatorState</a> <b>has</b> key
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
<code>version: u64</code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="sui_authenticator_state_AuthenticatorStateInner"></a>

## Struct `AuthenticatorStateInner`



<pre><code><b>public</b> <b>struct</b> <a href="../sui/authenticator_state.md#sui_authenticator_state_AuthenticatorStateInner">AuthenticatorStateInner</a> <b>has</b> store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>version: u64</code>
</dt>
<dd>
</dd>
<dt>
<code>active_jwks: vector&lt;<a href="../sui/authenticator_state.md#sui_authenticator_state_ActiveJwk">sui::authenticator_state::ActiveJwk</a>&gt;</code>
</dt>
<dd>
 List of currently active JWKs.
</dd>
</dl>


</details>

<a name="sui_authenticator_state_JWK"></a>

## Struct `JWK`

Must match the JWK struct in fastcrypto-zkp


<pre><code><b>public</b> <b>struct</b> <a href="../sui/authenticator_state.md#sui_authenticator_state_JWK">JWK</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>kty: <a href="../std/string.md#std_string_String">std::string::String</a></code>
</dt>
<dd>
</dd>
<dt>
<code>e: <a href="../std/string.md#std_string_String">std::string::String</a></code>
</dt>
<dd>
</dd>
<dt>
<code>n: <a href="../std/string.md#std_string_String">std::string::String</a></code>
</dt>
<dd>
</dd>
<dt>
<code>alg: <a href="../std/string.md#std_string_String">std::string::String</a></code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="sui_authenticator_state_JwkId"></a>

## Struct `JwkId`

Must match the JwkId struct in fastcrypto-zkp


<pre><code><b>public</b> <b>struct</b> <a href="../sui/authenticator_state.md#sui_authenticator_state_JwkId">JwkId</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>iss: <a href="../std/string.md#std_string_String">std::string::String</a></code>
</dt>
<dd>
</dd>
<dt>
<code>kid: <a href="../std/string.md#std_string_String">std::string::String</a></code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="sui_authenticator_state_ActiveJwk"></a>

## Struct `ActiveJwk`



<pre><code><b>public</b> <b>struct</b> <a href="../sui/authenticator_state.md#sui_authenticator_state_ActiveJwk">ActiveJwk</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>jwk_id: <a href="../sui/authenticator_state.md#sui_authenticator_state_JwkId">sui::authenticator_state::JwkId</a></code>
</dt>
<dd>
</dd>
<dt>
<code>jwk: <a href="../sui/authenticator_state.md#sui_authenticator_state_JWK">sui::authenticator_state::JWK</a></code>
</dt>
<dd>
</dd>
<dt>
<code>epoch: u64</code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="sui_authenticator_state_CurrentVersion"></a>



<pre><code><b>const</b> <a href="../sui/authenticator_state.md#sui_authenticator_state_CurrentVersion">CurrentVersion</a>: u64 = 1;
</code></pre>



<a name="sui_authenticator_state_EJwksNotSorted"></a>



<pre><code><b>const</b> <a href="../sui/authenticator_state.md#sui_authenticator_state_EJwksNotSorted">EJwksNotSorted</a>: u64 = 2;
</code></pre>



<a name="sui_authenticator_state_ENotSystemAddress"></a>

Sender is not @0x0 the system address.


<pre><code><b>const</b> <a href="../sui/authenticator_state.md#sui_authenticator_state_ENotSystemAddress">ENotSystemAddress</a>: u64 = 0;
</code></pre>



<a name="sui_authenticator_state_EWrongInnerVersion"></a>



<pre><code><b>const</b> <a href="../sui/authenticator_state.md#sui_authenticator_state_EWrongInnerVersion">EWrongInnerVersion</a>: u64 = 1;
</code></pre>



<a name="sui_authenticator_state_active_jwk_equal"></a>

## Function `active_jwk_equal`



<pre><code><b>fun</b> <a href="../sui/authenticator_state.md#sui_authenticator_state_active_jwk_equal">active_jwk_equal</a>(a: &<a href="../sui/authenticator_state.md#sui_authenticator_state_ActiveJwk">sui::authenticator_state::ActiveJwk</a>, b: &<a href="../sui/authenticator_state.md#sui_authenticator_state_ActiveJwk">sui::authenticator_state::ActiveJwk</a>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/authenticator_state.md#sui_authenticator_state_active_jwk_equal">active_jwk_equal</a>(a: &<a href="../sui/authenticator_state.md#sui_authenticator_state_ActiveJwk">ActiveJwk</a>, b: &<a href="../sui/authenticator_state.md#sui_authenticator_state_ActiveJwk">ActiveJwk</a>): bool {
    // note: epoch is ignored
    <a href="../sui/authenticator_state.md#sui_authenticator_state_jwk_equal">jwk_equal</a>(&a.jwk, &b.jwk) && <a href="../sui/authenticator_state.md#sui_authenticator_state_jwk_id_equal">jwk_id_equal</a>(&a.jwk_id, &b.jwk_id)
}
</code></pre>



</details>

<a name="sui_authenticator_state_jwk_equal"></a>

## Function `jwk_equal`



<pre><code><b>fun</b> <a href="../sui/authenticator_state.md#sui_authenticator_state_jwk_equal">jwk_equal</a>(a: &<a href="../sui/authenticator_state.md#sui_authenticator_state_JWK">sui::authenticator_state::JWK</a>, b: &<a href="../sui/authenticator_state.md#sui_authenticator_state_JWK">sui::authenticator_state::JWK</a>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/authenticator_state.md#sui_authenticator_state_jwk_equal">jwk_equal</a>(a: &<a href="../sui/authenticator_state.md#sui_authenticator_state_JWK">JWK</a>, b: &<a href="../sui/authenticator_state.md#sui_authenticator_state_JWK">JWK</a>): bool {
    (&a.kty == &b.kty) &&
        (&a.e == &b.e) &&
        (&a.n == &b.n) &&
        (&a.alg == &b.alg)
}
</code></pre>



</details>

<a name="sui_authenticator_state_jwk_id_equal"></a>

## Function `jwk_id_equal`



<pre><code><b>fun</b> <a href="../sui/authenticator_state.md#sui_authenticator_state_jwk_id_equal">jwk_id_equal</a>(a: &<a href="../sui/authenticator_state.md#sui_authenticator_state_JwkId">sui::authenticator_state::JwkId</a>, b: &<a href="../sui/authenticator_state.md#sui_authenticator_state_JwkId">sui::authenticator_state::JwkId</a>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/authenticator_state.md#sui_authenticator_state_jwk_id_equal">jwk_id_equal</a>(a: &<a href="../sui/authenticator_state.md#sui_authenticator_state_JwkId">JwkId</a>, b: &<a href="../sui/authenticator_state.md#sui_authenticator_state_JwkId">JwkId</a>): bool {
    (&a.iss == &b.iss) && (&a.kid == &b.kid)
}
</code></pre>



</details>

<a name="sui_authenticator_state_string_bytes_lt"></a>

## Function `string_bytes_lt`



<pre><code><b>fun</b> <a href="../sui/authenticator_state.md#sui_authenticator_state_string_bytes_lt">string_bytes_lt</a>(a: &<a href="../std/string.md#std_string_String">std::string::String</a>, b: &<a href="../std/string.md#std_string_String">std::string::String</a>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/authenticator_state.md#sui_authenticator_state_string_bytes_lt">string_bytes_lt</a>(a: &String, b: &String): bool {
    <b>let</b> a_bytes = a.as_bytes();
    <b>let</b> b_bytes = b.as_bytes();
    <b>if</b> (a_bytes.length() &lt; b_bytes.length()) {
        <b>true</b>
    } <b>else</b> <b>if</b> (a_bytes.length() &gt; b_bytes.length()) {
        <b>false</b>
    } <b>else</b> {
        <b>let</b> <b>mut</b> i = 0;
        <b>while</b> (i &lt; a_bytes.length()) {
            <b>let</b> a_byte = a_bytes[i];
            <b>let</b> b_byte = b_bytes[i];
            <b>if</b> (a_byte &lt; b_byte) {
                <b>return</b> <b>true</b>
            } <b>else</b> <b>if</b> (a_byte &gt; b_byte) {
                <b>return</b> <b>false</b>
            };
            i = i + 1;
        };
        // all bytes are equal
        <b>false</b>
    }
}
</code></pre>



</details>

<a name="sui_authenticator_state_jwk_lt"></a>

## Function `jwk_lt`



<pre><code><b>fun</b> <a href="../sui/authenticator_state.md#sui_authenticator_state_jwk_lt">jwk_lt</a>(a: &<a href="../sui/authenticator_state.md#sui_authenticator_state_ActiveJwk">sui::authenticator_state::ActiveJwk</a>, b: &<a href="../sui/authenticator_state.md#sui_authenticator_state_ActiveJwk">sui::authenticator_state::ActiveJwk</a>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/authenticator_state.md#sui_authenticator_state_jwk_lt">jwk_lt</a>(a: &<a href="../sui/authenticator_state.md#sui_authenticator_state_ActiveJwk">ActiveJwk</a>, b: &<a href="../sui/authenticator_state.md#sui_authenticator_state_ActiveJwk">ActiveJwk</a>): bool {
    // note: epoch is ignored
    <b>if</b> (&a.jwk_id.iss != &b.jwk_id.iss) {
        <b>return</b> <a href="../sui/authenticator_state.md#sui_authenticator_state_string_bytes_lt">string_bytes_lt</a>(&a.jwk_id.iss, &b.jwk_id.iss)
    };
    <b>if</b> (&a.jwk_id.kid != &b.jwk_id.kid) {
        <b>return</b> <a href="../sui/authenticator_state.md#sui_authenticator_state_string_bytes_lt">string_bytes_lt</a>(&a.jwk_id.kid, &b.jwk_id.kid)
    };
    <b>if</b> (&a.jwk.kty != &b.jwk.kty) {
        <b>return</b> <a href="../sui/authenticator_state.md#sui_authenticator_state_string_bytes_lt">string_bytes_lt</a>(&a.jwk.kty, &b.jwk.kty)
    };
    <b>if</b> (&a.jwk.e != &b.jwk.e) {
        <b>return</b> <a href="../sui/authenticator_state.md#sui_authenticator_state_string_bytes_lt">string_bytes_lt</a>(&a.jwk.e, &b.jwk.e)
    };
    <b>if</b> (&a.jwk.n != &b.jwk.n) {
        <b>return</b> <a href="../sui/authenticator_state.md#sui_authenticator_state_string_bytes_lt">string_bytes_lt</a>(&a.jwk.n, &b.jwk.n)
    };
    <a href="../sui/authenticator_state.md#sui_authenticator_state_string_bytes_lt">string_bytes_lt</a>(&a.jwk.alg, &b.jwk.alg)
}
</code></pre>



</details>

<a name="sui_authenticator_state_create"></a>

## Function `create`

Create and share the AuthenticatorState object. This function is call exactly once, when
the authenticator state object is first created.
Can only be called by genesis or change_epoch transactions.


<pre><code><b>fun</b> <a href="../sui/authenticator_state.md#sui_authenticator_state_create">create</a>(ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/authenticator_state.md#sui_authenticator_state_create">create</a>(ctx: &TxContext) {
    <b>assert</b>!(ctx.sender() == @0x0, <a href="../sui/authenticator_state.md#sui_authenticator_state_ENotSystemAddress">ENotSystemAddress</a>);
    <b>let</b> version = <a href="../sui/authenticator_state.md#sui_authenticator_state_CurrentVersion">CurrentVersion</a>;
    <b>let</b> inner = <a href="../sui/authenticator_state.md#sui_authenticator_state_AuthenticatorStateInner">AuthenticatorStateInner</a> {
        version,
        active_jwks: vector[],
    };
    <b>let</b> <b>mut</b> self = <a href="../sui/authenticator_state.md#sui_authenticator_state_AuthenticatorState">AuthenticatorState</a> {
        id: <a href="../sui/object.md#sui_object_authenticator_state">object::authenticator_state</a>(),
        version,
    };
    <a href="../sui/dynamic_field.md#sui_dynamic_field_add">dynamic_field::add</a>(&<b>mut</b> self.id, version, inner);
    <a href="../sui/transfer.md#sui_transfer_share_object">transfer::share_object</a>(self);
}
</code></pre>



</details>

<a name="sui_authenticator_state_load_inner_mut"></a>

## Function `load_inner_mut`



<pre><code><b>fun</b> <a href="../sui/authenticator_state.md#sui_authenticator_state_load_inner_mut">load_inner_mut</a>(self: &<b>mut</b> <a href="../sui/authenticator_state.md#sui_authenticator_state_AuthenticatorState">sui::authenticator_state::AuthenticatorState</a>): &<b>mut</b> <a href="../sui/authenticator_state.md#sui_authenticator_state_AuthenticatorStateInner">sui::authenticator_state::AuthenticatorStateInner</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/authenticator_state.md#sui_authenticator_state_load_inner_mut">load_inner_mut</a>(self: &<b>mut</b> <a href="../sui/authenticator_state.md#sui_authenticator_state_AuthenticatorState">AuthenticatorState</a>): &<b>mut</b> <a href="../sui/authenticator_state.md#sui_authenticator_state_AuthenticatorStateInner">AuthenticatorStateInner</a> {
    <b>let</b> version = self.version;
    // replace this with a lazy update function when we add a new version of the inner <a href="../sui/object.md#sui_object">object</a>.
    <b>assert</b>!(version == <a href="../sui/authenticator_state.md#sui_authenticator_state_CurrentVersion">CurrentVersion</a>, <a href="../sui/authenticator_state.md#sui_authenticator_state_EWrongInnerVersion">EWrongInnerVersion</a>);
    <b>let</b> inner: &<b>mut</b> <a href="../sui/authenticator_state.md#sui_authenticator_state_AuthenticatorStateInner">AuthenticatorStateInner</a> = <a href="../sui/dynamic_field.md#sui_dynamic_field_borrow_mut">dynamic_field::borrow_mut</a>(&<b>mut</b> self.id, self.version);
    <b>assert</b>!(inner.version == version, <a href="../sui/authenticator_state.md#sui_authenticator_state_EWrongInnerVersion">EWrongInnerVersion</a>);
    inner
}
</code></pre>



</details>

<a name="sui_authenticator_state_load_inner"></a>

## Function `load_inner`



<pre><code><b>fun</b> <a href="../sui/authenticator_state.md#sui_authenticator_state_load_inner">load_inner</a>(self: &<a href="../sui/authenticator_state.md#sui_authenticator_state_AuthenticatorState">sui::authenticator_state::AuthenticatorState</a>): &<a href="../sui/authenticator_state.md#sui_authenticator_state_AuthenticatorStateInner">sui::authenticator_state::AuthenticatorStateInner</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/authenticator_state.md#sui_authenticator_state_load_inner">load_inner</a>(self: &<a href="../sui/authenticator_state.md#sui_authenticator_state_AuthenticatorState">AuthenticatorState</a>): &<a href="../sui/authenticator_state.md#sui_authenticator_state_AuthenticatorStateInner">AuthenticatorStateInner</a> {
    <b>let</b> version = self.version;
    // replace this with a lazy update function when we add a new version of the inner <a href="../sui/object.md#sui_object">object</a>.
    <b>assert</b>!(version == <a href="../sui/authenticator_state.md#sui_authenticator_state_CurrentVersion">CurrentVersion</a>, <a href="../sui/authenticator_state.md#sui_authenticator_state_EWrongInnerVersion">EWrongInnerVersion</a>);
    <b>let</b> inner: &<a href="../sui/authenticator_state.md#sui_authenticator_state_AuthenticatorStateInner">AuthenticatorStateInner</a> = <a href="../sui/dynamic_field.md#sui_dynamic_field_borrow">dynamic_field::borrow</a>(&self.id, self.version);
    <b>assert</b>!(inner.version == version, <a href="../sui/authenticator_state.md#sui_authenticator_state_EWrongInnerVersion">EWrongInnerVersion</a>);
    inner
}
</code></pre>



</details>

<a name="sui_authenticator_state_check_sorted"></a>

## Function `check_sorted`



<pre><code><b>fun</b> <a href="../sui/authenticator_state.md#sui_authenticator_state_check_sorted">check_sorted</a>(new_active_jwks: &vector&lt;<a href="../sui/authenticator_state.md#sui_authenticator_state_ActiveJwk">sui::authenticator_state::ActiveJwk</a>&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/authenticator_state.md#sui_authenticator_state_check_sorted">check_sorted</a>(new_active_jwks: &vector&lt;<a href="../sui/authenticator_state.md#sui_authenticator_state_ActiveJwk">ActiveJwk</a>&gt;) {
    <b>let</b> <b>mut</b> i = 0;
    <b>while</b> (i &lt; new_active_jwks.length() - 1) {
        <b>let</b> a = &new_active_jwks[i];
        <b>let</b> b = &new_active_jwks[i + 1];
        <b>assert</b>!(<a href="../sui/authenticator_state.md#sui_authenticator_state_jwk_lt">jwk_lt</a>(a, b), <a href="../sui/authenticator_state.md#sui_authenticator_state_EJwksNotSorted">EJwksNotSorted</a>);
        i = i + 1;
    };
}
</code></pre>



</details>

<a name="sui_authenticator_state_update_authenticator_state"></a>

## Function `update_authenticator_state`

Record a new set of active_jwks. Called when executing the AuthenticatorStateUpdate system
transaction. The new input vector must be sorted and must not contain duplicates.
If a new JWK is already present, but with a previous epoch, then the epoch is updated to
indicate that the JWK has been validated in the current epoch and should not be expired.


<pre><code><b>fun</b> <a href="../sui/authenticator_state.md#sui_authenticator_state_update_authenticator_state">update_authenticator_state</a>(self: &<b>mut</b> <a href="../sui/authenticator_state.md#sui_authenticator_state_AuthenticatorState">sui::authenticator_state::AuthenticatorState</a>, new_active_jwks: vector&lt;<a href="../sui/authenticator_state.md#sui_authenticator_state_ActiveJwk">sui::authenticator_state::ActiveJwk</a>&gt;, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/authenticator_state.md#sui_authenticator_state_update_authenticator_state">update_authenticator_state</a>(
    self: &<b>mut</b> <a href="../sui/authenticator_state.md#sui_authenticator_state_AuthenticatorState">AuthenticatorState</a>,
    new_active_jwks: vector&lt;<a href="../sui/authenticator_state.md#sui_authenticator_state_ActiveJwk">ActiveJwk</a>&gt;,
    ctx: &TxContext,
) {
    // Validator will make a special system call with sender set <b>as</b> 0x0.
    <b>assert</b>!(ctx.sender() == @0x0, <a href="../sui/authenticator_state.md#sui_authenticator_state_ENotSystemAddress">ENotSystemAddress</a>);
    <a href="../sui/authenticator_state.md#sui_authenticator_state_check_sorted">check_sorted</a>(&new_active_jwks);
    <b>let</b> new_active_jwks = <a href="../sui/authenticator_state.md#sui_authenticator_state_deduplicate">deduplicate</a>(new_active_jwks);
    <b>let</b> inner = self.<a href="../sui/authenticator_state.md#sui_authenticator_state_load_inner_mut">load_inner_mut</a>();
    <b>let</b> <b>mut</b> res = vector[];
    <b>let</b> <b>mut</b> i = 0;
    <b>let</b> <b>mut</b> j = 0;
    <b>let</b> active_jwks_len = inner.active_jwks.length();
    <b>let</b> new_active_jwks_len = new_active_jwks.length();
    <b>while</b> (i &lt; active_jwks_len && j &lt; new_active_jwks_len) {
        <b>let</b> old_jwk = &inner.active_jwks[i];
        <b>let</b> new_jwk = &new_active_jwks[j];
        // when they are equal, push only one, but <b>use</b> the max epoch of the two
        <b>if</b> (<a href="../sui/authenticator_state.md#sui_authenticator_state_active_jwk_equal">active_jwk_equal</a>(old_jwk, new_jwk)) {
            <b>let</b> <b>mut</b> jwk = *old_jwk;
            jwk.epoch = old_jwk.epoch.max(new_jwk.epoch);
            res.push_back(jwk);
            i = i + 1;
            j = j + 1;
        } <b>else</b> <b>if</b> (<a href="../sui/authenticator_state.md#sui_authenticator_state_jwk_id_equal">jwk_id_equal</a>(&old_jwk.jwk_id, &new_jwk.jwk_id)) {
            // <b>if</b> only jwk_id is equal, then the key <b>has</b> changed. Providers should not send
            // JWKs like this, but <b>if</b> they do, we must ignore the new <a href="../sui/authenticator_state.md#sui_authenticator_state_JWK">JWK</a> to avoid having a
            // liveness / forking issues
            res.push_back(*old_jwk);
            i = i + 1;
            j = j + 1;
        } <b>else</b> <b>if</b> (<a href="../sui/authenticator_state.md#sui_authenticator_state_jwk_lt">jwk_lt</a>(old_jwk, new_jwk)) {
            res.push_back(*old_jwk);
            i = i + 1;
        } <b>else</b> {
            res.push_back(*new_jwk);
            j = j + 1;
        }
    };
    <b>while</b> (i &lt; active_jwks_len) {
        res.push_back(inner.active_jwks[i]);
        i = i + 1;
    };
    <b>while</b> (j &lt; new_active_jwks_len) {
        res.push_back(new_active_jwks[j]);
        j = j + 1;
    };
    inner.active_jwks = res;
}
</code></pre>



</details>

<a name="sui_authenticator_state_deduplicate"></a>

## Function `deduplicate`



<pre><code><b>fun</b> <a href="../sui/authenticator_state.md#sui_authenticator_state_deduplicate">deduplicate</a>(jwks: vector&lt;<a href="../sui/authenticator_state.md#sui_authenticator_state_ActiveJwk">sui::authenticator_state::ActiveJwk</a>&gt;): vector&lt;<a href="../sui/authenticator_state.md#sui_authenticator_state_ActiveJwk">sui::authenticator_state::ActiveJwk</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/authenticator_state.md#sui_authenticator_state_deduplicate">deduplicate</a>(jwks: vector&lt;<a href="../sui/authenticator_state.md#sui_authenticator_state_ActiveJwk">ActiveJwk</a>&gt;): vector&lt;<a href="../sui/authenticator_state.md#sui_authenticator_state_ActiveJwk">ActiveJwk</a>&gt; {
    <b>let</b> <b>mut</b> res = vector[];
    <b>let</b> <b>mut</b> i = 0;
    <b>let</b> <b>mut</b> prev: Option&lt;<a href="../sui/authenticator_state.md#sui_authenticator_state_JwkId">JwkId</a>&gt; = option::none();
    <b>while</b> (i &lt; jwks.length()) {
        <b>let</b> jwk = &jwks[i];
        <b>if</b> (prev.is_none()) {
            prev.fill(jwk.jwk_id);
        } <b>else</b> <b>if</b> (<a href="../sui/authenticator_state.md#sui_authenticator_state_jwk_id_equal">jwk_id_equal</a>(prev.<a href="../sui/borrow.md#sui_borrow">borrow</a>(), &jwk.jwk_id)) {
            // skip duplicate jwks in input
            i = i + 1;
            <b>continue</b>
        } <b>else</b> {
            *prev.borrow_mut() = jwk.jwk_id;
        };
        res.push_back(*jwk);
        i = i + 1;
    };
    res
}
</code></pre>



</details>

<a name="sui_authenticator_state_expire_jwks"></a>

## Function `expire_jwks`



<pre><code><b>fun</b> <a href="../sui/authenticator_state.md#sui_authenticator_state_expire_jwks">expire_jwks</a>(self: &<b>mut</b> <a href="../sui/authenticator_state.md#sui_authenticator_state_AuthenticatorState">sui::authenticator_state::AuthenticatorState</a>, min_epoch: u64, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/authenticator_state.md#sui_authenticator_state_expire_jwks">expire_jwks</a>(
    self: &<b>mut</b> <a href="../sui/authenticator_state.md#sui_authenticator_state_AuthenticatorState">AuthenticatorState</a>,
    // any jwk below this epoch is not retained
    min_epoch: u64,
    ctx: &TxContext,
) {
    // This will only be called by sui_system::advance_epoch
    <b>assert</b>!(ctx.sender() == @0x0, <a href="../sui/authenticator_state.md#sui_authenticator_state_ENotSystemAddress">ENotSystemAddress</a>);
    <b>let</b> inner = <a href="../sui/authenticator_state.md#sui_authenticator_state_load_inner_mut">load_inner_mut</a>(self);
    <b>let</b> len = inner.active_jwks.length();
    // first we count how many jwks from each issuer are above the min_epoch
    // and store the counts in a vector that parallels the (sorted) active_jwks vector
    <b>let</b> <b>mut</b> issuer_max_epochs = vector[];
    <b>let</b> <b>mut</b> i = 0;
    <b>let</b> <b>mut</b> prev_issuer: Option&lt;String&gt; = option::none();
    <b>while</b> (i &lt; len) {
        <b>let</b> cur = &inner.active_jwks[i];
        <b>let</b> cur_iss = &cur.jwk_id.iss;
        <b>if</b> (prev_issuer.is_none()) {
            prev_issuer.fill(*cur_iss);
            issuer_max_epochs.push_back(cur.epoch);
        } <b>else</b> {
            <b>if</b> (cur_iss == prev_issuer.<a href="../sui/borrow.md#sui_borrow">borrow</a>()) {
                <b>let</b> back = issuer_max_epochs.length() - 1;
                <b>let</b> prev_max_epoch = &<b>mut</b> issuer_max_epochs[back];
                *prev_max_epoch = (*prev_max_epoch).max(cur.epoch);
            } <b>else</b> {
                *prev_issuer.borrow_mut() = *cur_iss;
                issuer_max_epochs.push_back(cur.epoch);
            }
        };
        i = i + 1;
    };
    // Now, filter out any JWKs that are below the min_epoch, unless that issuer <b>has</b> no
    // JWKs &gt;= the min_epoch, in which case we keep all of them.
    <b>let</b> <b>mut</b> new_active_jwks: vector&lt;<a href="../sui/authenticator_state.md#sui_authenticator_state_ActiveJwk">ActiveJwk</a>&gt; = vector[];
    <b>let</b> <b>mut</b> prev_issuer: Option&lt;String&gt; = option::none();
    <b>let</b> <b>mut</b> i = 0;
    <b>let</b> <b>mut</b> j = 0;
    <b>while</b> (i &lt; len) {
        <b>let</b> jwk = &inner.active_jwks[i];
        <b>let</b> cur_iss = &jwk.jwk_id.iss;
        <b>if</b> (prev_issuer.is_none()) {
            prev_issuer.fill(*cur_iss);
        } <b>else</b> <b>if</b> (cur_iss != prev_issuer.<a href="../sui/borrow.md#sui_borrow">borrow</a>()) {
            *prev_issuer.borrow_mut() = *cur_iss;
            j = j + 1;
        };
        <b>let</b> max_epoch_for_iss = &issuer_max_epochs[j];
        // TODO: <b>if</b> the iss <b>for</b> this jwk <b>has</b> *no* jwks that meet the minimum epoch,
        // then expire nothing.
        <b>if</b> (*max_epoch_for_iss &lt; min_epoch || jwk.epoch &gt;= min_epoch) {
            new_active_jwks.push_back(*jwk);
        };
        i = i + 1;
    };
    inner.active_jwks = new_active_jwks;
}
</code></pre>



</details>

<a name="sui_authenticator_state_get_active_jwks"></a>

## Function `get_active_jwks`

Get the current active_jwks. Called when the node starts up in order to load the current
JWK state from the chain.


<pre><code><b>fun</b> <a href="../sui/authenticator_state.md#sui_authenticator_state_get_active_jwks">get_active_jwks</a>(self: &<a href="../sui/authenticator_state.md#sui_authenticator_state_AuthenticatorState">sui::authenticator_state::AuthenticatorState</a>, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): vector&lt;<a href="../sui/authenticator_state.md#sui_authenticator_state_ActiveJwk">sui::authenticator_state::ActiveJwk</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/authenticator_state.md#sui_authenticator_state_get_active_jwks">get_active_jwks</a>(self: &<a href="../sui/authenticator_state.md#sui_authenticator_state_AuthenticatorState">AuthenticatorState</a>, ctx: &TxContext): vector&lt;<a href="../sui/authenticator_state.md#sui_authenticator_state_ActiveJwk">ActiveJwk</a>&gt; {
    <b>assert</b>!(ctx.sender() == @0x0, <a href="../sui/authenticator_state.md#sui_authenticator_state_ENotSystemAddress">ENotSystemAddress</a>);
    self.<a href="../sui/authenticator_state.md#sui_authenticator_state_load_inner">load_inner</a>().active_jwks
}
</code></pre>



</details>
