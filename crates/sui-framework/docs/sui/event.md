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
-  [Constants](#@Constants_0)
-  [Function `emit`](#sui_event_emit)
-  [Function `add_to_mmr`](#sui_event_add_to_mmr)
-  [Function `hash_two_to_one_via_bcs`](#sui_event_hash_two_to_one_via_bcs)
-  [Function `update_head`](#sui_event_update_head)
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
<b>use</b> <a href="../sui/bcs.md#sui_bcs">sui::bcs</a>;
<b>use</b> <a href="../sui/dynamic_field.md#sui_dynamic_field">sui::dynamic_field</a>;
<b>use</b> <a href="../sui/hash.md#sui_hash">sui::hash</a>;
<b>use</b> <a href="../sui/hex.md#sui_hex">sui::hex</a>;
<b>use</b> <a href="../sui/object.md#sui_object">sui::object</a>;
<b>use</b> <a href="../sui/party.md#sui_party">sui::party</a>;
<b>use</b> <a href="../sui/transfer.md#sui_transfer">sui::transfer</a>;
<b>use</b> <a href="../sui/tx_context.md#sui_tx_context">sui::tx_context</a>;
<b>use</b> <a href="../sui/vec_map.md#sui_vec_map">sui::vec_map</a>;
</code></pre>



<a name="sui_event_EventStreamHead"></a>

## Struct `EventStreamHead`



<pre><code><b>public</b> <b>struct</b> <a href="../sui/event.md#sui_event_EventStreamHead">EventStreamHead</a> <b>has</b> store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>mmr: vector&lt;vector&lt;u8&gt;&gt;</code>
</dt>
<dd>
 Merkle Mountain Range of all events in the stream.
</dd>
<dt>
<code>checkpoint_seq: u64</code>
</dt>
<dd>
 Checkpoint sequence number at which the event stream was written.
</dd>
<dt>
<code>num_events: u64</code>
</dt>
<dd>
 Number of events in the stream.
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

<a name="sui_event_add_to_mmr"></a>

## Function `add_to_mmr`



<pre><code><b>fun</b> <a href="../sui/event.md#sui_event_add_to_mmr">add_to_mmr</a>(new_val: vector&lt;u8&gt;, mmr: &<b>mut</b> vector&lt;vector&lt;u8&gt;&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/event.md#sui_event_add_to_mmr">add_to_mmr</a>(new_val: vector&lt;u8&gt;, mmr: &<b>mut</b> vector&lt;vector&lt;u8&gt;&gt;) {
    <b>let</b> <b>mut</b> i = 0;
    <b>let</b> <b>mut</b> cur = new_val;
    <b>while</b> (i &lt; vector::length(mmr)) {
        <b>let</b> r = vector::borrow_mut(mmr, i);
        <b>if</b> (vector::is_empty(r)) {
            *r = cur;
            <b>return</b>
        } <b>else</b> {
            cur = <a href="../sui/event.md#sui_event_hash_two_to_one_via_bcs">hash_two_to_one_via_bcs</a>(*r, cur);
            *r = vector::empty();
        };
        i = i + 1;
    };
    // Vector length insufficient. Increase by 1.
    vector::push_back(mmr, cur);
}
</code></pre>



</details>

<a name="sui_event_hash_two_to_one_via_bcs"></a>

## Function `hash_two_to_one_via_bcs`



<pre><code><b>fun</b> <a href="../sui/event.md#sui_event_hash_two_to_one_via_bcs">hash_two_to_one_via_bcs</a>(left: vector&lt;u8&gt;, right: vector&lt;u8&gt;): vector&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/event.md#sui_event_hash_two_to_one_via_bcs">hash_two_to_one_via_bcs</a>(left: vector&lt;u8&gt;, right: vector&lt;u8&gt;): vector&lt;u8&gt; {
    <b>let</b> left_bytes = <a href="../sui/bcs.md#sui_bcs_to_bytes">bcs::to_bytes</a>(&left);
    <b>let</b> right_bytes = <a href="../sui/bcs.md#sui_bcs_to_bytes">bcs::to_bytes</a>(&right);
    <b>let</b> <b>mut</b> concatenated = left_bytes;
    vector::append(&<b>mut</b> concatenated, right_bytes);
    <a href="../sui/hash.md#sui_hash_blake2b256">hash::blake2b256</a>(&concatenated)
}
</code></pre>



</details>

<a name="sui_event_update_head"></a>

## Function `update_head`



<pre><code><b>entry</b> <b>fun</b> <a href="../sui/event.md#sui_event_update_head">update_head</a>(accumulator_root: &<b>mut</b> <a href="../sui/accumulator.md#sui_accumulator_AccumulatorRoot">sui::accumulator::AccumulatorRoot</a>, stream_id: <b>address</b>, new_root: vector&lt;u8&gt;, event_count_delta: u64, checkpoint_seq: u64, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>entry</b> <b>fun</b> <a href="../sui/event.md#sui_event_update_head">update_head</a>(
    accumulator_root: &<b>mut</b> <a href="../sui/accumulator.md#sui_accumulator_AccumulatorRoot">accumulator::AccumulatorRoot</a>,
    stream_id: <b>address</b>,
    new_root: vector&lt;u8&gt;,
    event_count_delta: u64,
    checkpoint_seq: u64,
    ctx: &TxContext,
) {
    <b>assert</b>!(ctx.sender() == @0x0, <a href="../sui/event.md#sui_event_ENotSystemAddress">ENotSystemAddress</a>);
    <b>let</b> name = <a href="../sui/accumulator.md#sui_accumulator_accumulator_key">accumulator::accumulator_key</a>&lt;<a href="../sui/event.md#sui_event_EventStreamHead">EventStreamHead</a>&gt;(stream_id);
    <b>if</b> (
        <a href="../sui/dynamic_field.md#sui_dynamic_field_exists_with_type">dynamic_field::exists_with_type</a>&lt;<a href="../sui/accumulator.md#sui_accumulator_Key">accumulator::Key</a>&lt;<a href="../sui/event.md#sui_event_EventStreamHead">EventStreamHead</a>&gt;, <a href="../sui/event.md#sui_event_EventStreamHead">EventStreamHead</a>&gt;(
            accumulator_root.id(),
            <b>copy</b> name,
        )
    ) {
        <b>let</b> head: &<b>mut</b> <a href="../sui/event.md#sui_event_EventStreamHead">EventStreamHead</a> = <a href="../sui/dynamic_field.md#sui_dynamic_field_borrow_mut">dynamic_field::borrow_mut</a>&lt;
            <a href="../sui/accumulator.md#sui_accumulator_Key">accumulator::Key</a>&lt;<a href="../sui/event.md#sui_event_EventStreamHead">EventStreamHead</a>&gt;,
            <a href="../sui/event.md#sui_event_EventStreamHead">EventStreamHead</a>,
        &gt;(accumulator_root.id_mut(), name);
        <a href="../sui/event.md#sui_event_add_to_mmr">add_to_mmr</a>(new_root, &<b>mut</b> head.mmr);
        head.num_events = head.num_events + event_count_delta;
        head.checkpoint_seq = checkpoint_seq;
    } <b>else</b> {
        <b>let</b> <b>mut</b> initial_mmr = vector::empty();
        <a href="../sui/event.md#sui_event_add_to_mmr">add_to_mmr</a>(new_root, &<b>mut</b> initial_mmr);
        <b>let</b> head = <a href="../sui/event.md#sui_event_EventStreamHead">EventStreamHead</a> {
            mmr: initial_mmr,
            checkpoint_seq: checkpoint_seq,
            num_events: event_count_delta,
        };
        <a href="../sui/dynamic_field.md#sui_dynamic_field_add">dynamic_field::add</a>&lt;<a href="../sui/accumulator.md#sui_accumulator_Key">accumulator::Key</a>&lt;<a href="../sui/event.md#sui_event_EventStreamHead">EventStreamHead</a>&gt;, <a href="../sui/event.md#sui_event_EventStreamHead">EventStreamHead</a>&gt;(
            accumulator_root.id_mut(),
            name,
            head,
        );
    };
}
</code></pre>



</details>

<a name="sui_event_emit_authenticated"></a>

## Function `emit_authenticated`

Emits a custom Move event which can be authenticated by a light client.

This method emits the authenticated event to the event stream for the Move package that
defines the event type <code>T</code>.
Only the package that defines the type <code>T</code> can emit authenticated events to this stream.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/event.md#sui_event_emit_authenticated">emit_authenticated</a>&lt;T: <b>copy</b>, drop&gt;(<a href="../sui/event.md#sui_event">event</a>: T)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/event.md#sui_event_emit_authenticated">emit_authenticated</a>&lt;T: <b>copy</b> + drop&gt;(<a href="../sui/event.md#sui_event">event</a>: T) {
    <b>let</b> stream_id = type_name::original_id&lt;T&gt;();
    <b>let</b> accumulator_addr = <a href="../sui/accumulator.md#sui_accumulator_accumulator_address">accumulator::accumulator_address</a>&lt;<a href="../sui/event.md#sui_event_EventStreamHead">EventStreamHead</a>&gt;(stream_id);
    <a href="../sui/event.md#sui_event_emit_authenticated_impl">emit_authenticated_impl</a>&lt;<a href="../sui/event.md#sui_event_EventStreamHead">EventStreamHead</a>, T&gt;(accumulator_addr, stream_id, <a href="../sui/event.md#sui_event">event</a>);
}
</code></pre>



</details>

<a name="sui_event_emit_authenticated_impl"></a>

## Function `emit_authenticated_impl`



<pre><code><b>fun</b> <a href="../sui/event.md#sui_event_emit_authenticated_impl">emit_authenticated_impl</a>&lt;StreamHeadT, T: <b>copy</b>, drop&gt;(accumulator_id: <b>address</b>, stream: <b>address</b>, <a href="../sui/event.md#sui_event">event</a>: T)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="../sui/event.md#sui_event_emit_authenticated_impl">emit_authenticated_impl</a>&lt;StreamHeadT, T: <b>copy</b> + drop&gt;(
    accumulator_id: <b>address</b>,
    stream: <b>address</b>,
    <a href="../sui/event.md#sui_event">event</a>: T,
);
</code></pre>



</details>
