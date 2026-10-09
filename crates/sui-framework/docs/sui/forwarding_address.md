---
title: Module `sui::forwarding_address`
---

Registry of forwarding addresses.

Address layout: <code>[u48 <a href="../sui/forwarding_address.md#sui_forwarding_address_master_id">master_id</a>, little-endian][9 bytes of 0xfa][u8 variant][16 payload bytes]</code>.
The master ID, magic and variant positions are fixed; the variant alone decides what the
payload bytes mean. Variant 0 gives them no on-chain meaning.

Resolution happens outside Move: at the end of every transaction, the adapter reroutes funds
deposited to a forwarding address to the registered master and emits <code><a href="../sui/forwarding_address.md#sui_forwarding_address_ForwardingDeposit">ForwardingDeposit</a></code>.
Objects cannot be sent to a forwarding address.

Two keys control an id. The master receives the funds and is expected to be hot. The
<code><a href="../sui/forwarding_address.md#sui_forwarding_address_MasterCap">MasterCap</a></code> is expected to be cold and is the only thing that can move funds elsewhere, through
a rotation that takes effect after the id's delay. Either key can pause the id, which makes
deposits fail until the cap unpauses it, so a leaked master key is contained by pausing and
then rotating, and a leaked cap is caught by the master cancelling the rotation during the
delay.


