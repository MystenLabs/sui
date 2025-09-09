---
title: Module `sui::accumulator_settlement`
---



-  [Constants](#@Constants_0)
-  [Function `settlement_prologue`](#sui_accumulator_settlement_settlement_prologue)
-  [Function `settle_u128`](#sui_accumulator_settlement_settle_u128)


<pre><code><b>use</b> <a href="../std/ascii.md#std_ascii">std::ascii</a>;
<b>use</b> <a href="../std/bcs.md#std_bcs">std::bcs</a>;
<b>use</b> <a href="../std/option.md#std_option">std::option</a>;
<b>use</b> <a href="../std/string.md#std_string">std::string</a>;
<b>use</b> <a href="../std/vector.md#std_vector">std::vector</a>;
<b>use</b> <a href="../sui/accumulator.md#sui_accumulator">sui::accumulator</a>;
<b>use</b> <a href="../sui/accumulator_metadata.md#sui_accumulator_metadata">sui::accumulator_metadata</a>;
<b>use</b> <a href="../sui/address.md#sui_address">sui::address</a>;
<b>use</b> <a href="../sui/bag.md#sui_bag">sui::bag</a>;
<b>use</b> <a href="../sui/dynamic_field.md#sui_dynamic_field">sui::dynamic_field</a>;
<b>use</b> <a href="../sui/hex.md#sui_hex">sui::hex</a>;
<b>use</b> <a href="../sui/object.md#sui_object">sui::object</a>;
<b>use</b> <a href="../sui/party.md#sui_party">sui::party</a>;
<b>use</b> <a href="../sui/transfer.md#sui_transfer">sui::transfer</a>;
<b>use</b> <a href="../sui/tx_context.md#sui_tx_context">sui::tx_context</a>;
<b>use</b> <a href="../sui/vec_map.md#sui_vec_map">sui::vec_map</a>;
</code></pre>



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


<pre><code><b>fun</b> <a href="../sui/accumulator_settlement.md#sui_accumulator_settlement_settlement_prologue">settlement_prologue</a>(_epoch: u64, _checkpoint_height: u64, _idx: u64, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/accumulator_settlement.md#sui_accumulator_settlement_settlement_prologue">settlement_prologue</a>(_epoch: u64, _checkpoint_height: u64, _idx: u64, ctx: &TxContext) {
    <b>assert</b>!(ctx.sender() == @0x0, <a href="../sui/accumulator_settlement.md#sui_accumulator_settlement_ENotSystemAddress">ENotSystemAddress</a>);
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
