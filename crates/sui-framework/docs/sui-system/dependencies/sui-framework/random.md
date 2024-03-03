
<a name="0x2_random"></a>

# Module `0x2::random`



-  [Resource `Random`](#0x2_random_Random)
-  [Struct `RandomInner`](#0x2_random_RandomInner)
-  [Constants](#@Constants_0)
-  [Function `create`](#0x2_random_create)
-  [Function `load_inner_mut`](#0x2_random_load_inner_mut)
-  [Function `load_inner`](#0x2_random_load_inner)
-  [Function `update_randomness_state`](#0x2_random_update_randomness_state)


<pre><code><b>use</b> <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">0x1::vector</a>;
<b>use</b> <a href="../../dependencies/sui-framework/object.md#0x2_object">0x2::object</a>;
<b>use</b> <a href="../../dependencies/sui-framework/transfer.md#0x2_transfer">0x2::transfer</a>;
<b>use</b> <a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context">0x2::tx_context</a>;
<b>use</b> <a href="../../dependencies/sui-framework/versioned.md#0x2_versioned">0x2::versioned</a>;
</code></pre>



<a name="0x2_random_Random"></a>

## Resource `Random`



<pre><code><b>struct</b> <a href="../../dependencies/sui-framework/random.md#0x2_random_Random">Random</a> <b>has</b> key
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>id: <a href="../../dependencies/sui-framework/object.md#0x2_object_UID">object::UID</a></code>
</dt>
<dd>

</dd>
<dt>
<code>inner: <a href="../../dependencies/sui-framework/versioned.md#0x2_versioned_Versioned">versioned::Versioned</a></code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x2_random_RandomInner"></a>

## Struct `RandomInner`



<pre><code><b>struct</b> <a href="../../dependencies/sui-framework/random.md#0x2_random_RandomInner">RandomInner</a> <b>has</b> store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>version: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>epoch: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>randomness_round: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>random_bytes: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0x2_random_ENotSystemAddress"></a>



<pre><code><b>const</b> <a href="../../dependencies/sui-framework/random.md#0x2_random_ENotSystemAddress">ENotSystemAddress</a>: u64 = 0;
</code></pre>



<a name="0x2_random_EWrongInnerVersion"></a>



<pre><code><b>const</b> <a href="../../dependencies/sui-framework/random.md#0x2_random_EWrongInnerVersion">EWrongInnerVersion</a>: u64 = 1;
</code></pre>



<a name="0x2_random_CURRENT_VERSION"></a>



<pre><code><b>const</b> <a href="../../dependencies/sui-framework/random.md#0x2_random_CURRENT_VERSION">CURRENT_VERSION</a>: u64 = 1;
</code></pre>



<a name="0x2_random_EInvalidRandomnessUpdate"></a>



<pre><code><b>const</b> <a href="../../dependencies/sui-framework/random.md#0x2_random_EInvalidRandomnessUpdate">EInvalidRandomnessUpdate</a>: u64 = 2;
</code></pre>



<a name="0x2_random_create"></a>

## Function `create`



<pre><code><b>fun</b> <a href="../../dependencies/sui-framework/random.md#0x2_random_create">create</a>(ctx: &<b>mut</b> <a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../../dependencies/sui-framework/random.md#0x2_random_create">create</a>(ctx: &<b>mut</b> TxContext) {
    <b>assert</b>!(<a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_sender">tx_context::sender</a>(ctx) == @0x0, <a href="../../dependencies/sui-framework/random.md#0x2_random_ENotSystemAddress">ENotSystemAddress</a>);

    <b>let</b> version = <a href="../../dependencies/sui-framework/random.md#0x2_random_CURRENT_VERSION">CURRENT_VERSION</a>;

    <b>let</b> inner = <a href="../../dependencies/sui-framework/random.md#0x2_random_RandomInner">RandomInner</a> {
        version,
        epoch: <a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_epoch">tx_context::epoch</a>(ctx),
        randomness_round: 0,
        random_bytes: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>[],
    };

    <b>let</b> self = <a href="../../dependencies/sui-framework/random.md#0x2_random_Random">Random</a> {
        id: <a href="../../dependencies/sui-framework/object.md#0x2_object_randomness_state">object::randomness_state</a>(),
        inner: <a href="../../dependencies/sui-framework/versioned.md#0x2_versioned_create">versioned::create</a>(version, inner, ctx),
    };
    <a href="../../dependencies/sui-framework/transfer.md#0x2_transfer_share_object">transfer::share_object</a>(self);
}
</code></pre>



</details>

<a name="0x2_random_load_inner_mut"></a>

## Function `load_inner_mut`



<pre><code><b>fun</b> <a href="../../dependencies/sui-framework/random.md#0x2_random_load_inner_mut">load_inner_mut</a>(self: &<b>mut</b> <a href="../../dependencies/sui-framework/random.md#0x2_random_Random">random::Random</a>): &<b>mut</b> <a href="../../dependencies/sui-framework/random.md#0x2_random_RandomInner">random::RandomInner</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../../dependencies/sui-framework/random.md#0x2_random_load_inner_mut">load_inner_mut</a>(
    self: &<b>mut</b> <a href="../../dependencies/sui-framework/random.md#0x2_random_Random">Random</a>,
): &<b>mut</b> <a href="../../dependencies/sui-framework/random.md#0x2_random_RandomInner">RandomInner</a> {
    <b>let</b> version = <a href="../../dependencies/sui-framework/versioned.md#0x2_versioned_version">versioned::version</a>(&self.inner);

    // Replace this <b>with</b> a lazy <b>update</b> function when we add a new version of the inner <a href="../../dependencies/sui-framework/object.md#0x2_object">object</a>.
    <b>assert</b>!(version == <a href="../../dependencies/sui-framework/random.md#0x2_random_CURRENT_VERSION">CURRENT_VERSION</a>, <a href="../../dependencies/sui-framework/random.md#0x2_random_EWrongInnerVersion">EWrongInnerVersion</a>);
    <b>let</b> inner: &<b>mut</b> <a href="../../dependencies/sui-framework/random.md#0x2_random_RandomInner">RandomInner</a> = <a href="../../dependencies/sui-framework/versioned.md#0x2_versioned_load_value_mut">versioned::load_value_mut</a>(&<b>mut</b> self.inner);
    <b>assert</b>!(inner.version == version, <a href="../../dependencies/sui-framework/random.md#0x2_random_EWrongInnerVersion">EWrongInnerVersion</a>);
    inner
}
</code></pre>



</details>

<a name="0x2_random_load_inner"></a>

## Function `load_inner`



<pre><code><b>fun</b> <a href="../../dependencies/sui-framework/random.md#0x2_random_load_inner">load_inner</a>(self: &<a href="../../dependencies/sui-framework/random.md#0x2_random_Random">random::Random</a>): &<a href="../../dependencies/sui-framework/random.md#0x2_random_RandomInner">random::RandomInner</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../../dependencies/sui-framework/random.md#0x2_random_load_inner">load_inner</a>(
    self: &<a href="../../dependencies/sui-framework/random.md#0x2_random_Random">Random</a>,
): &<a href="../../dependencies/sui-framework/random.md#0x2_random_RandomInner">RandomInner</a> {
    <b>let</b> version = <a href="../../dependencies/sui-framework/versioned.md#0x2_versioned_version">versioned::version</a>(&self.inner);

    // Replace this <b>with</b> a lazy <b>update</b> function when we add a new version of the inner <a href="../../dependencies/sui-framework/object.md#0x2_object">object</a>.
    <b>assert</b>!(version == <a href="../../dependencies/sui-framework/random.md#0x2_random_CURRENT_VERSION">CURRENT_VERSION</a>, <a href="../../dependencies/sui-framework/random.md#0x2_random_EWrongInnerVersion">EWrongInnerVersion</a>);
    <b>let</b> inner: &<a href="../../dependencies/sui-framework/random.md#0x2_random_RandomInner">RandomInner</a> = <a href="../../dependencies/sui-framework/versioned.md#0x2_versioned_load_value">versioned::load_value</a>(&self.inner);
    <b>assert</b>!(inner.version == version, <a href="../../dependencies/sui-framework/random.md#0x2_random_EWrongInnerVersion">EWrongInnerVersion</a>);
    inner
}
</code></pre>



</details>

<a name="0x2_random_update_randomness_state"></a>

## Function `update_randomness_state`



<pre><code><b>fun</b> <a href="../../dependencies/sui-framework/random.md#0x2_random_update_randomness_state">update_randomness_state</a>(self: &<b>mut</b> <a href="../../dependencies/sui-framework/random.md#0x2_random_Random">random::Random</a>, new_round: u64, new_bytes: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, ctx: &<a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../../dependencies/sui-framework/random.md#0x2_random_update_randomness_state">update_randomness_state</a>(
    self: &<b>mut</b> <a href="../../dependencies/sui-framework/random.md#0x2_random_Random">Random</a>,
    new_round: u64,
    new_bytes: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    ctx: &TxContext,
) {
    // Validator will make a special system call <b>with</b> sender set <b>as</b> 0x0.
    <b>assert</b>!(<a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_sender">tx_context::sender</a>(ctx) == @0x0, <a href="../../dependencies/sui-framework/random.md#0x2_random_ENotSystemAddress">ENotSystemAddress</a>);

    // Randomness should only be incremented.
    <b>let</b> epoch = <a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_epoch">tx_context::epoch</a>(ctx);
    <b>let</b> inner = <a href="../../dependencies/sui-framework/random.md#0x2_random_load_inner_mut">load_inner_mut</a>(self);
    <b>if</b> (inner.randomness_round == 0 && inner.epoch == 0 &&
            <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_is_empty">vector::is_empty</a>(&inner.random_bytes)) {
        // First <b>update</b> should be for round zero.
        <b>assert</b>!(new_round == 0, <a href="../../dependencies/sui-framework/random.md#0x2_random_EInvalidRandomnessUpdate">EInvalidRandomnessUpdate</a>);
    } <b>else</b> {
        // Subsequent updates should increment either epoch or randomness_round.
        <b>assert</b>!(
            (epoch == inner.epoch + 1 && new_round == 0) ||
                (new_round == inner.randomness_round + 1),
            <a href="../../dependencies/sui-framework/random.md#0x2_random_EInvalidRandomnessUpdate">EInvalidRandomnessUpdate</a>
        );
    };

    inner.epoch = <a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_epoch">tx_context::epoch</a>(ctx);
    inner.randomness_round = new_round;
    inner.random_bytes = new_bytes;
}
</code></pre>



</details>
