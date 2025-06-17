---
title: Module `sui::accumulator`
---



-  [Struct `AccumulatorRoot`](#sui_accumulator_AccumulatorRoot)
-  [Struct `Key`](#sui_accumulator_Key)
-  [Struct `AccumulatorU128`](#sui_accumulator_AccumulatorU128)
-  [Constants](#@Constants_0)
-  [Function `create`](#sui_accumulator_create)
-  [Function `get_accumulator_field_name`](#sui_accumulator_get_accumulator_field_name)
-  [Function `get_accumulator_field_address`](#sui_accumulator_get_accumulator_field_address)
-  [Function `emit_deposit_event`](#sui_accumulator_emit_deposit_event)
-  [Function `emit_withdraw_event`](#sui_accumulator_emit_withdraw_event)
-  [Function `settlement_prologue`](#sui_accumulator_settlement_prologue)
-  [Function `settle_u128`](#sui_accumulator_settle_u128)


<pre><code><b>use</b> <a href="../std/address.md#std_address">std::address</a>;
<b>use</b> <a href="../std/ascii.md#std_ascii">std::ascii</a>;
<b>use</b> <a href="../std/bcs.md#std_bcs">std::bcs</a>;
<b>use</b> <a href="../std/option.md#std_option">std::option</a>;
<b>use</b> <a href="../std/string.md#std_string">std::string</a>;
<b>use</b> <a href="../std/type_name.md#std_type_name">std::type_name</a>;
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

<a name="sui_accumulator_Key"></a>

## Struct `Key`



<pre><code><b>public</b> <b>struct</b> <a href="../sui/accumulator.md#sui_accumulator_Key">Key</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code><b>address</b>: <b>address</b></code>
</dt>
<dd>
</dd>
<dt>
<code>ty: <a href="../std/type_name.md#std_type_name_TypeName">std::type_name::TypeName</a></code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="sui_accumulator_AccumulatorU128"></a>

## Struct `AccumulatorU128`



<pre><code><b>public</b> <b>struct</b> <a href="../sui/accumulator.md#sui_accumulator_AccumulatorU128">AccumulatorU128</a> <b>has</b> store
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

<a name="sui_accumulator_get_accumulator_field_name"></a>

## Function `get_accumulator_field_name`



<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/accumulator.md#sui_accumulator_get_accumulator_field_name">get_accumulator_field_name</a>&lt;T&gt;(<b>address</b>: <b>address</b>): <a href="../sui/accumulator.md#sui_accumulator_Key">sui::accumulator::Key</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/accumulator.md#sui_accumulator_get_accumulator_field_name">get_accumulator_field_name</a>&lt;T&gt;(<b>address</b>: <b>address</b>): <a href="../sui/accumulator.md#sui_accumulator_Key">Key</a> {
    <b>let</b> ty = type_name::get_with_original_ids&lt;T&gt;();
    <a href="../sui/accumulator.md#sui_accumulator_Key">Key</a> { <b>address</b>, ty }
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
    <b>let</b> key = <a href="../sui/accumulator.md#sui_accumulator_get_accumulator_field_name">get_accumulator_field_name</a>&lt;T&gt;(<b>address</b>);
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
    <b>let</b> name = <a href="../sui/accumulator.md#sui_accumulator_get_accumulator_field_name">get_accumulator_field_name</a>&lt;T&gt;(owner);
    <b>let</b> root_id = &<b>mut</b> accumulator_root.id;
    <b>if</b> (<a href="../sui/dynamic_field.md#sui_dynamic_field_exists_with_type">dynamic_field::exists_with_type</a>&lt;<a href="../sui/accumulator.md#sui_accumulator_Key">Key</a>, <a href="../sui/accumulator.md#sui_accumulator_AccumulatorU128">AccumulatorU128</a>&gt;(root_id, name)) {
        <b>let</b> value: &<b>mut</b> <a href="../sui/accumulator.md#sui_accumulator_AccumulatorU128">AccumulatorU128</a> = <a href="../sui/dynamic_field.md#sui_dynamic_field_borrow_mut">dynamic_field::borrow_mut</a>(root_id, name);
        value.value = value.value + merge - split;
    } <b>else</b> {
        // cannot split <b>if</b> the field does not yet exist
        <b>assert</b>!(split == 0, <a href="../sui/accumulator.md#sui_accumulator_EInvalidSplitAmount">EInvalidSplitAmount</a>);
        <b>let</b> value = <a href="../sui/accumulator.md#sui_accumulator_AccumulatorU128">AccumulatorU128</a> {
            value: merge,
        };
        <a href="../sui/dynamic_field.md#sui_dynamic_field_add">dynamic_field::add</a>(root_id, name, value);
    };
}
</code></pre>



</details>
