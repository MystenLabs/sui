---
title: Module `sui::object`
---

Sui object identifiers


-  [Struct `ID`](#sui_object_ID)
-  [Struct `UID`](#sui_object_UID)
-  [Constants](#@Constants_0)
-  [Function `id_to_bytes`](#sui_object_id_to_bytes)
-  [Function `id_to_address`](#sui_object_id_to_address)
-  [Function `id_from_bytes`](#sui_object_id_from_bytes)
-  [Function `id_from_address`](#sui_object_id_from_address)
-  [Function `sui_system_state`](#sui_object_sui_system_state)
-  [Function `clock`](#sui_object_clock)
-  [Function `authenticator_state`](#sui_object_authenticator_state)
-  [Function `randomness_state`](#sui_object_randomness_state)
-  [Function `sui_deny_list_object_id`](#sui_object_sui_deny_list_object_id)
-  [Function `bridge`](#sui_object_bridge)
-  [Function `uid_as_inner`](#sui_object_uid_as_inner)
-  [Function `uid_to_inner`](#sui_object_uid_to_inner)
-  [Function `uid_to_bytes`](#sui_object_uid_to_bytes)
-  [Function `uid_to_address`](#sui_object_uid_to_address)
-  [Function `new`](#sui_object_new)
-  [Function `delete`](#sui_object_delete)
-  [Function `id`](#sui_object_id)
-  [Function `borrow_id`](#sui_object_borrow_id)
-  [Function `id_bytes`](#sui_object_id_bytes)
-  [Function `id_address`](#sui_object_id_address)
-  [Function `borrow_uid`](#sui_object_borrow_uid)
-  [Function `new_uid_from_hash`](#sui_object_new_uid_from_hash)
-  [Function `delete_impl`](#sui_object_delete_impl)
-  [Function `record_new_uid`](#sui_object_record_new_uid)


<pre><code><b>use</b> <a href="../std/ascii.md#std_ascii">std::ascii</a>;
<b>use</b> <a href="../std/bcs.md#std_bcs">std::bcs</a>;
<b>use</b> <a href="../std/option.md#std_option">std::option</a>;
<b>use</b> <a href="../std/string.md#std_string">std::string</a>;
<b>use</b> <a href="../std/vector.md#std_vector">std::vector</a>;
<b>use</b> <a href="../sui/address.md#sui_address">sui::address</a>;
<b>use</b> <a href="../sui/hex.md#sui_hex">sui::hex</a>;
<b>use</b> <a href="../sui/tx_context.md#sui_tx_context">sui::tx_context</a>;
</code></pre>



<a name="sui_object_ID"></a>

## Struct `ID`

An object ID. This is used to reference Sui Objects.
This is *not* guaranteed to be globally unique--anyone can create an <code><a href="../sui/object.md#sui_object_ID">ID</a></code> from a <code><a href="../sui/object.md#sui_object_UID">UID</a></code> or
from an object, and ID's can be freely copied and dropped.
Here, the values are not globally unique because there can be multiple values of type <code><a href="../sui/object.md#sui_object_ID">ID</a></code>
with the same underlying bytes. For example, <code><a href="../sui/object.md#sui_object_id">object::id</a>(&obj)</code> can be called as many times
as you want for a given <code>obj</code>, and each <code><a href="../sui/object.md#sui_object_ID">ID</a></code> value will be identical.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/object.md#sui_object_ID">ID</a> <b>has</b> <b>copy</b>, drop, store
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

<a name="sui_object_UID"></a>

## Struct `UID`

Globally unique IDs that define an object's ID in storage. Any Sui Object, that is a struct
with the <code>key</code> ability, must have <code><a href="../sui/object.md#sui_object_id">id</a>: <a href="../sui/object.md#sui_object_UID">UID</a></code> as its first field.
These are globally unique in the sense that no two values of type <code><a href="../sui/object.md#sui_object_UID">UID</a></code> are ever equal, in
other words for any two values <code>id1: <a href="../sui/object.md#sui_object_UID">UID</a></code> and <code>id2: <a href="../sui/object.md#sui_object_UID">UID</a></code>, <code>id1</code> != <code>id2</code>.
This is a privileged type that can only be derived from a <code>TxContext</code>.
<code><a href="../sui/object.md#sui_object_UID">UID</a></code> doesn't have the <code>drop</code> ability, so deleting a <code><a href="../sui/object.md#sui_object_UID">UID</a></code> requires a call to <code><a href="../sui/object.md#sui_object_delete">delete</a></code>.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/object.md#sui_object_UID">UID</a> <b>has</b> store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code><a href="../sui/object.md#sui_object_id">id</a>: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a></code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="sui_object_ENotSystemAddress"></a>

Sender is not @0x0 the system address.


<pre><code><b>const</b> <a href="../sui/object.md#sui_object_ENotSystemAddress">ENotSystemAddress</a>: u64 = 0;
</code></pre>



<a name="sui_object_SUI_AUTHENTICATOR_STATE_ID"></a>

The hardcoded ID for the singleton AuthenticatorState Object.


<pre><code><b>const</b> <a href="../sui/object.md#sui_object_SUI_AUTHENTICATOR_STATE_ID">SUI_AUTHENTICATOR_STATE_ID</a>: <b>address</b> = 0x7;
</code></pre>



<a name="sui_object_SUI_BRIDGE_ID"></a>

The hardcoded ID for the Bridge Object.


<pre><code><b>const</b> <a href="../sui/object.md#sui_object_SUI_BRIDGE_ID">SUI_BRIDGE_ID</a>: <b>address</b> = 0x9;
</code></pre>



<a name="sui_object_SUI_CLOCK_OBJECT_ID"></a>

The hardcoded ID for the singleton Clock Object.


<pre><code><b>const</b> <a href="../sui/object.md#sui_object_SUI_CLOCK_OBJECT_ID">SUI_CLOCK_OBJECT_ID</a>: <b>address</b> = 0x6;
</code></pre>



<a name="sui_object_SUI_DENY_LIST_OBJECT_ID"></a>

The hardcoded ID for the singleton DenyList.


<pre><code><b>const</b> <a href="../sui/object.md#sui_object_SUI_DENY_LIST_OBJECT_ID">SUI_DENY_LIST_OBJECT_ID</a>: <b>address</b> = 0x403;
</code></pre>



<a name="sui_object_SUI_RANDOM_ID"></a>

The hardcoded ID for the singleton Random Object.


<pre><code><b>const</b> <a href="../sui/object.md#sui_object_SUI_RANDOM_ID">SUI_RANDOM_ID</a>: <b>address</b> = 0x8;
</code></pre>



<a name="sui_object_SUI_SYSTEM_STATE_OBJECT_ID"></a>

The hardcoded ID for the singleton Sui System State Object.


<pre><code><b>const</b> <a href="../sui/object.md#sui_object_SUI_SYSTEM_STATE_OBJECT_ID">SUI_SYSTEM_STATE_OBJECT_ID</a>: <b>address</b> = 0x5;
</code></pre>



<a name="sui_object_id_to_bytes"></a>

## Function `id_to_bytes`

Get the raw bytes of a <code><a href="../sui/object.md#sui_object_ID">ID</a></code>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/object.md#sui_object_id_to_bytes">id_to_bytes</a>(<a href="../sui/object.md#sui_object_id">id</a>: &<a href="../sui/object.md#sui_object_ID">sui::object::ID</a>): vector&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/object.md#sui_object_id_to_bytes">id_to_bytes</a>(<a href="../sui/object.md#sui_object_id">id</a>: &<a href="../sui/object.md#sui_object_ID">ID</a>): vector&lt;u8&gt; {
    <a href="../sui/bcs.md#sui_bcs_to_bytes">bcs::to_bytes</a>(&<a href="../sui/object.md#sui_object_id">id</a>.bytes)
}
</code></pre>



</details>

<a name="sui_object_id_to_address"></a>

## Function `id_to_address`

Get the inner bytes of <code><a href="../sui/object.md#sui_object_id">id</a></code> as an address.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/object.md#sui_object_id_to_address">id_to_address</a>(<a href="../sui/object.md#sui_object_id">id</a>: &<a href="../sui/object.md#sui_object_ID">sui::object::ID</a>): <b>address</b>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/object.md#sui_object_id_to_address">id_to_address</a>(<a href="../sui/object.md#sui_object_id">id</a>: &<a href="../sui/object.md#sui_object_ID">ID</a>): <b>address</b> {
    <a href="../sui/object.md#sui_object_id">id</a>.bytes
}
</code></pre>



</details>

<a name="sui_object_id_from_bytes"></a>

## Function `id_from_bytes`

Make an <code><a href="../sui/object.md#sui_object_ID">ID</a></code> from raw bytes.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/object.md#sui_object_id_from_bytes">id_from_bytes</a>(bytes: vector&lt;u8&gt;): <a href="../sui/object.md#sui_object_ID">sui::object::ID</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/object.md#sui_object_id_from_bytes">id_from_bytes</a>(bytes: vector&lt;u8&gt;): <a href="../sui/object.md#sui_object_ID">ID</a> {
    <a href="../sui/address.md#sui_address_from_bytes">address::from_bytes</a>(bytes).to_id()
}
</code></pre>



</details>

<a name="sui_object_id_from_address"></a>

## Function `id_from_address`

Make an <code><a href="../sui/object.md#sui_object_ID">ID</a></code> from an address.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/object.md#sui_object_id_from_address">id_from_address</a>(bytes: <b>address</b>): <a href="../sui/object.md#sui_object_ID">sui::object::ID</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/object.md#sui_object_id_from_address">id_from_address</a>(bytes: <b>address</b>): <a href="../sui/object.md#sui_object_ID">ID</a> {
    <a href="../sui/object.md#sui_object_ID">ID</a> { bytes }
}
</code></pre>



</details>

<a name="sui_object_sui_system_state"></a>

## Function `sui_system_state`

Create the <code><a href="../sui/object.md#sui_object_UID">UID</a></code> for the singleton <code>SuiSystemState</code> object.
This should only be called once from <code>sui_system</code>.


<pre><code><b>fun</b> <a href="../sui/object.md#sui_object_sui_system_state">sui_system_state</a>(ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <a href="../sui/object.md#sui_object_UID">sui::object::UID</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/object.md#sui_object_sui_system_state">sui_system_state</a>(ctx: &TxContext): <a href="../sui/object.md#sui_object_UID">UID</a> {
    <b>assert</b>!(ctx.sender() == @0x0, <a href="../sui/object.md#sui_object_ENotSystemAddress">ENotSystemAddress</a>);
    <a href="../sui/object.md#sui_object_UID">UID</a> {
        <a href="../sui/object.md#sui_object_id">id</a>: <a href="../sui/object.md#sui_object_ID">ID</a> { bytes: <a href="../sui/object.md#sui_object_SUI_SYSTEM_STATE_OBJECT_ID">SUI_SYSTEM_STATE_OBJECT_ID</a> },
    }
}
</code></pre>



</details>

<a name="sui_object_clock"></a>

## Function `clock`

Create the <code><a href="../sui/object.md#sui_object_UID">UID</a></code> for the singleton <code>Clock</code> object.
This should only be called once from <code><a href="../sui/clock.md#sui_clock">clock</a></code>.


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/clock.md#sui_clock">clock</a>(): <a href="../sui/object.md#sui_object_UID">sui::object::UID</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/clock.md#sui_clock">clock</a>(): <a href="../sui/object.md#sui_object_UID">UID</a> {
    <a href="../sui/object.md#sui_object_UID">UID</a> {
        <a href="../sui/object.md#sui_object_id">id</a>: <a href="../sui/object.md#sui_object_ID">ID</a> { bytes: <a href="../sui/object.md#sui_object_SUI_CLOCK_OBJECT_ID">SUI_CLOCK_OBJECT_ID</a> },
    }
}
</code></pre>



</details>

<a name="sui_object_authenticator_state"></a>

## Function `authenticator_state`

Create the <code><a href="../sui/object.md#sui_object_UID">UID</a></code> for the singleton <code>AuthenticatorState</code> object.
This should only be called once from <code><a href="../sui/authenticator_state.md#sui_authenticator_state">authenticator_state</a></code>.


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/authenticator_state.md#sui_authenticator_state">authenticator_state</a>(): <a href="../sui/object.md#sui_object_UID">sui::object::UID</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/authenticator_state.md#sui_authenticator_state">authenticator_state</a>(): <a href="../sui/object.md#sui_object_UID">UID</a> {
    <a href="../sui/object.md#sui_object_UID">UID</a> {
        <a href="../sui/object.md#sui_object_id">id</a>: <a href="../sui/object.md#sui_object_ID">ID</a> { bytes: <a href="../sui/object.md#sui_object_SUI_AUTHENTICATOR_STATE_ID">SUI_AUTHENTICATOR_STATE_ID</a> },
    }
}
</code></pre>



</details>

<a name="sui_object_randomness_state"></a>

## Function `randomness_state`

Create the <code><a href="../sui/object.md#sui_object_UID">UID</a></code> for the singleton <code>Random</code> object.
This should only be called once from <code><a href="../sui/random.md#sui_random">random</a></code>.


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/object.md#sui_object_randomness_state">randomness_state</a>(): <a href="../sui/object.md#sui_object_UID">sui::object::UID</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/object.md#sui_object_randomness_state">randomness_state</a>(): <a href="../sui/object.md#sui_object_UID">UID</a> {
    <a href="../sui/object.md#sui_object_UID">UID</a> {
        <a href="../sui/object.md#sui_object_id">id</a>: <a href="../sui/object.md#sui_object_ID">ID</a> { bytes: <a href="../sui/object.md#sui_object_SUI_RANDOM_ID">SUI_RANDOM_ID</a> },
    }
}
</code></pre>



</details>

<a name="sui_object_sui_deny_list_object_id"></a>

## Function `sui_deny_list_object_id`

Create the <code><a href="../sui/object.md#sui_object_UID">UID</a></code> for the singleton <code>DenyList</code> object.
This should only be called once from <code><a href="../sui/deny_list.md#sui_deny_list">deny_list</a></code>.


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/object.md#sui_object_sui_deny_list_object_id">sui_deny_list_object_id</a>(): <a href="../sui/object.md#sui_object_UID">sui::object::UID</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/object.md#sui_object_sui_deny_list_object_id">sui_deny_list_object_id</a>(): <a href="../sui/object.md#sui_object_UID">UID</a> {
    <a href="../sui/object.md#sui_object_UID">UID</a> {
        <a href="../sui/object.md#sui_object_id">id</a>: <a href="../sui/object.md#sui_object_ID">ID</a> { bytes: <a href="../sui/object.md#sui_object_SUI_DENY_LIST_OBJECT_ID">SUI_DENY_LIST_OBJECT_ID</a> },
    }
}
</code></pre>



</details>

<a name="sui_object_bridge"></a>

## Function `bridge`

Create the <code><a href="../sui/object.md#sui_object_UID">UID</a></code> for the singleton <code>Bridge</code> object.
This should only be called once from <code><a href="../sui/object.md#sui_object_bridge">bridge</a></code>.


<pre><code><b>fun</b> <a href="../sui/object.md#sui_object_bridge">bridge</a>(): <a href="../sui/object.md#sui_object_UID">sui::object::UID</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/object.md#sui_object_bridge">bridge</a>(): <a href="../sui/object.md#sui_object_UID">UID</a> {
    <a href="../sui/object.md#sui_object_UID">UID</a> {
        <a href="../sui/object.md#sui_object_id">id</a>: <a href="../sui/object.md#sui_object_ID">ID</a> { bytes: <a href="../sui/object.md#sui_object_SUI_BRIDGE_ID">SUI_BRIDGE_ID</a> },
    }
}
</code></pre>



</details>

<a name="sui_object_uid_as_inner"></a>

## Function `uid_as_inner`

Get the inner <code><a href="../sui/object.md#sui_object_ID">ID</a></code> of <code>uid</code>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/object.md#sui_object_uid_as_inner">uid_as_inner</a>(uid: &<a href="../sui/object.md#sui_object_UID">sui::object::UID</a>): &<a href="../sui/object.md#sui_object_ID">sui::object::ID</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/object.md#sui_object_uid_as_inner">uid_as_inner</a>(uid: &<a href="../sui/object.md#sui_object_UID">UID</a>): &<a href="../sui/object.md#sui_object_ID">ID</a> {
    &uid.<a href="../sui/object.md#sui_object_id">id</a>
}
</code></pre>



</details>

<a name="sui_object_uid_to_inner"></a>

## Function `uid_to_inner`

Get the raw bytes of a <code>uid</code>'s inner <code><a href="../sui/object.md#sui_object_ID">ID</a></code>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/object.md#sui_object_uid_to_inner">uid_to_inner</a>(uid: &<a href="../sui/object.md#sui_object_UID">sui::object::UID</a>): <a href="../sui/object.md#sui_object_ID">sui::object::ID</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/object.md#sui_object_uid_to_inner">uid_to_inner</a>(uid: &<a href="../sui/object.md#sui_object_UID">UID</a>): <a href="../sui/object.md#sui_object_ID">ID</a> {
    uid.<a href="../sui/object.md#sui_object_id">id</a>
}
</code></pre>



</details>

<a name="sui_object_uid_to_bytes"></a>

## Function `uid_to_bytes`

Get the raw bytes of a <code><a href="../sui/object.md#sui_object_UID">UID</a></code>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/object.md#sui_object_uid_to_bytes">uid_to_bytes</a>(uid: &<a href="../sui/object.md#sui_object_UID">sui::object::UID</a>): vector&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/object.md#sui_object_uid_to_bytes">uid_to_bytes</a>(uid: &<a href="../sui/object.md#sui_object_UID">UID</a>): vector&lt;u8&gt; {
    <a href="../sui/bcs.md#sui_bcs_to_bytes">bcs::to_bytes</a>(&uid.<a href="../sui/object.md#sui_object_id">id</a>.bytes)
}
</code></pre>



</details>

<a name="sui_object_uid_to_address"></a>

## Function `uid_to_address`

Get the inner bytes of <code><a href="../sui/object.md#sui_object_id">id</a></code> as an address.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/object.md#sui_object_uid_to_address">uid_to_address</a>(uid: &<a href="../sui/object.md#sui_object_UID">sui::object::UID</a>): <b>address</b>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/object.md#sui_object_uid_to_address">uid_to_address</a>(uid: &<a href="../sui/object.md#sui_object_UID">UID</a>): <b>address</b> {
    uid.<a href="../sui/object.md#sui_object_id">id</a>.bytes
}
</code></pre>



</details>

<a name="sui_object_new"></a>

## Function `new`

Create a new object. Returns the <code><a href="../sui/object.md#sui_object_UID">UID</a></code> that must be stored in a Sui object.
This is the only way to create <code><a href="../sui/object.md#sui_object_UID">UID</a></code>s.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/object.md#sui_object_new">new</a>(ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <a href="../sui/object.md#sui_object_UID">sui::object::UID</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/object.md#sui_object_new">new</a>(ctx: &<b>mut</b> TxContext): <a href="../sui/object.md#sui_object_UID">UID</a> {
    <a href="../sui/object.md#sui_object_UID">UID</a> {
        <a href="../sui/object.md#sui_object_id">id</a>: <a href="../sui/object.md#sui_object_ID">ID</a> { bytes: ctx.fresh_object_address() },
    }
}
</code></pre>



</details>

<a name="sui_object_delete"></a>

## Function `delete`

Delete the object and its <code><a href="../sui/object.md#sui_object_UID">UID</a></code>. This is the only way to eliminate a <code><a href="../sui/object.md#sui_object_UID">UID</a></code>.
This exists to inform Sui of object deletions. When an object
gets unpacked, the programmer will have to do something with its
<code><a href="../sui/object.md#sui_object_UID">UID</a></code>. The implementation of this function emits a deleted
system event so Sui knows to process the object deletion


<pre><code><b>public</b> <b>fun</b> <a href="../sui/object.md#sui_object_delete">delete</a>(<a href="../sui/object.md#sui_object_id">id</a>: <a href="../sui/object.md#sui_object_UID">sui::object::UID</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/object.md#sui_object_delete">delete</a>(<a href="../sui/object.md#sui_object_id">id</a>: <a href="../sui/object.md#sui_object_UID">UID</a>) {
    <b>let</b> <a href="../sui/object.md#sui_object_UID">UID</a> { <a href="../sui/object.md#sui_object_id">id</a>: <a href="../sui/object.md#sui_object_ID">ID</a> { bytes } } = <a href="../sui/object.md#sui_object_id">id</a>;
    <a href="../sui/object.md#sui_object_delete_impl">delete_impl</a>(bytes)
}
</code></pre>



</details>

<a name="sui_object_id"></a>

## Function `id`

Get the underlying <code><a href="../sui/object.md#sui_object_ID">ID</a></code> of <code>obj</code>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/object.md#sui_object_id">id</a>&lt;T: key&gt;(obj: &T): <a href="../sui/object.md#sui_object_ID">sui::object::ID</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/object.md#sui_object_id">id</a>&lt;T: key&gt;(obj: &T): <a href="../sui/object.md#sui_object_ID">ID</a> {
    <a href="../sui/object.md#sui_object_borrow_uid">borrow_uid</a>(obj).<a href="../sui/object.md#sui_object_id">id</a>
}
</code></pre>



</details>

<a name="sui_object_borrow_id"></a>

## Function `borrow_id`

Borrow the underlying <code><a href="../sui/object.md#sui_object_ID">ID</a></code> of <code>obj</code>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/object.md#sui_object_borrow_id">borrow_id</a>&lt;T: key&gt;(obj: &T): &<a href="../sui/object.md#sui_object_ID">sui::object::ID</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/object.md#sui_object_borrow_id">borrow_id</a>&lt;T: key&gt;(obj: &T): &<a href="../sui/object.md#sui_object_ID">ID</a> {
    &<a href="../sui/object.md#sui_object_borrow_uid">borrow_uid</a>(obj).<a href="../sui/object.md#sui_object_id">id</a>
}
</code></pre>



</details>

<a name="sui_object_id_bytes"></a>

## Function `id_bytes`

Get the raw bytes for the underlying <code><a href="../sui/object.md#sui_object_ID">ID</a></code> of <code>obj</code>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/object.md#sui_object_id_bytes">id_bytes</a>&lt;T: key&gt;(obj: &T): vector&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/object.md#sui_object_id_bytes">id_bytes</a>&lt;T: key&gt;(obj: &T): vector&lt;u8&gt; {
    <a href="../sui/bcs.md#sui_bcs_to_bytes">bcs::to_bytes</a>(&<a href="../sui/object.md#sui_object_borrow_uid">borrow_uid</a>(obj).<a href="../sui/object.md#sui_object_id">id</a>)
}
</code></pre>



</details>

<a name="sui_object_id_address"></a>

## Function `id_address`

Get the inner bytes for the underlying <code><a href="../sui/object.md#sui_object_ID">ID</a></code> of <code>obj</code>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/object.md#sui_object_id_address">id_address</a>&lt;T: key&gt;(obj: &T): <b>address</b>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/object.md#sui_object_id_address">id_address</a>&lt;T: key&gt;(obj: &T): <b>address</b> {
    <a href="../sui/object.md#sui_object_borrow_uid">borrow_uid</a>(obj).<a href="../sui/object.md#sui_object_id">id</a>.bytes
}
</code></pre>



</details>

<a name="sui_object_borrow_uid"></a>

## Function `borrow_uid`

Get the <code><a href="../sui/object.md#sui_object_UID">UID</a></code> for <code>obj</code>.
Safe because Sui has an extra bytecode verifier pass that forces every struct with
the <code>key</code> ability to have a distinguished <code><a href="../sui/object.md#sui_object_UID">UID</a></code> field.
Cannot be made public as the access to <code><a href="../sui/object.md#sui_object_UID">UID</a></code> for a given object must be privileged, and
restrictable in the object's module.


<pre><code><b>fun</b> <a href="../sui/object.md#sui_object_borrow_uid">borrow_uid</a>&lt;T: key&gt;(obj: &T): &<a href="../sui/object.md#sui_object_UID">sui::object::UID</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="../sui/object.md#sui_object_borrow_uid">borrow_uid</a>&lt;T: key&gt;(obj: &T): &<a href="../sui/object.md#sui_object_UID">UID</a>;
</code></pre>



</details>

<a name="sui_object_new_uid_from_hash"></a>

## Function `new_uid_from_hash`

Generate a new UID specifically used for creating a UID from a hash


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/object.md#sui_object_new_uid_from_hash">new_uid_from_hash</a>(bytes: <b>address</b>): <a href="../sui/object.md#sui_object_UID">sui::object::UID</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/object.md#sui_object_new_uid_from_hash">new_uid_from_hash</a>(bytes: <b>address</b>): <a href="../sui/object.md#sui_object_UID">UID</a> {
    <a href="../sui/object.md#sui_object_record_new_uid">record_new_uid</a>(bytes);
    <a href="../sui/object.md#sui_object_UID">UID</a> { <a href="../sui/object.md#sui_object_id">id</a>: <a href="../sui/object.md#sui_object_ID">ID</a> { bytes } }
}
</code></pre>



</details>

<a name="sui_object_delete_impl"></a>

## Function `delete_impl`



<pre><code><b>fun</b> <a href="../sui/object.md#sui_object_delete_impl">delete_impl</a>(<a href="../sui/object.md#sui_object_id">id</a>: <b>address</b>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="../sui/object.md#sui_object_delete_impl">delete_impl</a>(<a href="../sui/object.md#sui_object_id">id</a>: <b>address</b>);
</code></pre>



</details>

<a name="sui_object_record_new_uid"></a>

## Function `record_new_uid`



<pre><code><b>fun</b> <a href="../sui/object.md#sui_object_record_new_uid">record_new_uid</a>(<a href="../sui/object.md#sui_object_id">id</a>: <b>address</b>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="../sui/object.md#sui_object_record_new_uid">record_new_uid</a>(<a href="../sui/object.md#sui_object_id">id</a>: <b>address</b>);
</code></pre>



</details>
