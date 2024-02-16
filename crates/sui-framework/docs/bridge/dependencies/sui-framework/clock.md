
<a name="0x2_clock"></a>

# Module `0x2::clock`



-  [Resource `Clock`](#0x2_clock_Clock)
-  [Constants](#@Constants_0)
-  [Function `timestamp_ms`](#0x2_clock_timestamp_ms)
-  [Function `create`](#0x2_clock_create)
-  [Function `consensus_commit_prologue`](#0x2_clock_consensus_commit_prologue)


<pre><code><b>use</b> <a href="../../dependencies/sui-framework/object.md#0x2_object">0x2::object</a>;
<b>use</b> <a href="../../dependencies/sui-framework/transfer.md#0x2_transfer">0x2::transfer</a>;
<b>use</b> <a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context">0x2::tx_context</a>;
</code></pre>



<a name="0x2_clock_Clock"></a>

## Resource `Clock`



<pre><code><b>struct</b> <a href="../../dependencies/sui-framework/clock.md#0x2_clock_Clock">Clock</a> <b>has</b> key
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
<code>timestamp_ms: u64</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0x2_clock_ENotSystemAddress"></a>



<pre><code><b>const</b> <a href="../../dependencies/sui-framework/clock.md#0x2_clock_ENotSystemAddress">ENotSystemAddress</a>: u64 = 0;
</code></pre>



<a name="0x2_clock_timestamp_ms"></a>

## Function `timestamp_ms`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/clock.md#0x2_clock_timestamp_ms">timestamp_ms</a>(<a href="../../dependencies/sui-framework/clock.md#0x2_clock">clock</a>: &<a href="../../dependencies/sui-framework/clock.md#0x2_clock_Clock">clock::Clock</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/clock.md#0x2_clock_timestamp_ms">timestamp_ms</a>(<a href="../../dependencies/sui-framework/clock.md#0x2_clock">clock</a>: &<a href="../../dependencies/sui-framework/clock.md#0x2_clock_Clock">Clock</a>): u64 {
    <a href="../../dependencies/sui-framework/clock.md#0x2_clock">clock</a>.timestamp_ms
}
</code></pre>



</details>

<a name="0x2_clock_create"></a>

## Function `create`



<pre><code><b>fun</b> <a href="../../dependencies/sui-framework/clock.md#0x2_clock_create">create</a>(ctx: &<a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../../dependencies/sui-framework/clock.md#0x2_clock_create">create</a>(ctx: &TxContext) {
    <b>assert</b>!(<a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_sender">tx_context::sender</a>(ctx) == @0x0, <a href="../../dependencies/sui-framework/clock.md#0x2_clock_ENotSystemAddress">ENotSystemAddress</a>);

    <a href="../../dependencies/sui-framework/transfer.md#0x2_transfer_share_object">transfer::share_object</a>(<a href="../../dependencies/sui-framework/clock.md#0x2_clock_Clock">Clock</a> {
        id: <a href="../../dependencies/sui-framework/object.md#0x2_object_clock">object::clock</a>(),
        // Initialised <b>to</b> zero, but set <b>to</b> a real timestamp by a
        // system transaction before it can be witnessed by a <b>move</b>
        // call.
        timestamp_ms: 0,
    })
}
</code></pre>



</details>

<a name="0x2_clock_consensus_commit_prologue"></a>

## Function `consensus_commit_prologue`



<pre><code><b>fun</b> <a href="../../dependencies/sui-framework/clock.md#0x2_clock_consensus_commit_prologue">consensus_commit_prologue</a>(<a href="../../dependencies/sui-framework/clock.md#0x2_clock">clock</a>: &<b>mut</b> <a href="../../dependencies/sui-framework/clock.md#0x2_clock_Clock">clock::Clock</a>, timestamp_ms: u64, ctx: &<a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../../dependencies/sui-framework/clock.md#0x2_clock_consensus_commit_prologue">consensus_commit_prologue</a>(
    <a href="../../dependencies/sui-framework/clock.md#0x2_clock">clock</a>: &<b>mut</b> <a href="../../dependencies/sui-framework/clock.md#0x2_clock_Clock">Clock</a>,
    timestamp_ms: u64,
    ctx: &TxContext,
) {
    // Validator will make a special system call <b>with</b> sender set <b>as</b> 0x0.
    <b>assert</b>!(<a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_sender">tx_context::sender</a>(ctx) == @0x0, <a href="../../dependencies/sui-framework/clock.md#0x2_clock_ENotSystemAddress">ENotSystemAddress</a>);

    <a href="../../dependencies/sui-framework/clock.md#0x2_clock">clock</a>.timestamp_ms = timestamp_ms
}
</code></pre>



</details>
