
<a name="0x2_clock"></a>

# Module `0x2::clock`

APIs for accessing time from move calls, via the <code><a href="clock.md#0x2_clock_Clock">Clock</a></code>: a unique
shared object that is created at 0x6 during genesis.


-  [Resource `Clock`](#0x2_clock_Clock)
-  [Constants](#@Constants_0)
-  [Function `timestamp_ms`](#0x2_clock_timestamp_ms)
-  [Function `create`](#0x2_clock_create)
-  [Function `consensus_commit_prologue`](#0x2_clock_consensus_commit_prologue)


<pre><code><b>use</b> <a href="object.md#0x2_object">0x2::object</a>;
<b>use</b> <a href="transfer.md#0x2_transfer">0x2::transfer</a>;
<b>use</b> <a href="tx_context.md#0x2_tx_context">0x2::tx_context</a>;
</code></pre>



<a name="0x2_clock_Clock"></a>

## Resource `Clock`

Singleton shared object that exposes time to Move calls.  This
object is found at address 0x6, and can only be read (accessed
via an immutable reference) by entry functions.

Entry Functions that attempt to accept <code><a href="clock.md#0x2_clock_Clock">Clock</a></code> by mutable
reference or value will fail to verify, and honest validators
will not sign or execute transactions that use <code><a href="clock.md#0x2_clock_Clock">Clock</a></code> as an
input parameter, unless it is passed by immutable reference.


<pre><code><b>struct</b> <a href="clock.md#0x2_clock_Clock">Clock</a> <b>has</b> key
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>id: <a href="object.md#0x2_object_UID">object::UID</a></code>
</dt>
<dd>

</dd>
<dt>
<code>timestamp_ms: u64</code>
</dt>
<dd>
 The clock's timestamp, which is set automatically by a
 system transaction every time consensus commits a
 schedule, or by <code>sui::clock::increment_for_testing</code> during
 testing.
</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0x2_clock_ENotSystemAddress"></a>

Sender is not @0x0 the system address.


<pre><code><b>const</b> <a href="clock.md#0x2_clock_ENotSystemAddress">ENotSystemAddress</a>: u64 = 0;
</code></pre>



<a name="0x2_clock_timestamp_ms"></a>

## Function `timestamp_ms`

The <code><a href="clock.md#0x2_clock">clock</a></code>'s current timestamp as a running total of
milliseconds since an arbitrary point in the past.


<pre><code><b>public</b> <b>fun</b> <a href="clock.md#0x2_clock_timestamp_ms">timestamp_ms</a>(<a href="clock.md#0x2_clock">clock</a>: &<a href="clock.md#0x2_clock_Clock">clock::Clock</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="clock.md#0x2_clock_timestamp_ms">timestamp_ms</a>(<a href="clock.md#0x2_clock">clock</a>: &<a href="clock.md#0x2_clock_Clock">Clock</a>): u64 {
    <a href="clock.md#0x2_clock">clock</a>.timestamp_ms
}
</code></pre>



</details>

<a name="0x2_clock_create"></a>

## Function `create`

Create and share the singleton Clock -- this function is
called exactly once, during genesis.


<pre><code><b>fun</b> <a href="clock.md#0x2_clock_create">create</a>(ctx: &<a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="clock.md#0x2_clock_create">create</a>(ctx: &TxContext) {
    <b>assert</b>!(<a href="tx_context.md#0x2_tx_context_sender">tx_context::sender</a>(ctx) == @0x0, <a href="clock.md#0x2_clock_ENotSystemAddress">ENotSystemAddress</a>);

    <a href="transfer.md#0x2_transfer_share_object">transfer::share_object</a>(<a href="clock.md#0x2_clock_Clock">Clock</a> {
        id: <a href="object.md#0x2_object_clock">object::clock</a>(),
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



<pre><code><b>fun</b> <a href="clock.md#0x2_clock_consensus_commit_prologue">consensus_commit_prologue</a>(<a href="clock.md#0x2_clock">clock</a>: &<b>mut</b> <a href="clock.md#0x2_clock_Clock">clock::Clock</a>, timestamp_ms: u64, ctx: &<a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="clock.md#0x2_clock_consensus_commit_prologue">consensus_commit_prologue</a>(
    <a href="clock.md#0x2_clock">clock</a>: &<b>mut</b> <a href="clock.md#0x2_clock_Clock">Clock</a>,
    timestamp_ms: u64,
    ctx: &TxContext,
) {
    // Validator will make a special system call <b>with</b> sender set <b>as</b> 0x0.
    <b>assert</b>!(<a href="tx_context.md#0x2_tx_context_sender">tx_context::sender</a>(ctx) == @0x0, <a href="clock.md#0x2_clock_ENotSystemAddress">ENotSystemAddress</a>);

    <a href="clock.md#0x2_clock">clock</a>.timestamp_ms = timestamp_ms
}
</code></pre>



</details>
