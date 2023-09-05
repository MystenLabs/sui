
<a name="0x2_authenticator_state"></a>

# Module `0x2::authenticator_state`



-  [Resource `AuthenticatorState`](#0x2_authenticator_state_AuthenticatorState)
-  [Struct `AuthenticatorStateInner`](#0x2_authenticator_state_AuthenticatorStateInner)
-  [Struct `JWK`](#0x2_authenticator_state_JWK)
-  [Struct `JwkId`](#0x2_authenticator_state_JwkId)
-  [Struct `ActiveJwk`](#0x2_authenticator_state_ActiveJwk)
-  [Constants](#@Constants_0)
-  [Function `create`](#0x2_authenticator_state_create)


<pre><code><b>use</b> <a href="">0x1::string</a>;
<b>use</b> <a href="dynamic_field.md#0x2_dynamic_field">0x2::dynamic_field</a>;
<b>use</b> <a href="object.md#0x2_object">0x2::object</a>;
<b>use</b> <a href="transfer.md#0x2_transfer">0x2::transfer</a>;
<b>use</b> <a href="tx_context.md#0x2_tx_context">0x2::tx_context</a>;
</code></pre>



<a name="0x2_authenticator_state_AuthenticatorState"></a>

## Resource `AuthenticatorState`

Singleton shared object which stores the global authenticator state.
The actual state is stored in a dynamic field of type AuthenticatorStateInner to support
future versions of the authenticator state.


<pre><code><b>struct</b> <a href="authenticator_state.md#0x2_authenticator_state_AuthenticatorState">AuthenticatorState</a> <b>has</b> key
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>id: <a href="object.md#0x2_object_UID">object::UID</a></code>
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

<a name="0x2_authenticator_state_AuthenticatorStateInner"></a>

## Struct `AuthenticatorStateInner`



<pre><code><b>struct</b> <a href="authenticator_state.md#0x2_authenticator_state_AuthenticatorStateInner">AuthenticatorStateInner</a> <b>has</b> store
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
<code>active_jwks: <a href="">vector</a>&lt;<a href="authenticator_state.md#0x2_authenticator_state_ActiveJwk">authenticator_state::ActiveJwk</a>&gt;</code>
</dt>
<dd>
 List of currently active JWKs.
</dd>
</dl>


</details>

<a name="0x2_authenticator_state_JWK"></a>

## Struct `JWK`

Must match the JWK struct in fastcrypto-zkp


<pre><code><b>struct</b> <a href="authenticator_state.md#0x2_authenticator_state_JWK">JWK</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>kty: <a href="_String">string::String</a></code>
</dt>
<dd>

</dd>
<dt>
<code>e: <a href="_String">string::String</a></code>
</dt>
<dd>

</dd>
<dt>
<code>n: <a href="_String">string::String</a></code>
</dt>
<dd>

</dd>
<dt>
<code>alg: <a href="_String">string::String</a></code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x2_authenticator_state_JwkId"></a>

## Struct `JwkId`

Must match the JwkId struct in fastcrypto-zkp


<pre><code><b>struct</b> <a href="authenticator_state.md#0x2_authenticator_state_JwkId">JwkId</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>iss: <a href="_String">string::String</a></code>
</dt>
<dd>

</dd>
<dt>
<code>kid: <a href="_String">string::String</a></code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x2_authenticator_state_ActiveJwk"></a>

## Struct `ActiveJwk`



<pre><code><b>struct</b> <a href="authenticator_state.md#0x2_authenticator_state_ActiveJwk">ActiveJwk</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>jwk_id: <a href="authenticator_state.md#0x2_authenticator_state_JwkId">authenticator_state::JwkId</a></code>
</dt>
<dd>

</dd>
<dt>
<code>jwk: <a href="authenticator_state.md#0x2_authenticator_state_JWK">authenticator_state::JWK</a></code>
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


<a name="0x2_authenticator_state_ENotSystemAddress"></a>

Sender is not @0x0 the system address.


<pre><code><b>const</b> <a href="authenticator_state.md#0x2_authenticator_state_ENotSystemAddress">ENotSystemAddress</a>: u64 = 0;
</code></pre>



<a name="0x2_authenticator_state_CURRENT_VERSION"></a>



<pre><code><b>const</b> <a href="authenticator_state.md#0x2_authenticator_state_CURRENT_VERSION">CURRENT_VERSION</a>: u64 = 1;
</code></pre>



<a name="0x2_authenticator_state_create"></a>

## Function `create`

Create and share the AuthenticatorState object. This function is call exactly once, when
the authenticator state object is first created.
Can only be called by genesis or change_epoch transactions.


<pre><code><b>fun</b> <a href="authenticator_state.md#0x2_authenticator_state_create">create</a>(ctx: &<a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="authenticator_state.md#0x2_authenticator_state_create">create</a>(ctx: &TxContext) {
    <b>assert</b>!(<a href="tx_context.md#0x2_tx_context_sender">tx_context::sender</a>(ctx) == @0x0, <a href="authenticator_state.md#0x2_authenticator_state_ENotSystemAddress">ENotSystemAddress</a>);

    <b>let</b> version = <a href="authenticator_state.md#0x2_authenticator_state_CURRENT_VERSION">CURRENT_VERSION</a>;

    <b>let</b> inner = <a href="authenticator_state.md#0x2_authenticator_state_AuthenticatorStateInner">AuthenticatorStateInner</a> {
        version,
        active_jwks: <a href="">vector</a>[],
    };

    <b>let</b> self = <a href="authenticator_state.md#0x2_authenticator_state_AuthenticatorState">AuthenticatorState</a> {
        id: <a href="object.md#0x2_object_authenticator_state">object::authenticator_state</a>(),
        version,
    };

    <a href="dynamic_field.md#0x2_dynamic_field_add">dynamic_field::add</a>(&<b>mut</b> self.id, version, inner);
    <a href="transfer.md#0x2_transfer_share_object">transfer::share_object</a>(self);
}
</code></pre>



</details>
