---
title: Module `sui::deny_list`
---

Defines the <code><a href="../sui/deny_list.md#sui_deny_list_DenyList">DenyList</a></code> type. The <code><a href="../sui/deny_list.md#sui_deny_list_DenyList">DenyList</a></code> shared object is used to restrict access to
instances of certain core types from being used as inputs by specified addresses in the deny
list.


-  [Struct `DenyList`](#sui_deny_list_DenyList)
-  [Struct `ConfigWriteCap`](#sui_deny_list_ConfigWriteCap)
-  [Struct `ConfigKey`](#sui_deny_list_ConfigKey)
-  [Struct `AddressKey`](#sui_deny_list_AddressKey)
-  [Struct `GlobalPauseKey`](#sui_deny_list_GlobalPauseKey)
-  [Struct `PerTypeConfigCreated`](#sui_deny_list_PerTypeConfigCreated)
-  [Struct `PerTypeList`](#sui_deny_list_PerTypeList)
-  [Constants](#@Constants_0)
-  [Function `v2_add`](#sui_deny_list_v2_add)
-  [Function `v2_remove`](#sui_deny_list_v2_remove)
-  [Function `v2_contains_current_epoch`](#sui_deny_list_v2_contains_current_epoch)
-  [Function `v2_contains_next_epoch`](#sui_deny_list_v2_contains_next_epoch)
-  [Function `v2_enable_global_pause`](#sui_deny_list_v2_enable_global_pause)
-  [Function `v2_disable_global_pause`](#sui_deny_list_v2_disable_global_pause)
-  [Function `v2_is_global_pause_enabled_current_epoch`](#sui_deny_list_v2_is_global_pause_enabled_current_epoch)
-  [Function `v2_is_global_pause_enabled_next_epoch`](#sui_deny_list_v2_is_global_pause_enabled_next_epoch)
-  [Function `migrate_v1_to_v2`](#sui_deny_list_migrate_v1_to_v2)
-  [Function `add_per_type_config`](#sui_deny_list_add_per_type_config)
-  [Function `borrow_per_type_config_mut`](#sui_deny_list_borrow_per_type_config_mut)
-  [Function `borrow_per_type_config`](#sui_deny_list_borrow_per_type_config)
-  [Function `per_type_exists`](#sui_deny_list_per_type_exists)
-  [Macro function `per_type_config_entry`](#sui_deny_list_per_type_config_entry)
-  [Function `v1_add`](#sui_deny_list_v1_add)
-  [Function `v1_per_type_list_add`](#sui_deny_list_v1_per_type_list_add)
-  [Function `v1_remove`](#sui_deny_list_v1_remove)
-  [Function `v1_per_type_list_remove`](#sui_deny_list_v1_per_type_list_remove)
-  [Function `v1_contains`](#sui_deny_list_v1_contains)
-  [Function `v1_per_type_list_contains`](#sui_deny_list_v1_per_type_list_contains)
-  [Function `create`](#sui_deny_list_create)
-  [Function `per_type_list`](#sui_deny_list_per_type_list)


<pre><code><b>use</b> <a href="../std/ascii.md#std_ascii">std::ascii</a>;
<b>use</b> <a href="../std/bcs.md#std_bcs">std::bcs</a>;
<b>use</b> <a href="../std/option.md#std_option">std::option</a>;
<b>use</b> <a href="../std/string.md#std_string">std::string</a>;
<b>use</b> <a href="../std/vector.md#std_vector">std::vector</a>;
<b>use</b> <a href="../sui/address.md#sui_address">sui::address</a>;
<b>use</b> <a href="../sui/bag.md#sui_bag">sui::bag</a>;
<b>use</b> <a href="../sui/config.md#sui_config">sui::config</a>;
<b>use</b> <a href="../sui/dynamic_field.md#sui_dynamic_field">sui::dynamic_field</a>;
<b>use</b> <a href="../sui/dynamic_object_field.md#sui_dynamic_object_field">sui::dynamic_object_field</a>;
<b>use</b> <a href="../sui/event.md#sui_event">sui::event</a>;
<b>use</b> <a href="../sui/hex.md#sui_hex">sui::hex</a>;
<b>use</b> <a href="../sui/object.md#sui_object">sui::object</a>;
<b>use</b> <a href="../sui/table.md#sui_table">sui::table</a>;
<b>use</b> <a href="../sui/transfer.md#sui_transfer">sui::transfer</a>;
<b>use</b> <a href="../sui/tx_context.md#sui_tx_context">sui::tx_context</a>;
<b>use</b> <a href="../sui/vec_set.md#sui_vec_set">sui::vec_set</a>;
</code></pre>



<a name="sui_deny_list_DenyList"></a>

## Struct `DenyList`

A shared object that stores the addresses that are blocked for a given core type.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/deny_list.md#sui_deny_list_DenyList">DenyList</a> <b>has</b> key
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
<code>lists: <a href="../sui/bag.md#sui_bag_Bag">sui::bag::Bag</a></code>
</dt>
<dd>
 The individual deny lists.
</dd>
</dl>


</details>

<a name="sui_deny_list_ConfigWriteCap"></a>

## Struct `ConfigWriteCap`

The capability used to write to the deny list config. Ensures that the Configs for the
DenyList are modified only by this module.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/deny_list.md#sui_deny_list_ConfigWriteCap">ConfigWriteCap</a> <b>has</b> drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
</dl>


</details>

<a name="sui_deny_list_ConfigKey"></a>

## Struct `ConfigKey`

The dynamic object field key used to store the <code>Config</code> for a given type, essentially a
<code>(per_type_index, per_type_key)</code> pair.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/deny_list.md#sui_deny_list_ConfigKey">ConfigKey</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>per_type_index: u64</code>
</dt>
<dd>
</dd>
<dt>
<code>per_type_key: vector&lt;u8&gt;</code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="sui_deny_list_AddressKey"></a>

## Struct `AddressKey`

The setting key used to store the deny list for a given address in the <code>Config</code>.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/deny_list.md#sui_deny_list_AddressKey">AddressKey</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>0: <b>address</b></code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="sui_deny_list_GlobalPauseKey"></a>

## Struct `GlobalPauseKey`

The setting key used to store the global pause setting in the <code>Config</code>.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/deny_list.md#sui_deny_list_GlobalPauseKey">GlobalPauseKey</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
</dl>


</details>

<a name="sui_deny_list_PerTypeConfigCreated"></a>

## Struct `PerTypeConfigCreated`

The event emitted when a new <code>Config</code> is created for a given type. This can be useful for
tracking the <code>ID</code> of a type's <code>Config</code> object.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/deny_list.md#sui_deny_list_PerTypeConfigCreated">PerTypeConfigCreated</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>key: <a href="../sui/deny_list.md#sui_deny_list_ConfigKey">sui::deny_list::ConfigKey</a></code>
</dt>
<dd>
</dd>
<dt>
<code>config_id: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a></code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="sui_deny_list_PerTypeList"></a>

## Struct `PerTypeList`

Stores the addresses that are denied for a given core type.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/deny_list.md#sui_deny_list_PerTypeList">PerTypeList</a> <b>has</b> key, store
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
<code>denied_count: <a href="../sui/table.md#sui_table_Table">sui::table::Table</a>&lt;<b>address</b>, u64&gt;</code>
</dt>
<dd>
 Number of object types that have been banned for a given address.
 Used to quickly skip checks for most addresses.
</dd>
<dt>
<code>denied_addresses: <a href="../sui/table.md#sui_table_Table">sui::table::Table</a>&lt;vector&lt;u8&gt;, <a href="../sui/vec_set.md#sui_vec_set_VecSet">sui::vec_set::VecSet</a>&lt;<b>address</b>&gt;&gt;</code>
</dt>
<dd>
 Set of addresses that are banned for a given type.
 For example with <code><a href="../sui/coin.md#sui_coin_Coin">sui::coin::Coin</a></code>: If addresses A and B are banned from using
 "0...0123::my_coin::MY_COIN", this will be "0...0123::my_coin::MY_COIN" -> {A, B}.
</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="sui_deny_list_COIN_INDEX"></a>

The index into the deny list vector for the <code><a href="../sui/coin.md#sui_coin_Coin">sui::coin::Coin</a></code> type.


<pre><code><b>const</b> <a href="../sui/deny_list.md#sui_deny_list_COIN_INDEX">COIN_INDEX</a>: u64 = 0;
</code></pre>



<a name="sui_deny_list_EInvalidAddress"></a>

The specified address cannot be added to the deny list.


<pre><code><b>const</b> <a href="../sui/deny_list.md#sui_deny_list_EInvalidAddress">EInvalidAddress</a>: u64 = 1;
</code></pre>



<a name="sui_deny_list_ENotDenied"></a>

The specified address to be removed is not already in the deny list.


<pre><code><b>const</b> <a href="../sui/deny_list.md#sui_deny_list_ENotDenied">ENotDenied</a>: u64 = 1;
</code></pre>



<a name="sui_deny_list_ENotSystemAddress"></a>

Trying to create a deny list object when not called by the system address.


<pre><code><b>const</b> <a href="../sui/deny_list.md#sui_deny_list_ENotSystemAddress">ENotSystemAddress</a>: u64 = 0;
</code></pre>



<a name="sui_deny_list_RESERVED"></a>

These addresses are reserved and cannot be added to the deny list.
The addresses listed are well known package and object addresses. So it would be
meaningless to add them to the deny list.


<pre><code><b>const</b> <a href="../sui/deny_list.md#sui_deny_list_RESERVED">RESERVED</a>: vector&lt;<b>address</b>&gt; = vector[0x0, 0x1, 0x2, 0x3, 0x4, 0x5, 0x6, 0x7, 0x8, 0x9, 0xa, 0xb, 0xc, 0xd, 0xe, 0xf, 0x403, 0xdee9];
</code></pre>



<a name="sui_deny_list_v2_add"></a>

## Function `v2_add`



<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/deny_list.md#sui_deny_list_v2_add">v2_add</a>(<a href="../sui/deny_list.md#sui_deny_list">deny_list</a>: &<b>mut</b> <a href="../sui/deny_list.md#sui_deny_list_DenyList">sui::deny_list::DenyList</a>, per_type_index: u64, per_type_key: vector&lt;u8&gt;, addr: <b>address</b>, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/deny_list.md#sui_deny_list_v2_add">v2_add</a>(
    <a href="../sui/deny_list.md#sui_deny_list">deny_list</a>: &<b>mut</b> <a href="../sui/deny_list.md#sui_deny_list_DenyList">DenyList</a>,
    per_type_index: u64,
    per_type_key: vector&lt;u8&gt;,
    addr: <b>address</b>,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> per_type_config = <a href="../sui/deny_list.md#sui_deny_list">deny_list</a>.<a href="../sui/deny_list.md#sui_deny_list_per_type_config_entry">per_type_config_entry</a>!(per_type_index, per_type_key, ctx);
    <b>let</b> setting_name = <a href="../sui/deny_list.md#sui_deny_list_AddressKey">AddressKey</a>(addr);
    <b>let</b> next_epoch_entry = per_type_config.<b>entry</b>!&lt;_, <a href="../sui/deny_list.md#sui_deny_list_AddressKey">AddressKey</a>, bool&gt;(
        &<b>mut</b> <a href="../sui/deny_list.md#sui_deny_list_ConfigWriteCap">ConfigWriteCap</a>(),
        setting_name,
        |_deny_list, _cap, _ctx| <b>true</b>,
        ctx,
    );
    *next_epoch_entry = <b>true</b>;
}
</code></pre>



</details>

<a name="sui_deny_list_v2_remove"></a>

## Function `v2_remove`



<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/deny_list.md#sui_deny_list_v2_remove">v2_remove</a>(<a href="../sui/deny_list.md#sui_deny_list">deny_list</a>: &<b>mut</b> <a href="../sui/deny_list.md#sui_deny_list_DenyList">sui::deny_list::DenyList</a>, per_type_index: u64, per_type_key: vector&lt;u8&gt;, addr: <b>address</b>, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/deny_list.md#sui_deny_list_v2_remove">v2_remove</a>(
    <a href="../sui/deny_list.md#sui_deny_list">deny_list</a>: &<b>mut</b> <a href="../sui/deny_list.md#sui_deny_list_DenyList">DenyList</a>,
    per_type_index: u64,
    per_type_key: vector&lt;u8&gt;,
    addr: <b>address</b>,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> per_type_config = <a href="../sui/deny_list.md#sui_deny_list">deny_list</a>.<a href="../sui/deny_list.md#sui_deny_list_per_type_config_entry">per_type_config_entry</a>!(per_type_index, per_type_key, ctx);
    <b>let</b> setting_name = <a href="../sui/deny_list.md#sui_deny_list_AddressKey">AddressKey</a>(addr);
    per_type_config.remove_for_next_epoch&lt;_, <a href="../sui/deny_list.md#sui_deny_list_AddressKey">AddressKey</a>, bool&gt;(
        &<b>mut</b> <a href="../sui/deny_list.md#sui_deny_list_ConfigWriteCap">ConfigWriteCap</a>(),
        setting_name,
        ctx,
    );
}
</code></pre>



</details>

<a name="sui_deny_list_v2_contains_current_epoch"></a>

## Function `v2_contains_current_epoch`



<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/deny_list.md#sui_deny_list_v2_contains_current_epoch">v2_contains_current_epoch</a>(<a href="../sui/deny_list.md#sui_deny_list">deny_list</a>: &<a href="../sui/deny_list.md#sui_deny_list_DenyList">sui::deny_list::DenyList</a>, per_type_index: u64, per_type_key: vector&lt;u8&gt;, addr: <b>address</b>, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/deny_list.md#sui_deny_list_v2_contains_current_epoch">v2_contains_current_epoch</a>(
    <a href="../sui/deny_list.md#sui_deny_list">deny_list</a>: &<a href="../sui/deny_list.md#sui_deny_list_DenyList">DenyList</a>,
    per_type_index: u64,
    per_type_key: vector&lt;u8&gt;,
    addr: <b>address</b>,
    ctx: &TxContext,
): bool {
    <b>if</b> (!<a href="../sui/deny_list.md#sui_deny_list">deny_list</a>.<a href="../sui/deny_list.md#sui_deny_list_per_type_exists">per_type_exists</a>(per_type_index, per_type_key)) <b>return</b> <b>false</b>;
    <b>let</b> per_type_config = <a href="../sui/deny_list.md#sui_deny_list">deny_list</a>.<a href="../sui/deny_list.md#sui_deny_list_borrow_per_type_config">borrow_per_type_config</a>(per_type_index, per_type_key);
    <b>let</b> setting_name = <a href="../sui/deny_list.md#sui_deny_list_AddressKey">AddressKey</a>(addr);
    <a href="../sui/config.md#sui_config_read_setting">config::read_setting</a>(<a href="../sui/object.md#sui_object_id">object::id</a>(per_type_config), setting_name, ctx).destroy_or!(<b>false</b>)
}
</code></pre>



</details>

<a name="sui_deny_list_v2_contains_next_epoch"></a>

## Function `v2_contains_next_epoch`



<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/deny_list.md#sui_deny_list_v2_contains_next_epoch">v2_contains_next_epoch</a>(<a href="../sui/deny_list.md#sui_deny_list">deny_list</a>: &<a href="../sui/deny_list.md#sui_deny_list_DenyList">sui::deny_list::DenyList</a>, per_type_index: u64, per_type_key: vector&lt;u8&gt;, addr: <b>address</b>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/deny_list.md#sui_deny_list_v2_contains_next_epoch">v2_contains_next_epoch</a>(
    <a href="../sui/deny_list.md#sui_deny_list">deny_list</a>: &<a href="../sui/deny_list.md#sui_deny_list_DenyList">DenyList</a>,
    per_type_index: u64,
    per_type_key: vector&lt;u8&gt;,
    addr: <b>address</b>,
): bool {
    <b>if</b> (!<a href="../sui/deny_list.md#sui_deny_list">deny_list</a>.<a href="../sui/deny_list.md#sui_deny_list_per_type_exists">per_type_exists</a>(per_type_index, per_type_key)) <b>return</b> <b>false</b>;
    <b>let</b> per_type_config = <a href="../sui/deny_list.md#sui_deny_list">deny_list</a>.<a href="../sui/deny_list.md#sui_deny_list_borrow_per_type_config">borrow_per_type_config</a>(per_type_index, per_type_key);
    <b>let</b> setting_name = <a href="../sui/deny_list.md#sui_deny_list_AddressKey">AddressKey</a>(addr);
    per_type_config.read_setting_for_next_epoch(setting_name).destroy_or!(<b>false</b>)
}
</code></pre>



</details>

<a name="sui_deny_list_v2_enable_global_pause"></a>

## Function `v2_enable_global_pause`



<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/deny_list.md#sui_deny_list_v2_enable_global_pause">v2_enable_global_pause</a>(<a href="../sui/deny_list.md#sui_deny_list">deny_list</a>: &<b>mut</b> <a href="../sui/deny_list.md#sui_deny_list_DenyList">sui::deny_list::DenyList</a>, per_type_index: u64, per_type_key: vector&lt;u8&gt;, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/deny_list.md#sui_deny_list_v2_enable_global_pause">v2_enable_global_pause</a>(
    <a href="../sui/deny_list.md#sui_deny_list">deny_list</a>: &<b>mut</b> <a href="../sui/deny_list.md#sui_deny_list_DenyList">DenyList</a>,
    per_type_index: u64,
    per_type_key: vector&lt;u8&gt;,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> per_type_config = <a href="../sui/deny_list.md#sui_deny_list">deny_list</a>.<a href="../sui/deny_list.md#sui_deny_list_per_type_config_entry">per_type_config_entry</a>!(per_type_index, per_type_key, ctx);
    <b>let</b> setting_name = <a href="../sui/deny_list.md#sui_deny_list_GlobalPauseKey">GlobalPauseKey</a>();
    <b>let</b> next_epoch_entry = per_type_config.<b>entry</b>!&lt;_, <a href="../sui/deny_list.md#sui_deny_list_GlobalPauseKey">GlobalPauseKey</a>, bool&gt;(
        &<b>mut</b> <a href="../sui/deny_list.md#sui_deny_list_ConfigWriteCap">ConfigWriteCap</a>(),
        setting_name,
        |_deny_list, _cap, _ctx| <b>true</b>,
        ctx,
    );
    *next_epoch_entry = <b>true</b>;
}
</code></pre>



</details>

<a name="sui_deny_list_v2_disable_global_pause"></a>

## Function `v2_disable_global_pause`



<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/deny_list.md#sui_deny_list_v2_disable_global_pause">v2_disable_global_pause</a>(<a href="../sui/deny_list.md#sui_deny_list">deny_list</a>: &<b>mut</b> <a href="../sui/deny_list.md#sui_deny_list_DenyList">sui::deny_list::DenyList</a>, per_type_index: u64, per_type_key: vector&lt;u8&gt;, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/deny_list.md#sui_deny_list_v2_disable_global_pause">v2_disable_global_pause</a>(
    <a href="../sui/deny_list.md#sui_deny_list">deny_list</a>: &<b>mut</b> <a href="../sui/deny_list.md#sui_deny_list_DenyList">DenyList</a>,
    per_type_index: u64,
    per_type_key: vector&lt;u8&gt;,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> per_type_config = <a href="../sui/deny_list.md#sui_deny_list">deny_list</a>.<a href="../sui/deny_list.md#sui_deny_list_per_type_config_entry">per_type_config_entry</a>!(per_type_index, per_type_key, ctx);
    <b>let</b> setting_name = <a href="../sui/deny_list.md#sui_deny_list_GlobalPauseKey">GlobalPauseKey</a>();
    per_type_config.remove_for_next_epoch&lt;_, <a href="../sui/deny_list.md#sui_deny_list_GlobalPauseKey">GlobalPauseKey</a>, bool&gt;(
        &<b>mut</b> <a href="../sui/deny_list.md#sui_deny_list_ConfigWriteCap">ConfigWriteCap</a>(),
        setting_name,
        ctx,
    );
}
</code></pre>



</details>

<a name="sui_deny_list_v2_is_global_pause_enabled_current_epoch"></a>

## Function `v2_is_global_pause_enabled_current_epoch`



<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/deny_list.md#sui_deny_list_v2_is_global_pause_enabled_current_epoch">v2_is_global_pause_enabled_current_epoch</a>(<a href="../sui/deny_list.md#sui_deny_list">deny_list</a>: &<a href="../sui/deny_list.md#sui_deny_list_DenyList">sui::deny_list::DenyList</a>, per_type_index: u64, per_type_key: vector&lt;u8&gt;, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/deny_list.md#sui_deny_list_v2_is_global_pause_enabled_current_epoch">v2_is_global_pause_enabled_current_epoch</a>(
    <a href="../sui/deny_list.md#sui_deny_list">deny_list</a>: &<a href="../sui/deny_list.md#sui_deny_list_DenyList">DenyList</a>,
    per_type_index: u64,
    per_type_key: vector&lt;u8&gt;,
    ctx: &TxContext,
): bool {
    <b>if</b> (!<a href="../sui/deny_list.md#sui_deny_list">deny_list</a>.<a href="../sui/deny_list.md#sui_deny_list_per_type_exists">per_type_exists</a>(per_type_index, per_type_key)) <b>return</b> <b>false</b>;
    <b>let</b> per_type_config = <a href="../sui/deny_list.md#sui_deny_list">deny_list</a>.<a href="../sui/deny_list.md#sui_deny_list_borrow_per_type_config">borrow_per_type_config</a>(per_type_index, per_type_key);
    <b>let</b> setting_name = <a href="../sui/deny_list.md#sui_deny_list_GlobalPauseKey">GlobalPauseKey</a>();
    <a href="../sui/config.md#sui_config_read_setting">config::read_setting</a>(<a href="../sui/object.md#sui_object_id">object::id</a>(per_type_config), setting_name, ctx).destroy_or!(<b>false</b>)
}
</code></pre>



</details>

<a name="sui_deny_list_v2_is_global_pause_enabled_next_epoch"></a>

## Function `v2_is_global_pause_enabled_next_epoch`



<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/deny_list.md#sui_deny_list_v2_is_global_pause_enabled_next_epoch">v2_is_global_pause_enabled_next_epoch</a>(<a href="../sui/deny_list.md#sui_deny_list">deny_list</a>: &<a href="../sui/deny_list.md#sui_deny_list_DenyList">sui::deny_list::DenyList</a>, per_type_index: u64, per_type_key: vector&lt;u8&gt;): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/deny_list.md#sui_deny_list_v2_is_global_pause_enabled_next_epoch">v2_is_global_pause_enabled_next_epoch</a>(
    <a href="../sui/deny_list.md#sui_deny_list">deny_list</a>: &<a href="../sui/deny_list.md#sui_deny_list_DenyList">DenyList</a>,
    per_type_index: u64,
    per_type_key: vector&lt;u8&gt;,
): bool {
    <b>if</b> (!<a href="../sui/deny_list.md#sui_deny_list">deny_list</a>.<a href="../sui/deny_list.md#sui_deny_list_per_type_exists">per_type_exists</a>(per_type_index, per_type_key)) <b>return</b> <b>false</b>;
    <b>let</b> per_type_config = <a href="../sui/deny_list.md#sui_deny_list">deny_list</a>.<a href="../sui/deny_list.md#sui_deny_list_borrow_per_type_config">borrow_per_type_config</a>(per_type_index, per_type_key);
    <b>let</b> setting_name = <a href="../sui/deny_list.md#sui_deny_list_GlobalPauseKey">GlobalPauseKey</a>();
    per_type_config.read_setting_for_next_epoch(setting_name).destroy_or!(<b>false</b>)
}
</code></pre>



</details>

<a name="sui_deny_list_migrate_v1_to_v2"></a>

## Function `migrate_v1_to_v2`



<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/deny_list.md#sui_deny_list_migrate_v1_to_v2">migrate_v1_to_v2</a>(<a href="../sui/deny_list.md#sui_deny_list">deny_list</a>: &<b>mut</b> <a href="../sui/deny_list.md#sui_deny_list_DenyList">sui::deny_list::DenyList</a>, per_type_index: u64, per_type_key: vector&lt;u8&gt;, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/deny_list.md#sui_deny_list_migrate_v1_to_v2">migrate_v1_to_v2</a>(
    <a href="../sui/deny_list.md#sui_deny_list">deny_list</a>: &<b>mut</b> <a href="../sui/deny_list.md#sui_deny_list_DenyList">DenyList</a>,
    per_type_index: u64,
    per_type_key: vector&lt;u8&gt;,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> bag_entry: &<b>mut</b> <a href="../sui/deny_list.md#sui_deny_list_PerTypeList">PerTypeList</a> = &<b>mut</b> <a href="../sui/deny_list.md#sui_deny_list">deny_list</a>.lists[per_type_index];
    <b>let</b> elements = <b>if</b> (!bag_entry.denied_addresses.contains(per_type_key)) vector[]
    <b>else</b> bag_entry.denied_addresses.remove(per_type_key).into_keys();
    elements.do_ref!(|addr| {
        <b>let</b> addr = *addr;
        <b>let</b> denied_count = &<b>mut</b> bag_entry.denied_count[addr];
        *denied_count = *denied_count - 1;
        <b>if</b> (*denied_count == 0) {
            bag_entry.denied_count.remove(addr);
        }
    });
    <b>let</b> per_type_config = <a href="../sui/deny_list.md#sui_deny_list">deny_list</a>.<a href="../sui/deny_list.md#sui_deny_list_per_type_config_entry">per_type_config_entry</a>!(per_type_index, per_type_key, ctx);
    elements.do!(|addr| {
        <b>let</b> setting_name = <a href="../sui/deny_list.md#sui_deny_list_AddressKey">AddressKey</a>(addr);
        <b>let</b> next_epoch_entry = per_type_config.<b>entry</b>!&lt;_, <a href="../sui/deny_list.md#sui_deny_list_AddressKey">AddressKey</a>, bool&gt;(
            &<b>mut</b> <a href="../sui/deny_list.md#sui_deny_list_ConfigWriteCap">ConfigWriteCap</a>(),
            setting_name,
            |_deny_list, _cap, _ctx| <b>true</b>,
            ctx,
        );
        *next_epoch_entry = <b>true</b>;
    });
}
</code></pre>



</details>

<a name="sui_deny_list_add_per_type_config"></a>

## Function `add_per_type_config`



<pre><code><b>fun</b> <a href="../sui/deny_list.md#sui_deny_list_add_per_type_config">add_per_type_config</a>(<a href="../sui/deny_list.md#sui_deny_list">deny_list</a>: &<b>mut</b> <a href="../sui/deny_list.md#sui_deny_list_DenyList">sui::deny_list::DenyList</a>, per_type_index: u64, per_type_key: vector&lt;u8&gt;, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/deny_list.md#sui_deny_list_add_per_type_config">add_per_type_config</a>(
    <a href="../sui/deny_list.md#sui_deny_list">deny_list</a>: &<b>mut</b> <a href="../sui/deny_list.md#sui_deny_list_DenyList">DenyList</a>,
    per_type_index: u64,
    per_type_key: vector&lt;u8&gt;,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> key = <a href="../sui/deny_list.md#sui_deny_list_ConfigKey">ConfigKey</a> { per_type_index, per_type_key };
    <b>let</b> <a href="../sui/config.md#sui_config">config</a> = <a href="../sui/config.md#sui_config_new">config::new</a>(&<b>mut</b> <a href="../sui/deny_list.md#sui_deny_list_ConfigWriteCap">ConfigWriteCap</a>(), ctx);
    <b>let</b> config_id = <a href="../sui/object.md#sui_object_id">object::id</a>(&<a href="../sui/config.md#sui_config">config</a>);
    ofield::internal_add(&<b>mut</b> <a href="../sui/deny_list.md#sui_deny_list">deny_list</a>.id, key, <a href="../sui/config.md#sui_config">config</a>);
    <a href="../sui/event.md#sui_event_emit">sui::event::emit</a>(<a href="../sui/deny_list.md#sui_deny_list_PerTypeConfigCreated">PerTypeConfigCreated</a> { key, config_id });
}
</code></pre>



</details>

<a name="sui_deny_list_borrow_per_type_config_mut"></a>

## Function `borrow_per_type_config_mut`



<pre><code><b>fun</b> <a href="../sui/deny_list.md#sui_deny_list_borrow_per_type_config_mut">borrow_per_type_config_mut</a>(<a href="../sui/deny_list.md#sui_deny_list">deny_list</a>: &<b>mut</b> <a href="../sui/deny_list.md#sui_deny_list_DenyList">sui::deny_list::DenyList</a>, per_type_index: u64, per_type_key: vector&lt;u8&gt;): &<b>mut</b> <a href="../sui/config.md#sui_config_Config">sui::config::Config</a>&lt;<a href="../sui/deny_list.md#sui_deny_list_ConfigWriteCap">sui::deny_list::ConfigWriteCap</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/deny_list.md#sui_deny_list_borrow_per_type_config_mut">borrow_per_type_config_mut</a>(
    <a href="../sui/deny_list.md#sui_deny_list">deny_list</a>: &<b>mut</b> <a href="../sui/deny_list.md#sui_deny_list_DenyList">DenyList</a>,
    per_type_index: u64,
    per_type_key: vector&lt;u8&gt;,
): &<b>mut</b> Config&lt;<a href="../sui/deny_list.md#sui_deny_list_ConfigWriteCap">ConfigWriteCap</a>&gt; {
    <b>let</b> key = <a href="../sui/deny_list.md#sui_deny_list_ConfigKey">ConfigKey</a> { per_type_index, per_type_key };
    ofield::internal_borrow_mut(&<b>mut</b> <a href="../sui/deny_list.md#sui_deny_list">deny_list</a>.id, key)
}
</code></pre>



</details>

<a name="sui_deny_list_borrow_per_type_config"></a>

## Function `borrow_per_type_config`



<pre><code><b>fun</b> <a href="../sui/deny_list.md#sui_deny_list_borrow_per_type_config">borrow_per_type_config</a>(<a href="../sui/deny_list.md#sui_deny_list">deny_list</a>: &<a href="../sui/deny_list.md#sui_deny_list_DenyList">sui::deny_list::DenyList</a>, per_type_index: u64, per_type_key: vector&lt;u8&gt;): &<a href="../sui/config.md#sui_config_Config">sui::config::Config</a>&lt;<a href="../sui/deny_list.md#sui_deny_list_ConfigWriteCap">sui::deny_list::ConfigWriteCap</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/deny_list.md#sui_deny_list_borrow_per_type_config">borrow_per_type_config</a>(
    <a href="../sui/deny_list.md#sui_deny_list">deny_list</a>: &<a href="../sui/deny_list.md#sui_deny_list_DenyList">DenyList</a>,
    per_type_index: u64,
    per_type_key: vector&lt;u8&gt;,
): &Config&lt;<a href="../sui/deny_list.md#sui_deny_list_ConfigWriteCap">ConfigWriteCap</a>&gt; {
    <b>let</b> key = <a href="../sui/deny_list.md#sui_deny_list_ConfigKey">ConfigKey</a> { per_type_index, per_type_key };
    ofield::internal_borrow(&<a href="../sui/deny_list.md#sui_deny_list">deny_list</a>.id, key)
}
</code></pre>



</details>

<a name="sui_deny_list_per_type_exists"></a>

## Function `per_type_exists`



<pre><code><b>fun</b> <a href="../sui/deny_list.md#sui_deny_list_per_type_exists">per_type_exists</a>(<a href="../sui/deny_list.md#sui_deny_list">deny_list</a>: &<a href="../sui/deny_list.md#sui_deny_list_DenyList">sui::deny_list::DenyList</a>, per_type_index: u64, per_type_key: vector&lt;u8&gt;): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/deny_list.md#sui_deny_list_per_type_exists">per_type_exists</a>(<a href="../sui/deny_list.md#sui_deny_list">deny_list</a>: &<a href="../sui/deny_list.md#sui_deny_list_DenyList">DenyList</a>, per_type_index: u64, per_type_key: vector&lt;u8&gt;): bool {
    <b>let</b> key = <a href="../sui/deny_list.md#sui_deny_list_ConfigKey">ConfigKey</a> { per_type_index, per_type_key };
    ofield::exists_(&<a href="../sui/deny_list.md#sui_deny_list">deny_list</a>.id, key)
}
</code></pre>



</details>

<a name="sui_deny_list_per_type_config_entry"></a>

## Macro function `per_type_config_entry`



<pre><code><b>macro</b> <b>fun</b> <a href="../sui/deny_list.md#sui_deny_list_per_type_config_entry">per_type_config_entry</a>($<a href="../sui/deny_list.md#sui_deny_list">deny_list</a>: &<b>mut</b> <a href="../sui/deny_list.md#sui_deny_list_DenyList">sui::deny_list::DenyList</a>, $per_type_index: u64, $per_type_key: vector&lt;u8&gt;, $ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): &<b>mut</b> <a href="../sui/config.md#sui_config_Config">sui::config::Config</a>&lt;<a href="../sui/deny_list.md#sui_deny_list_ConfigWriteCap">sui::deny_list::ConfigWriteCap</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>macro</b> <b>fun</b> <a href="../sui/deny_list.md#sui_deny_list_per_type_config_entry">per_type_config_entry</a>(
    $<a href="../sui/deny_list.md#sui_deny_list">deny_list</a>: &<b>mut</b> <a href="../sui/deny_list.md#sui_deny_list_DenyList">DenyList</a>,
    $per_type_index: u64,
    $per_type_key: vector&lt;u8&gt;,
    $ctx: &<b>mut</b> TxContext,
): &<b>mut</b> Config&lt;<a href="../sui/deny_list.md#sui_deny_list_ConfigWriteCap">ConfigWriteCap</a>&gt; {
    <b>let</b> <a href="../sui/deny_list.md#sui_deny_list">deny_list</a> = $<a href="../sui/deny_list.md#sui_deny_list">deny_list</a>;
    <b>let</b> per_type_index = $per_type_index;
    <b>let</b> per_type_key = $per_type_key;
    <b>let</b> ctx = $ctx;
    <b>if</b> (!<a href="../sui/deny_list.md#sui_deny_list">deny_list</a>.<a href="../sui/deny_list.md#sui_deny_list_per_type_exists">per_type_exists</a>(per_type_index, per_type_key)) {
        <a href="../sui/deny_list.md#sui_deny_list">deny_list</a>.<a href="../sui/deny_list.md#sui_deny_list_add_per_type_config">add_per_type_config</a>(per_type_index, per_type_key, ctx);
    };
    <a href="../sui/deny_list.md#sui_deny_list">deny_list</a>.<a href="../sui/deny_list.md#sui_deny_list_borrow_per_type_config_mut">borrow_per_type_config_mut</a>(per_type_index, per_type_key)
}
</code></pre>



</details>

<a name="sui_deny_list_v1_add"></a>

## Function `v1_add`

Adds the given address to the deny list of the specified type, preventing it
from interacting with instances of that type as an input to a transaction. For coins,
the type specified is the type of the coin, not the coin type itself. For example,
"00...0123::my_coin::MY_COIN" would be the type, not "00...02::coin::Coin".


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/deny_list.md#sui_deny_list_v1_add">v1_add</a>(<a href="../sui/deny_list.md#sui_deny_list">deny_list</a>: &<b>mut</b> <a href="../sui/deny_list.md#sui_deny_list_DenyList">sui::deny_list::DenyList</a>, per_type_index: u64, type: vector&lt;u8&gt;, addr: <b>address</b>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/deny_list.md#sui_deny_list_v1_add">v1_add</a>(
    <a href="../sui/deny_list.md#sui_deny_list">deny_list</a>: &<b>mut</b> <a href="../sui/deny_list.md#sui_deny_list_DenyList">DenyList</a>,
    per_type_index: u64,
    `type`: vector&lt;u8&gt;,
    addr: <b>address</b>,
) {
    <b>let</b> reserved = <a href="../sui/deny_list.md#sui_deny_list_RESERVED">RESERVED</a>;
    <b>assert</b>!(!reserved.contains(&addr), <a href="../sui/deny_list.md#sui_deny_list_EInvalidAddress">EInvalidAddress</a>);
    <b>let</b> bag_entry: &<b>mut</b> <a href="../sui/deny_list.md#sui_deny_list_PerTypeList">PerTypeList</a> = &<b>mut</b> <a href="../sui/deny_list.md#sui_deny_list">deny_list</a>.lists[per_type_index];
    bag_entry.<a href="../sui/deny_list.md#sui_deny_list_v1_per_type_list_add">v1_per_type_list_add</a>(`type`, addr)
}
</code></pre>



</details>

<a name="sui_deny_list_v1_per_type_list_add"></a>

## Function `v1_per_type_list_add`



<pre><code><b>fun</b> <a href="../sui/deny_list.md#sui_deny_list_v1_per_type_list_add">v1_per_type_list_add</a>(list: &<b>mut</b> <a href="../sui/deny_list.md#sui_deny_list_PerTypeList">sui::deny_list::PerTypeList</a>, type: vector&lt;u8&gt;, addr: <b>address</b>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/deny_list.md#sui_deny_list_v1_per_type_list_add">v1_per_type_list_add</a>(list: &<b>mut</b> <a href="../sui/deny_list.md#sui_deny_list_PerTypeList">PerTypeList</a>, `type`: vector&lt;u8&gt;, addr: <b>address</b>) {
    <b>if</b> (!list.denied_addresses.contains(`type`)) {
        list.denied_addresses.add(`type`, <a href="../sui/vec_set.md#sui_vec_set_empty">vec_set::empty</a>());
    };
    <b>let</b> denied_addresses = &<b>mut</b> list.denied_addresses[`type`];
    <b>let</b> already_denied = denied_addresses.contains(&addr);
    <b>if</b> (already_denied) <b>return</b>;
    denied_addresses.insert(addr);
    <b>if</b> (!list.denied_count.contains(addr)) {
        list.denied_count.add(addr, 0);
    };
    <b>let</b> denied_count = &<b>mut</b> list.denied_count[addr];
    *denied_count = *denied_count + 1;
}
</code></pre>



</details>

<a name="sui_deny_list_v1_remove"></a>

## Function `v1_remove`

Removes a previously denied address from the list.
Aborts with <code><a href="../sui/deny_list.md#sui_deny_list_ENotDenied">ENotDenied</a></code> if the address is not on the list.


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/deny_list.md#sui_deny_list_v1_remove">v1_remove</a>(<a href="../sui/deny_list.md#sui_deny_list">deny_list</a>: &<b>mut</b> <a href="../sui/deny_list.md#sui_deny_list_DenyList">sui::deny_list::DenyList</a>, per_type_index: u64, type: vector&lt;u8&gt;, addr: <b>address</b>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/deny_list.md#sui_deny_list_v1_remove">v1_remove</a>(
    <a href="../sui/deny_list.md#sui_deny_list">deny_list</a>: &<b>mut</b> <a href="../sui/deny_list.md#sui_deny_list_DenyList">DenyList</a>,
    per_type_index: u64,
    `type`: vector&lt;u8&gt;,
    addr: <b>address</b>,
) {
    <b>let</b> reserved = <a href="../sui/deny_list.md#sui_deny_list_RESERVED">RESERVED</a>;
    <b>assert</b>!(!reserved.contains(&addr), <a href="../sui/deny_list.md#sui_deny_list_EInvalidAddress">EInvalidAddress</a>);
    <b>let</b> bag_entry: &<b>mut</b> <a href="../sui/deny_list.md#sui_deny_list_PerTypeList">PerTypeList</a> = &<b>mut</b> <a href="../sui/deny_list.md#sui_deny_list">deny_list</a>.lists[per_type_index];
    bag_entry.<a href="../sui/deny_list.md#sui_deny_list_v1_per_type_list_remove">v1_per_type_list_remove</a>(`type`, addr)
}
</code></pre>



</details>

<a name="sui_deny_list_v1_per_type_list_remove"></a>

## Function `v1_per_type_list_remove`



<pre><code><b>fun</b> <a href="../sui/deny_list.md#sui_deny_list_v1_per_type_list_remove">v1_per_type_list_remove</a>(list: &<b>mut</b> <a href="../sui/deny_list.md#sui_deny_list_PerTypeList">sui::deny_list::PerTypeList</a>, type: vector&lt;u8&gt;, addr: <b>address</b>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/deny_list.md#sui_deny_list_v1_per_type_list_remove">v1_per_type_list_remove</a>(list: &<b>mut</b> <a href="../sui/deny_list.md#sui_deny_list_PerTypeList">PerTypeList</a>, `type`: vector&lt;u8&gt;, addr: <b>address</b>) {
    <b>let</b> denied_addresses = &<b>mut</b> list.denied_addresses[`type`];
    <b>assert</b>!(denied_addresses.contains(&addr), <a href="../sui/deny_list.md#sui_deny_list_ENotDenied">ENotDenied</a>);
    denied_addresses.remove(&addr);
    <b>let</b> denied_count = &<b>mut</b> list.denied_count[addr];
    *denied_count = *denied_count - 1;
    <b>if</b> (*denied_count == 0) {
        list.denied_count.remove(addr);
    }
}
</code></pre>



</details>

<a name="sui_deny_list_v1_contains"></a>

## Function `v1_contains`

Returns true iff the given address is denied for the given type.


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/deny_list.md#sui_deny_list_v1_contains">v1_contains</a>(<a href="../sui/deny_list.md#sui_deny_list">deny_list</a>: &<a href="../sui/deny_list.md#sui_deny_list_DenyList">sui::deny_list::DenyList</a>, per_type_index: u64, type: vector&lt;u8&gt;, addr: <b>address</b>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/deny_list.md#sui_deny_list_v1_contains">v1_contains</a>(
    <a href="../sui/deny_list.md#sui_deny_list">deny_list</a>: &<a href="../sui/deny_list.md#sui_deny_list_DenyList">DenyList</a>,
    per_type_index: u64,
    `type`: vector&lt;u8&gt;,
    addr: <b>address</b>,
): bool {
    <b>let</b> reserved = <a href="../sui/deny_list.md#sui_deny_list_RESERVED">RESERVED</a>;
    <b>if</b> (reserved.contains(&addr)) <b>return</b> <b>false</b>;
    <b>let</b> bag_entry: &<a href="../sui/deny_list.md#sui_deny_list_PerTypeList">PerTypeList</a> = &<a href="../sui/deny_list.md#sui_deny_list">deny_list</a>.lists[per_type_index];
    bag_entry.<a href="../sui/deny_list.md#sui_deny_list_v1_per_type_list_contains">v1_per_type_list_contains</a>(`type`, addr)
}
</code></pre>



</details>

<a name="sui_deny_list_v1_per_type_list_contains"></a>

## Function `v1_per_type_list_contains`



<pre><code><b>fun</b> <a href="../sui/deny_list.md#sui_deny_list_v1_per_type_list_contains">v1_per_type_list_contains</a>(list: &<a href="../sui/deny_list.md#sui_deny_list_PerTypeList">sui::deny_list::PerTypeList</a>, type: vector&lt;u8&gt;, addr: <b>address</b>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/deny_list.md#sui_deny_list_v1_per_type_list_contains">v1_per_type_list_contains</a>(list: &<a href="../sui/deny_list.md#sui_deny_list_PerTypeList">PerTypeList</a>, `type`: vector&lt;u8&gt;, addr: <b>address</b>): bool {
    <b>if</b> (!list.denied_count.contains(addr)) <b>return</b> <b>false</b>;
    <b>let</b> denied_count = &list.denied_count[addr];
    <b>if</b> (*denied_count == 0) <b>return</b> <b>false</b>;
    <b>if</b> (!list.denied_addresses.contains(`type`)) <b>return</b> <b>false</b>;
    <b>let</b> denied_addresses = &list.denied_addresses[`type`];
    denied_addresses.contains(&addr)
}
</code></pre>



</details>

<a name="sui_deny_list_create"></a>

## Function `create`

Creation of the deny list object is restricted to the system address
via a system transaction.


<pre><code><b>fun</b> <a href="../sui/deny_list.md#sui_deny_list_create">create</a>(ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/deny_list.md#sui_deny_list_create">create</a>(ctx: &<b>mut</b> TxContext) {
    <b>assert</b>!(ctx.sender() == @0x0, <a href="../sui/deny_list.md#sui_deny_list_ENotSystemAddress">ENotSystemAddress</a>);
    <b>let</b> <b>mut</b> lists = <a href="../sui/bag.md#sui_bag_new">bag::new</a>(ctx);
    lists.add(<a href="../sui/deny_list.md#sui_deny_list_COIN_INDEX">COIN_INDEX</a>, <a href="../sui/deny_list.md#sui_deny_list_per_type_list">per_type_list</a>(ctx));
    <b>let</b> deny_list_object = <a href="../sui/deny_list.md#sui_deny_list_DenyList">DenyList</a> {
        id: <a href="../sui/object.md#sui_object_sui_deny_list_object_id">object::sui_deny_list_object_id</a>(),
        lists,
    };
    <a href="../sui/transfer.md#sui_transfer_share_object">transfer::share_object</a>(deny_list_object);
}
</code></pre>



</details>

<a name="sui_deny_list_per_type_list"></a>

## Function `per_type_list`



<pre><code><b>fun</b> <a href="../sui/deny_list.md#sui_deny_list_per_type_list">per_type_list</a>(ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <a href="../sui/deny_list.md#sui_deny_list_PerTypeList">sui::deny_list::PerTypeList</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/deny_list.md#sui_deny_list_per_type_list">per_type_list</a>(ctx: &<b>mut</b> TxContext): <a href="../sui/deny_list.md#sui_deny_list_PerTypeList">PerTypeList</a> {
    <a href="../sui/deny_list.md#sui_deny_list_PerTypeList">PerTypeList</a> {
        id: <a href="../sui/object.md#sui_object_new">object::new</a>(ctx),
        denied_count: <a href="../sui/table.md#sui_table_new">table::new</a>(ctx),
        denied_addresses: <a href="../sui/table.md#sui_table_new">table::new</a>(ctx),
    }
}
</code></pre>



</details>
