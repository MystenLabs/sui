---
title: Module `sui::tx_context`
---



-  [Struct `TxContext`](#sui_tx_context_TxContext)
-  [Function `sender`](#sui_tx_context_sender)
-  [Function `native_sender`](#sui_tx_context_native_sender)
-  [Function `digest`](#sui_tx_context_digest)
-  [Function `epoch`](#sui_tx_context_epoch)
-  [Function `native_epoch`](#sui_tx_context_native_epoch)
-  [Function `epoch_timestamp_ms`](#sui_tx_context_epoch_timestamp_ms)
-  [Function `native_epoch_timestamp_ms`](#sui_tx_context_native_epoch_timestamp_ms)
-  [Function `sponsor`](#sui_tx_context_sponsor)
-  [Function `fresh_object_address`](#sui_tx_context_fresh_object_address)
-  [Function `fresh_id`](#sui_tx_context_fresh_id)
-  [Function `ids_created`](#sui_tx_context_ids_created)
-  [Function `native_ids_created`](#sui_tx_context_native_ids_created)
-  [Function `native_gas_price`](#sui_tx_context_native_gas_price)
-  [Function `native_gas_budget`](#sui_tx_context_native_gas_budget)
-  [Function `option_sponsor`](#sui_tx_context_option_sponsor)
-  [Function `native_sponsor`](#sui_tx_context_native_sponsor)
-  [Function `derive_id`](#sui_tx_context_derive_id)


<pre><code><b>use</b> <a href="../std/option.md#std_option">std::option</a>;
<b>use</b> <a href="../std/vector.md#std_vector">std::vector</a>;
</code></pre>



<a name="sui_tx_context_TxContext"></a>

## Struct `TxContext`

Information about the transaction currently being executed.
This cannot be constructed by a transaction--it is a privileged object created by
the VM and passed in to the entrypoint of the transaction as <code>&<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">TxContext</a></code>.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">TxContext</a> <b>has</b> drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code><a href="../sui/tx_context.md#sui_tx_context_sender">sender</a>: <b>address</b></code>
</dt>
<dd>
 The address of the user that signed the current transaction
</dd>
<dt>
<code>tx_hash: vector&lt;u8&gt;</code>
</dt>
<dd>
 Hash of the current transaction
</dd>
<dt>
<code><a href="../sui/tx_context.md#sui_tx_context_epoch">epoch</a>: u64</code>
</dt>
<dd>
 The current epoch number
</dd>
<dt>
<code><a href="../sui/tx_context.md#sui_tx_context_epoch_timestamp_ms">epoch_timestamp_ms</a>: u64</code>
</dt>
<dd>
 Timestamp that the epoch started at
</dd>
<dt>
<code><a href="../sui/tx_context.md#sui_tx_context_ids_created">ids_created</a>: u64</code>
</dt>
<dd>
 Counter recording the number of fresh id's created while executing
 this transaction. Always 0 at the start of a transaction
</dd>
</dl>


</details>

<a name="sui_tx_context_sender"></a>

## Function `sender`

Return the address of the user that signed the current
transaction


<pre><code><b>public</b> <b>fun</b> <a href="../sui/tx_context.md#sui_tx_context_sender">sender</a>(_self: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <b>address</b>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/tx_context.md#sui_tx_context_sender">sender</a>(_self: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">TxContext</a>): <b>address</b> {
    <a href="../sui/tx_context.md#sui_tx_context_native_sender">native_sender</a>()
}
</code></pre>



</details>

<a name="sui_tx_context_native_sender"></a>

## Function `native_sender`



<pre><code><b>fun</b> <a href="../sui/tx_context.md#sui_tx_context_native_sender">native_sender</a>(): <b>address</b>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="../sui/tx_context.md#sui_tx_context_native_sender">native_sender</a>(): <b>address</b>;
</code></pre>



</details>

<a name="sui_tx_context_digest"></a>

## Function `digest`

Return the transaction digest (hash of transaction inputs).
Please do not use as a source of randomness.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/tx_context.md#sui_tx_context_digest">digest</a>(self: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): &vector&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/tx_context.md#sui_tx_context_digest">digest</a>(self: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">TxContext</a>): &vector&lt;u8&gt; {
    &self.tx_hash
}
</code></pre>



</details>

<a name="sui_tx_context_epoch"></a>

## Function `epoch`

Return the current epoch


<pre><code><b>public</b> <b>fun</b> <a href="../sui/tx_context.md#sui_tx_context_epoch">epoch</a>(_self: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/tx_context.md#sui_tx_context_epoch">epoch</a>(_self: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">TxContext</a>): u64 {
    <a href="../sui/tx_context.md#sui_tx_context_native_epoch">native_epoch</a>()
}
</code></pre>



</details>

<a name="sui_tx_context_native_epoch"></a>

## Function `native_epoch`



<pre><code><b>fun</b> <a href="../sui/tx_context.md#sui_tx_context_native_epoch">native_epoch</a>(): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="../sui/tx_context.md#sui_tx_context_native_epoch">native_epoch</a>(): u64;
</code></pre>



</details>

<a name="sui_tx_context_epoch_timestamp_ms"></a>

## Function `epoch_timestamp_ms`

Return the epoch start time as a unix timestamp in milliseconds.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/tx_context.md#sui_tx_context_epoch_timestamp_ms">epoch_timestamp_ms</a>(_self: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/tx_context.md#sui_tx_context_epoch_timestamp_ms">epoch_timestamp_ms</a>(_self: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">TxContext</a>): u64 {
    <a href="../sui/tx_context.md#sui_tx_context_native_epoch_timestamp_ms">native_epoch_timestamp_ms</a>()
}
</code></pre>



</details>

<a name="sui_tx_context_native_epoch_timestamp_ms"></a>

## Function `native_epoch_timestamp_ms`



<pre><code><b>fun</b> <a href="../sui/tx_context.md#sui_tx_context_native_epoch_timestamp_ms">native_epoch_timestamp_ms</a>(): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="../sui/tx_context.md#sui_tx_context_native_epoch_timestamp_ms">native_epoch_timestamp_ms</a>(): u64;
</code></pre>



</details>

<a name="sui_tx_context_sponsor"></a>

## Function `sponsor`

Return the adress of the transaction sponsor or <code>None</code> if there was no sponsor.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/tx_context.md#sui_tx_context_sponsor">sponsor</a>(_self: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;<b>address</b>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/tx_context.md#sui_tx_context_sponsor">sponsor</a>(_self: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">TxContext</a>): Option&lt;<b>address</b>&gt; {
    <a href="../sui/tx_context.md#sui_tx_context_option_sponsor">option_sponsor</a>()
}
</code></pre>



</details>

<a name="sui_tx_context_fresh_object_address"></a>

## Function `fresh_object_address`

Create an <code><b>address</b></code> that has not been used. As it is an object address, it will never
occur as the address for a user.
In other words, the generated address is a globally unique object ID.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/tx_context.md#sui_tx_context_fresh_object_address">fresh_object_address</a>(_ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <b>address</b>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/tx_context.md#sui_tx_context_fresh_object_address">fresh_object_address</a>(_ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">TxContext</a>): <b>address</b> {
    <a href="../sui/tx_context.md#sui_tx_context_fresh_id">fresh_id</a>()
}
</code></pre>



</details>

<a name="sui_tx_context_fresh_id"></a>

## Function `fresh_id`



<pre><code><b>fun</b> <a href="../sui/tx_context.md#sui_tx_context_fresh_id">fresh_id</a>(): <b>address</b>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="../sui/tx_context.md#sui_tx_context_fresh_id">fresh_id</a>(): <b>address</b>;
</code></pre>



</details>

<a name="sui_tx_context_ids_created"></a>

## Function `ids_created`

Return the number of id's created by the current transaction.
Hidden for now, but may expose later


<pre><code><b>fun</b> <a href="../sui/tx_context.md#sui_tx_context_ids_created">ids_created</a>(_self: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/tx_context.md#sui_tx_context_ids_created">ids_created</a>(_self: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">TxContext</a>): u64 {
    <a href="../sui/tx_context.md#sui_tx_context_native_ids_created">native_ids_created</a>()
}
</code></pre>



</details>

<a name="sui_tx_context_native_ids_created"></a>

## Function `native_ids_created`



<pre><code><b>fun</b> <a href="../sui/tx_context.md#sui_tx_context_native_ids_created">native_ids_created</a>(): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="../sui/tx_context.md#sui_tx_context_native_ids_created">native_ids_created</a>(): u64;
</code></pre>



</details>

<a name="sui_tx_context_native_gas_price"></a>

## Function `native_gas_price`



<pre><code><b>fun</b> <a href="../sui/tx_context.md#sui_tx_context_native_gas_price">native_gas_price</a>(): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="../sui/tx_context.md#sui_tx_context_native_gas_price">native_gas_price</a>(): u64;
</code></pre>



</details>

<a name="sui_tx_context_native_gas_budget"></a>

## Function `native_gas_budget`



<pre><code><b>fun</b> <a href="../sui/tx_context.md#sui_tx_context_native_gas_budget">native_gas_budget</a>(): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="../sui/tx_context.md#sui_tx_context_native_gas_budget">native_gas_budget</a>(): u64;
</code></pre>



</details>

<a name="sui_tx_context_option_sponsor"></a>

## Function `option_sponsor`



<pre><code><b>fun</b> <a href="../sui/tx_context.md#sui_tx_context_option_sponsor">option_sponsor</a>(): <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;<b>address</b>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/tx_context.md#sui_tx_context_option_sponsor">option_sponsor</a>(): Option&lt;<b>address</b>&gt; {
    <b>let</b> <a href="../sui/tx_context.md#sui_tx_context_sponsor">sponsor</a> = <a href="../sui/tx_context.md#sui_tx_context_native_sponsor">native_sponsor</a>();
    <b>if</b> (<a href="../sui/tx_context.md#sui_tx_context_sponsor">sponsor</a>.length() == 0) option::none() <b>else</b> option::some(<a href="../sui/tx_context.md#sui_tx_context_sponsor">sponsor</a>[0])
}
</code></pre>



</details>

<a name="sui_tx_context_native_sponsor"></a>

## Function `native_sponsor`



<pre><code><b>fun</b> <a href="../sui/tx_context.md#sui_tx_context_native_sponsor">native_sponsor</a>(): vector&lt;<b>address</b>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="../sui/tx_context.md#sui_tx_context_native_sponsor">native_sponsor</a>(): vector&lt;<b>address</b>&gt;;
</code></pre>



</details>

<a name="sui_tx_context_derive_id"></a>

## Function `derive_id`

Native function for deriving an ID via hash(tx_hash || ids_created)


<pre><code><b>fun</b> <a href="../sui/tx_context.md#sui_tx_context_derive_id">derive_id</a>(tx_hash: vector&lt;u8&gt;, <a href="../sui/tx_context.md#sui_tx_context_ids_created">ids_created</a>: u64): <b>address</b>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="../sui/tx_context.md#sui_tx_context_derive_id">derive_id</a>(tx_hash: vector&lt;u8&gt;, <a href="../sui/tx_context.md#sui_tx_context_ids_created">ids_created</a>: u64): <b>address</b>;
</code></pre>



</details>
