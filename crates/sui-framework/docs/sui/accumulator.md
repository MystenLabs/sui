---
title: Module `sui::accumulator`
---



-  [Struct `AccumulatorRoot`](#sui_accumulator_AccumulatorRoot)
-  [Struct `AccumulatorName`](#sui_accumulator_AccumulatorName)
-  [Struct `U128`](#sui_accumulator_U128)
-  [Constants](#@Constants_0)
-  [Function `create`](#sui_accumulator_create)
-  [Function `accumulator_root_has_balance`](#sui_accumulator_accumulator_root_has_balance)
-  [Function `accumulator_root_add_balance`](#sui_accumulator_accumulator_root_add_balance)
-  [Function `accumulator_root_borrow_balance_mut`](#sui_accumulator_accumulator_root_borrow_balance_mut)
-  [Function `accumulator_root_remove_balance`](#sui_accumulator_accumulator_root_remove_balance)
-  [Function `get_accumulator_field_address`](#sui_accumulator_get_accumulator_field_address)
-  [Function `emit_deposit_event`](#sui_accumulator_emit_deposit_event)
-  [Function `emit_withdraw_event`](#sui_accumulator_emit_withdraw_event)
-  [Function `settlement_prologue`](#sui_accumulator_settlement_prologue)
-  [Function `settle_u128`](#sui_accumulator_settle_u128)


<pre><code><b>use</b> <a href="../std/ascii.md#std_ascii">std::ascii</a>;
<b>use</b> <a href="../std/bcs.md#std_bcs">std::bcs</a>;
<b>use</b> <a href="../std/option.md#std_option">std::option</a>;
<b>use</b> <a href="../std/string.md#std_string">std::string</a>;
<b>use</b> <a href="../std/vector.md#std_vector">std::vector</a>;
<b>use</b> <a href="../sui/address.md#sui_address">sui::address</a>;
<b>use</b> <a href="../sui/dynamic_field.md#sui_dynamic_field">sui::dynamic_field</a>;
<b>use</b> <a href="../sui/hex.md#sui_hex">sui::hex</a>;
<b>use</b> <a href="../sui/object.md#sui_object">sui::object</a>;
<b>use</b> <a href="../sui/party.md#sui_party">sui::party</a>;
<b>use</b> <a href="../sui/transfer.md#sui_transfer">sui::transfer</a>;
<b>use</b> <a href="../sui/tx_context.md#sui_tx_context">sui::tx_context</a>;
<b>use</b> <a href="../sui/vec_map.md#sui_vec_map">sui::vec_map</a>;
</code></pre>



<a name="sui_accumulator_AccumulatorRoot"></a>

## Struct `AccumulatorRoot`



<pre><code><b>public</b> <b>struct</b> <a href="../sui/accumulator.md#sui_accumulator_AccumulatorRoot">AccumulatorRoot</a> <b>has</b> key
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

<a name="sui_accumulator_AccumulatorName"></a>

## Struct `AccumulatorName`

Balance object methods
The key type to look up a balance object.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/accumulator.md#sui_accumulator_AccumulatorName">AccumulatorName</a>&lt;<b>phantom</b> T&gt; <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code><b>address</b>: <b>address</b></code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="sui_accumulator_U128"></a>

## Struct `U128`

A value type for storing any type that is represented in move as a u64.
The additional bits are to prevent overflow, as it would take 2^64 deposits of U64_MAX
to cause an overflow.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/accumulator.md#sui_accumulator_U128">U128</a> <b>has</b> store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>value: u128</code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="sui_accumulator_ENotSystemAddress"></a>



<pre><code><b>const</b> <a href="../sui/accumulator.md#sui_accumulator_ENotSystemAddress">ENotSystemAddress</a>: u64 = 0;
</code></pre>



<a name="sui_accumulator_EInvalidSplitAmount"></a>



<pre><code><b>const</b> <a href="../sui/accumulator.md#sui_accumulator_EInvalidSplitAmount">EInvalidSplitAmount</a>: u64 = 1;
</code></pre>



<a name="sui_accumulator_create"></a>

## Function `create`



<pre><code><b>fun</b> <a href="../sui/accumulator.md#sui_accumulator_create">create</a>(ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/accumulator.md#sui_accumulator_create">create</a>(ctx: &TxContext) {
    <b>assert</b>!(ctx.sender() == @0x0, <a href="../sui/accumulator.md#sui_accumulator_ENotSystemAddress">ENotSystemAddress</a>);
    <a href="../sui/transfer.md#sui_transfer_share_object">transfer::share_object</a>(<a href="../sui/accumulator.md#sui_accumulator_AccumulatorRoot">AccumulatorRoot</a> {
        id: <a href="../sui/object.md#sui_object_sui_accumulator_root_object_id">object::sui_accumulator_root_object_id</a>(),
    })
}
</code></pre>



</details>

<a name="sui_accumulator_accumulator_root_has_balance"></a>

## Function `accumulator_root_has_balance`



<pre><code><b>fun</b> <a href="../sui/accumulator.md#sui_accumulator_accumulator_root_has_balance">accumulator_root_has_balance</a>&lt;K, V: store&gt;(accumulator_root: &<a href="../sui/accumulator.md#sui_accumulator_AccumulatorRoot">sui::accumulator::AccumulatorRoot</a>, name: <a href="../sui/accumulator.md#sui_accumulator_AccumulatorName">sui::accumulator::AccumulatorName</a>&lt;K&gt;): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/accumulator.md#sui_accumulator_accumulator_root_has_balance">accumulator_root_has_balance</a>&lt;K, V: store&gt;(
    accumulator_root: &<a href="../sui/accumulator.md#sui_accumulator_AccumulatorRoot">AccumulatorRoot</a>,
    name: <a href="../sui/accumulator.md#sui_accumulator_AccumulatorName">AccumulatorName</a>&lt;K&gt;,
): bool {
    <a href="../sui/dynamic_field.md#sui_dynamic_field_exists_with_type">dynamic_field::exists_with_type</a>&lt;<a href="../sui/accumulator.md#sui_accumulator_AccumulatorName">AccumulatorName</a>&lt;K&gt;, V&gt;(&accumulator_root.id, name)
}
</code></pre>



</details>

<a name="sui_accumulator_accumulator_root_add_balance"></a>

## Function `accumulator_root_add_balance`



<pre><code><b>fun</b> <a href="../sui/accumulator.md#sui_accumulator_accumulator_root_add_balance">accumulator_root_add_balance</a>&lt;K, V: store&gt;(accumulator_root: &<b>mut</b> <a href="../sui/accumulator.md#sui_accumulator_AccumulatorRoot">sui::accumulator::AccumulatorRoot</a>, name: <a href="../sui/accumulator.md#sui_accumulator_AccumulatorName">sui::accumulator::AccumulatorName</a>&lt;K&gt;, value: V)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/accumulator.md#sui_accumulator_accumulator_root_add_balance">accumulator_root_add_balance</a>&lt;K, V: store&gt;(
    accumulator_root: &<b>mut</b> <a href="../sui/accumulator.md#sui_accumulator_AccumulatorRoot">AccumulatorRoot</a>,
    name: <a href="../sui/accumulator.md#sui_accumulator_AccumulatorName">AccumulatorName</a>&lt;K&gt;,
    value: V,
) {
    <a href="../sui/dynamic_field.md#sui_dynamic_field_add">dynamic_field::add</a>(&<b>mut</b> accumulator_root.id, name, value);
}
</code></pre>



</details>

<a name="sui_accumulator_accumulator_root_borrow_balance_mut"></a>

## Function `accumulator_root_borrow_balance_mut`



<pre><code><b>fun</b> <a href="../sui/accumulator.md#sui_accumulator_accumulator_root_borrow_balance_mut">accumulator_root_borrow_balance_mut</a>&lt;K, V: store&gt;(accumulator_root: &<b>mut</b> <a href="../sui/accumulator.md#sui_accumulator_AccumulatorRoot">sui::accumulator::AccumulatorRoot</a>, name: <a href="../sui/accumulator.md#sui_accumulator_AccumulatorName">sui::accumulator::AccumulatorName</a>&lt;K&gt;): &<b>mut</b> V
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/accumulator.md#sui_accumulator_accumulator_root_borrow_balance_mut">accumulator_root_borrow_balance_mut</a>&lt;K, V: store&gt;(
    accumulator_root: &<b>mut</b> <a href="../sui/accumulator.md#sui_accumulator_AccumulatorRoot">AccumulatorRoot</a>,
    name: <a href="../sui/accumulator.md#sui_accumulator_AccumulatorName">AccumulatorName</a>&lt;K&gt;,
): &<b>mut</b> V {
    <a href="../sui/dynamic_field.md#sui_dynamic_field_borrow_mut">dynamic_field::borrow_mut</a>&lt;<a href="../sui/accumulator.md#sui_accumulator_AccumulatorName">AccumulatorName</a>&lt;K&gt;, V&gt;(&<b>mut</b> accumulator_root.id, name)
}
</code></pre>



</details>

<a name="sui_accumulator_accumulator_root_remove_balance"></a>

## Function `accumulator_root_remove_balance`



<pre><code><b>fun</b> <a href="../sui/accumulator.md#sui_accumulator_accumulator_root_remove_balance">accumulator_root_remove_balance</a>&lt;K, V: store&gt;(accumulator_root: &<b>mut</b> <a href="../sui/accumulator.md#sui_accumulator_AccumulatorRoot">sui::accumulator::AccumulatorRoot</a>, name: <a href="../sui/accumulator.md#sui_accumulator_AccumulatorName">sui::accumulator::AccumulatorName</a>&lt;K&gt;): V
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/accumulator.md#sui_accumulator_accumulator_root_remove_balance">accumulator_root_remove_balance</a>&lt;K, V: store&gt;(
    accumulator_root: &<b>mut</b> <a href="../sui/accumulator.md#sui_accumulator_AccumulatorRoot">AccumulatorRoot</a>,
    name: <a href="../sui/accumulator.md#sui_accumulator_AccumulatorName">AccumulatorName</a>&lt;K&gt;,
): V {
    <a href="../sui/dynamic_field.md#sui_dynamic_field_remove">dynamic_field::remove</a>&lt;<a href="../sui/accumulator.md#sui_accumulator_AccumulatorName">AccumulatorName</a>&lt;K&gt;, V&gt;(&<b>mut</b> accumulator_root.id, name)
}
</code></pre>



</details>

<a name="sui_accumulator_get_accumulator_field_address"></a>

## Function `get_accumulator_field_address`



<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/accumulator.md#sui_accumulator_get_accumulator_field_address">get_accumulator_field_address</a>&lt;T&gt;(<b>address</b>: <b>address</b>): <b>address</b>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/accumulator.md#sui_accumulator_get_accumulator_field_address">get_accumulator_field_address</a>&lt;T&gt;(<b>address</b>: <b>address</b>): <b>address</b> {
    <b>let</b> key = <a href="../sui/accumulator.md#sui_accumulator_AccumulatorName">AccumulatorName</a>&lt;T&gt; { <b>address</b> };
    <a href="../sui/dynamic_field.md#sui_dynamic_field_hash_type_and_key">dynamic_field::hash_type_and_key</a>(sui_accumulator_root_address(), key)
}
</code></pre>



</details>

<a name="sui_accumulator_emit_deposit_event"></a>

## Function `emit_deposit_event`



<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/accumulator.md#sui_accumulator_emit_deposit_event">emit_deposit_event</a>&lt;T&gt;(<a href="../sui/accumulator.md#sui_accumulator">accumulator</a>: <b>address</b>, recipient: <b>address</b>, amount: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>native</b> <b>fun</b> <a href="../sui/accumulator.md#sui_accumulator_emit_deposit_event">emit_deposit_event</a>&lt;T&gt;(
    <a href="../sui/accumulator.md#sui_accumulator">accumulator</a>: <b>address</b>,
    recipient: <b>address</b>,
    amount: u64,
);
</code></pre>



</details>

<a name="sui_accumulator_emit_withdraw_event"></a>

## Function `emit_withdraw_event`



<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/accumulator.md#sui_accumulator_emit_withdraw_event">emit_withdraw_event</a>&lt;T&gt;(<a href="../sui/accumulator.md#sui_accumulator">accumulator</a>: <b>address</b>, owner: <b>address</b>, amount: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>native</b> <b>fun</b> <a href="../sui/accumulator.md#sui_accumulator_emit_withdraw_event">emit_withdraw_event</a>&lt;T&gt;(
    <a href="../sui/accumulator.md#sui_accumulator">accumulator</a>: <b>address</b>,
    owner: <b>address</b>,
    amount: u64,
);
</code></pre>



</details>

<a name="sui_accumulator_settlement_prologue"></a>

## Function `settlement_prologue`

Called by settlement transactions to ensure that the settlement transaction has a unique
digest.


<pre><code><b>fun</b> <a href="../sui/accumulator.md#sui_accumulator_settlement_prologue">settlement_prologue</a>(_epoch: u64, _checkpoint_height: u64, _idx: u64, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/accumulator.md#sui_accumulator_settlement_prologue">settlement_prologue</a>(_epoch: u64, _checkpoint_height: u64, _idx: u64, ctx: &TxContext) {
    <b>assert</b>!(ctx.sender() == @0x0, <a href="../sui/accumulator.md#sui_accumulator_ENotSystemAddress">ENotSystemAddress</a>);
}
</code></pre>



</details>

<a name="sui_accumulator_settle_u128"></a>

## Function `settle_u128`



<pre><code><b>fun</b> <a href="../sui/accumulator.md#sui_accumulator_settle_u128">settle_u128</a>&lt;T&gt;(accumulator_root: &<b>mut</b> <a href="../sui/accumulator.md#sui_accumulator_AccumulatorRoot">sui::accumulator::AccumulatorRoot</a>, owner: <b>address</b>, merge: u128, split: u128, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/accumulator.md#sui_accumulator_settle_u128">settle_u128</a>&lt;T&gt;(
    accumulator_root: &<b>mut</b> <a href="../sui/accumulator.md#sui_accumulator_AccumulatorRoot">AccumulatorRoot</a>,
    owner: <b>address</b>,
    merge: u128,
    split: u128,
    ctx: &TxContext,
) {
    <b>assert</b>!(ctx.sender() == @0x0, <a href="../sui/accumulator.md#sui_accumulator_ENotSystemAddress">ENotSystemAddress</a>);
    // Merge and split should be netted out prior to calling this function.
    <b>assert</b>!((merge == 0 ) != (split == 0), <a href="../sui/accumulator.md#sui_accumulator_EInvalidSplitAmount">EInvalidSplitAmount</a>);
    <b>let</b> name = <a href="../sui/accumulator.md#sui_accumulator_AccumulatorName">AccumulatorName</a>&lt;T&gt; { <b>address</b>: owner };
    <b>if</b> (accumulator_root.has_balance&lt;T, <a href="../sui/accumulator.md#sui_accumulator_U128">U128</a>&gt;(name)) {
        <b>let</b> is_zero = {
            <b>let</b> value: &<b>mut</b> <a href="../sui/accumulator.md#sui_accumulator_U128">U128</a> = accumulator_root.borrow_balance_mut(name);
            value.value = value.value + merge - split;
            value.value == 0
        };
        <b>if</b> (is_zero) {
            <b>let</b> <a href="../sui/accumulator.md#sui_accumulator_U128">U128</a> { value: _ } = accumulator_root.remove_balance&lt;T, <a href="../sui/accumulator.md#sui_accumulator_U128">U128</a>&gt;(
                name,
            );
        }
    } <b>else</b> {
        // cannot split <b>if</b> the field does not yet exist
        <b>assert</b>!(split == 0, <a href="../sui/accumulator.md#sui_accumulator_EInvalidSplitAmount">EInvalidSplitAmount</a>);
        <b>let</b> value = <a href="../sui/accumulator.md#sui_accumulator_U128">U128</a> {
            value: merge,
        };
        accumulator_root.add_balance(name, value);
    };
}
</code></pre>



</details>
