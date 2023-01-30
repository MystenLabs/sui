
<a name="0x2_clock"></a>

# Module `0x2::clock`

APIs for accessing time from move calls, via the <code><a href="clock.md#0x2_clock_Clock">Clock</a></code>: a unique
shared object that is created at 0x6 during genesis.


-  [Resource `Clock`](#0x2_clock_Clock)
-  [Function `timestamp_ms`](#0x2_clock_timestamp_ms)
-  [Function `create`](#0x2_clock_create)


<pre><code><b>use</b> <a href="object.md#0x2_object">0x2::object</a>;
<b>use</b> <a href="transfer.md#0x2_transfer">0x2::transfer</a>;
</code></pre>



<a name="0x2_clock_Clock"></a>

## Resource `Clock`



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

</dd>
</dl>


</details>

<a name="0x2_clock_timestamp_ms"></a>

## Function `timestamp_ms`

The <code><a href="clock.md#0x2_clock">clock</a></code>'s current timestamp, in milliseconds.


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


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="clock.md#0x2_clock_create">create</a>()
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="clock.md#0x2_clock_create">create</a>() {
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
