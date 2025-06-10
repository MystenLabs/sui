---
title: Module `sui::accumulator`
---



-  [Struct `Accumulator`](#sui_accumulator_Accumulator)
-  [Constants](#@Constants_0)
-  [Function `create`](#sui_accumulator_create)


<pre><code><b>use</b> <a href="../std/ascii.md#std_ascii">std::ascii</a>;
<b>use</b> <a href="../std/bcs.md#std_bcs">std::bcs</a>;
<b>use</b> <a href="../std/option.md#std_option">std::option</a>;
<b>use</b> <a href="../std/string.md#std_string">std::string</a>;
<b>use</b> <a href="../std/vector.md#std_vector">std::vector</a>;
<b>use</b> <a href="../sui/address.md#sui_address">sui::address</a>;
<b>use</b> <a href="../sui/hex.md#sui_hex">sui::hex</a>;
<b>use</b> <a href="../sui/object.md#sui_object">sui::object</a>;
<b>use</b> <a href="../sui/party.md#sui_party">sui::party</a>;
<b>use</b> <a href="../sui/transfer.md#sui_transfer">sui::transfer</a>;
<b>use</b> <a href="../sui/tx_context.md#sui_tx_context">sui::tx_context</a>;
<b>use</b> <a href="../sui/vec_map.md#sui_vec_map">sui::vec_map</a>;
</code></pre>



<a name="sui_accumulator_Accumulator"></a>

## Struct `Accumulator`



<pre><code><b>public</b> <b>struct</b> <a href="../sui/accumulator.md#sui_accumulator_Accumulator">Accumulator</a> <b>has</b> key
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

<a name="@Constants_0"></a>

## Constants


<a name="sui_accumulator_ENotSystemAddress"></a>



<pre><code><b>const</b> <a href="../sui/accumulator.md#sui_accumulator_ENotSystemAddress">ENotSystemAddress</a>: u64 = 0;
</code></pre>



<a name="sui_accumulator_create"></a>

## Function `create`



<pre><code><b>fun</b> <a href="../sui/accumulator.md#sui_accumulator_create">create</a>(ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/accumulator.md#sui_accumulator_create">create</a>(ctx: &TxContext) {
    <b>assert</b>!(ctx.sender() == @0x0, <a href="../sui/accumulator.md#sui_accumulator_ENotSystemAddress">ENotSystemAddress</a>);
    <a href="../sui/transfer.md#sui_transfer_share_object">transfer::share_object</a>(<a href="../sui/accumulator.md#sui_accumulator_Accumulator">Accumulator</a> {
        id: <a href="../sui/object.md#sui_object_sui_accumulator_root_object_id">object::sui_accumulator_root_object_id</a>(),
    })
}
</code></pre>



</details>
