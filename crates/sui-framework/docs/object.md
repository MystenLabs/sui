
<a name="0x2_object"></a>

# Module `0x2::object`

Sui object identifiers


-  [Struct `ID`](#0x2_object_ID)
-  [Struct `UID`](#0x2_object_UID)
-  [Constants](#@Constants_0)
-  [Function `address_from_bytes`](#0x2_object_address_from_bytes)
-  [Function `id_to_bytes`](#0x2_object_id_to_bytes)
-  [Function `id_to_address`](#0x2_object_id_to_address)
-  [Function `sui_system_state`](#0x2_object_sui_system_state)
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


<pre><code><b>use</b> <a href="">0x1::bcs</a>;
<b>use</b> <a href="tx_context.md#0x2_tx_context">0x2::tx_context</a>;
</code></pre>



<a name="0x2_object_ID"></a>

## Struct `ID`

An object ID. This is used to reference Sui Objects.
This is *not* guaranteed to be globally unique--anyone can create an <code><a href="object.md#0x2_object_ID">ID</a></code> from a <code><a href="object.md#0x2_object_UID">UID</a></code> or
from an object, and ID's can be freely copied and dropped.
Here, the values are not globally unique because there can be multiple values of type <code><a href="object.md#0x2_object_ID">ID</a></code>
with the same underlying bytes. For example, <code><a href="object.md#0x2_object_id">object::id</a>(&obj)</code> can be called as many times
as you want for a given <code>obj</code>, and each <code><a href="object.md#0x2_object_ID">ID</a></code> value will be identical.


<pre><code><b>struct</b> <a href="object.md#0x2_object_ID">ID</a> <b>has</b> <b>copy</b>, drop, store
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

Globally unique IDs that define an object's ID in storage. Any Sui Object, that is a struct
with the <code>key</code> ability, must have <code>id: <a href="object.md#0x2_object_UID">UID</a></code> as its first field.
These are globaly unique in the sense that no two values of type <code><a href="object.md#0x2_object_UID">UID</a></code> are ever equal, in
other words for any two values <code>id1: <a href="object.md#0x2_object_UID">UID</a></code> and <code>id2: <a href="object.md#0x2_object_UID">UID</a></code>, <code>id1</code> != <code>id2</code>.
This is a privileged type that can only be derived from a <code>TxContext</code>.
<code><a href="object.md#0x2_object_UID">UID</a></code> doesn't have the <code>drop</code> ability, so deleting a <code><a href="object.md#0x2_object_UID">UID</a></code> requires a call to <code>delete</code>.


<pre><code><b>struct</b> <a href="object.md#0x2_object_UID">UID</a> <b>has</b> store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>id: <a href="object.md#0x2_object_ID">object::ID</a></code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0x2_object_EAddressParseError"></a>

Error from <code>address_from_bytes</code> when it is supplied too many or too few bytes.


<pre><code><b>const</b> <a href="object.md#0x2_object_EAddressParseError">EAddressParseError</a>: u64 = 0;
</code></pre>



<a name="0x2_object_SUI_SYSTEM_STATE_OBJECT_ID"></a>

The hardcoded ID for the singleton Sui System State Object.


<pre><code><b>const</b> <a href="object.md#0x2_object_SUI_SYSTEM_STATE_OBJECT_ID">SUI_SYSTEM_STATE_OBJECT_ID</a>: <b>address</b> = 5;
</code></pre>



<a name="0x2_object_address_from_bytes"></a>

## Function `address_from_bytes`

Convert raw bytes into an address, aborts if supplied too many
or too few bytes.


<pre><code><b>public</b> <b>fun</b> <a href="object.md#0x2_object_address_from_bytes">address_from_bytes</a>(bytes: <a href="">vector</a>&lt;u8&gt;): <b>address</b>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>native</b> <b>fun</b> <a href="object.md#0x2_object_address_from_bytes">address_from_bytes</a>(bytes: <a href="">vector</a>&lt;u8&gt;): <b>address</b>;
</code></pre>



</details>

<a name="0x2_object_id_to_bytes"></a>

## Function `id_to_bytes`

Get the raw bytes of a <code><a href="object.md#0x2_object_ID">ID</a></code>


<pre><code><b>public</b> <b>fun</b> <a href="object.md#0x2_object_id_to_bytes">id_to_bytes</a>(id: &<a href="object.md#0x2_object_ID">object::ID</a>): <a href="">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="object.md#0x2_object_id_to_bytes">id_to_bytes</a>(id: &<a href="object.md#0x2_object_ID">ID</a>): <a href="">vector</a>&lt;u8&gt; {
    <a href="_to_bytes">bcs::to_bytes</a>(&id.bytes)
}
</code></pre>



</details>

<a name="0x2_object_id_to_address"></a>

## Function `id_to_address`

Get the inner bytes of <code>id</code> as an address.


<pre><code><b>public</b> <b>fun</b> <a href="object.md#0x2_object_id_to_address">id_to_address</a>(id: &<a href="object.md#0x2_object_ID">object::ID</a>): <b>address</b>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="object.md#0x2_object_id_to_address">id_to_address</a>(id: &<a href="object.md#0x2_object_ID">ID</a>): <b>address</b> {
    id.bytes
}
</code></pre>



</details>

<a name="0x2_object_sui_system_state"></a>

## Function `sui_system_state`

Create the <code><a href="object.md#0x2_object_UID">UID</a></code> for the singleton <code>SuiSystemState</code> object.
This should only be called once from <code><a href="sui_system.md#0x2_sui_system">sui_system</a></code>.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="object.md#0x2_object_sui_system_state">sui_system_state</a>(): <a href="object.md#0x2_object_UID">object::UID</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="object.md#0x2_object_sui_system_state">sui_system_state</a>(): <a href="object.md#0x2_object_UID">UID</a> {
    <a href="object.md#0x2_object_UID">UID</a> {
        id: <a href="object.md#0x2_object_ID">ID</a> { bytes: <a href="object.md#0x2_object_SUI_SYSTEM_STATE_OBJECT_ID">SUI_SYSTEM_STATE_OBJECT_ID</a> },
    }
}
</code></pre>



</details>

<a name="0x2_object_uid_as_inner"></a>

## Function `uid_as_inner`

Get the inner <code><a href="object.md#0x2_object_ID">ID</a></code> of <code>uid</code>


<pre><code><b>public</b> <b>fun</b> <a href="object.md#0x2_object_uid_as_inner">uid_as_inner</a>(uid: &<a href="object.md#0x2_object_UID">object::UID</a>): &<a href="object.md#0x2_object_ID">object::ID</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="object.md#0x2_object_uid_as_inner">uid_as_inner</a>(uid: &<a href="object.md#0x2_object_UID">UID</a>): &<a href="object.md#0x2_object_ID">ID</a> {
    &uid.id
}
</code></pre>



</details>

<a name="0x2_object_uid_to_inner"></a>

## Function `uid_to_inner`

Get the raw bytes of a <code>uid</code>'s inner <code><a href="object.md#0x2_object_ID">ID</a></code>


<pre><code><b>public</b> <b>fun</b> <a href="object.md#0x2_object_uid_to_inner">uid_to_inner</a>(uid: &<a href="object.md#0x2_object_UID">object::UID</a>): <a href="object.md#0x2_object_ID">object::ID</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="object.md#0x2_object_uid_to_inner">uid_to_inner</a>(uid: &<a href="object.md#0x2_object_UID">UID</a>): <a href="object.md#0x2_object_ID">ID</a> {
    uid.id
}
</code></pre>



</details>

<a name="0x2_object_uid_to_bytes"></a>

## Function `uid_to_bytes`

Get the raw bytes of a <code><a href="object.md#0x2_object_UID">UID</a></code>


<pre><code><b>public</b> <b>fun</b> <a href="object.md#0x2_object_uid_to_bytes">uid_to_bytes</a>(uid: &<a href="object.md#0x2_object_UID">object::UID</a>): <a href="">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="object.md#0x2_object_uid_to_bytes">uid_to_bytes</a>(uid: &<a href="object.md#0x2_object_UID">UID</a>): <a href="">vector</a>&lt;u8&gt; {
    <a href="_to_bytes">bcs::to_bytes</a>(&uid.id.bytes)
}
</code></pre>



</details>

<a name="0x2_object_uid_to_address"></a>

## Function `uid_to_address`

Get the inner bytes of <code>id</code> as an address.


<pre><code><b>public</b> <b>fun</b> <a href="object.md#0x2_object_uid_to_address">uid_to_address</a>(uid: &<a href="object.md#0x2_object_UID">object::UID</a>): <b>address</b>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="object.md#0x2_object_uid_to_address">uid_to_address</a>(uid: &<a href="object.md#0x2_object_UID">UID</a>): <b>address</b> {
    uid.id.bytes
}
</code></pre>



</details>

<a name="0x2_object_new"></a>

## Function `new`

Create a new object. Returns the <code><a href="object.md#0x2_object_UID">UID</a></code> that must be stored in a Sui object.
This is the only way to create <code><a href="object.md#0x2_object_UID">UID</a></code>s.


<pre><code><b>public</b> <b>fun</b> <a href="object.md#0x2_object_new">new</a>(ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="object.md#0x2_object_UID">object::UID</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="object.md#0x2_object_new">new</a>(ctx: &<b>mut</b> TxContext): <a href="object.md#0x2_object_UID">UID</a> {
    <a href="object.md#0x2_object_UID">UID</a> {
        id: <a href="object.md#0x2_object_ID">ID</a> { bytes: <a href="tx_context.md#0x2_tx_context_new_object">tx_context::new_object</a>(ctx) },
    }
}
</code></pre>



</details>

<a name="0x2_object_delete"></a>

## Function `delete`

Delete the object and it's <code><a href="object.md#0x2_object_UID">UID</a></code>. This is the only way to eliminate a <code><a href="object.md#0x2_object_UID">UID</a></code>.


<pre><code><b>public</b> <b>fun</b> <a href="object.md#0x2_object_delete">delete</a>(id: <a href="object.md#0x2_object_UID">object::UID</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="object.md#0x2_object_delete">delete</a>(id: <a href="object.md#0x2_object_UID">UID</a>) {
    <b>let</b> <a href="object.md#0x2_object_UID">UID</a> { id: <a href="object.md#0x2_object_ID">ID</a> { bytes } } = id;
    <a href="object.md#0x2_object_delete_impl">delete_impl</a>(bytes)
}
</code></pre>



</details>

<a name="0x2_object_id"></a>

## Function `id`

Get the underlying <code><a href="object.md#0x2_object_ID">ID</a></code> of <code>obj</code>


<pre><code><b>public</b> <b>fun</b> <a href="object.md#0x2_object_id">id</a>&lt;T: key&gt;(obj: &T): <a href="object.md#0x2_object_ID">object::ID</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="object.md#0x2_object_id">id</a>&lt;T: key&gt;(obj: &T): <a href="object.md#0x2_object_ID">ID</a> {
    <a href="object.md#0x2_object_borrow_uid">borrow_uid</a>(obj).id
}
</code></pre>



</details>

<a name="0x2_object_borrow_id"></a>

## Function `borrow_id`

Borrow the underlying <code><a href="object.md#0x2_object_ID">ID</a></code> of <code>obj</code>


<pre><code><b>public</b> <b>fun</b> <a href="object.md#0x2_object_borrow_id">borrow_id</a>&lt;T: key&gt;(obj: &T): &<a href="object.md#0x2_object_ID">object::ID</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="object.md#0x2_object_borrow_id">borrow_id</a>&lt;T: key&gt;(obj: &T): &<a href="object.md#0x2_object_ID">ID</a> {
    &<a href="object.md#0x2_object_borrow_uid">borrow_uid</a>(obj).id
}
</code></pre>



</details>

<a name="0x2_object_id_bytes"></a>

## Function `id_bytes`

Get the raw bytes for the underlying <code><a href="object.md#0x2_object_ID">ID</a></code> of <code>obj</code>


<pre><code><b>public</b> <b>fun</b> <a href="object.md#0x2_object_id_bytes">id_bytes</a>&lt;T: key&gt;(obj: &T): <a href="">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="object.md#0x2_object_id_bytes">id_bytes</a>&lt;T: key&gt;(obj: &T): <a href="">vector</a>&lt;u8&gt; {
    <a href="_to_bytes">bcs::to_bytes</a>(&<a href="object.md#0x2_object_borrow_uid">borrow_uid</a>(obj).id)
}
</code></pre>



</details>

<a name="0x2_object_id_address"></a>

## Function `id_address`

Get the inner bytes for the underlying <code><a href="object.md#0x2_object_ID">ID</a></code> of <code>obj</code>


<pre><code><b>public</b> <b>fun</b> <a href="object.md#0x2_object_id_address">id_address</a>&lt;T: key&gt;(obj: &T): <b>address</b>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="object.md#0x2_object_id_address">id_address</a>&lt;T: key&gt;(obj: &T): <b>address</b> {
    <a href="object.md#0x2_object_borrow_uid">borrow_uid</a>(obj).id.bytes
}
</code></pre>



</details>

<a name="0x2_object_borrow_uid"></a>

## Function `borrow_uid`

Get the <code><a href="object.md#0x2_object_UID">UID</a></code> for <code>obj</code>.
Safe because Sui has an extra bytecode verifier pass that forces every struct with
the <code>key</code> ability to have a distinguished <code><a href="object.md#0x2_object_UID">UID</a></code> field.
Cannot be made public as the access to <code><a href="object.md#0x2_object_UID">UID</a></code> for a given object must be privledged, and
restrictable in the object's module.


<pre><code><b>fun</b> <a href="object.md#0x2_object_borrow_uid">borrow_uid</a>&lt;T: key&gt;(obj: &T): &<a href="object.md#0x2_object_UID">object::UID</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="object.md#0x2_object_borrow_uid">borrow_uid</a>&lt;T: key&gt;(obj: &T): &<a href="object.md#0x2_object_UID">UID</a>;
</code></pre>



</details>

<a name="0x2_object_new_uid_from_hash"></a>

## Function `new_uid_from_hash`

Generate a new UID specifically used for creating a UID from a hash


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="object.md#0x2_object_new_uid_from_hash">new_uid_from_hash</a>(bytes: <b>address</b>): <a href="object.md#0x2_object_UID">object::UID</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="object.md#0x2_object_new_uid_from_hash">new_uid_from_hash</a>(bytes: <b>address</b>): <a href="object.md#0x2_object_UID">UID</a> {
    <a href="object.md#0x2_object_record_new_uid">record_new_uid</a>(bytes);
    <a href="object.md#0x2_object_UID">UID</a> { id: <a href="object.md#0x2_object_ID">ID</a> { bytes } }
}
</code></pre>



</details>

<a name="0x2_object_delete_impl"></a>

## Function `delete_impl`



<pre><code><b>fun</b> <a href="object.md#0x2_object_delete_impl">delete_impl</a>(id: <b>address</b>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="object.md#0x2_object_delete_impl">delete_impl</a>(id: <b>address</b>);
</code></pre>



</details>

<a name="0x2_object_record_new_uid"></a>

## Function `record_new_uid`



<pre><code><b>fun</b> <a href="object.md#0x2_object_record_new_uid">record_new_uid</a>(id: <b>address</b>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="object.md#0x2_object_record_new_uid">record_new_uid</a>(id: <b>address</b>);
</code></pre>



</details>
