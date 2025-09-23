---
title: Module `sui::accumulator_settlement`
---



-  [Struct `EventStreamHead`](#sui_accumulator_settlement_EventStreamHead)
-  [Constants](#@Constants_0)
-  [Function `settlement_prologue`](#sui_accumulator_settlement_settlement_prologue)
-  [Function `settle_u128`](#sui_accumulator_settlement_settle_u128)
-  [Function `record_settlement_sui_conservation`](#sui_accumulator_settlement_record_settlement_sui_conservation)
-  [Function `add_to_mmr`](#sui_accumulator_settlement_add_to_mmr)
-  [Function `u256_from_bytes`](#sui_accumulator_settlement_u256_from_bytes)
-  [Function `hash_two_to_one_u256`](#sui_accumulator_settlement_hash_two_to_one_u256)
-  [Function `new_stream_head`](#sui_accumulator_settlement_new_stream_head)
-  [Function `settle_events`](#sui_accumulator_settlement_settle_events)


<pre><code><b>use</b> <a href="../std/ascii.md#std_ascii">std::ascii</a>;
<b>use</b> <a href="../std/bcs.md#std_bcs">std::bcs</a>;
<b>use</b> <a href="../std/option.md#std_option">std::option</a>;
<b>use</b> <a href="../std/string.md#std_string">std::string</a>;
<b>use</b> <a href="../std/vector.md#std_vector">std::vector</a>;
<b>use</b> <a href="../sui/accumulator.md#sui_accumulator">sui::accumulator</a>;
<b>use</b> <a href="../sui/accumulator_metadata.md#sui_accumulator_metadata">sui::accumulator_metadata</a>;
<b>use</b> <a href="../sui/address.md#sui_address">sui::address</a>;
<b>use</b> <a href="../sui/bag.md#sui_bag">sui::bag</a>;
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



<a name="sui_accumulator_settlement_EventStreamHead"></a>

## Struct `EventStreamHead`



<pre><code><b>public</b> <b>struct</b> <a href="../sui/accumulator_settlement.md#sui_accumulator_settlement_EventStreamHead">EventStreamHead</a> <b>has</b> store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>mmr: vector&lt;u256&gt;</code>
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


<a name="sui_accumulator_settlement_ENotSystemAddress"></a>



<pre><code><b>const</b> <a href="../sui/accumulator_settlement.md#sui_accumulator_settlement_ENotSystemAddress">ENotSystemAddress</a>: u64 = 0;
</code></pre>



<a name="sui_accumulator_settlement_EInvalidSplitAmount"></a>



<pre><code><b>const</b> <a href="../sui/accumulator_settlement.md#sui_accumulator_settlement_EInvalidSplitAmount">EInvalidSplitAmount</a>: u64 = 1;
</code></pre>



<a name="sui_accumulator_settlement_settlement_prologue"></a>

## Function `settlement_prologue`

Called by settlement transactions to ensure that the settlement transaction has a unique
digest.


<pre><code><b>fun</b> <a href="../sui/accumulator_settlement.md#sui_accumulator_settlement_settlement_prologue">settlement_prologue</a>(_epoch: u64, _checkpoint_height: u64, _idx: u64, input_sui: u64, output_sui: u64, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/accumulator_settlement.md#sui_accumulator_settlement_settlement_prologue">settlement_prologue</a>(
    _epoch: u64,
    _checkpoint_height: u64,
    _idx: u64,
    // Total input <a href="../sui/sui.md#sui_sui">sui</a> received from user transactions
    input_sui: u64,
    // Total output <a href="../sui/sui.md#sui_sui">sui</a> withdrawn by user transactions
    output_sui: u64,
    ctx: &TxContext,
) {
    <b>assert</b>!(ctx.sender() == @0x0, <a href="../sui/accumulator_settlement.md#sui_accumulator_settlement_ENotSystemAddress">ENotSystemAddress</a>);
    <a href="../sui/accumulator_settlement.md#sui_accumulator_settlement_record_settlement_sui_conservation">record_settlement_sui_conservation</a>(input_sui, output_sui);
}
</code></pre>



</details>

<a name="sui_accumulator_settlement_settle_u128"></a>

## Function `settle_u128`



<pre><code><b>fun</b> <a href="../sui/accumulator_settlement.md#sui_accumulator_settlement_settle_u128">settle_u128</a>&lt;T&gt;(accumulator_root: &<b>mut</b> <a href="../sui/accumulator.md#sui_accumulator_AccumulatorRoot">sui::accumulator::AccumulatorRoot</a>, owner: <b>address</b>, merge: u128, split: u128, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/accumulator_settlement.md#sui_accumulator_settlement_settle_u128">settle_u128</a>&lt;T&gt;(
    accumulator_root: &<b>mut</b> AccumulatorRoot,
    owner: <b>address</b>,
    merge: u128,
    split: u128,
    ctx: &<b>mut</b> TxContext,
) {
    <b>assert</b>!(ctx.sender() == @0x0, <a href="../sui/accumulator_settlement.md#sui_accumulator_settlement_ENotSystemAddress">ENotSystemAddress</a>);
    // Merge and split should be netted out prior to calling this function.
    <b>assert</b>!((merge == 0 ) != (split == 0), <a href="../sui/accumulator_settlement.md#sui_accumulator_settlement_EInvalidSplitAmount">EInvalidSplitAmount</a>);
    <b>let</b> name = accumulator_key&lt;T&gt;(owner);
    <b>if</b> (accumulator_root.has_accumulator&lt;T, U128&gt;(name)) {
        <b>let</b> is_zero = {
            <b>let</b> value: &<b>mut</b> U128 = accumulator_root.borrow_accumulator_mut(name);
            value.update(merge, split);
            value.is_zero()
        };
        <b>if</b> (is_zero) {
            <b>let</b> value = accumulator_root.remove_accumulator&lt;T, U128&gt;(name);
            destroy_u128(value);
            accumulator_root.remove_metadata&lt;T&gt;(owner);
        }
    } <b>else</b> {
        // cannot split <b>if</b> the field does not yet exist
        <b>assert</b>!(split == 0, <a href="../sui/accumulator_settlement.md#sui_accumulator_settlement_EInvalidSplitAmount">EInvalidSplitAmount</a>);
        <b>let</b> value = create_u128(merge);
        accumulator_root.add_accumulator(name, value);
        accumulator_root.create_metadata&lt;T&gt;(owner, ctx);
    };
}
</code></pre>



</details>

<a name="sui_accumulator_settlement_record_settlement_sui_conservation"></a>

## Function `record_settlement_sui_conservation`

Called by the settlement transaction to track conservation of SUI.


<pre><code><b>fun</b> <a href="../sui/accumulator_settlement.md#sui_accumulator_settlement_record_settlement_sui_conservation">record_settlement_sui_conservation</a>(input_sui: u64, output_sui: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="../sui/accumulator_settlement.md#sui_accumulator_settlement_record_settlement_sui_conservation">record_settlement_sui_conservation</a>(input_sui: u64, output_sui: u64);
</code></pre>



</details>

<a name="sui_accumulator_settlement_add_to_mmr"></a>

## Function `add_to_mmr`



<pre><code><b>fun</b> <a href="../sui/accumulator_settlement.md#sui_accumulator_settlement_add_to_mmr">add_to_mmr</a>(new_val: u256, mmr: &<b>mut</b> vector&lt;u256&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/accumulator_settlement.md#sui_accumulator_settlement_add_to_mmr">add_to_mmr</a>(new_val: u256, mmr: &<b>mut</b> vector&lt;u256&gt;) {
    <b>let</b> <b>mut</b> i = 0;
    <b>let</b> <b>mut</b> cur = new_val;
    <b>while</b> (i &lt; vector::length(mmr)) {
        <b>let</b> r = vector::borrow_mut(mmr, i);
        <b>if</b> (*r == 0) {
            *r = cur;
            <b>return</b>
        } <b>else</b> {
            cur = <a href="../sui/accumulator_settlement.md#sui_accumulator_settlement_hash_two_to_one_u256">hash_two_to_one_u256</a>(*r, cur);
            *r = 0;
        };
        i = i + 1;
    };
    // Vector length insufficient. Increase by 1.
    vector::push_back(mmr, cur);
}
</code></pre>



</details>

<a name="sui_accumulator_settlement_u256_from_bytes"></a>

## Function `u256_from_bytes`



<pre><code><b>fun</b> <a href="../sui/accumulator_settlement.md#sui_accumulator_settlement_u256_from_bytes">u256_from_bytes</a>(bytes: vector&lt;u8&gt;): u256
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/accumulator_settlement.md#sui_accumulator_settlement_u256_from_bytes">u256_from_bytes</a>(bytes: vector&lt;u8&gt;): u256 {
    <a href="../sui/bcs.md#sui_bcs_new">bcs::new</a>(bytes).peel_u256()
}
</code></pre>



</details>

<a name="sui_accumulator_settlement_hash_two_to_one_u256"></a>

## Function `hash_two_to_one_u256`



<pre><code><b>fun</b> <a href="../sui/accumulator_settlement.md#sui_accumulator_settlement_hash_two_to_one_u256">hash_two_to_one_u256</a>(left: u256, right: u256): u256
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/accumulator_settlement.md#sui_accumulator_settlement_hash_two_to_one_u256">hash_two_to_one_u256</a>(left: u256, right: u256): u256 {
    <b>let</b> left_bytes = <a href="../sui/bcs.md#sui_bcs_to_bytes">bcs::to_bytes</a>(&left);
    <b>let</b> right_bytes = <a href="../sui/bcs.md#sui_bcs_to_bytes">bcs::to_bytes</a>(&right);
    <b>let</b> <b>mut</b> concatenated = left_bytes;
    vector::append(&<b>mut</b> concatenated, right_bytes);
    <a href="../sui/accumulator_settlement.md#sui_accumulator_settlement_u256_from_bytes">u256_from_bytes</a>(<a href="../sui/hash.md#sui_hash_blake2b256">hash::blake2b256</a>(&concatenated))
}
</code></pre>



</details>

<a name="sui_accumulator_settlement_new_stream_head"></a>

## Function `new_stream_head`



<pre><code><b>fun</b> <a href="../sui/accumulator_settlement.md#sui_accumulator_settlement_new_stream_head">new_stream_head</a>(new_root: u256, event_count_delta: u64, checkpoint_seq: u64): <a href="../sui/accumulator_settlement.md#sui_accumulator_settlement_EventStreamHead">sui::accumulator_settlement::EventStreamHead</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/accumulator_settlement.md#sui_accumulator_settlement_new_stream_head">new_stream_head</a>(new_root: u256, event_count_delta: u64, checkpoint_seq: u64): <a href="../sui/accumulator_settlement.md#sui_accumulator_settlement_EventStreamHead">EventStreamHead</a> {
    <b>let</b> <b>mut</b> initial_mmr = vector::empty();
    <a href="../sui/accumulator_settlement.md#sui_accumulator_settlement_add_to_mmr">add_to_mmr</a>(new_root, &<b>mut</b> initial_mmr);
    <a href="../sui/accumulator_settlement.md#sui_accumulator_settlement_EventStreamHead">EventStreamHead</a> {
        mmr: initial_mmr,
        checkpoint_seq: checkpoint_seq,
        num_events: event_count_delta,
    }
}
</code></pre>



</details>

<a name="sui_accumulator_settlement_settle_events"></a>

## Function `settle_events`



<pre><code><b>entry</b> <b>fun</b> <a href="../sui/accumulator_settlement.md#sui_accumulator_settlement_settle_events">settle_events</a>(accumulator_root: &<b>mut</b> <a href="../sui/accumulator.md#sui_accumulator_AccumulatorRoot">sui::accumulator::AccumulatorRoot</a>, stream_id: <b>address</b>, new_root: u256, event_count_delta: u64, checkpoint_seq: u64, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>entry</b> <b>fun</b> <a href="../sui/accumulator_settlement.md#sui_accumulator_settlement_settle_events">settle_events</a>(
    accumulator_root: &<b>mut</b> AccumulatorRoot,
    stream_id: <b>address</b>,
    new_root: u256,
    event_count_delta: u64,
    checkpoint_seq: u64,
    ctx: &TxContext,
) {
    <b>assert</b>!(ctx.sender() == @0x0, <a href="../sui/accumulator_settlement.md#sui_accumulator_settlement_ENotSystemAddress">ENotSystemAddress</a>);
    <b>let</b> name = accumulator_key&lt;<a href="../sui/accumulator_settlement.md#sui_accumulator_settlement_EventStreamHead">EventStreamHead</a>&gt;(stream_id);
    <b>if</b> (accumulator_root.has_accumulator&lt;<a href="../sui/accumulator_settlement.md#sui_accumulator_settlement_EventStreamHead">EventStreamHead</a>, <a href="../sui/accumulator_settlement.md#sui_accumulator_settlement_EventStreamHead">EventStreamHead</a>&gt;(<b>copy</b> name)) {
        <b>let</b> head: &<b>mut</b> <a href="../sui/accumulator_settlement.md#sui_accumulator_settlement_EventStreamHead">EventStreamHead</a> = accumulator_root.borrow_accumulator_mut(name);
        <a href="../sui/accumulator_settlement.md#sui_accumulator_settlement_add_to_mmr">add_to_mmr</a>(new_root, &<b>mut</b> head.mmr);
        head.num_events = head.num_events + event_count_delta;
        head.checkpoint_seq = checkpoint_seq;
    } <b>else</b> {
        <b>let</b> head = <a href="../sui/accumulator_settlement.md#sui_accumulator_settlement_new_stream_head">new_stream_head</a>(new_root, event_count_delta, checkpoint_seq);
        accumulator_root.add_accumulator(name, head);
    };
}
</code></pre>



</details>