-  [Struct `ForwardingAddressRegistry`](#sui_forwarding_address_ForwardingAddressRegistry)
-  [Struct `MasterCap`](#sui_forwarding_address_MasterCap)
-  [Struct `MasterRecord`](#sui_forwarding_address_MasterRecord)
-  [Struct `PendingRotation`](#sui_forwarding_address_PendingRotation)
-  [Struct `MasterIdCounter`](#sui_forwarding_address_MasterIdCounter)
-  [Struct `ForwardingDeposit`](#sui_forwarding_address_ForwardingDeposit)
-  [Struct `MasterRegistered`](#sui_forwarding_address_MasterRegistered)
-  [Struct `Paused`](#sui_forwarding_address_Paused)
-  [Struct `Unpaused`](#sui_forwarding_address_Unpaused)
-  [Struct `RotationProposed`](#sui_forwarding_address_RotationProposed)
-  [Struct `RotationCancelled`](#sui_forwarding_address_RotationCancelled)
-  [Struct `RotationFinalized`](#sui_forwarding_address_RotationFinalized)
-  [Struct `RotationDelayIncreased`](#sui_forwarding_address_RotationDelayIncreased)
-  [Constants](#@Constants_0)
-  [Function `register`](#sui_forwarding_address_register)
-  [Function `master_id`](#sui_forwarding_address_master_id)
-  [Function `pause`](#sui_forwarding_address_pause)
-  [Function `pause_by_master`](#sui_forwarding_address_pause_by_master)
-  [Function `unpause`](#sui_forwarding_address_unpause)
-  [Function `pause_impl`](#sui_forwarding_address_pause_impl)
-  [Function `propose_rotation`](#sui_forwarding_address_propose_rotation)
-  [Function `cancel_rotation`](#sui_forwarding_address_cancel_rotation)
-  [Function `cancel_rotation_by_master`](#sui_forwarding_address_cancel_rotation_by_master)
-  [Function `cancel_rotation_impl`](#sui_forwarding_address_cancel_rotation_impl)
-  [Function `finalize_rotation`](#sui_forwarding_address_finalize_rotation)
-  [Function `increase_rotation_delay`](#sui_forwarding_address_increase_rotation_delay)
-  [Function `assert_master`](#sui_forwarding_address_assert_master)
-  [Function `record_mut`](#sui_forwarding_address_record_mut)
-  [Function `is_forwarding_address`](#sui_forwarding_address_is_forwarding_address)
-  [Function `charge_registration_fee`](#sui_forwarding_address_charge_registration_fee)
-  [Function `allocate_master_id`](#sui_forwarding_address_allocate_master_id)
-  [Function `mix_master_id`](#sui_forwarding_address_mix_master_id)
-  [Function `mul_mod_2_48`](#sui_forwarding_address_mul_mod_2_48)
-  [Function `create`](#sui_forwarding_address_create)


<pre><code><b>use</b> <a href="../std/address.md#std_address">std::address</a>;
<b>use</b> <a href="../std/ascii.md#std_ascii">std::ascii</a>;
<b>use</b> <a href="../std/bcs.md#std_bcs">std::bcs</a>;
<b>use</b> <a href="../std/option.md#std_option">std::option</a>;
<b>use</b> <a href="../std/string.md#std_string">std::string</a>;
<b>use</b> <a href="../std/type_name.md#std_type_name">std::type_name</a>;
<b>use</b> <a href="../std/vector.md#std_vector">std::vector</a>;
<b>use</b> <a href="../sui/accumulator.md#sui_accumulator">sui::accumulator</a>;
<b>use</b> <a href="../sui/accumulator_settlement.md#sui_accumulator_settlement">sui::accumulator_settlement</a>;
<b>use</b> <a href="../sui/address.md#sui_address">sui::address</a>;
<b>use</b> <a href="../sui/bcs.md#sui_bcs">sui::bcs</a>;
<b>use</b> <a href="../sui/dynamic_field.md#sui_dynamic_field">sui::dynamic_field</a>;
<b>use</b> <a href="../sui/event.md#sui_event">sui::event</a>;
<b>use</b> <a href="../sui/hash.md#sui_hash">sui::hash</a>;
<b>use</b> <a href="../sui/hex.md#sui_hex">sui::hex</a>;
<b>use</b> <a href="../sui/object.md#sui_object">sui::object</a>;
<b>use</b> <a href="../sui/party.md#sui_party">sui::party</a>;
<b>use</b> <a href="../sui/transfer.md#sui_transfer">sui::transfer</a>;
<b>use</b> <a href="../sui/tx_context.md#sui_tx_context">sui::tx_context</a>;
<b>use</b> <a href="../sui/vec_map.md#sui_vec_map">sui::vec_map</a>;
</code></pre>



<a name="sui_forwarding_address_ForwardingAddressRegistry"></a>

## Struct `ForwardingAddressRegistry`

Singleton shared object whose UID owns the master ID records and the allocation counter.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_ForwardingAddressRegistry">ForwardingAddressRegistry</a> <b>has</b> key
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

<a name="sui_forwarding_address_MasterCap"></a>

## Struct `MasterCap`

Ownership of a master ID, handed to the registrant. Keep it cold: it is the only key that can
redirect funds (rotation) or resume deposits (unpause).


<pre><code><b>public</b> <b>struct</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_MasterCap">MasterCap</a> <b>has</b> key, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>id: <a href="../sui/object.md#sui_object_UID">sui::object::UID</a></code>
</dt>
<dd>
</dd>
<dt>
<code><a href="../sui/forwarding_address.md#sui_forwarding_address_master_id">master_id</a>: u64</code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="sui_forwarding_address_MasterRecord"></a>

## Struct `MasterRecord`

Dynamic field on the registry, keyed by master ID.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_MasterRecord">MasterRecord</a> <b>has</b> store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>master: <b>address</b></code>
</dt>
<dd>
</dd>
<dt>
<code>paused: bool</code>
</dt>
<dd>
 Deposits to the id fail while paused.
</dd>
<dt>
<code>pending: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;<a href="../sui/forwarding_address.md#sui_forwarding_address_PendingRotation">sui::forwarding_address::PendingRotation</a>&gt;</code>
</dt>
<dd>
</dd>
<dt>
<code>rotation_delay_epochs: u64</code>
</dt>
<dd>
 Epochs between proposing a rotation and being able to finalize it.
</dd>
</dl>


</details>

<a name="sui_forwarding_address_PendingRotation"></a>

## Struct `PendingRotation`



<pre><code><b>public</b> <b>struct</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_PendingRotation">PendingRotation</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>new_master: <b>address</b></code>
</dt>
<dd>
</dd>
<dt>
<code>effective_epoch: u64</code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="sui_forwarding_address_MasterIdCounter"></a>

## Struct `MasterIdCounter`

Dynamic field key for the next master ID counter (a <code>u64</code>; the last 48-bit id is allocatable).


<pre><code><b>public</b> <b>struct</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_MasterIdCounter">MasterIdCounter</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
</dl>


</details>

<a name="sui_forwarding_address_ForwardingDeposit"></a>

## Struct `ForwardingDeposit`

Emitted by the adapter when funds deposited to a forwarding address are rerouted to its
master. <code>T</code> is the coin type of the <code>Balance</code> deposited.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_ForwardingDeposit">ForwardingDeposit</a>&lt;<b>phantom</b> T&gt; <b>has</b> <b>copy</b>, drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code><a href="../sui/forwarding_address.md#sui_forwarding_address">forwarding_address</a>: <b>address</b></code>
</dt>
<dd>
</dd>
<dt>
<code>master: <b>address</b></code>
</dt>
<dd>
</dd>
<dt>
<code>amount: u64</code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="sui_forwarding_address_MasterRegistered"></a>

## Struct `MasterRegistered`

Emitted when a master ID is allocated.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_MasterRegistered">MasterRegistered</a> <b>has</b> <b>copy</b>, drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code><a href="../sui/forwarding_address.md#sui_forwarding_address_master_id">master_id</a>: u64</code>
</dt>
<dd>
</dd>
<dt>
<code>master: <b>address</b></code>
</dt>
<dd>
</dd>
<dt>
<code>cap_id: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a></code>
</dt>
<dd>
</dd>
<dt>
<code>rotation_delay_epochs: u64</code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="sui_forwarding_address_Paused"></a>

## Struct `Paused`



<pre><code><b>public</b> <b>struct</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_Paused">Paused</a> <b>has</b> <b>copy</b>, drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code><a href="../sui/forwarding_address.md#sui_forwarding_address_master_id">master_id</a>: u64</code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="sui_forwarding_address_Unpaused"></a>

## Struct `Unpaused`



<pre><code><b>public</b> <b>struct</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_Unpaused">Unpaused</a> <b>has</b> <b>copy</b>, drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code><a href="../sui/forwarding_address.md#sui_forwarding_address_master_id">master_id</a>: u64</code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="sui_forwarding_address_RotationProposed"></a>

## Struct `RotationProposed`



<pre><code><b>public</b> <b>struct</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_RotationProposed">RotationProposed</a> <b>has</b> <b>copy</b>, drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code><a href="../sui/forwarding_address.md#sui_forwarding_address_master_id">master_id</a>: u64</code>
</dt>
<dd>
</dd>
<dt>
<code>new_master: <b>address</b></code>
</dt>
<dd>
</dd>
<dt>
<code>effective_epoch: u64</code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="sui_forwarding_address_RotationCancelled"></a>

## Struct `RotationCancelled`



<pre><code><b>public</b> <b>struct</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_RotationCancelled">RotationCancelled</a> <b>has</b> <b>copy</b>, drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code><a href="../sui/forwarding_address.md#sui_forwarding_address_master_id">master_id</a>: u64</code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="sui_forwarding_address_RotationFinalized"></a>

## Struct `RotationFinalized`



<pre><code><b>public</b> <b>struct</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_RotationFinalized">RotationFinalized</a> <b>has</b> <b>copy</b>, drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code><a href="../sui/forwarding_address.md#sui_forwarding_address_master_id">master_id</a>: u64</code>
</dt>
<dd>
</dd>
<dt>
<code>master: <b>address</b></code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="sui_forwarding_address_RotationDelayIncreased"></a>

## Struct `RotationDelayIncreased`



<pre><code><b>public</b> <b>struct</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_RotationDelayIncreased">RotationDelayIncreased</a> <b>has</b> <b>copy</b>, drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code><a href="../sui/forwarding_address.md#sui_forwarding_address_master_id">master_id</a>: u64</code>
</dt>
<dd>
</dd>
<dt>
<code>rotation_delay_epochs: u64</code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="sui_forwarding_address_ENotSystemAddress"></a>



<pre><code>#[error]
<b>const</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_ENotSystemAddress">ENotSystemAddress</a>: vector&lt;u8&gt; = b"Only the system can <a href="../sui/forwarding_address.md#sui_forwarding_address_create">create</a> the forwarding <b>address</b> registry.";
</code></pre>



<a name="sui_forwarding_address_EMasterIdsExhausted"></a>



<pre><code>#[error]
<b>const</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_EMasterIdsExhausted">EMasterIdsExhausted</a>: vector&lt;u8&gt; = b"All master IDs have been allocated.";
</code></pre>



<a name="sui_forwarding_address_EInvalidRotationDelay"></a>



<pre><code>#[error]
<b>const</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_EInvalidRotationDelay">EInvalidRotationDelay</a>: vector&lt;u8&gt; = b"The rotation delay must be between 1 and 30 epochs and can only be increased.";
</code></pre>



<a name="sui_forwarding_address_ENotMaster"></a>



<pre><code>#[error]
<b>const</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_ENotMaster">ENotMaster</a>: vector&lt;u8&gt; = b"Only the current master can do this without the <a href="../sui/forwarding_address.md#sui_forwarding_address_MasterCap">MasterCap</a>.";
</code></pre>



<a name="sui_forwarding_address_EForwardingAddressMaster"></a>



<pre><code>#[error]
<b>const</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_EForwardingAddressMaster">EForwardingAddressMaster</a>: vector&lt;u8&gt; = b"A forwarding <b>address</b> cannot be the master of another forwarding <b>address</b>.";
</code></pre>



<a name="sui_forwarding_address_ENoPendingRotation"></a>



<pre><code>#[error]
<b>const</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_ENoPendingRotation">ENoPendingRotation</a>: vector&lt;u8&gt; = b"No rotation is pending <b>for</b> this master ID.";
</code></pre>



<a name="sui_forwarding_address_ERotationNotDue"></a>



<pre><code>#[error]
<b>const</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_ERotationNotDue">ERotationNotDue</a>: vector&lt;u8&gt; = b"The pending rotation's delay <b>has</b> not elapsed.";
</code></pre>



<a name="sui_forwarding_address_MAX_MASTER_ID"></a>

Master ids are 48 bits: the address layout stores them in six bytes.


<pre><code><b>const</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_MAX_MASTER_ID">MAX_MASTER_ID</a>: u64 = 281474976710655;
</code></pre>



<a name="sui_forwarding_address_MIN_ROTATION_DELAY_EPOCHS"></a>



<pre><code><b>const</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_MIN_ROTATION_DELAY_EPOCHS">MIN_ROTATION_DELAY_EPOCHS</a>: u64 = 1;
</code></pre>



<a name="sui_forwarding_address_MAX_ROTATION_DELAY_EPOCHS"></a>



<pre><code><b>const</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_MAX_ROTATION_DELAY_EPOCHS">MAX_ROTATION_DELAY_EPOCHS</a>: u64 = 30;
</code></pre>



<a name="sui_forwarding_address_MAGIC_START"></a>

Where the forwarding magic sits in an address: bytes 6..15 are all <code>0xfa</code>.


<pre><code><b>const</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_MAGIC_START">MAGIC_START</a>: u64 = 6;
</code></pre>



<a name="sui_forwarding_address_MAGIC_END"></a>



<pre><code><b>const</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_MAGIC_END">MAGIC_END</a>: u64 = 15;
</code></pre>



<a name="sui_forwarding_address_MAGIC_BYTE"></a>



<pre><code><b>const</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_MAGIC_BYTE">MAGIC_BYTE</a>: u8 = 250;
</code></pre>



<a name="sui_forwarding_address_register"></a>

## Function `register`

Allocate a fresh master ID for <code>ctx.sender()</code> and return the capability for it.
<code>rotation_delay_epochs</code> is how long a proposed rotation waits before it can be finalized; it
must be between 1 and 30 epochs and can later only be increased.

Charges a deliberately high gas fee, since every registration permanently grows the registry.
Aborts once every master ID has been allocated; IDs are never reused.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_register">register</a>(registry: &<b>mut</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_ForwardingAddressRegistry">sui::forwarding_address::ForwardingAddressRegistry</a>, rotation_delay_epochs: u64, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <a href="../sui/forwarding_address.md#sui_forwarding_address_MasterCap">sui::forwarding_address::MasterCap</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_register">register</a>(
    registry: &<b>mut</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_ForwardingAddressRegistry">ForwardingAddressRegistry</a>,
    rotation_delay_epochs: u64,
    ctx: &<b>mut</b> TxContext,
): <a href="../sui/forwarding_address.md#sui_forwarding_address_MasterCap">MasterCap</a> {
    <b>assert</b>!(
        <a href="../sui/forwarding_address.md#sui_forwarding_address_MIN_ROTATION_DELAY_EPOCHS">MIN_ROTATION_DELAY_EPOCHS</a> &lt;= rotation_delay_epochs &&
            rotation_delay_epochs &lt;= <a href="../sui/forwarding_address.md#sui_forwarding_address_MAX_ROTATION_DELAY_EPOCHS">MAX_ROTATION_DELAY_EPOCHS</a>,
        <a href="../sui/forwarding_address.md#sui_forwarding_address_EInvalidRotationDelay">EInvalidRotationDelay</a>,
    );
    <a href="../sui/forwarding_address.md#sui_forwarding_address_charge_registration_fee">charge_registration_fee</a>();
    <b>let</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_master_id">master_id</a> = <a href="../sui/forwarding_address.md#sui_forwarding_address_allocate_master_id">allocate_master_id</a>(registry);
    <b>let</b> master = ctx.sender();
    <a href="../sui/dynamic_field.md#sui_dynamic_field_add">dynamic_field::add</a>(
        &<b>mut</b> registry.id,
        <a href="../sui/forwarding_address.md#sui_forwarding_address_master_id">master_id</a>,
        <a href="../sui/forwarding_address.md#sui_forwarding_address_MasterRecord">MasterRecord</a> { master, paused: <b>false</b>, pending: option::none(), rotation_delay_epochs },
    );
    <b>let</b> cap = <a href="../sui/forwarding_address.md#sui_forwarding_address_MasterCap">MasterCap</a> { id: <a href="../sui/object.md#sui_object_new">object::new</a>(ctx), <a href="../sui/forwarding_address.md#sui_forwarding_address_master_id">master_id</a> };
    <a href="../sui/event.md#sui_event_emit">event::emit</a>(<a href="../sui/forwarding_address.md#sui_forwarding_address_MasterRegistered">MasterRegistered</a> {
        <a href="../sui/forwarding_address.md#sui_forwarding_address_master_id">master_id</a>,
        master,
        cap_id: <a href="../sui/object.md#sui_object_id">object::id</a>(&cap),
        rotation_delay_epochs,
    });
    cap
}
</code></pre>



</details>

<a name="sui_forwarding_address_master_id"></a>

## Function `master_id`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_master_id">master_id</a>(cap: &<a href="../sui/forwarding_address.md#sui_forwarding_address_MasterCap">sui::forwarding_address::MasterCap</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_master_id">master_id</a>(cap: &<a href="../sui/forwarding_address.md#sui_forwarding_address_MasterCap">MasterCap</a>): u64 {
    cap.<a href="../sui/forwarding_address.md#sui_forwarding_address_master_id">master_id</a>
}
</code></pre>



</details>

<a name="sui_forwarding_address_pause"></a>

## Function `pause`

Stop deposits to the id. Takes effect at the end of this transaction.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_pause">pause</a>(registry: &<b>mut</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_ForwardingAddressRegistry">sui::forwarding_address::ForwardingAddressRegistry</a>, cap: &<a href="../sui/forwarding_address.md#sui_forwarding_address_MasterCap">sui::forwarding_address::MasterCap</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_pause">pause</a>(registry: &<b>mut</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_ForwardingAddressRegistry">ForwardingAddressRegistry</a>, cap: &<a href="../sui/forwarding_address.md#sui_forwarding_address_MasterCap">MasterCap</a>) {
    <a href="../sui/forwarding_address.md#sui_forwarding_address_pause_impl">pause_impl</a>(registry, cap.<a href="../sui/forwarding_address.md#sui_forwarding_address_master_id">master_id</a>);
}
</code></pre>



</details>

<a name="sui_forwarding_address_pause_by_master"></a>

## Function `pause_by_master`

The current master can pause without the cap, so a hot key can hit the brake.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_pause_by_master">pause_by_master</a>(registry: &<b>mut</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_ForwardingAddressRegistry">sui::forwarding_address::ForwardingAddressRegistry</a>, <a href="../sui/forwarding_address.md#sui_forwarding_address_master_id">master_id</a>: u64, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_pause_by_master">pause_by_master</a>(
    registry: &<b>mut</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_ForwardingAddressRegistry">ForwardingAddressRegistry</a>,
    <a href="../sui/forwarding_address.md#sui_forwarding_address_master_id">master_id</a>: u64,
    ctx: &TxContext,
) {
    <a href="../sui/forwarding_address.md#sui_forwarding_address_assert_master">assert_master</a>(registry, <a href="../sui/forwarding_address.md#sui_forwarding_address_master_id">master_id</a>, ctx);
    <a href="../sui/forwarding_address.md#sui_forwarding_address_pause_impl">pause_impl</a>(registry, <a href="../sui/forwarding_address.md#sui_forwarding_address_master_id">master_id</a>);
}
</code></pre>



</details>

<a name="sui_forwarding_address_unpause"></a>

## Function `unpause`

Only the cap can resume deposits, so a leaked master key cannot undo a pause.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_unpause">unpause</a>(registry: &<b>mut</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_ForwardingAddressRegistry">sui::forwarding_address::ForwardingAddressRegistry</a>, cap: &<a href="../sui/forwarding_address.md#sui_forwarding_address_MasterCap">sui::forwarding_address::MasterCap</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_unpause">unpause</a>(registry: &<b>mut</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_ForwardingAddressRegistry">ForwardingAddressRegistry</a>, cap: &<a href="../sui/forwarding_address.md#sui_forwarding_address_MasterCap">MasterCap</a>) {
    <b>let</b> record = <a href="../sui/forwarding_address.md#sui_forwarding_address_record_mut">record_mut</a>(registry, cap.<a href="../sui/forwarding_address.md#sui_forwarding_address_master_id">master_id</a>);
    <b>if</b> (record.paused) {
        record.paused = <b>false</b>;
        <a href="../sui/event.md#sui_event_emit">event::emit</a>(<a href="../sui/forwarding_address.md#sui_forwarding_address_Unpaused">Unpaused</a> { <a href="../sui/forwarding_address.md#sui_forwarding_address_master_id">master_id</a>: cap.<a href="../sui/forwarding_address.md#sui_forwarding_address_master_id">master_id</a> });
    }
}
</code></pre>



</details>

<a name="sui_forwarding_address_pause_impl"></a>

## Function `pause_impl`



<pre><code><b>fun</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_pause_impl">pause_impl</a>(registry: &<b>mut</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_ForwardingAddressRegistry">sui::forwarding_address::ForwardingAddressRegistry</a>, <a href="../sui/forwarding_address.md#sui_forwarding_address_master_id">master_id</a>: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_pause_impl">pause_impl</a>(registry: &<b>mut</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_ForwardingAddressRegistry">ForwardingAddressRegistry</a>, <a href="../sui/forwarding_address.md#sui_forwarding_address_master_id">master_id</a>: u64) {
    <b>let</b> record = <a href="../sui/forwarding_address.md#sui_forwarding_address_record_mut">record_mut</a>(registry, <a href="../sui/forwarding_address.md#sui_forwarding_address_master_id">master_id</a>);
    <b>if</b> (!record.paused) {
        record.paused = <b>true</b>;
        <a href="../sui/event.md#sui_event_emit">event::emit</a>(<a href="../sui/forwarding_address.md#sui_forwarding_address_Paused">Paused</a> { <a href="../sui/forwarding_address.md#sui_forwarding_address_master_id">master_id</a> });
    }
}
</code></pre>



</details>

<a name="sui_forwarding_address_propose_rotation"></a>

## Function `propose_rotation`

Propose a new master. It takes effect once <code><a href="../sui/forwarding_address.md#sui_forwarding_address_finalize_rotation">finalize_rotation</a></code> is called in an epoch at least
<code>rotation_delay_epochs</code> after this one. Deposits keep going to the current master meanwhile;
pause first if they should stop. A new proposal replaces a pending one.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_propose_rotation">propose_rotation</a>(registry: &<b>mut</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_ForwardingAddressRegistry">sui::forwarding_address::ForwardingAddressRegistry</a>, cap: &<a href="../sui/forwarding_address.md#sui_forwarding_address_MasterCap">sui::forwarding_address::MasterCap</a>, new_master: <b>address</b>, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_propose_rotation">propose_rotation</a>(
    registry: &<b>mut</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_ForwardingAddressRegistry">ForwardingAddressRegistry</a>,
    cap: &<a href="../sui/forwarding_address.md#sui_forwarding_address_MasterCap">MasterCap</a>,
    new_master: <b>address</b>,
    ctx: &TxContext,
) {
    <b>assert</b>!(!<a href="../sui/forwarding_address.md#sui_forwarding_address_is_forwarding_address">is_forwarding_address</a>(new_master), <a href="../sui/forwarding_address.md#sui_forwarding_address_EForwardingAddressMaster">EForwardingAddressMaster</a>);
    <b>let</b> record = <a href="../sui/forwarding_address.md#sui_forwarding_address_record_mut">record_mut</a>(registry, cap.<a href="../sui/forwarding_address.md#sui_forwarding_address_master_id">master_id</a>);
    <b>let</b> effective_epoch = ctx.epoch() + record.rotation_delay_epochs;
    record.pending = option::some(<a href="../sui/forwarding_address.md#sui_forwarding_address_PendingRotation">PendingRotation</a> { new_master, effective_epoch });
    <a href="../sui/event.md#sui_event_emit">event::emit</a>(<a href="../sui/forwarding_address.md#sui_forwarding_address_RotationProposed">RotationProposed</a> { <a href="../sui/forwarding_address.md#sui_forwarding_address_master_id">master_id</a>: cap.<a href="../sui/forwarding_address.md#sui_forwarding_address_master_id">master_id</a>, new_master, effective_epoch });
}
</code></pre>



</details>

<a name="sui_forwarding_address_cancel_rotation"></a>

## Function `cancel_rotation`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_cancel_rotation">cancel_rotation</a>(registry: &<b>mut</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_ForwardingAddressRegistry">sui::forwarding_address::ForwardingAddressRegistry</a>, cap: &<a href="../sui/forwarding_address.md#sui_forwarding_address_MasterCap">sui::forwarding_address::MasterCap</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_cancel_rotation">cancel_rotation</a>(registry: &<b>mut</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_ForwardingAddressRegistry">ForwardingAddressRegistry</a>, cap: &<a href="../sui/forwarding_address.md#sui_forwarding_address_MasterCap">MasterCap</a>) {
    <a href="../sui/forwarding_address.md#sui_forwarding_address_cancel_rotation_impl">cancel_rotation_impl</a>(registry, cap.<a href="../sui/forwarding_address.md#sui_forwarding_address_master_id">master_id</a>);
}
</code></pre>



</details>

<a name="sui_forwarding_address_cancel_rotation_by_master"></a>

## Function `cancel_rotation_by_master`

The current master can cancel without the cap, which is what stops a rotation proposed with a
stolen cap.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_cancel_rotation_by_master">cancel_rotation_by_master</a>(registry: &<b>mut</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_ForwardingAddressRegistry">sui::forwarding_address::ForwardingAddressRegistry</a>, <a href="../sui/forwarding_address.md#sui_forwarding_address_master_id">master_id</a>: u64, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_cancel_rotation_by_master">cancel_rotation_by_master</a>(
    registry: &<b>mut</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_ForwardingAddressRegistry">ForwardingAddressRegistry</a>,
    <a href="../sui/forwarding_address.md#sui_forwarding_address_master_id">master_id</a>: u64,
    ctx: &TxContext,
) {
    <a href="../sui/forwarding_address.md#sui_forwarding_address_assert_master">assert_master</a>(registry, <a href="../sui/forwarding_address.md#sui_forwarding_address_master_id">master_id</a>, ctx);
    <a href="../sui/forwarding_address.md#sui_forwarding_address_cancel_rotation_impl">cancel_rotation_impl</a>(registry, <a href="../sui/forwarding_address.md#sui_forwarding_address_master_id">master_id</a>);
}
</code></pre>



</details>

<a name="sui_forwarding_address_cancel_rotation_impl"></a>

## Function `cancel_rotation_impl`



<pre><code><b>fun</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_cancel_rotation_impl">cancel_rotation_impl</a>(registry: &<b>mut</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_ForwardingAddressRegistry">sui::forwarding_address::ForwardingAddressRegistry</a>, <a href="../sui/forwarding_address.md#sui_forwarding_address_master_id">master_id</a>: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_cancel_rotation_impl">cancel_rotation_impl</a>(registry: &<b>mut</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_ForwardingAddressRegistry">ForwardingAddressRegistry</a>, <a href="../sui/forwarding_address.md#sui_forwarding_address_master_id">master_id</a>: u64) {
    <b>let</b> record = <a href="../sui/forwarding_address.md#sui_forwarding_address_record_mut">record_mut</a>(registry, <a href="../sui/forwarding_address.md#sui_forwarding_address_master_id">master_id</a>);
    <b>assert</b>!(record.pending.is_some(), <a href="../sui/forwarding_address.md#sui_forwarding_address_ENoPendingRotation">ENoPendingRotation</a>);
    record.pending = option::none();
    <a href="../sui/event.md#sui_event_emit">event::emit</a>(<a href="../sui/forwarding_address.md#sui_forwarding_address_RotationCancelled">RotationCancelled</a> { <a href="../sui/forwarding_address.md#sui_forwarding_address_master_id">master_id</a> });
}
</code></pre>



</details>

<a name="sui_forwarding_address_finalize_rotation"></a>

## Function `finalize_rotation`

Anyone can finalize a due rotation, so the cap can stay cold once it has proposed.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_finalize_rotation">finalize_rotation</a>(registry: &<b>mut</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_ForwardingAddressRegistry">sui::forwarding_address::ForwardingAddressRegistry</a>, <a href="../sui/forwarding_address.md#sui_forwarding_address_master_id">master_id</a>: u64, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_finalize_rotation">finalize_rotation</a>(
    registry: &<b>mut</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_ForwardingAddressRegistry">ForwardingAddressRegistry</a>,
    <a href="../sui/forwarding_address.md#sui_forwarding_address_master_id">master_id</a>: u64,
    ctx: &TxContext,
) {
    <b>let</b> record = <a href="../sui/forwarding_address.md#sui_forwarding_address_record_mut">record_mut</a>(registry, <a href="../sui/forwarding_address.md#sui_forwarding_address_master_id">master_id</a>);
    <b>assert</b>!(record.pending.is_some(), <a href="../sui/forwarding_address.md#sui_forwarding_address_ENoPendingRotation">ENoPendingRotation</a>);
    <b>let</b> pending = record.pending.extract();
    <b>assert</b>!(ctx.epoch() &gt;= pending.effective_epoch, <a href="../sui/forwarding_address.md#sui_forwarding_address_ERotationNotDue">ERotationNotDue</a>);
    record.master = pending.new_master;
    <a href="../sui/event.md#sui_event_emit">event::emit</a>(<a href="../sui/forwarding_address.md#sui_forwarding_address_RotationFinalized">RotationFinalized</a> { <a href="../sui/forwarding_address.md#sui_forwarding_address_master_id">master_id</a>, master: pending.new_master });
}
</code></pre>



</details>

<a name="sui_forwarding_address_increase_rotation_delay"></a>

## Function `increase_rotation_delay`

Lengthen the rotation delay. Shortening it is not allowed: a stolen cap could otherwise
shorten it and rotate before the master notices.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_increase_rotation_delay">increase_rotation_delay</a>(registry: &<b>mut</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_ForwardingAddressRegistry">sui::forwarding_address::ForwardingAddressRegistry</a>, cap: &<a href="../sui/forwarding_address.md#sui_forwarding_address_MasterCap">sui::forwarding_address::MasterCap</a>, rotation_delay_epochs: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_increase_rotation_delay">increase_rotation_delay</a>(
    registry: &<b>mut</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_ForwardingAddressRegistry">ForwardingAddressRegistry</a>,
    cap: &<a href="../sui/forwarding_address.md#sui_forwarding_address_MasterCap">MasterCap</a>,
    rotation_delay_epochs: u64,
) {
    <b>let</b> record = <a href="../sui/forwarding_address.md#sui_forwarding_address_record_mut">record_mut</a>(registry, cap.<a href="../sui/forwarding_address.md#sui_forwarding_address_master_id">master_id</a>);
    <b>assert</b>!(
        record.rotation_delay_epochs &lt; rotation_delay_epochs &&
            rotation_delay_epochs &lt;= <a href="../sui/forwarding_address.md#sui_forwarding_address_MAX_ROTATION_DELAY_EPOCHS">MAX_ROTATION_DELAY_EPOCHS</a>,
        <a href="../sui/forwarding_address.md#sui_forwarding_address_EInvalidRotationDelay">EInvalidRotationDelay</a>,
    );
    record.rotation_delay_epochs = rotation_delay_epochs;
    <a href="../sui/event.md#sui_event_emit">event::emit</a>(<a href="../sui/forwarding_address.md#sui_forwarding_address_RotationDelayIncreased">RotationDelayIncreased</a> { <a href="../sui/forwarding_address.md#sui_forwarding_address_master_id">master_id</a>: cap.<a href="../sui/forwarding_address.md#sui_forwarding_address_master_id">master_id</a>, rotation_delay_epochs });
}
</code></pre>



</details>

<a name="sui_forwarding_address_assert_master"></a>

## Function `assert_master`



<pre><code><b>fun</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_assert_master">assert_master</a>(registry: &<a href="../sui/forwarding_address.md#sui_forwarding_address_ForwardingAddressRegistry">sui::forwarding_address::ForwardingAddressRegistry</a>, <a href="../sui/forwarding_address.md#sui_forwarding_address_master_id">master_id</a>: u64, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_assert_master">assert_master</a>(registry: &<a href="../sui/forwarding_address.md#sui_forwarding_address_ForwardingAddressRegistry">ForwardingAddressRegistry</a>, <a href="../sui/forwarding_address.md#sui_forwarding_address_master_id">master_id</a>: u64, ctx: &TxContext) {
    <b>let</b> record = <a href="../sui/dynamic_field.md#sui_dynamic_field_borrow">dynamic_field::borrow</a>&lt;u64, <a href="../sui/forwarding_address.md#sui_forwarding_address_MasterRecord">MasterRecord</a>&gt;(&registry.id, <a href="../sui/forwarding_address.md#sui_forwarding_address_master_id">master_id</a>);
    <b>assert</b>!(record.master == ctx.sender(), <a href="../sui/forwarding_address.md#sui_forwarding_address_ENotMaster">ENotMaster</a>);
}
</code></pre>



</details>

<a name="sui_forwarding_address_record_mut"></a>

## Function `record_mut`



<pre><code><b>fun</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_record_mut">record_mut</a>(registry: &<b>mut</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_ForwardingAddressRegistry">sui::forwarding_address::ForwardingAddressRegistry</a>, <a href="../sui/forwarding_address.md#sui_forwarding_address_master_id">master_id</a>: u64): &<b>mut</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_MasterRecord">sui::forwarding_address::MasterRecord</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_record_mut">record_mut</a>(registry: &<b>mut</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_ForwardingAddressRegistry">ForwardingAddressRegistry</a>, <a href="../sui/forwarding_address.md#sui_forwarding_address_master_id">master_id</a>: u64): &<b>mut</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_MasterRecord">MasterRecord</a> {
    <a href="../sui/dynamic_field.md#sui_dynamic_field_borrow_mut">dynamic_field::borrow_mut</a>&lt;u64, <a href="../sui/forwarding_address.md#sui_forwarding_address_MasterRecord">MasterRecord</a>&gt;(&<b>mut</b> registry.id, <a href="../sui/forwarding_address.md#sui_forwarding_address_master_id">master_id</a>)
}
</code></pre>



</details>

<a name="sui_forwarding_address_is_forwarding_address"></a>

## Function `is_forwarding_address`

Whether <code>addr</code> carries the forwarding magic, whatever its id or variant.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_is_forwarding_address">is_forwarding_address</a>(addr: <b>address</b>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_is_forwarding_address">is_forwarding_address</a>(addr: <b>address</b>): bool {
    <b>let</b> bytes = addr.to_bytes();
    <b>let</b> <b>mut</b> i = <a href="../sui/forwarding_address.md#sui_forwarding_address_MAGIC_START">MAGIC_START</a>;
    <b>while</b> (i &lt; <a href="../sui/forwarding_address.md#sui_forwarding_address_MAGIC_END">MAGIC_END</a>) {
        <b>if</b> (bytes[i] != <a href="../sui/forwarding_address.md#sui_forwarding_address_MAGIC_BYTE">MAGIC_BYTE</a>) <b>return</b> <b>false</b>;
        i = i + 1;
    };
    <b>true</b>
}
</code></pre>



</details>

<a name="sui_forwarding_address_charge_registration_fee"></a>

## Function `charge_registration_fee`



<pre><code><b>fun</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_charge_registration_fee">charge_registration_fee</a>()
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_charge_registration_fee">charge_registration_fee</a>();
</code></pre>



</details>

<a name="sui_forwarding_address_allocate_master_id"></a>

## Function `allocate_master_id`



<pre><code><b>fun</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_allocate_master_id">allocate_master_id</a>(registry: &<b>mut</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_ForwardingAddressRegistry">sui::forwarding_address::ForwardingAddressRegistry</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_allocate_master_id">allocate_master_id</a>(registry: &<b>mut</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_ForwardingAddressRegistry">ForwardingAddressRegistry</a>): u64 {
    <b>if</b> (!<a href="../sui/dynamic_field.md#sui_dynamic_field_exists">dynamic_field::exists</a>(&registry.id, <a href="../sui/forwarding_address.md#sui_forwarding_address_MasterIdCounter">MasterIdCounter</a> {})) {
        // Counter 0 is never allocated so that master ID 0 stays reserved.
        <a href="../sui/dynamic_field.md#sui_dynamic_field_add">dynamic_field::add</a>(&<b>mut</b> registry.id, <a href="../sui/forwarding_address.md#sui_forwarding_address_MasterIdCounter">MasterIdCounter</a> {}, 1u64);
    };
    <b>let</b> next = <a href="../sui/dynamic_field.md#sui_dynamic_field_borrow_mut">dynamic_field::borrow_mut</a>&lt;<a href="../sui/forwarding_address.md#sui_forwarding_address_MasterIdCounter">MasterIdCounter</a>, u64&gt;(
        &<b>mut</b> registry.id,
        <a href="../sui/forwarding_address.md#sui_forwarding_address_MasterIdCounter">MasterIdCounter</a> {},
    );
    <b>assert</b>!(*next &lt;= <a href="../sui/forwarding_address.md#sui_forwarding_address_MAX_MASTER_ID">MAX_MASTER_ID</a>, <a href="../sui/forwarding_address.md#sui_forwarding_address_EMasterIdsExhausted">EMasterIdsExhausted</a>);
    <b>let</b> counter = *next;
    *next = *next + 1;
    <a href="../sui/forwarding_address.md#sui_forwarding_address_mix_master_id">mix_master_id</a>(counter)
}
</code></pre>



</details>

<a name="sui_forwarding_address_mix_master_id"></a>

## Function `mix_master_id`

A permutation of the 48-bit id space built from xor-shifts and odd multiplications (the
lowbias32 construction widened to 48 bits), so distinct counters always give distinct IDs and
0 is the only preimage of 0. IDs look mixed but are not secret; the counter is public and the
function is invertible. A shift of 24 on a 48-bit value is its own inverse.


<pre><code><b>fun</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_mix_master_id">mix_master_id</a>(x: u64): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_mix_master_id">mix_master_id</a>(x: u64): u64 {
    <b>let</b> x = x ^ (x &gt;&gt; 24);
    <b>let</b> x = <a href="../sui/forwarding_address.md#sui_forwarding_address_mul_mod_2_48">mul_mod_2_48</a>(x, 0x9e3779b97f4b);
    <b>let</b> x = x ^ (x &gt;&gt; 24);
    <b>let</b> x = <a href="../sui/forwarding_address.md#sui_forwarding_address_mul_mod_2_48">mul_mod_2_48</a>(x, 0x5851f42d4c95);
    x ^ (x &gt;&gt; 24)
}
</code></pre>



</details>

<a name="sui_forwarding_address_mul_mod_2_48"></a>

## Function `mul_mod_2_48`



<pre><code><b>fun</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_mul_mod_2_48">mul_mod_2_48</a>(a: u64, b: u64): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_mul_mod_2_48">mul_mod_2_48</a>(a: u64, b: u64): u64 {
    (((a <b>as</b> u128) * (b <b>as</b> u128)) & (<a href="../sui/forwarding_address.md#sui_forwarding_address_MAX_MASTER_ID">MAX_MASTER_ID</a> <b>as</b> u128)) <b>as</b> u64
}
</code></pre>



</details>

<a name="sui_forwarding_address_create"></a>

## Function `create`

Create and share the singleton registry at genesis or protocol upgrade.


<pre><code><b>fun</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_create">create</a>(ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/forwarding_address.md#sui_forwarding_address_create">create</a>(ctx: &TxContext) {
    <b>assert</b>!(ctx.sender() == @0x0, <a href="../sui/forwarding_address.md#sui_forwarding_address_ENotSystemAddress">ENotSystemAddress</a>);
    <a href="../sui/transfer.md#sui_transfer_share_object">transfer::share_object</a>(<a href="../sui/forwarding_address.md#sui_forwarding_address_ForwardingAddressRegistry">ForwardingAddressRegistry</a> {
        id: <a href="../sui/object.md#sui_object_forwarding_address_registry">object::forwarding_address_registry</a>(),
    });
}
</code></pre>



</details>
