---
title: Module `sui::party`
---



-  [Struct `Party`](#sui_party_Party)
-  [Struct `Permissions`](#sui_party_Permissions)
-  [Constants](#@Constants_0)
-  [Function `single_owner`](#sui_party_single_owner)
-  [Macro function `transfer`](#sui_party_transfer)
-  [Macro function `public_transfer`](#sui_party_public_transfer)
-  [Function `empty`](#sui_party_empty)
-  [Function `set_permissions`](#sui_party_set_permissions)
-  [Function `is_single_owner`](#sui_party_is_single_owner)
-  [Function `into_native`](#sui_party_into_native)


<pre><code><b>use</b> <a href="../std/option.md#std_option">std::option</a>;
<b>use</b> <a href="../std/vector.md#std_vector">std::vector</a>;
<b>use</b> <a href="../sui/vec_map.md#sui_vec_map">sui::vec_map</a>;
</code></pre>



<a name="sui_party_Party"></a>

## Struct `Party`

The permissions that apply to a party object. If the transaction sender has an entry in
the <code>members</code> map, the permissions in that entry apply. Otherwise, the <code>default</code> permissions
are used.
If the party has the <code><a href="../sui/party.md#sui_party_READ">READ</a></code> permission, the object can be taken as an immutable input.
If the party has the <code><a href="../sui/party.md#sui_party_WRITE">WRITE</a></code>, <code><a href="../sui/party.md#sui_party_DELETE">DELETE</a></code>, or <code><a href="../sui/party.md#sui_party_TRANSFER">TRANSFER</a></code> permissions, the object can be taken as
a mutable input. Additional restrictions pertaining to each permission are checked at the end
of transaction execution.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/party.md#sui_party_Party">Party</a> <b>has</b> <b>copy</b>, drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>default: <a href="../sui/party.md#sui_party_Permissions">sui::party::Permissions</a></code>
</dt>
<dd>
 The permissions that apply if no specific permissions are set in the <code>members</code> map.
</dd>
<dt>
<code>members: <a href="../sui/vec_map.md#sui_vec_map_VecMap">sui::vec_map::VecMap</a>&lt;<b>address</b>, <a href="../sui/party.md#sui_party_Permissions">sui::party::Permissions</a>&gt;</code>
</dt>
<dd>
 The permissions per transaction sender.
</dd>
</dl>


</details>

<a name="sui_party_Permissions"></a>

## Struct `Permissions`

The permissions that a party has. The permissions are a bitset of the <code><a href="../sui/party.md#sui_party_READ">READ</a></code>, <code><a href="../sui/party.md#sui_party_WRITE">WRITE</a></code>,
<code><a href="../sui/party.md#sui_party_DELETE">DELETE</a></code>, and <code><a href="../sui/party.md#sui_party_TRANSFER">TRANSFER</a></code> constants.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/party.md#sui_party_Permissions">Permissions</a> <b>has</b> <b>copy</b>, drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>0: u64</code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="sui_party_READ"></a>

A party can read the object, taking it as an immutable argument. This restriction is checked
when sending the transaction.


<pre><code><b>const</b> <a href="../sui/party.md#sui_party_READ">READ</a>: u8 = 1;
</code></pre>



<a name="sui_party_WRITE"></a>

The party can mutate the object, but not change its owner or delete it. This is checked at
end end of transaction execution.


<pre><code><b>const</b> <a href="../sui/party.md#sui_party_WRITE">WRITE</a>: u8 = 2;
</code></pre>



<a name="sui_party_DELETE"></a>

The party can delete the object, but not otherwise modify it. This is checked at the end of
transaction execution.


<pre><code><b>const</b> <a href="../sui/party.md#sui_party_DELETE">DELETE</a>: u8 = 4;
</code></pre>



<a name="sui_party_TRANSFER"></a>

The party can change the owner of the object, but not otherwise modify it. This is checked at
the end of transaction execution.


<pre><code><b>const</b> <a href="../sui/party.md#sui_party_TRANSFER">TRANSFER</a>: u8 = 8;
</code></pre>



<a name="sui_party_NO_PERMISSIONS"></a>

No permissions.


<pre><code><b>const</b> <a href="../sui/party.md#sui_party_NO_PERMISSIONS">NO_PERMISSIONS</a>: u64 = 0;
</code></pre>



<a name="sui_party_ALL_PERMISSIONS"></a>

All permissions.


<pre><code><b>const</b> <a href="../sui/party.md#sui_party_ALL_PERMISSIONS">ALL_PERMISSIONS</a>: u64 = 15;
</code></pre>



<a name="sui_party_single_owner"></a>

## Function `single_owner`

Creates a <code><a href="../sui/party.md#sui_party_Party">Party</a></code> value with a single "owner" that has all permissions. No other party
has any permissions. And there are no default permissions.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/party.md#sui_party_single_owner">single_owner</a>(owner: <b>address</b>): <a href="../sui/party.md#sui_party_Party">sui::party::Party</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/party.md#sui_party_single_owner">single_owner</a>(owner: <b>address</b>): <a href="../sui/party.md#sui_party_Party">Party</a> {
    <b>let</b> <b>mut</b> mp = <a href="../sui/party.md#sui_party_empty">empty</a>();
    mp.<a href="../sui/party.md#sui_party_set_permissions">set_permissions</a>(owner, <a href="../sui/party.md#sui_party_Permissions">Permissions</a>(<a href="../sui/party.md#sui_party_ALL_PERMISSIONS">ALL_PERMISSIONS</a>));
    mp
}
</code></pre>



</details>

<a name="sui_party_transfer"></a>

## Macro function `transfer`

A helper <code><b>macro</b></code> that calls <code><a href="../sui/transfer.md#sui_transfer_party_transfer">sui::transfer::party_transfer</a></code>.


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../sui/transfer.md#sui_transfer">transfer</a>&lt;$T: key&gt;($self: <a href="../sui/party.md#sui_party_Party">sui::party::Party</a>, $obj: $T)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../sui/transfer.md#sui_transfer">transfer</a>&lt;$T: key&gt;($self: <a href="../sui/party.md#sui_party_Party">Party</a>, $obj: $T) {
    <b>let</b> mp = $self;
    <a href="../sui/transfer.md#sui_transfer_party_transfer">sui::transfer::party_transfer</a>($obj, mp)
}
</code></pre>



</details>

<a name="sui_party_public_transfer"></a>

## Macro function `public_transfer`

A helper <code><b>macro</b></code> that calls <code><a href="../sui/transfer.md#sui_transfer_public_party_transfer">sui::transfer::public_party_transfer</a></code>.


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../sui/party.md#sui_party_public_transfer">public_transfer</a>&lt;$T: key, store&gt;($self: <a href="../sui/party.md#sui_party_Party">sui::party::Party</a>, $obj: $T)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../sui/party.md#sui_party_public_transfer">public_transfer</a>&lt;$T: key + store&gt;($self: <a href="../sui/party.md#sui_party_Party">Party</a>, $obj: $T) {
    <b>let</b> mp = $self;
    <a href="../sui/transfer.md#sui_transfer_public_party_transfer">sui::transfer::public_party_transfer</a>($obj, mp)
}
</code></pre>



</details>

<a name="sui_party_empty"></a>

## Function `empty`



<pre><code><b>fun</b> <a href="../sui/party.md#sui_party_empty">empty</a>(): <a href="../sui/party.md#sui_party_Party">sui::party::Party</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/party.md#sui_party_empty">empty</a>(): <a href="../sui/party.md#sui_party_Party">Party</a> {
<a href="../sui/party.md#sui_party_Party">Party</a> {
default: <a href="../sui/party.md#sui_party_Permissions">Permissions</a>(<a href="../sui/party.md#sui_party_NO_PERMISSIONS">NO_PERMISSIONS</a>),
members: <a href="../sui/vec_map.md#sui_vec_map_empty">vec_map::empty</a>(),
}
}
</code></pre>



</details>

<a name="sui_party_set_permissions"></a>

## Function `set_permissions`



<pre><code><b>fun</b> <a href="../sui/party.md#sui_party_set_permissions">set_permissions</a>(p: &<b>mut</b> <a href="../sui/party.md#sui_party_Party">sui::party::Party</a>, <b>address</b>: <b>address</b>, permissions: <a href="../sui/party.md#sui_party_Permissions">sui::party::Permissions</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/party.md#sui_party_set_permissions">set_permissions</a>(p: &<b>mut</b> <a href="../sui/party.md#sui_party_Party">Party</a>, <b>address</b>: <b>address</b>, permissions: <a href="../sui/party.md#sui_party_Permissions">Permissions</a>) {
<b>if</b> (p.members.contains(&<b>address</b>)) {
p.members.remove(&<b>address</b>);
};
p.members.insert(<b>address</b>, permissions);
}
</code></pre>



</details>

<a name="sui_party_is_single_owner"></a>

## Function `is_single_owner`



<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/party.md#sui_party_is_single_owner">is_single_owner</a>(p: &<a href="../sui/party.md#sui_party_Party">sui::party::Party</a>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/party.md#sui_party_is_single_owner">is_single_owner</a>(p: &<a href="../sui/party.md#sui_party_Party">Party</a>): bool {
    p.default.0 == <a href="../sui/party.md#sui_party_NO_PERMISSIONS">NO_PERMISSIONS</a> &&
    p.members.size() == 1 &&
    { <b>let</b> (_, m) = p.members.get_entry_by_idx(0); m.0 == <a href="../sui/party.md#sui_party_ALL_PERMISSIONS">ALL_PERMISSIONS</a> }
}
</code></pre>



</details>

<a name="sui_party_into_native"></a>

## Function `into_native`



<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/party.md#sui_party_into_native">into_native</a>(p: <a href="../sui/party.md#sui_party_Party">sui::party::Party</a>): (u64, vector&lt;<b>address</b>&gt;, vector&lt;u64&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/party.md#sui_party_into_native">into_native</a>(p: <a href="../sui/party.md#sui_party_Party">Party</a>): (u64, vector&lt;<b>address</b>&gt;, vector&lt;u64&gt;) {
    <b>let</b> <a href="../sui/party.md#sui_party_Party">Party</a> { default, members } = p;
    <b>let</b> (addresses, permissions) = members.into_keys_values();
    <b>let</b> permissions = permissions.map!(|<a href="../sui/party.md#sui_party_Permissions">Permissions</a>(x)| x);
    (default.0, addresses, permissions)
}
</code></pre>



</details>
