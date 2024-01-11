
<a name="0x2_object"></a>

# Module `0x2::object`



-  [Struct `ID`](#0x2_object_ID)
-  [Struct `UID`](#0x2_object_UID)
-  [Struct `Ownership`](#0x2_object_Ownership)
-  [Struct `DynamicFields`](#0x2_object_DynamicFields)
-  [Constants](#@Constants_0)
-  [Function `id_to_bytes`](#0x2_object_id_to_bytes)
-  [Function `id_to_address`](#0x2_object_id_to_address)
-  [Function `id_from_bytes`](#0x2_object_id_from_bytes)
-  [Function `id_from_address`](#0x2_object_id_from_address)
-  [Function `sui_system_state`](#0x2_object_sui_system_state)
-  [Function `clock`](#0x2_object_clock)
-  [Function `authenticator_state`](#0x2_object_authenticator_state)
-  [Function `randomness_state`](#0x2_object_randomness_state)
-  [Function `sui_deny_list_object_id`](#0x2_object_sui_deny_list_object_id)
-  [Function `uid_as_inner`](#0x2_object_uid_as_inner)
-  [Function `uid_to_inner`](#0x2_object_uid_to_inner)
-  [Function `uid_to_bytes`](#0x2_object_uid_to_bytes)
-  [Function `uid_to_address`](#0x2_object_uid_to_address)
-  [Function `new`](#0x2_object_new)
-  [Function `delete`](#0x2_object_delete)
-  [Function `id`](#0x2_object_id)
-  [Function `borrow_id`](#0x2_object_borrow_id)
-  [Function `id_bytes`](#0x2_object_id_bytes)
-  [Function `id_address`](#0x2_object_id_address)
-  [Function `borrow_uid`](#0x2_object_borrow_uid)
-  [Function `new_uid_from_hash`](#0x2_object_new_uid_from_hash)
-  [Function `delete_impl`](#0x2_object_delete_impl)
-  [Function `record_new_uid`](#0x2_object_record_new_uid)


<pre><code><b>use</b> <a href="../../dependencies/move-stdlib/bcs.md#0x1_bcs">0x1::bcs</a>;
<b>use</b> <a href="../../dependencies/sui-framework/address.md#0x2_address">0x2::address</a>;
<b>use</b> <a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context">0x2::tx_context</a>;
</code></pre>



<a name="0x2_object_ID"></a>

## Struct `ID`



<pre><code><b>struct</b> <a href="../../dependencies/sui-framework/object.md#0x2_object_ID">ID</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>bytes: <b>address</b></code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x2_object_UID"></a>

## Struct `UID`



<pre><code><b>struct</b> <a href="../../dependencies/sui-framework/object.md#0x2_object_UID">UID</a> <b>has</b> store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>id: <a href="../../dependencies/sui-framework/object.md#0x2_object_ID">object::ID</a></code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x2_object_Ownership"></a>

## Struct `Ownership`



<pre><code><b>struct</b> <a href="../../dependencies/sui-framework/object.md#0x2_object_Ownership">Ownership</a>
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>owner: <b>address</b></code>
</dt>
<dd>

</dd>
<dt>
<code>status: u64</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x2_object_DynamicFields"></a>

## Struct `DynamicFields`



<pre><code><b>struct</b> <a href="../../dependencies/sui-framework/object.md#0x2_object_DynamicFields">DynamicFields</a>&lt;K: <b>copy</b>, drop, store&gt;
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>names: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;K&gt;</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0x2_object_ENotSystemAddress"></a>



<pre><code><b>const</b> <a href="../../dependencies/sui-framework/object.md#0x2_object_ENotSystemAddress">ENotSystemAddress</a>: u64 = 0;
</code></pre>



<a name="0x2_object_SUI_AUTHENTICATOR_STATE_ID"></a>



<pre><code><b>const</b> <a href="../../dependencies/sui-framework/object.md#0x2_object_SUI_AUTHENTICATOR_STATE_ID">SUI_AUTHENTICATOR_STATE_ID</a>: <b>address</b> = 7;
</code></pre>



<a name="0x2_object_SUI_CLOCK_OBJECT_ID"></a>



<pre><code><b>const</b> <a href="../../dependencies/sui-framework/object.md#0x2_object_SUI_CLOCK_OBJECT_ID">SUI_CLOCK_OBJECT_ID</a>: <b>address</b> = 6;
</code></pre>



<a name="0x2_object_SUI_DENY_LIST_OBJECT_ID"></a>



<pre><code><b>const</b> <a href="../../dependencies/sui-framework/object.md#0x2_object_SUI_DENY_LIST_OBJECT_ID">SUI_DENY_LIST_OBJECT_ID</a>: <b>address</b> = 403;
</code></pre>



<a name="0x2_object_SUI_RANDOM_ID"></a>



<pre><code><b>const</b> <a href="../../dependencies/sui-framework/object.md#0x2_object_SUI_RANDOM_ID">SUI_RANDOM_ID</a>: <b>address</b> = 8;
</code></pre>



<a name="0x2_object_SUI_SYSTEM_STATE_OBJECT_ID"></a>



<pre><code><b>const</b> <a href="../../dependencies/sui-framework/object.md#0x2_object_SUI_SYSTEM_STATE_OBJECT_ID">SUI_SYSTEM_STATE_OBJECT_ID</a>: <b>address</b> = 5;
</code></pre>



<a name="0x2_object_id_to_bytes"></a>

## Function `id_to_bytes`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/object.md#0x2_object_id_to_bytes">id_to_bytes</a>(id: &<a href="../../dependencies/sui-framework/object.md#0x2_object_ID">object::ID</a>): <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/object.md#0x2_object_id_to_bytes">id_to_bytes</a>(id: &<a href="../../dependencies/sui-framework/object.md#0x2_object_ID">ID</a>): <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt; {
    <a href="../../dependencies/move-stdlib/bcs.md#0x1_bcs_to_bytes">bcs::to_bytes</a>(&id.bytes)
}
</code></pre>



</details>

<a name="0x2_object_id_to_address"></a>

## Function `id_to_address`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/object.md#0x2_object_id_to_address">id_to_address</a>(id: &<a href="../../dependencies/sui-framework/object.md#0x2_object_ID">object::ID</a>): <b>address</b>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/object.md#0x2_object_id_to_address">id_to_address</a>(id: &<a href="../../dependencies/sui-framework/object.md#0x2_object_ID">ID</a>): <b>address</b> {
    id.bytes
}
</code></pre>



</details>

<a name="0x2_object_id_from_bytes"></a>

## Function `id_from_bytes`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/object.md#0x2_object_id_from_bytes">id_from_bytes</a>(bytes: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;): <a href="../../dependencies/sui-framework/object.md#0x2_object_ID">object::ID</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/object.md#0x2_object_id_from_bytes">id_from_bytes</a>(bytes: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;): <a href="../../dependencies/sui-framework/object.md#0x2_object_ID">ID</a> {
    <a href="../../dependencies/sui-framework/object.md#0x2_object_id_from_address">id_from_address</a>(address::from_bytes(bytes))
}
</code></pre>



</details>

<a name="0x2_object_id_from_address"></a>

## Function `id_from_address`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/object.md#0x2_object_id_from_address">id_from_address</a>(bytes: <b>address</b>): <a href="../../dependencies/sui-framework/object.md#0x2_object_ID">object::ID</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/object.md#0x2_object_id_from_address">id_from_address</a>(bytes: <b>address</b>): <a href="../../dependencies/sui-framework/object.md#0x2_object_ID">ID</a> {
    <a href="../../dependencies/sui-framework/object.md#0x2_object_ID">ID</a> { bytes }
}
</code></pre>



</details>

<a name="0x2_object_sui_system_state"></a>

## Function `sui_system_state`



<pre><code><b>fun</b> <a href="../../dependencies/sui-framework/object.md#0x2_object_sui_system_state">sui_system_state</a>(ctx: &<a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="../../dependencies/sui-framework/object.md#0x2_object_UID">object::UID</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../../dependencies/sui-framework/object.md#0x2_object_sui_system_state">sui_system_state</a>(ctx: &TxContext): <a href="../../dependencies/sui-framework/object.md#0x2_object_UID">UID</a> {
    <b>assert</b>!(<a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_sender">tx_context::sender</a>(ctx) == @0x0, <a href="../../dependencies/sui-framework/object.md#0x2_object_ENotSystemAddress">ENotSystemAddress</a>);
    <a href="../../dependencies/sui-framework/object.md#0x2_object_UID">UID</a> {
        id: <a href="../../dependencies/sui-framework/object.md#0x2_object_ID">ID</a> { bytes: <a href="../../dependencies/sui-framework/object.md#0x2_object_SUI_SYSTEM_STATE_OBJECT_ID">SUI_SYSTEM_STATE_OBJECT_ID</a> },
    }
}
</code></pre>



</details>

<a name="0x2_object_clock"></a>

## Function `clock`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="../../dependencies/sui-framework/clock.md#0x2_clock">clock</a>(): <a href="../../dependencies/sui-framework/object.md#0x2_object_UID">object::UID</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="../../dependencies/sui-framework/clock.md#0x2_clock">clock</a>(): <a href="../../dependencies/sui-framework/object.md#0x2_object_UID">UID</a> {
    <a href="../../dependencies/sui-framework/object.md#0x2_object_UID">UID</a> {
        id: <a href="../../dependencies/sui-framework/object.md#0x2_object_ID">ID</a> { bytes: <a href="../../dependencies/sui-framework/object.md#0x2_object_SUI_CLOCK_OBJECT_ID">SUI_CLOCK_OBJECT_ID</a> }
    }
}
</code></pre>



</details>

<a name="0x2_object_authenticator_state"></a>

## Function `authenticator_state`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="../../dependencies/sui-framework/authenticator_state.md#0x2_authenticator_state">authenticator_state</a>(): <a href="../../dependencies/sui-framework/object.md#0x2_object_UID">object::UID</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="../../dependencies/sui-framework/authenticator_state.md#0x2_authenticator_state">authenticator_state</a>(): <a href="../../dependencies/sui-framework/object.md#0x2_object_UID">UID</a> {
    <a href="../../dependencies/sui-framework/object.md#0x2_object_UID">UID</a> {
        id: <a href="../../dependencies/sui-framework/object.md#0x2_object_ID">ID</a> { bytes: <a href="../../dependencies/sui-framework/object.md#0x2_object_SUI_AUTHENTICATOR_STATE_ID">SUI_AUTHENTICATOR_STATE_ID</a> }
    }
}
</code></pre>



</details>

<a name="0x2_object_randomness_state"></a>

## Function `randomness_state`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="../../dependencies/sui-framework/object.md#0x2_object_randomness_state">randomness_state</a>(): <a href="../../dependencies/sui-framework/object.md#0x2_object_UID">object::UID</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="../../dependencies/sui-framework/object.md#0x2_object_randomness_state">randomness_state</a>(): <a href="../../dependencies/sui-framework/object.md#0x2_object_UID">UID</a> {
    <a href="../../dependencies/sui-framework/object.md#0x2_object_UID">UID</a> {
        id: <a href="../../dependencies/sui-framework/object.md#0x2_object_ID">ID</a> { bytes: <a href="../../dependencies/sui-framework/object.md#0x2_object_SUI_RANDOM_ID">SUI_RANDOM_ID</a> }
    }
}
</code></pre>



</details>

<a name="0x2_object_sui_deny_list_object_id"></a>

## Function `sui_deny_list_object_id`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="../../dependencies/sui-framework/object.md#0x2_object_sui_deny_list_object_id">sui_deny_list_object_id</a>(): <a href="../../dependencies/sui-framework/object.md#0x2_object_UID">object::UID</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="../../dependencies/sui-framework/object.md#0x2_object_sui_deny_list_object_id">sui_deny_list_object_id</a>(): <a href="../../dependencies/sui-framework/object.md#0x2_object_UID">UID</a> {
    <a href="../../dependencies/sui-framework/object.md#0x2_object_UID">UID</a> {
        id: <a href="../../dependencies/sui-framework/object.md#0x2_object_ID">ID</a> { bytes: <a href="../../dependencies/sui-framework/object.md#0x2_object_SUI_DENY_LIST_OBJECT_ID">SUI_DENY_LIST_OBJECT_ID</a> }
    }
}
</code></pre>



</details>

<a name="0x2_object_uid_as_inner"></a>

## Function `uid_as_inner`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/object.md#0x2_object_uid_as_inner">uid_as_inner</a>(uid: &<a href="../../dependencies/sui-framework/object.md#0x2_object_UID">object::UID</a>): &<a href="../../dependencies/sui-framework/object.md#0x2_object_ID">object::ID</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/object.md#0x2_object_uid_as_inner">uid_as_inner</a>(uid: &<a href="../../dependencies/sui-framework/object.md#0x2_object_UID">UID</a>): &<a href="../../dependencies/sui-framework/object.md#0x2_object_ID">ID</a> {
    &uid.id
}
</code></pre>



</details>

<a name="0x2_object_uid_to_inner"></a>

## Function `uid_to_inner`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/object.md#0x2_object_uid_to_inner">uid_to_inner</a>(uid: &<a href="../../dependencies/sui-framework/object.md#0x2_object_UID">object::UID</a>): <a href="../../dependencies/sui-framework/object.md#0x2_object_ID">object::ID</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/object.md#0x2_object_uid_to_inner">uid_to_inner</a>(uid: &<a href="../../dependencies/sui-framework/object.md#0x2_object_UID">UID</a>): <a href="../../dependencies/sui-framework/object.md#0x2_object_ID">ID</a> {
    uid.id
}
</code></pre>



</details>

<a name="0x2_object_uid_to_bytes"></a>

## Function `uid_to_bytes`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/object.md#0x2_object_uid_to_bytes">uid_to_bytes</a>(uid: &<a href="../../dependencies/sui-framework/object.md#0x2_object_UID">object::UID</a>): <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/object.md#0x2_object_uid_to_bytes">uid_to_bytes</a>(uid: &<a href="../../dependencies/sui-framework/object.md#0x2_object_UID">UID</a>): <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt; {
    <a href="../../dependencies/move-stdlib/bcs.md#0x1_bcs_to_bytes">bcs::to_bytes</a>(&uid.id.bytes)
}
</code></pre>



</details>

<a name="0x2_object_uid_to_address"></a>

## Function `uid_to_address`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/object.md#0x2_object_uid_to_address">uid_to_address</a>(uid: &<a href="../../dependencies/sui-framework/object.md#0x2_object_UID">object::UID</a>): <b>address</b>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/object.md#0x2_object_uid_to_address">uid_to_address</a>(uid: &<a href="../../dependencies/sui-framework/object.md#0x2_object_UID">UID</a>): <b>address</b> {
    uid.id.bytes
}
</code></pre>



</details>

<a name="0x2_object_new"></a>

## Function `new`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/object.md#0x2_object_new">new</a>(ctx: &<b>mut</b> <a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="../../dependencies/sui-framework/object.md#0x2_object_UID">object::UID</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/object.md#0x2_object_new">new</a>(ctx: &<b>mut</b> TxContext): <a href="../../dependencies/sui-framework/object.md#0x2_object_UID">UID</a> {
    <a href="../../dependencies/sui-framework/object.md#0x2_object_UID">UID</a> {
        id: <a href="../../dependencies/sui-framework/object.md#0x2_object_ID">ID</a> { bytes: <a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_fresh_object_address">tx_context::fresh_object_address</a>(ctx) },
    }
}
</code></pre>



</details>

<a name="0x2_object_delete"></a>

## Function `delete`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/object.md#0x2_object_delete">delete</a>(id: <a href="../../dependencies/sui-framework/object.md#0x2_object_UID">object::UID</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/object.md#0x2_object_delete">delete</a>(id: <a href="../../dependencies/sui-framework/object.md#0x2_object_UID">UID</a>) {
    <b>let</b> <a href="../../dependencies/sui-framework/object.md#0x2_object_UID">UID</a> { id: <a href="../../dependencies/sui-framework/object.md#0x2_object_ID">ID</a> { bytes } } = id;
    <a href="../../dependencies/sui-framework/object.md#0x2_object_delete_impl">delete_impl</a>(bytes)
}
</code></pre>



</details>

<a name="0x2_object_id"></a>

## Function `id`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/object.md#0x2_object_id">id</a>&lt;T: key&gt;(obj: &T): <a href="../../dependencies/sui-framework/object.md#0x2_object_ID">object::ID</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/object.md#0x2_object_id">id</a>&lt;T: key&gt;(obj: &T): <a href="../../dependencies/sui-framework/object.md#0x2_object_ID">ID</a> {
    <a href="../../dependencies/sui-framework/object.md#0x2_object_borrow_uid">borrow_uid</a>(obj).id
}
</code></pre>



</details>

<a name="0x2_object_borrow_id"></a>

## Function `borrow_id`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/object.md#0x2_object_borrow_id">borrow_id</a>&lt;T: key&gt;(obj: &T): &<a href="../../dependencies/sui-framework/object.md#0x2_object_ID">object::ID</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/object.md#0x2_object_borrow_id">borrow_id</a>&lt;T: key&gt;(obj: &T): &<a href="../../dependencies/sui-framework/object.md#0x2_object_ID">ID</a> {
    &<a href="../../dependencies/sui-framework/object.md#0x2_object_borrow_uid">borrow_uid</a>(obj).id
}
</code></pre>



</details>

<a name="0x2_object_id_bytes"></a>

## Function `id_bytes`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/object.md#0x2_object_id_bytes">id_bytes</a>&lt;T: key&gt;(obj: &T): <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/object.md#0x2_object_id_bytes">id_bytes</a>&lt;T: key&gt;(obj: &T): <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt; {
    <a href="../../dependencies/move-stdlib/bcs.md#0x1_bcs_to_bytes">bcs::to_bytes</a>(&<a href="../../dependencies/sui-framework/object.md#0x2_object_borrow_uid">borrow_uid</a>(obj).id)
}
</code></pre>



</details>

<a name="0x2_object_id_address"></a>

## Function `id_address`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/object.md#0x2_object_id_address">id_address</a>&lt;T: key&gt;(obj: &T): <b>address</b>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/object.md#0x2_object_id_address">id_address</a>&lt;T: key&gt;(obj: &T): <b>address</b> {
    <a href="../../dependencies/sui-framework/object.md#0x2_object_borrow_uid">borrow_uid</a>(obj).id.bytes
}
</code></pre>



</details>

<a name="0x2_object_borrow_uid"></a>

## Function `borrow_uid`



<pre><code><b>fun</b> <a href="../../dependencies/sui-framework/object.md#0x2_object_borrow_uid">borrow_uid</a>&lt;T: key&gt;(obj: &T): &<a href="../../dependencies/sui-framework/object.md#0x2_object_UID">object::UID</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="../../dependencies/sui-framework/object.md#0x2_object_borrow_uid">borrow_uid</a>&lt;T: key&gt;(obj: &T): &<a href="../../dependencies/sui-framework/object.md#0x2_object_UID">UID</a>;
</code></pre>



</details>

<a name="0x2_object_new_uid_from_hash"></a>

## Function `new_uid_from_hash`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="../../dependencies/sui-framework/object.md#0x2_object_new_uid_from_hash">new_uid_from_hash</a>(bytes: <b>address</b>): <a href="../../dependencies/sui-framework/object.md#0x2_object_UID">object::UID</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="../../dependencies/sui-framework/object.md#0x2_object_new_uid_from_hash">new_uid_from_hash</a>(bytes: <b>address</b>): <a href="../../dependencies/sui-framework/object.md#0x2_object_UID">UID</a> {
    <a href="../../dependencies/sui-framework/object.md#0x2_object_record_new_uid">record_new_uid</a>(bytes);
    <a href="../../dependencies/sui-framework/object.md#0x2_object_UID">UID</a> { id: <a href="../../dependencies/sui-framework/object.md#0x2_object_ID">ID</a> { bytes } }
}
</code></pre>



</details>

<a name="0x2_object_delete_impl"></a>

## Function `delete_impl`



<pre><code><b>fun</b> <a href="../../dependencies/sui-framework/object.md#0x2_object_delete_impl">delete_impl</a>(id: <b>address</b>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="../../dependencies/sui-framework/object.md#0x2_object_delete_impl">delete_impl</a>(id: <b>address</b>);
</code></pre>



</details>

<a name="0x2_object_record_new_uid"></a>

## Function `record_new_uid`



<pre><code><b>fun</b> <a href="../../dependencies/sui-framework/object.md#0x2_object_record_new_uid">record_new_uid</a>(id: <b>address</b>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="../../dependencies/sui-framework/object.md#0x2_object_record_new_uid">record_new_uid</a>(id: <b>address</b>);
</code></pre>



</details>
