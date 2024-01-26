
<a name="0x2_deny_list"></a>

# Module `0x2::deny_list`

Defines the <code><a href="deny_list.md#0x2_deny_list_DenyList">DenyList</a></code> type. The <code><a href="deny_list.md#0x2_deny_list_DenyList">DenyList</a></code> shared object is used to restrict access to
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


<pre><code><b>use</b> <a href="bag.md#0x2_bag">0x2::bag</a>;
<b>use</b> <a href="object.md#0x2_object">0x2::object</a>;
<b>use</b> <a href="table.md#0x2_table">0x2::table</a>;
<b>use</b> <a href="transfer.md#0x2_transfer">0x2::transfer</a>;
<b>use</b> <a href="tx_context.md#0x2_tx_context">0x2::tx_context</a>;
<b>use</b> <a href="vec_set.md#0x2_vec_set">0x2::vec_set</a>;
</code></pre>



<a name="0x2_deny_list_DenyList"></a>

## Resource `DenyList`

A shared object that stores the addresses that are blocked for a given core type.


<pre><code><b>struct</b> <a href="deny_list.md#0x2_deny_list_DenyList">DenyList</a> <b>has</b> key
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
<code>lists: <a href="bag.md#0x2_bag_Bag">bag::Bag</a></code>
</dt>
<dd>
 The individual deny lists.
</dd>
</dl>


</details>

<a name="0x2_deny_list_PerTypeList"></a>

## Resource `PerTypeList`

Stores the addresses that are denied for a given core type.


<pre><code><b>struct</b> <a href="deny_list.md#0x2_deny_list_PerTypeList">PerTypeList</a> <b>has</b> store, key
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
<code>denied_count: <a href="table.md#0x2_table_Table">table::Table</a>&lt;<b>address</b>, u64&gt;</code>
</dt>
<dd>
 Number of object types that have been banned for a given address.
 Used to quickly skip checks for most addresses.
</dd>
<dt>
<code>denied_addresses: <a href="table.md#0x2_table_Table">table::Table</a>&lt;<a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, <a href="vec_set.md#0x2_vec_set_VecSet">vec_set::VecSet</a>&lt;<b>address</b>&gt;&gt;</code>
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


<pre><code><b>const</b> <a href="deny_list.md#0x2_deny_list_ENotSystemAddress">ENotSystemAddress</a>: u64 = 0;
</code></pre>



<a name="0x2_deny_list_COIN_INDEX"></a>

The index into the deny list vector for the <code>sui::coin::Coin</code> type.


<pre><code><b>const</b> <a href="deny_list.md#0x2_deny_list_COIN_INDEX">COIN_INDEX</a>: u64 = 0;
</code></pre>



<a name="0x2_deny_list_ENotDenied"></a>

The specified address to be removed is not already in the deny list.


<pre><code><b>const</b> <a href="deny_list.md#0x2_deny_list_ENotDenied">ENotDenied</a>: u64 = 1;
</code></pre>



<a name="0x2_deny_list_add"></a>

## Function `add`

