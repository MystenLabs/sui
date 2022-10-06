
<a name="0x2_epoch_time_lock"></a>

# Module `0x2::epoch_time_lock`



-  [Struct `EpochTimeLock`](#0x2_epoch_time_lock_EpochTimeLock)
-  [Constants](#@Constants_0)
-  [Function `new`](#0x2_epoch_time_lock_new)
-  [Function `destroy`](#0x2_epoch_time_lock_destroy)
-  [Function `epoch`](#0x2_epoch_time_lock_epoch)


<pre><code><b>use</b> <a href="tx_context.md#0x2_tx_context">0x2::tx_context</a>;
</code></pre>



<a name="0x2_epoch_time_lock_EpochTimeLock"></a>

## Struct `EpochTimeLock`

Holder of an epoch number that can only be discarded in the epoch or
after the epoch has passed.


<pre><code><b>struct</b> <a href="epoch_time_lock.md#0x2_epoch_time_lock_EpochTimeLock">EpochTimeLock</a> <b>has</b> <b>copy</b>, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>epoch: u64</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0x2_epoch_time_lock_EEpochAlreadyPassed"></a>

The epoch passed into the creation of a lock has already passed.


<pre><code><b>const</b> <a href="epoch_time_lock.md#0x2_epoch_time_lock_EEpochAlreadyPassed">EEpochAlreadyPassed</a>: u64 = 0;
</code></pre>



<a name="0x2_epoch_time_lock_EEpochNotYetEnded"></a>

Attempt is made to unlock a lock that cannot be unlocked yet.


<pre><code><b>const</b> <a href="epoch_time_lock.md#0x2_epoch_time_lock_EEpochNotYetEnded">EEpochNotYetEnded</a>: u64 = 1;
</code></pre>



<a name="0x2_epoch_time_lock_new"></a>

## Function `new`

Create a new epoch time lock with <code>epoch</code>. Aborts if the current epoch is less than the input epoch.


<pre><code><b>public</b> <b>fun</b> <a href="epoch_time_lock.md#0x2_epoch_time_lock_new">new</a>(epoch: u64, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="epoch_time_lock.md#0x2_epoch_time_lock_EpochTimeLock">epoch_time_lock::EpochTimeLock</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="epoch_time_lock.md#0x2_epoch_time_lock_new">new</a>(epoch: u64, ctx: &<b>mut</b> TxContext) : <a href="epoch_time_lock.md#0x2_epoch_time_lock_EpochTimeLock">EpochTimeLock</a> {
    <b>assert</b>!(<a href="tx_context.md#0x2_tx_context_epoch">tx_context::epoch</a>(ctx) &lt; epoch, <a href="epoch_time_lock.md#0x2_epoch_time_lock_EEpochAlreadyPassed">EEpochAlreadyPassed</a>);
    <a href="epoch_time_lock.md#0x2_epoch_time_lock_EpochTimeLock">EpochTimeLock</a> { epoch }
}
</code></pre>



</details>

<a name="0x2_epoch_time_lock_destroy"></a>

## Function `destroy`

Destroys an epoch time lock. Aborts if the current epoch is less than the locked epoch.


<pre><code><b>public</b> <b>fun</b> <a href="epoch_time_lock.md#0x2_epoch_time_lock_destroy">destroy</a>(lock: <a href="epoch_time_lock.md#0x2_epoch_time_lock_EpochTimeLock">epoch_time_lock::EpochTimeLock</a>, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="epoch_time_lock.md#0x2_epoch_time_lock_destroy">destroy</a>(lock: <a href="epoch_time_lock.md#0x2_epoch_time_lock_EpochTimeLock">EpochTimeLock</a>, ctx: &<b>mut</b> TxContext) {
    <b>let</b> <a href="epoch_time_lock.md#0x2_epoch_time_lock_EpochTimeLock">EpochTimeLock</a> { epoch } = lock;
    <b>assert</b>!(<a href="tx_context.md#0x2_tx_context_epoch">tx_context::epoch</a>(ctx) &gt;= epoch, <a href="epoch_time_lock.md#0x2_epoch_time_lock_EEpochNotYetEnded">EEpochNotYetEnded</a>);
}
</code></pre>



</details>

<a name="0x2_epoch_time_lock_epoch"></a>

## Function `epoch`

Getter for the epoch number.


<pre><code><b>public</b> <b>fun</b> <a href="epoch_time_lock.md#0x2_epoch_time_lock_epoch">epoch</a>(lock: &<a href="epoch_time_lock.md#0x2_epoch_time_lock_EpochTimeLock">epoch_time_lock::EpochTimeLock</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="epoch_time_lock.md#0x2_epoch_time_lock_epoch">epoch</a>(lock: &<a href="epoch_time_lock.md#0x2_epoch_time_lock_EpochTimeLock">EpochTimeLock</a>): u64 {
    lock.epoch
}
</code></pre>



</details>
