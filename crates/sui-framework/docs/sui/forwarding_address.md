---
title: Module `sui::forwarding_address`
---

Registry and resolution for forwarding addresses.

Address layout: <code>[u32 <a href="../sui/forwarding_address.md#sui_forwarding_address_master_id">master_id</a>, little-endian][10 bytes of 0xfa][u8 variant][17 payload bytes]</code>.
The master ID, magic and variant positions are fixed; the variant alone decides what the
payload bytes mean. Variant 0 gives them no on-chain meaning.


-  [Struct `ForwardingAddressRegistry`](#sui_forwarding_address_ForwardingAddressRegistry)
-  [Struct `MasterCap`](#sui_forwarding_address_MasterCap)
-  [Struct `MasterRecord`](#sui_forwarding_address_MasterRecord)
-  [Struct `MasterIdCounter`](#sui_forwarding_address_MasterIdCounter)
-  [Struct `ForwardingDeposit`](#sui_forwarding_address_ForwardingDeposit)
-  [Struct `MasterRegistered`](#sui_forwarding_address_MasterRegistered)
-  [Constants](#@Constants_0)
-  [Function `register`](#sui_forwarding_address_register)
-  [Function `master_id`](#sui_forwarding_address_master_id)
-  [Function `resolve`](#sui_forwarding_address_resolve)
-  [Function `resolve_impl`](#sui_forwarding_address_resolve_impl)
-  [Function `charge_registration_fee`](#sui_forwarding_address_charge_registration_fee)
-  [Function `allocate_master_id`](#sui_forwarding_address_allocate_master_id)
-  [Function `mix_master_id`](#sui_forwarding_address_mix_master_id)
-  [Function `mul_mod_2_32`](#sui_forwarding_address_mul_mod_2_32)
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

Singleton shared object whose UID owns the master ID records and the allocation counter.


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

<a name="sui_forwarding_address_MasterCap"></a>

## Struct `MasterCap`

Ownership of a master ID, handed to the registrant. Keep it cold; it is what a later
rotation or pause will require.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_MasterCap">MasterCap</a> <b>has</b> key, store
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
<code><a href="../sui/forwarding_address.md#sui_forwarding_address_master_id">master_id</a>: u32</code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="sui_forwarding_address_MasterRecord"></a>

## Struct `MasterRecord`

Dynamic field on the registry, keyed by master ID.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_MasterRecord">MasterRecord</a> <b>has</b> store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>master: <b>address</b></code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="sui_forwarding_address_MasterIdCounter"></a>

## Struct `MasterIdCounter`

Dynamic field key for the next master ID counter (a <code>u64</code>, so the last <code>u32</code> is allocatable).


<pre><code><b>public</b> <b>struct</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_MasterIdCounter">MasterIdCounter</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
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
</dl>


</details>

<a name="sui_forwarding_address_MasterRegistered"></a>

## Struct `MasterRegistered`

Emitted when a master ID is allocated.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_MasterRegistered">MasterRegistered</a> <b>has</b> <b>copy</b>, drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code><a href="../sui/forwarding_address.md#sui_forwarding_address_master_id">master_id</a>: u32</code>
</dt>
<dd>
</dd>
<dt>
<code>master: <b>address</b></code>
</dt>
<dd>
</dd>
<dt>
<code>cap_id: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a></code>
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



<a name="sui_forwarding_address_EForwardingAddressVariantUnsupported"></a>



<pre><code>#[error]
<b>const</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_EForwardingAddressVariantUnsupported">EForwardingAddressVariantUnsupported</a>: vector&lt;u8&gt; = b"The forwarding <b>address</b> variant is not supported by this protocol version.";
</code></pre>



<a name="sui_forwarding_address_EMasterIdsExhausted"></a>



<pre><code>#[error]
<b>const</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_EMasterIdsExhausted">EMasterIdsExhausted</a>: vector&lt;u8&gt; = b"All master IDs have been allocated.";
</code></pre>



<a name="sui_forwarding_address_MAX_MASTER_ID"></a>



<pre><code><b>const</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_MAX_MASTER_ID">MAX_MASTER_ID</a>: u64 = 4294967295;
</code></pre>



<a name="sui_forwarding_address_register"></a>

## Function `register`

Allocate a fresh master ID for <code>ctx.sender()</code> and return the capability for it.

Charges a deliberately high gas fee, since every registration permanently grows the registry.
Aborts once every master ID has been allocated; IDs are never reused.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_register">register</a>(registry: &<b>mut</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_ForwardingAddressRegistry">sui::forwarding_address::ForwardingAddressRegistry</a>, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <a href="../sui/forwarding_address.md#sui_forwarding_address_MasterCap">sui::forwarding_address::MasterCap</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_register">register</a>(registry: &<b>mut</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_ForwardingAddressRegistry">ForwardingAddressRegistry</a>, ctx: &<b>mut</b> TxContext): <a href="../sui/forwarding_address.md#sui_forwarding_address_MasterCap">MasterCap</a> {
    <a href="../sui/forwarding_address.md#sui_forwarding_address_charge_registration_fee">charge_registration_fee</a>();
    <b>let</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_master_id">master_id</a> = <a href="../sui/forwarding_address.md#sui_forwarding_address_allocate_master_id">allocate_master_id</a>(registry);
    <b>let</b> master = ctx.sender();
    <a href="../sui/dynamic_field.md#sui_dynamic_field_add">dynamic_field::add</a>(&<b>mut</b> registry.id, <a href="../sui/forwarding_address.md#sui_forwarding_address_master_id">master_id</a>, <a href="../sui/forwarding_address.md#sui_forwarding_address_MasterRecord">MasterRecord</a> { master });
    <b>let</b> cap = <a href="../sui/forwarding_address.md#sui_forwarding_address_MasterCap">MasterCap</a> { id: <a href="../sui/object.md#sui_object_new">object::new</a>(ctx), <a href="../sui/forwarding_address.md#sui_forwarding_address_master_id">master_id</a> };
    <a href="../sui/event.md#sui_event_emit">event::emit</a>(<a href="../sui/forwarding_address.md#sui_forwarding_address_MasterRegistered">MasterRegistered</a> { <a href="../sui/forwarding_address.md#sui_forwarding_address_master_id">master_id</a>, master, cap_id: <a href="../sui/object.md#sui_object_id">object::id</a>(&cap) });
    cap
}
</code></pre>



</details>

<a name="sui_forwarding_address_master_id"></a>

## Function `master_id`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_master_id">master_id</a>(cap: &<a href="../sui/forwarding_address.md#sui_forwarding_address_MasterCap">sui::forwarding_address::MasterCap</a>): u32
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_master_id">master_id</a>(cap: &<a href="../sui/forwarding_address.md#sui_forwarding_address_MasterCap">MasterCap</a>): u32 {
    cap.<a href="../sui/forwarding_address.md#sui_forwarding_address_master_id">master_id</a>
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
    <b>let</b> (master, forwarded) = <a href="../sui/forwarding_address.md#sui_forwarding_address_resolve_impl">resolve_impl</a>(recipient);
    <b>if</b> (forwarded) {
        <a href="../sui/event.md#sui_event_emit">event::emit</a>(<a href="../sui/forwarding_address.md#sui_forwarding_address_ForwardingDeposit">ForwardingDeposit</a>&lt;T&gt; { <a href="../sui/forwarding_address.md#sui_forwarding_address">forwarding_address</a>: recipient, master, amount });
    };
    master
}
</code></pre>



</details>

<a name="sui_forwarding_address_resolve_impl"></a>

## Function `resolve_impl`



<pre><code><b>fun</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_resolve_impl">resolve_impl</a>(recipient: <b>address</b>): (<b>address</b>, bool)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_resolve_impl">resolve_impl</a>(recipient: <b>address</b>): (<b>address</b>, bool);
</code></pre>



</details>

<a name="sui_forwarding_address_charge_registration_fee"></a>

## Function `charge_registration_fee`



<pre><code><b>fun</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_charge_registration_fee">charge_registration_fee</a>()
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_charge_registration_fee">charge_registration_fee</a>();
</code></pre>



</details>

<a name="sui_forwarding_address_allocate_master_id"></a>

## Function `allocate_master_id`



<pre><code><b>fun</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_allocate_master_id">allocate_master_id</a>(registry: &<b>mut</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_ForwardingAddressRegistry">sui::forwarding_address::ForwardingAddressRegistry</a>): u32
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_allocate_master_id">allocate_master_id</a>(registry: &<b>mut</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_ForwardingAddressRegistry">ForwardingAddressRegistry</a>): u32 {
    <b>if</b> (!<a href="../sui/dynamic_field.md#sui_dynamic_field_exists">dynamic_field::exists</a>(&registry.id, <a href="../sui/forwarding_address.md#sui_forwarding_address_MasterIdCounter">MasterIdCounter</a> {})) {
        // Counter 0 is never allocated so that master ID 0 stays reserved.
        <a href="../sui/dynamic_field.md#sui_dynamic_field_add">dynamic_field::add</a>(&<b>mut</b> registry.id, <a href="../sui/forwarding_address.md#sui_forwarding_address_MasterIdCounter">MasterIdCounter</a> {}, 1u64);
    };
    <b>let</b> next = <a href="../sui/dynamic_field.md#sui_dynamic_field_borrow_mut">dynamic_field::borrow_mut</a>&lt;<a href="../sui/forwarding_address.md#sui_forwarding_address_MasterIdCounter">MasterIdCounter</a>, u64&gt;(
        &<b>mut</b> registry.id,
        <a href="../sui/forwarding_address.md#sui_forwarding_address_MasterIdCounter">MasterIdCounter</a> {},
    );
    <b>assert</b>!(*next &lt;= <a href="../sui/forwarding_address.md#sui_forwarding_address_MAX_MASTER_ID">MAX_MASTER_ID</a>, <a href="../sui/forwarding_address.md#sui_forwarding_address_EMasterIdsExhausted">EMasterIdsExhausted</a>);
    <b>let</b> counter = (*next <b>as</b> u32);
    *next = *next + 1;
    <a href="../sui/forwarding_address.md#sui_forwarding_address_mix_master_id">mix_master_id</a>(counter)
}
</code></pre>



</details>

<a name="sui_forwarding_address_mix_master_id"></a>

## Function `mix_master_id`

lowbias32: a permutation of <code>u32</code> built from xor-shifts and odd multiplications, so distinct
counters always give distinct IDs and 0 is the only preimage of 0. IDs look mixed but are not
secret; the counter is public and the function is invertible.


<pre><code><b>fun</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_mix_master_id">mix_master_id</a>(x: u32): u32
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_mix_master_id">mix_master_id</a>(x: u32): u32 {
    <b>let</b> x = x ^ (x &gt;&gt; 16);
    <b>let</b> x = <a href="../sui/forwarding_address.md#sui_forwarding_address_mul_mod_2_32">mul_mod_2_32</a>(x, 0x7feb352d);
    <b>let</b> x = x ^ (x &gt;&gt; 15);
    <b>let</b> x = <a href="../sui/forwarding_address.md#sui_forwarding_address_mul_mod_2_32">mul_mod_2_32</a>(x, 0x846ca68b);
    x ^ (x &gt;&gt; 16)
}
</code></pre>



</details>

<a name="sui_forwarding_address_mul_mod_2_32"></a>

## Function `mul_mod_2_32`



<pre><code><b>fun</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_mul_mod_2_32">mul_mod_2_32</a>(a: u32, b: u32): u32
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_mul_mod_2_32">mul_mod_2_32</a>(a: u32, b: u32): u32 {
    (((a <b>as</b> u64) * (b <b>as</b> u64)) & <a href="../sui/forwarding_address.md#sui_forwarding_address_MAX_MASTER_ID">MAX_MASTER_ID</a>) <b>as</b> u32
}
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