Adds the given address to the deny list of the specified type, preventing it
from interacting with instances of that type as an input to a transaction. For coins,
the type specified is the type of the coin, not the coin type itself. For example,
"00...0123::my_coin::MY_COIN" would be the type, not "00...02::coin::Coin".


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="deny_list.md#0x2_deny_list_add">add</a>(<a href="deny_list.md#0x2_deny_list">deny_list</a>: &<b>mut</b> <a href="deny_list.md#0x2_deny_list_DenyList">deny_list::DenyList</a>, per_type_index: u64, type: <a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, addr: <b>address</b>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="deny_list.md#0x2_deny_list_add">add</a>(
    <a href="deny_list.md#0x2_deny_list">deny_list</a>: &<b>mut</b> <a href="deny_list.md#0x2_deny_list_DenyList">DenyList</a>,
    per_type_index: u64,
    type: <a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    addr: <b>address</b>,
) {
    <a href="deny_list.md#0x2_deny_list_per_type_list_add">per_type_list_add</a>(<a href="bag.md#0x2_bag_borrow_mut">bag::borrow_mut</a>(&<b>mut</b> <a href="deny_list.md#0x2_deny_list">deny_list</a>.lists, per_type_index), type, addr)
}
</code></pre>



</details>

<a name="0x2_deny_list_per_type_list_add"></a>

## Function `per_type_list_add`



<pre><code><b>fun</b> <a href="deny_list.md#0x2_deny_list_per_type_list_add">per_type_list_add</a>(list: &<b>mut</b> <a href="deny_list.md#0x2_deny_list_PerTypeList">deny_list::PerTypeList</a>, type: <a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, addr: <b>address</b>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="deny_list.md#0x2_deny_list_per_type_list_add">per_type_list_add</a>(
    list: &<b>mut</b> <a href="deny_list.md#0x2_deny_list_PerTypeList">PerTypeList</a>,
    type: <a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    addr: <b>address</b>,
) {
    <b>if</b> (!<a href="table.md#0x2_table_contains">table::contains</a>(&list.denied_addresses, type)) {
        <a href="table.md#0x2_table_add">table::add</a>(&<b>mut</b> list.denied_addresses, type, <a href="vec_set.md#0x2_vec_set_empty">vec_set::empty</a>());
    };
    <b>let</b> denied_addresses = <a href="table.md#0x2_table_borrow_mut">table::borrow_mut</a>(&<b>mut</b> list.denied_addresses, type);
    <b>let</b> already_denied = <a href="vec_set.md#0x2_vec_set_contains">vec_set::contains</a>(denied_addresses, &addr);
    <b>if</b> (already_denied) <b>return</b>;

    <a href="vec_set.md#0x2_vec_set_insert">vec_set::insert</a>(denied_addresses, addr);
    <b>if</b> (!<a href="table.md#0x2_table_contains">table::contains</a>(&list.denied_count, addr)) {
        <a href="table.md#0x2_table_add">table::add</a>(&<b>mut</b> list.denied_count, addr, 0);
    };
    <b>let</b> denied_count = <a href="table.md#0x2_table_borrow_mut">table::borrow_mut</a>(&<b>mut</b> list.denied_count, addr);
    *denied_count = *denied_count + 1;
}
</code></pre>



</details>

<a name="0x2_deny_list_remove"></a>

## Function `remove`

Removes a previously denied address from the list.
Aborts with <code><a href="deny_list.md#0x2_deny_list_ENotDenied">ENotDenied</a></code> if the address is not on the list.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="deny_list.md#0x2_deny_list_remove">remove</a>(<a href="deny_list.md#0x2_deny_list">deny_list</a>: &<b>mut</b> <a href="deny_list.md#0x2_deny_list_DenyList">deny_list::DenyList</a>, per_type_index: u64, type: <a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, addr: <b>address</b>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="deny_list.md#0x2_deny_list_remove">remove</a>(
    <a href="deny_list.md#0x2_deny_list">deny_list</a>: &<b>mut</b> <a href="deny_list.md#0x2_deny_list_DenyList">DenyList</a>,
    per_type_index: u64,
    type: <a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    addr: <b>address</b>,
) {
    <a href="deny_list.md#0x2_deny_list_per_type_list_remove">per_type_list_remove</a>(<a href="bag.md#0x2_bag_borrow_mut">bag::borrow_mut</a>(&<b>mut</b> <a href="deny_list.md#0x2_deny_list">deny_list</a>.lists, per_type_index), type, addr)
}
</code></pre>



</details>

<a name="0x2_deny_list_per_type_list_remove"></a>

## Function `per_type_list_remove`



<pre><code><b>fun</b> <a href="deny_list.md#0x2_deny_list_per_type_list_remove">per_type_list_remove</a>(list: &<b>mut</b> <a href="deny_list.md#0x2_deny_list_PerTypeList">deny_list::PerTypeList</a>, type: <a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, addr: <b>address</b>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="deny_list.md#0x2_deny_list_per_type_list_remove">per_type_list_remove</a>(
    list: &<b>mut</b> <a href="deny_list.md#0x2_deny_list_PerTypeList">PerTypeList</a>,
    type: <a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    addr: <b>address</b>,
) {
    <b>let</b> denied_addresses = <a href="table.md#0x2_table_borrow_mut">table::borrow_mut</a>(&<b>mut</b> list.denied_addresses, type);
    <b>assert</b>!(<a href="vec_set.md#0x2_vec_set_contains">vec_set::contains</a>(denied_addresses, &addr), <a href="deny_list.md#0x2_deny_list_ENotDenied">ENotDenied</a>);
    <a href="vec_set.md#0x2_vec_set_remove">vec_set::remove</a>(denied_addresses, &addr);
    <b>let</b> denied_count = <a href="table.md#0x2_table_borrow_mut">table::borrow_mut</a>(&<b>mut</b> list.denied_count, addr);
    *denied_count = *denied_count - 1;
    <b>if</b> (*denied_count == 0) {
        <a href="table.md#0x2_table_remove">table::remove</a>(&<b>mut</b> list.denied_count, addr);
    }
}
</code></pre>



</details>

<a name="0x2_deny_list_contains"></a>

## Function `contains`

Returns true iff the given address is denied for the given type.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="deny_list.md#0x2_deny_list_contains">contains</a>(<a href="deny_list.md#0x2_deny_list">deny_list</a>: &<a href="deny_list.md#0x2_deny_list_DenyList">deny_list::DenyList</a>, per_type_index: u64, type: <a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, addr: <b>address</b>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="deny_list.md#0x2_deny_list_contains">contains</a>(
    <a href="deny_list.md#0x2_deny_list">deny_list</a>: &<a href="deny_list.md#0x2_deny_list_DenyList">DenyList</a>,
    per_type_index: u64,
    type: <a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    addr: <b>address</b>,
): bool {
    <a href="deny_list.md#0x2_deny_list_per_type_list_contains">per_type_list_contains</a>(<a href="bag.md#0x2_bag_borrow">bag::borrow</a>(&<a href="deny_list.md#0x2_deny_list">deny_list</a>.lists, per_type_index), type, addr)
}
</code></pre>



</details>

<a name="0x2_deny_list_per_type_list_contains"></a>

## Function `per_type_list_contains`



<pre><code><b>fun</b> <a href="deny_list.md#0x2_deny_list_per_type_list_contains">per_type_list_contains</a>(list: &<a href="deny_list.md#0x2_deny_list_PerTypeList">deny_list::PerTypeList</a>, type: <a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, addr: <b>address</b>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="deny_list.md#0x2_deny_list_per_type_list_contains">per_type_list_contains</a>(
    list: &<a href="deny_list.md#0x2_deny_list_PerTypeList">PerTypeList</a>,
    type: <a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    addr: <b>address</b>,
): bool {
    <b>if</b> (!<a href="table.md#0x2_table_contains">table::contains</a>(&list.denied_count, addr)) <b>return</b> <b>false</b>;

    <b>let</b> denied_count = <a href="table.md#0x2_table_borrow">table::borrow</a>(&list.denied_count, addr);
    <b>if</b> (*denied_count == 0) <b>return</b> <b>false</b>;

    <b>if</b> (!<a href="table.md#0x2_table_contains">table::contains</a>(&list.denied_addresses, type)) <b>return</b> <b>false</b>;

    <b>let</b> denied_addresses = <a href="table.md#0x2_table_borrow">table::borrow</a>(&list.denied_addresses, type);
    <a href="vec_set.md#0x2_vec_set_contains">vec_set::contains</a>(denied_addresses, &addr)
}
</code></pre>



</details>

<a name="0x2_deny_list_create"></a>

## Function `create`

Creation of the deny list object is restricted to the system address
via a system transaction.


<pre><code><b>fun</b> <a href="deny_list.md#0x2_deny_list_create">create</a>(ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="deny_list.md#0x2_deny_list_create">create</a>(ctx: &<b>mut</b> TxContext) {
    <b>assert</b>!(<a href="tx_context.md#0x2_tx_context_sender">tx_context::sender</a>(ctx) == @0x0, <a href="deny_list.md#0x2_deny_list_ENotSystemAddress">ENotSystemAddress</a>);

    <b>let</b> lists = <a href="bag.md#0x2_bag_new">bag::new</a>(ctx);
    <a href="bag.md#0x2_bag_add">bag::add</a>(&<b>mut</b> lists, <a href="deny_list.md#0x2_deny_list_COIN_INDEX">COIN_INDEX</a>, <a href="deny_list.md#0x2_deny_list_per_type_list">per_type_list</a>(ctx));
    <b>let</b> deny_list_object = <a href="deny_list.md#0x2_deny_list_DenyList">DenyList</a> {
        id: <a href="object.md#0x2_object_sui_deny_list_object_id">object::sui_deny_list_object_id</a>(),
        lists,
    };
    <a href="transfer.md#0x2_transfer_share_object">transfer::share_object</a>(deny_list_object);
}
</code></pre>



</details>

<a name="0x2_deny_list_per_type_list"></a>

## Function `per_type_list`



<pre><code><b>fun</b> <a href="deny_list.md#0x2_deny_list_per_type_list">per_type_list</a>(ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="deny_list.md#0x2_deny_list_PerTypeList">deny_list::PerTypeList</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="deny_list.md#0x2_deny_list_per_type_list">per_type_list</a>(ctx: &<b>mut</b> TxContext): <a href="deny_list.md#0x2_deny_list_PerTypeList">PerTypeList</a> {
    <a href="deny_list.md#0x2_deny_list_PerTypeList">PerTypeList</a> {
        id: <a href="object.md#0x2_object_new">object::new</a>(ctx),
        denied_count: <a href="table.md#0x2_table_new">table::new</a>(ctx),
        denied_addresses: <a href="table.md#0x2_table_new">table::new</a>(ctx),
    }
}
</code></pre>



</details>
