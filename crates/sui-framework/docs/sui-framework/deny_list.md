---
title: Module `0x2::deny_list`
---

Defines the <code><a href="../sui-framework/deny_list.md#0x2_deny_list_DenyList">DenyList</a></code> type. The <code><a href="../sui-framework/deny_list.md#0x2_deny_list_DenyList">DenyList</a></code> shared object is used to restrict access to
instances of certain core types from being used as inputs by specified addresses in the deny
list.


-  [Resource `DenyList`](#0x2_deny_list_DenyList)
-  [Resource `PerTypeList`](#0x2_deny_list_PerTypeList)
-  [Constants](#@Constants_0)
-  [Function `add`](#0x2_deny_list_add)
-  [Function `per_type_list_add`](#0x2_deny_list_per_type_list_add)
-  [Function `remove`](#0x2_deny_list_remove)
-  [Function `per_type_list_remove`](#0x2_deny_list_per_type_list_remove)
-  [Function `contains`](#0x2_deny_list_contains)
-  [Function `per_type_list_contains`](#0x2_deny_list_per_type_list_contains)
-  [Function `create`](#0x2_deny_list_create)
-  [Function `per_type_list`](#0x2_deny_list_per_type_list)


<pre><code><b>use</b> <a href="../move-stdlib/vector.md#0x1_vector">0x1::vector</a>;
<b>use</b> <a href="../sui-framework/bag.md#0x2_bag">0x2::bag</a>;
<b>use</b> <a href="../sui-framework/object.md#0x2_object">0x2::object</a>;
<b>use</b> <a href="../sui-framework/table.md#0x2_table">0x2::table</a>;
<b>use</b> <a href="../sui-framework/transfer.md#0x2_transfer">0x2::transfer</a>;
<b>use</b> <a href="../sui-framework/tx_context.md#0x2_tx_context">0x2::tx_context</a>;
<b>use</b> <a href="../sui-framework/vec_set.md#0x2_vec_set">0x2::vec_set</a>;
</code></pre>



<a name="0x2_deny_list_DenyList"></a>

## Resource `DenyList`

A shared object that stores the addresses that are blocked for a given core type.


<pre><code><b>struct</b> <a href="../sui-framework/deny_list.md#0x2_deny_list_DenyList">DenyList</a> <b>has</b> key
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>id: <a href="../sui-framework/object.md#0x2_object_UID">object::UID</a></code>
</dt>
<dd>

</dd>
<dt>
<code>lists: <a href="../sui-framework/bag.md#0x2_bag_Bag">bag::Bag</a></code>
</dt>
<dd>
 The individual deny lists.
</dd>
</dl>


</details>

<a name="0x2_deny_list_PerTypeList"></a>

## Resource `PerTypeList`

Stores the addresses that are denied for a given core type.


<pre><code><b>struct</b> <a href="../sui-framework/deny_list.md#0x2_deny_list_PerTypeList">PerTypeList</a> <b>has</b> store, key
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>id: <a href="../sui-framework/object.md#0x2_object_UID">object::UID</a></code>
</dt>
<dd>

</dd>
<dt>
<code>denied_count: <a href="../sui-framework/table.md#0x2_table_Table">table::Table</a>&lt;<b>address</b>, <a href="../move-stdlib/u64.md#0x1_u64">u64</a>&gt;</code>
</dt>
<dd>
 Number of object types that have been banned for a given address.
 Used to quickly skip checks for most addresses.
</dd>
<dt>
<code>denied_addresses: <a href="../sui-framework/table.md#0x2_table_Table">table::Table</a>&lt;<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, <a href="../sui-framework/vec_set.md#0x2_vec_set_VecSet">vec_set::VecSet</a>&lt;<b>address</b>&gt;&gt;</code>
</dt>
<dd>
 Set of addresses that are banned for a given type.
 For example with <code>sui::coin::Coin</code>: If addresses A and B are banned from using
 "0...0123::my_coin::MY_COIN", this will be "0...0123::my_coin::MY_COIN" -> {A, B}.
</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0x2_deny_list_ENotSystemAddress"></a>

Trying to create a deny list object when not called by the system address.


<pre><code><b>const</b> <a href="../sui-framework/deny_list.md#0x2_deny_list_ENotSystemAddress">ENotSystemAddress</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 0;
</code></pre>



<a name="0x2_deny_list_COIN_INDEX"></a>

The index into the deny list vector for the <code>sui::coin::Coin</code> type.


<pre><code><b>const</b> <a href="../sui-framework/deny_list.md#0x2_deny_list_COIN_INDEX">COIN_INDEX</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 0;
</code></pre>



<a name="0x2_deny_list_EInvalidAddress"></a>

The specified address cannot be added to the deny list.


<pre><code><b>const</b> <a href="../sui-framework/deny_list.md#0x2_deny_list_EInvalidAddress">EInvalidAddress</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 1;
</code></pre>



<a name="0x2_deny_list_ENotDenied"></a>

The specified address to be removed is not already in the deny list.


<pre><code><b>const</b> <a href="../sui-framework/deny_list.md#0x2_deny_list_ENotDenied">ENotDenied</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 1;
</code></pre>



<a name="0x2_deny_list_RESERVED"></a>

These addresses are reserved and cannot be added to the deny list.
The addresses listed are well known package and object addresses. So it would be
meaningless to add them to the deny list.


<pre><code><b>const</b> <a href="../sui-framework/deny_list.md#0x2_deny_list_RESERVED">RESERVED</a>: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;<b>address</b>&gt; = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 1027, 57065];
</code></pre>



<a name="0x2_deny_list_add"></a>

## Function `add`

Adds the given address to the deny list of the specified type, preventing it
from interacting with instances of that type as an input to a transaction. For coins,
the type specified is the type of the coin, not the coin type itself. For example,
"00...0123::my_coin::MY_COIN" would be the type, not "00...02::coin::Coin".


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="../sui-framework/deny_list.md#0x2_deny_list_add">add</a>(<a href="../sui-framework/deny_list.md#0x2_deny_list">deny_list</a>: &<b>mut</b> <a href="../sui-framework/deny_list.md#0x2_deny_list_DenyList">deny_list::DenyList</a>, per_type_index: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>, type: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, addr: <b>address</b>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui-framework/deny_list.md#0x2_deny_list_add">add</a>(
    <a href="../sui-framework/deny_list.md#0x2_deny_list">deny_list</a>: &<b>mut</b> <a href="../sui-framework/deny_list.md#0x2_deny_list_DenyList">DenyList</a>,
    per_type_index: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>,
    `type`: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    addr: <b>address</b>,
) {
    <b>let</b> reserved = <a href="../sui-framework/deny_list.md#0x2_deny_list_RESERVED">RESERVED</a>;
    <b>assert</b>!(!reserved.<a href="../sui-framework/deny_list.md#0x2_deny_list_contains">contains</a>(&addr), <a href="../sui-framework/deny_list.md#0x2_deny_list_EInvalidAddress">EInvalidAddress</a>);
    <b>let</b> bag_entry: &<b>mut</b> <a href="../sui-framework/deny_list.md#0x2_deny_list_PerTypeList">PerTypeList</a> = &<b>mut</b> <a href="../sui-framework/deny_list.md#0x2_deny_list">deny_list</a>.lists[per_type_index];
    bag_entry.<a href="../sui-framework/deny_list.md#0x2_deny_list_per_type_list_add">per_type_list_add</a>(`type`, addr)
}
</code></pre>



</details>

<a name="0x2_deny_list_per_type_list_add"></a>

## Function `per_type_list_add`



<pre><code><b>fun</b> <a href="../sui-framework/deny_list.md#0x2_deny_list_per_type_list_add">per_type_list_add</a>(list: &<b>mut</b> <a href="../sui-framework/deny_list.md#0x2_deny_list_PerTypeList">deny_list::PerTypeList</a>, type: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, addr: <b>address</b>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui-framework/deny_list.md#0x2_deny_list_per_type_list_add">per_type_list_add</a>(
    list: &<b>mut</b> <a href="../sui-framework/deny_list.md#0x2_deny_list_PerTypeList">PerTypeList</a>,
    `type`: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    addr: <b>address</b>,
) {
    <b>if</b> (!list.denied_addresses.<a href="../sui-framework/deny_list.md#0x2_deny_list_contains">contains</a>(`type`)) {
        list.denied_addresses.<a href="../sui-framework/deny_list.md#0x2_deny_list_add">add</a>(`type`, <a href="../sui-framework/vec_set.md#0x2_vec_set_empty">vec_set::empty</a>());
    };
    <b>let</b> denied_addresses = &<b>mut</b> list.denied_addresses[`type`];
    <b>let</b> already_denied = denied_addresses.<a href="../sui-framework/deny_list.md#0x2_deny_list_contains">contains</a>(&addr);
    <b>if</b> (already_denied) <b>return</b>;

    denied_addresses.insert(addr);
    <b>if</b> (!list.denied_count.<a href="../sui-framework/deny_list.md#0x2_deny_list_contains">contains</a>(addr)) {
        list.denied_count.<a href="../sui-framework/deny_list.md#0x2_deny_list_add">add</a>(addr, 0);
    };
    <b>let</b> denied_count = &<b>mut</b> list.denied_count[addr];
    *denied_count = *denied_count + 1;
}
</code></pre>



</details>

<a name="0x2_deny_list_remove"></a>

## Function `remove`

Removes a previously denied address from the list.
Aborts with <code><a href="../sui-framework/deny_list.md#0x2_deny_list_ENotDenied">ENotDenied</a></code> if the address is not on the list.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="../sui-framework/deny_list.md#0x2_deny_list_remove">remove</a>(<a href="../sui-framework/deny_list.md#0x2_deny_list">deny_list</a>: &<b>mut</b> <a href="../sui-framework/deny_list.md#0x2_deny_list_DenyList">deny_list::DenyList</a>, per_type_index: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>, type: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, addr: <b>address</b>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui-framework/deny_list.md#0x2_deny_list_remove">remove</a>(
    <a href="../sui-framework/deny_list.md#0x2_deny_list">deny_list</a>: &<b>mut</b> <a href="../sui-framework/deny_list.md#0x2_deny_list_DenyList">DenyList</a>,
    per_type_index: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>,
    `type`: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    addr: <b>address</b>,
) {
    <b>let</b> reserved = <a href="../sui-framework/deny_list.md#0x2_deny_list_RESERVED">RESERVED</a>;
    <b>assert</b>!(!reserved.<a href="../sui-framework/deny_list.md#0x2_deny_list_contains">contains</a>(&addr), <a href="../sui-framework/deny_list.md#0x2_deny_list_EInvalidAddress">EInvalidAddress</a>);
    <a href="../sui-framework/deny_list.md#0x2_deny_list_per_type_list_remove">per_type_list_remove</a>(&<b>mut</b> <a href="../sui-framework/deny_list.md#0x2_deny_list">deny_list</a>.lists[per_type_index], `type`, addr)
}
</code></pre>



</details>

<a name="0x2_deny_list_per_type_list_remove"></a>

## Function `per_type_list_remove`



<pre><code><b>fun</b> <a href="../sui-framework/deny_list.md#0x2_deny_list_per_type_list_remove">per_type_list_remove</a>(list: &<b>mut</b> <a href="../sui-framework/deny_list.md#0x2_deny_list_PerTypeList">deny_list::PerTypeList</a>, type: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, addr: <b>address</b>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui-framework/deny_list.md#0x2_deny_list_per_type_list_remove">per_type_list_remove</a>(
    list: &<b>mut</b> <a href="../sui-framework/deny_list.md#0x2_deny_list_PerTypeList">PerTypeList</a>,
    `type`: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    addr: <b>address</b>,
) {
    <b>let</b> denied_addresses = &<b>mut</b> list.denied_addresses[`type`];
    <b>assert</b>!(denied_addresses.<a href="../sui-framework/deny_list.md#0x2_deny_list_contains">contains</a>(&addr), <a href="../sui-framework/deny_list.md#0x2_deny_list_ENotDenied">ENotDenied</a>);
    denied_addresses.<a href="../sui-framework/deny_list.md#0x2_deny_list_remove">remove</a>(&addr);
    <b>let</b> denied_count = &<b>mut</b> list.denied_count[addr];
    *denied_count = *denied_count - 1;
    <b>if</b> (*denied_count == 0) {
        list.denied_count.<a href="../sui-framework/deny_list.md#0x2_deny_list_remove">remove</a>(addr);
    }
}
</code></pre>



</details>

<a name="0x2_deny_list_contains"></a>

## Function `contains`

Returns true iff the given address is denied for the given type.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="../sui-framework/deny_list.md#0x2_deny_list_contains">contains</a>(<a href="../sui-framework/deny_list.md#0x2_deny_list">deny_list</a>: &<a href="../sui-framework/deny_list.md#0x2_deny_list_DenyList">deny_list::DenyList</a>, per_type_index: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>, type: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, addr: <b>address</b>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui-framework/deny_list.md#0x2_deny_list_contains">contains</a>(
    <a href="../sui-framework/deny_list.md#0x2_deny_list">deny_list</a>: &<a href="../sui-framework/deny_list.md#0x2_deny_list_DenyList">DenyList</a>,
    per_type_index: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>,
    `type`: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    addr: <b>address</b>,
): bool {
    <b>let</b> reserved = <a href="../sui-framework/deny_list.md#0x2_deny_list_RESERVED">RESERVED</a>;
    <b>if</b> (reserved.<a href="../sui-framework/deny_list.md#0x2_deny_list_contains">contains</a>(&addr)) <b>return</b> <b>false</b>;
    <a href="../sui-framework/deny_list.md#0x2_deny_list_per_type_list_contains">per_type_list_contains</a>(&<a href="../sui-framework/deny_list.md#0x2_deny_list">deny_list</a>.lists[per_type_index], `type`, addr)
}
</code></pre>



</details>

<a name="0x2_deny_list_per_type_list_contains"></a>

## Function `per_type_list_contains`



<pre><code><b>fun</b> <a href="../sui-framework/deny_list.md#0x2_deny_list_per_type_list_contains">per_type_list_contains</a>(list: &<a href="../sui-framework/deny_list.md#0x2_deny_list_PerTypeList">deny_list::PerTypeList</a>, type: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, addr: <b>address</b>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui-framework/deny_list.md#0x2_deny_list_per_type_list_contains">per_type_list_contains</a>(
    list: &<a href="../sui-framework/deny_list.md#0x2_deny_list_PerTypeList">PerTypeList</a>,
    `type`: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    addr: <b>address</b>,
): bool {
    <b>if</b> (!list.denied_count.<a href="../sui-framework/deny_list.md#0x2_deny_list_contains">contains</a>(addr)) <b>return</b> <b>false</b>;

    <b>let</b> denied_count = &list.denied_count[addr];
    <b>if</b> (*denied_count == 0) <b>return</b> <b>false</b>;

    <b>if</b> (!list.denied_addresses.<a href="../sui-framework/deny_list.md#0x2_deny_list_contains">contains</a>(`type`)) <b>return</b> <b>false</b>;

    <b>let</b> denied_addresses = &list.denied_addresses[`type`];
    denied_addresses.<a href="../sui-framework/deny_list.md#0x2_deny_list_contains">contains</a>(&addr)
}
</code></pre>



</details>

<a name="0x2_deny_list_create"></a>

## Function `create`

Creation of the deny list object is restricted to the system address
via a system transaction.


<pre><code><b>fun</b> <a href="../sui-framework/deny_list.md#0x2_deny_list_create">create</a>(ctx: &<b>mut</b> <a href="../sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui-framework/deny_list.md#0x2_deny_list_create">create</a>(ctx: &<b>mut</b> TxContext) {
    <b>assert</b>!(ctx.sender() == @0x0, <a href="../sui-framework/deny_list.md#0x2_deny_list_ENotSystemAddress">ENotSystemAddress</a>);

    <b>let</b> <b>mut</b> lists = <a href="../sui-framework/bag.md#0x2_bag_new">bag::new</a>(ctx);
    lists.<a href="../sui-framework/deny_list.md#0x2_deny_list_add">add</a>(<a href="../sui-framework/deny_list.md#0x2_deny_list_COIN_INDEX">COIN_INDEX</a>, <a href="../sui-framework/deny_list.md#0x2_deny_list_per_type_list">per_type_list</a>(ctx));
    <b>let</b> deny_list_object = <a href="../sui-framework/deny_list.md#0x2_deny_list_DenyList">DenyList</a> {
        id: <a href="../sui-framework/object.md#0x2_object_sui_deny_list_object_id">object::sui_deny_list_object_id</a>(),
        lists,
    };
    <a href="../sui-framework/transfer.md#0x2_transfer_share_object">transfer::share_object</a>(deny_list_object);
}
</code></pre>



</details>

<a name="0x2_deny_list_per_type_list"></a>

## Function `per_type_list`



<pre><code><b>fun</b> <a href="../sui-framework/deny_list.md#0x2_deny_list_per_type_list">per_type_list</a>(ctx: &<b>mut</b> <a href="../sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="../sui-framework/deny_list.md#0x2_deny_list_PerTypeList">deny_list::PerTypeList</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui-framework/deny_list.md#0x2_deny_list_per_type_list">per_type_list</a>(ctx: &<b>mut</b> TxContext): <a href="../sui-framework/deny_list.md#0x2_deny_list_PerTypeList">PerTypeList</a> {
    <a href="../sui-framework/deny_list.md#0x2_deny_list_PerTypeList">PerTypeList</a> {
        id: <a href="../sui-framework/object.md#0x2_object_new">object::new</a>(ctx),
        denied_count: <a href="../sui-framework/table.md#0x2_table_new">table::new</a>(ctx),
        denied_addresses: <a href="../sui-framework/table.md#0x2_table_new">table::new</a>(ctx),
    }
}
</code></pre>



</details>
