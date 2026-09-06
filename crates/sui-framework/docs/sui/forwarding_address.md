---
title: Module `sui::forwarding_address`
---

Registry and resolution for forwarding addresses.

The prototype layout is <code>[8-byte master ID][8 bytes of 0xfd][16-byte tag]</code>.
Integer fields use little-endian BCS encoding.


-  [Struct `ForwardingAddressRegistry`](#sui_forwarding_address_ForwardingAddressRegistry)
-  [Struct `ForwardingDeposit`](#sui_forwarding_address_ForwardingDeposit)
-  [Constants](#@Constants_0)
-  [Function `register`](#sui_forwarding_address_register)
-  [Function `resolve`](#sui_forwarding_address_resolve)
-  [Function `resolve_impl`](#sui_forwarding_address_resolve_impl)
-  [Function `create`](#sui_forwarding_address_create)


<pre><code><b>use</b> <a href="../std/address.md#std_address">std::address</a>;
<b>use</b> <a href="../std/ascii.md#std_ascii">std::ascii</a>;
<b>use</b> <a href="../std/bcs.md#std_bcs">std::bcs</a>;
<b>use</b> <a href="../std/option.md#std_option">std::option</a>;
<b>use</b> <a href="../std/string.md#std_string">std::string</a>;
<b>use</b> <a href="../std/type_name.md#std_type_name">std::type_name</a>;
<b>use</b> <a href="../std/vector.md#std_vector">std::vector</a>;
<b>use</b> <a href="../sui/accumulator.md#sui_accumulator">sui::accumulator</a>;
<b>use</b> <a href="../sui/accumulator_settlement.md#sui_accumulator_settlement">sui::accumulator_settlement</a>;
<b>use</b> <a href="../sui/address.md#sui_address">sui::address</a>;
<b>use</b> <a href="../sui/bcs.md#sui_bcs">sui::bcs</a>;
<b>use</b> <a href="../sui/dynamic_field.md#sui_dynamic_field">sui::dynamic_field</a>;
<b>use</b> <a href="../sui/event.md#sui_event">sui::event</a>;
<b>use</b> <a href="../sui/hash.md#sui_hash">sui::hash</a>;
<b>use</b> <a href="../sui/hex.md#sui_hex">sui::hex</a>;
<b>use</b> <a href="../sui/object.md#sui_object">sui::object</a>;
<b>use</b> <a href="../sui/party.md#sui_party">sui::party</a>;
<b>use</b> <a href="../sui/transfer.md#sui_transfer">sui::transfer</a>;
<b>use</b> <a href="../sui/tx_context.md#sui_tx_context">sui::tx_context</a>;
<b>use</b> <a href="../sui/vec_map.md#sui_vec_map">sui::vec_map</a>;
</code></pre>



<a name="sui_forwarding_address_ForwardingAddressRegistry"></a>

## Struct `ForwardingAddressRegistry`

Singleton shared object whose UID owns immutable master ID registrations.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_ForwardingAddressRegistry">ForwardingAddressRegistry</a> <b>has</b> key
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

<a name="sui_forwarding_address_ForwardingDeposit"></a>

## Struct `ForwardingDeposit`

Emitted when a balance deposit is redirected from a forwarding address to its master.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_ForwardingDeposit">ForwardingDeposit</a>&lt;<b>phantom</b> T&gt; <b>has</b> <b>copy</b>, drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code><a href="../sui/forwarding_address.md#sui_forwarding_address">forwarding_address</a>: <b>address</b></code>
</dt>
<dd>
</dd>
<dt>
<code>master: <b>address</b></code>
</dt>
<dd>
</dd>
<dt>
<code>amount: u64</code>
</dt>
<dd>
</dd>
<dt>
<code>tag: u128</code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="sui_forwarding_address_ENotSystemAddress"></a>



<pre><code>#[error]
<b>const</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_ENotSystemAddress">ENotSystemAddress</a>: vector&lt;u8&gt; = b"Only the system can <a href="../sui/forwarding_address.md#sui_forwarding_address_create">create</a> the forwarding <b>address</b> registry.";
</code></pre>



<a name="sui_forwarding_address_EForwardingAddressUnregistered"></a>



<pre><code>#[error]
<b>const</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_EForwardingAddressUnregistered">EForwardingAddressUnregistered</a>: vector&lt;u8&gt; = b"The forwarding <b>address</b> master ID is not registered.";
</code></pre>



<a name="sui_forwarding_address_register"></a>

## Function `register`

Claim an unregistered <code>master_id</code> for <code>ctx.sender()</code>.

Master IDs are a first-come namespace; they have no external owner. Senders must construct
forwarding addresses only after confirming the intended master registered the ID.

Aborts if <code>master_id</code> is already registered.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_register">register</a>(registry: &<b>mut</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_ForwardingAddressRegistry">sui::forwarding_address::ForwardingAddressRegistry</a>, master_id: u64, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_register">register</a>(
    registry: &<b>mut</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_ForwardingAddressRegistry">ForwardingAddressRegistry</a>,
    master_id: u64,
    ctx: &TxContext,
) {
    <a href="../sui/dynamic_field.md#sui_dynamic_field_add">sui::dynamic_field::add</a>(&<b>mut</b> registry.id, master_id, ctx.sender());
}
</code></pre>



</details>

<a name="sui_forwarding_address_resolve"></a>

## Function `resolve`

Resolve <code>recipient</code> and emit an attribution event when it is a forwarding address.


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_resolve">resolve</a>&lt;T&gt;(recipient: <b>address</b>, amount: u64): <b>address</b>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_resolve">resolve</a>&lt;T&gt;(recipient: <b>address</b>, amount: u64): <b>address</b> {
    <b>let</b> (master, tag, forwarded) = <a href="../sui/forwarding_address.md#sui_forwarding_address_resolve_impl">resolve_impl</a>(recipient);
    <b>if</b> (forwarded) {
        <a href="../sui/event.md#sui_event_emit">sui::event::emit</a>(<a href="../sui/forwarding_address.md#sui_forwarding_address_ForwardingDeposit">ForwardingDeposit</a>&lt;T&gt; {
            <a href="../sui/forwarding_address.md#sui_forwarding_address">forwarding_address</a>: recipient,
            master,
            amount,
            tag,
        });
    };
    master
}
</code></pre>



</details>

<a name="sui_forwarding_address_resolve_impl"></a>

## Function `resolve_impl`



<pre><code><b>fun</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_resolve_impl">resolve_impl</a>(recipient: <b>address</b>): (<b>address</b>, u128, bool)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_resolve_impl">resolve_impl</a>(recipient: <b>address</b>): (<b>address</b>, u128, bool);
</code></pre>



</details>

<a name="sui_forwarding_address_create"></a>

## Function `create`

Create and share the singleton registry at genesis or protocol upgrade.


<pre><code><b>fun</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_create">create</a>(ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_create">create</a>(ctx: &TxContext) {
    <b>assert</b>!(ctx.sender() == @0x0, <a href="../sui/forwarding_address.md#sui_forwarding_address_ENotSystemAddress">ENotSystemAddress</a>);
    <a href="../sui/transfer.md#sui_transfer_share_object">transfer::share_object</a>(<a href="../sui/forwarding_address.md#sui_forwarding_address_ForwardingAddressRegistry">ForwardingAddressRegistry</a> {
        id: <a href="../sui/object.md#sui_object_forwarding_address_registry">object::forwarding_address_registry</a>(),
    });
}
</code></pre>



</details>
