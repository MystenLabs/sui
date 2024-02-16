
<a name="0x2_tx_context"></a>

# Module `0x2::tx_context`



-  [Struct `TxContext`](#0x2_tx_context_TxContext)
-  [Function `sender`](#0x2_tx_context_sender)
-  [Function `digest`](#0x2_tx_context_digest)
-  [Function `epoch`](#0x2_tx_context_epoch)
-  [Function `epoch_timestamp_ms`](#0x2_tx_context_epoch_timestamp_ms)
-  [Function `fresh_object_address`](#0x2_tx_context_fresh_object_address)
-  [Function `ids_created`](#0x2_tx_context_ids_created)
-  [Function `derive_id`](#0x2_tx_context_derive_id)


<pre><code></code></pre>



<a name="0x2_tx_context_TxContext"></a>

## Struct `TxContext`



<pre><code><b>struct</b> <a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">TxContext</a> <b>has</b> drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>sender: <b>address</b></code>
</dt>
<dd>

</dd>
<dt>
<code>tx_hash: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>epoch: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>epoch_timestamp_ms: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>ids_created: u64</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x2_tx_context_sender"></a>

## Function `sender`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_sender">sender</a>(self: &<a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <b>address</b>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_sender">sender</a>(self: &<a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">TxContext</a>): <b>address</b> {
    self.sender
}
</code></pre>



</details>

<a name="0x2_tx_context_digest"></a>

## Function `digest`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_digest">digest</a>(self: &<a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): &<a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_digest">digest</a>(self: &<a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">TxContext</a>): &<a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt; {
    &self.tx_hash
}
</code></pre>



</details>

<a name="0x2_tx_context_epoch"></a>

## Function `epoch`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_epoch">epoch</a>(self: &<a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_epoch">epoch</a>(self: &<a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">TxContext</a>): u64 {
    self.epoch
}
</code></pre>



</details>

<a name="0x2_tx_context_epoch_timestamp_ms"></a>

## Function `epoch_timestamp_ms`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_epoch_timestamp_ms">epoch_timestamp_ms</a>(self: &<a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_epoch_timestamp_ms">epoch_timestamp_ms</a>(self: &<a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">TxContext</a>): u64 {
   self.epoch_timestamp_ms
}
</code></pre>



</details>

<a name="0x2_tx_context_fresh_object_address"></a>

## Function `fresh_object_address`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_fresh_object_address">fresh_object_address</a>(ctx: &<b>mut</b> <a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <b>address</b>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_fresh_object_address">fresh_object_address</a>(ctx: &<b>mut</b> <a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">TxContext</a>): <b>address</b> {
    <b>let</b> ids_created = ctx.ids_created;
    <b>let</b> id = <a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_derive_id">derive_id</a>(*&ctx.tx_hash, ids_created);
    ctx.ids_created = ids_created + 1;
    id
}
</code></pre>



</details>

<a name="0x2_tx_context_ids_created"></a>

## Function `ids_created`



<pre><code><b>fun</b> <a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_ids_created">ids_created</a>(self: &<a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_ids_created">ids_created</a>(self: &<a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">TxContext</a>): u64 {
    self.ids_created
}
</code></pre>



</details>

<a name="0x2_tx_context_derive_id"></a>

## Function `derive_id`



<pre><code><b>fun</b> <a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_derive_id">derive_id</a>(tx_hash: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, ids_created: u64): <b>address</b>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_derive_id">derive_id</a>(tx_hash: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, ids_created: u64): <b>address</b>;
</code></pre>



</details>
