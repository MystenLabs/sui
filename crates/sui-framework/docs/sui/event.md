---
title: Module `sui::event`
---

Events module. Defines the <code><a href="../sui/event.md#sui_event_emit">sui::event::emit</a></code> function which
creates and sends a custom MoveEvent as a part of the effects
certificate of the transaction.

Every MoveEvent has the following properties:
- sender
- type signature (<code>T</code>)
- event data (the value of <code>T</code>)
- timestamp (local to a node)
- transaction digest

Example:
```
module my::marketplace {
use sui::event;
/* ... */
struct ItemPurchased has copy, drop {
item_id: ID, buyer: address
}
entry fun buy(/* .... */) {
/* ... */
event::emit(ItemPurchased { item_id: ..., buyer: .... })
}
}
```


-  [Struct `EventStreamHead`](#sui_event_EventStreamHead)
-  [Struct `EventStream`](#sui_event_EventStream)
-  [Struct `EventStreamCap`](#sui_event_EventStreamCap)
-  [Constants](#@Constants_0)
-  [Function `emit`](#sui_event_emit)
-  [Function `update_head`](#sui_event_update_head)
-  [Function `new_event_stream`](#sui_event_new_event_stream)
-  [Function `destroy_stream`](#sui_event_destroy_stream)
-  [Function `get_cap`](#sui_event_get_cap)
-  [Function `default_event_stream_cap`](#sui_event_default_event_stream_cap)
-  [Function `destroy_cap`](#sui_event_destroy_cap)
-  [Function `emit_authenticated`](#sui_event_emit_authenticated)
-  [Function `emit_authenticated_impl`](#sui_event_emit_authenticated_impl)


<pre><code><b>use</b> <a href="../std/address.md#std_address">std::address</a>;
<b>use</b> <a href="../std/ascii.md#std_ascii">std::ascii</a>;
<b>use</b> <a href="../std/bcs.md#std_bcs">std::bcs</a>;
<b>use</b> <a href="../std/option.md#std_option">std::option</a>;
<b>use</b> <a href="../std/string.md#std_string">std::string</a>;
<b>use</b> <a href="../std/type_name.md#std_type_name">std::type_name</a>;
<b>use</b> <a href="../std/vector.md#std_vector">std::vector</a>;
<b>use</b> <a href="../sui/accumulator.md#sui_accumulator">sui::accumulator</a>;
<b>use</b> <a href="../sui/address.md#sui_address">sui::address</a>;
<b>use</b> <a href="../sui/dynamic_field.md#sui_dynamic_field">sui::dynamic_field</a>;
<b>use</b> <a href="../sui/hash.md#sui_hash">sui::hash</a>;
<b>use</b> <a href="../sui/hex.md#sui_hex">sui::hex</a>;
<b>use</b> <a href="../sui/object.md#sui_object">sui::object</a>;
<b>use</b> <a href="../sui/transfer.md#sui_transfer">sui::transfer</a>;
<b>use</b> <a href="../sui/tx_context.md#sui_tx_context">sui::tx_context</a>;
</code></pre>



<a name="sui_event_EventStreamHead"></a>

## Struct `EventStreamHead`



<pre><code><b>public</b> <b>struct</b> <a href="../sui/event.md#sui_event_EventStreamHead">EventStreamHead</a> <b>has</b> store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>root: vector&lt;u8&gt;</code>
</dt>
<dd>
 Merkle root for all events in the current checkpoint.
</dd>
<dt>
<code>prev: vector&lt;u8&gt;</code>
</dt>
<dd>
 Hash of the previous version of the head object.
</dd>
</dl>


</details>

<a name="sui_event_EventStream"></a>

## Struct `EventStream`



<pre><code><b>public</b> <b>struct</b> <a href="../sui/event.md#sui_event_EventStream">EventStream</a> <b>has</b> store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>name: <a href="../sui/object.md#sui_object_UID">sui::object::UID</a></code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="sui_event_EventStreamCap"></a>

## Struct `EventStreamCap`



<pre><code><b>public</b> <b>struct</b> <a href="../sui/event.md#sui_event_EventStreamCap">EventStreamCap</a> <b>has</b> key, store
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
<code>stream_id: <b>address</b></code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="sui_event_ENotSystemAddress"></a>



<pre><code><b>const</b> <a href="../sui/event.md#sui_event_ENotSystemAddress">ENotSystemAddress</a>: u64 = 0;
</code></pre>



<a name="sui_event_emit"></a>

## Function `emit`

Emit a custom Move event, sending the data offchain.

Used for creating custom indexes and tracking onchain
activity in a way that suits a specific application the most.

The type <code>T</code> is the main way to index the event, and can contain
phantom parameters, eg <code><a href="../sui/event.md#sui_event_emit">emit</a>(MyEvent&lt;<b>phantom</b> T&gt;)</code>.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/event.md#sui_event_emit">emit</a>&lt;T: <b>copy</b>, drop&gt;(<a href="../sui/event.md#sui_event">event</a>: T)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>native</b> <b>fun</b> <a href="../sui/event.md#sui_event_emit">emit</a>&lt;T: <b>copy</b> + drop&gt;(<a href="../sui/event.md#sui_event">event</a>: T);
</code></pre>



</details>

<a name="sui_event_update_head"></a>

## Function `update_head`



<pre><code><b>entry</b> <b>fun</b> <a href="../sui/event.md#sui_event_update_head">update_head</a>(accumulator_root: &<b>mut</b> <a href="../sui/accumulator.md#sui_accumulator_Accumulator">sui::accumulator::Accumulator</a>, stream_id: <b>address</b>, new_root: vector&lt;u8&gt;, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>entry</b> <b>fun</b> <a href="../sui/event.md#sui_event_update_head">update_head</a>(accumulator_root: &<b>mut</b> <a href="../sui/accumulator.md#sui_accumulator_Accumulator">accumulator::Accumulator</a>, stream_id: <b>address</b>, new_root: vector&lt;u8&gt;, ctx: &TxContext) {
    <b>assert</b>!(ctx.sender() == @0x0, <a href="../sui/event.md#sui_event_ENotSystemAddress">ENotSystemAddress</a>);
    <b>let</b> name = <a href="../sui/accumulator.md#sui_accumulator_get_accumulator_field_name">accumulator::get_accumulator_field_name</a>&lt;<a href="../sui/event.md#sui_event_EventStreamHead">EventStreamHead</a>&gt;(stream_id);
    <b>let</b> accumulator_root_id = accumulator_root.id();
    <b>if</b> (<a href="../sui/dynamic_field.md#sui_dynamic_field_exists_with_type">dynamic_field::exists_with_type</a>&lt;<a href="../sui/accumulator.md#sui_accumulator_Key">accumulator::Key</a>, <a href="../sui/event.md#sui_event_EventStreamHead">EventStreamHead</a>&gt;(accumulator_root_id, name)) {
        <b>let</b> head: &<b>mut</b> <a href="../sui/event.md#sui_event_EventStreamHead">EventStreamHead</a> = <a href="../sui/dynamic_field.md#sui_dynamic_field_borrow_mut">dynamic_field::borrow_mut</a>(accumulator_root_id, name);
        <b>let</b> prev_bytes = <a href="../sui/bcs.md#sui_bcs_to_bytes">bcs::to_bytes</a>(head);
        <b>let</b> prev = <a href="../sui/hash.md#sui_hash_blake2b256">hash::blake2b256</a>(&prev_bytes);
        head.prev = prev;
        head.root = new_root;
    } <b>else</b> {
        <b>let</b> head = <a href="../sui/event.md#sui_event_EventStreamHead">EventStreamHead</a> {
            root: new_root,
            prev: <a href="../sui/address.md#sui_address_to_bytes">address::to_bytes</a>(<a href="../sui/address.md#sui_address_from_u256">address::from_u256</a>(0)),
        };
        <a href="../sui/dynamic_field.md#sui_dynamic_field_add">dynamic_field::add</a>(accumulator_root_id, name, head);
    };
}
</code></pre>



</details>

<a name="sui_event_new_event_stream"></a>

## Function `new_event_stream`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/event.md#sui_event_new_event_stream">new_event_stream</a>(ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <a href="../sui/event.md#sui_event_EventStream">sui::event::EventStream</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/event.md#sui_event_new_event_stream">new_event_stream</a>(ctx: &<b>mut</b> TxContext): <a href="../sui/event.md#sui_event_EventStream">EventStream</a> {
    <a href="../sui/event.md#sui_event_EventStream">EventStream</a> {
        name: <a href="../sui/object.md#sui_object_new">object::new</a>(ctx),
    }
}
</code></pre>



</details>

<a name="sui_event_destroy_stream"></a>

## Function `destroy_stream`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/event.md#sui_event_destroy_stream">destroy_stream</a>(stream: <a href="../sui/event.md#sui_event_EventStream">sui::event::EventStream</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/event.md#sui_event_destroy_stream">destroy_stream</a>(stream: <a href="../sui/event.md#sui_event_EventStream">EventStream</a>) {
    <b>let</b> <a href="../sui/event.md#sui_event_EventStream">EventStream</a> { name } = stream;
    name.delete();
}
</code></pre>



</details>

<a name="sui_event_get_cap"></a>

## Function `get_cap`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/event.md#sui_event_get_cap">get_cap</a>(stream: &<a href="../sui/event.md#sui_event_EventStream">sui::event::EventStream</a>, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <a href="../sui/event.md#sui_event_EventStreamCap">sui::event::EventStreamCap</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/event.md#sui_event_get_cap">get_cap</a>(stream: &<a href="../sui/event.md#sui_event_EventStream">EventStream</a>, ctx: &<b>mut</b> TxContext): <a href="../sui/event.md#sui_event_EventStreamCap">EventStreamCap</a> {
    <a href="../sui/event.md#sui_event_EventStreamCap">EventStreamCap</a> {
        id: <a href="../sui/object.md#sui_object_new">object::new</a>(ctx),
        stream_id: stream.name.to_address(),
    }
}
</code></pre>



</details>

<a name="sui_event_default_event_stream_cap"></a>

## Function `default_event_stream_cap`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/event.md#sui_event_default_event_stream_cap">default_event_stream_cap</a>&lt;T: <b>copy</b>, drop&gt;(ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <a href="../sui/event.md#sui_event_EventStreamCap">sui::event::EventStreamCap</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/event.md#sui_event_default_event_stream_cap">default_event_stream_cap</a>&lt;T: <b>copy</b> + drop&gt;(ctx: &<b>mut</b> TxContext): <a href="../sui/event.md#sui_event_EventStreamCap">EventStreamCap</a> {
    <a href="../sui/event.md#sui_event_EventStreamCap">EventStreamCap</a> {
        id: <a href="../sui/object.md#sui_object_new">object::new</a>(ctx),
        stream_id: type_name::get_original_package_id&lt;T&gt;(),
    }
}
</code></pre>



</details>

<a name="sui_event_destroy_cap"></a>

## Function `destroy_cap`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/event.md#sui_event_destroy_cap">destroy_cap</a>(cap: <a href="../sui/event.md#sui_event_EventStreamCap">sui::event::EventStreamCap</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/event.md#sui_event_destroy_cap">destroy_cap</a>(cap: <a href="../sui/event.md#sui_event_EventStreamCap">EventStreamCap</a>) {
    <b>let</b> <a href="../sui/event.md#sui_event_EventStreamCap">EventStreamCap</a> { id, .. } = cap;
    id.delete();
}
</code></pre>



</details>

<a name="sui_event_emit_authenticated"></a>

## Function `emit_authenticated`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/event.md#sui_event_emit_authenticated">emit_authenticated</a>&lt;T: <b>copy</b>, drop&gt;(cap: &<a href="../sui/event.md#sui_event_EventStreamCap">sui::event::EventStreamCap</a>, <a href="../sui/event.md#sui_event">event</a>: T)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/event.md#sui_event_emit_authenticated">emit_authenticated</a>&lt;T: <b>copy</b> + drop&gt;(cap: &<a href="../sui/event.md#sui_event_EventStreamCap">EventStreamCap</a>, <a href="../sui/event.md#sui_event">event</a>: T) {
    <b>let</b> accumulator_addr = <a href="../sui/accumulator.md#sui_accumulator_get_accumulator_field_address">accumulator::get_accumulator_field_address</a>&lt;<a href="../sui/event.md#sui_event_EventStreamHead">EventStreamHead</a>&gt;(cap.stream_id);
    <a href="../sui/event.md#sui_event_emit_authenticated_impl">emit_authenticated_impl</a>&lt;<a href="../sui/event.md#sui_event_EventStreamHead">EventStreamHead</a>, T&gt;(accumulator_addr, cap.stream_id, <a href="../sui/event.md#sui_event">event</a>);
}
</code></pre>



</details>

<a name="sui_event_emit_authenticated_impl"></a>

## Function `emit_authenticated_impl`

TODO: needs verifier rule like <code><a href="../sui/event.md#sui_event_emit">emit</a></code> to ensure it is only called in package that defines <code>T</code>
Like <code><a href="../sui/event.md#sui_event_emit">emit</a></code>, but also adds an on-chain committment to the event to the
stream <code>stream</code>.


<pre><code><b>fun</b> <a href="../sui/event.md#sui_event_emit_authenticated_impl">emit_authenticated_impl</a>&lt;StreamHeadT, T: <b>copy</b>, drop&gt;(accumulator_id: <b>address</b>, stream: <b>address</b>, <a href="../sui/event.md#sui_event">event</a>: T)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="../sui/event.md#sui_event_emit_authenticated_impl">emit_authenticated_impl</a>&lt;StreamHeadT, T: <b>copy</b> + drop&gt;(accumulator_id: <b>address</b>, stream: <b>address</b>, <a href="../sui/event.md#sui_event">event</a>: T);
</code></pre>



</details>
