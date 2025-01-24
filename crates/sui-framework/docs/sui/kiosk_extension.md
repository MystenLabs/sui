---
title: Module `sui::kiosk_extension`
---

This module implements the Kiosk Extensions functionality. It allows
exposing previously protected (only-owner) methods to third-party apps.

A Kiosk Extension is a module that implements any functionality on top of
the <code>Kiosk</code> without discarding nor blocking the base. Given that <code>Kiosk</code>
itself is a trading primitive, most of the extensions are expected to be
related to trading. However, there's no limit to what can be built using the
<code><a href="../sui/kiosk_extension.md#sui_kiosk_extension">kiosk_extension</a></code> module, as it gives certain benefits such as using <code>Kiosk</code>
as the storage for any type of data / assets.


<a name="@Flow:_0"></a>

#### Flow:

- An extension can only be installed by the Kiosk Owner and requires an
authorization via the <code>KioskOwnerCap</code>.
- When installed, the extension is given a permission bitmap that allows it
to perform certain protected actions (eg <code><a href="../sui/kiosk_extension.md#sui_kiosk_extension_place">place</a></code>, <code><a href="../sui/kiosk_extension.md#sui_kiosk_extension_lock">lock</a></code>). However, it is
possible to install an extension that does not have any permissions.
- Kiosk Owner can <code><a href="../sui/kiosk_extension.md#sui_kiosk_extension_disable">disable</a></code> the extension at any time, which prevents it
from performing any protected actions. The storage is still available to the
extension until it is completely removed.
- A disabled extension can be <code><a href="../sui/kiosk_extension.md#sui_kiosk_extension_enable">enable</a></code>d at any time giving the permissions
back to the extension.
- An extension permissions follow the all-or-nothing policy. Either all of
the requested permissions are granted or none of them (can't install).


<a name="@Examples:_1"></a>

#### Examples:

- An Auction extension can utilize the storage to store Auction-related data
while utilizing the same <code>Kiosk</code> object that the items are stored in.
- A Marketplace extension that implements custom events and fees for the
default trading functionality.


<a name="@Notes:_2"></a>

#### Notes:

- Trading functionality can utilize the <code>PurchaseCap</code> to build a custom
logic around the purchase flow. However, it should be carefully managed to
prevent asset locking.
- <code><a href="../sui/kiosk_extension.md#sui_kiosk_extension">kiosk_extension</a></code> is a friend module to <code><a href="../sui/kiosk.md#sui_kiosk">kiosk</a></code> and has access to its
internal functions (such as <code>place_internal</code> and <code>lock_internal</code> to
implement custom authorization scheme for <code><a href="../sui/kiosk_extension.md#sui_kiosk_extension_place">place</a></code> and <code><a href="../sui/kiosk_extension.md#sui_kiosk_extension_lock">lock</a></code> respectively).


        -  [Flow:](#@Flow:_0)
        -  [Examples:](#@Examples:_1)
        -  [Notes:](#@Notes:_2)
-  [Struct `Extension`](#sui_kiosk_extension_Extension)
-  [Struct `ExtensionKey`](#sui_kiosk_extension_ExtensionKey)
-  [Constants](#@Constants_3)
-  [Function `add`](#sui_kiosk_extension_add)
-  [Function `disable`](#sui_kiosk_extension_disable)
-  [Function `enable`](#sui_kiosk_extension_enable)
-  [Function `remove`](#sui_kiosk_extension_remove)
-  [Function `storage`](#sui_kiosk_extension_storage)
-  [Function `storage_mut`](#sui_kiosk_extension_storage_mut)
-  [Function `place`](#sui_kiosk_extension_place)
-  [Function `lock`](#sui_kiosk_extension_lock)
-  [Function `is_installed`](#sui_kiosk_extension_is_installed)
-  [Function `is_enabled`](#sui_kiosk_extension_is_enabled)
-  [Function `can_place`](#sui_kiosk_extension_can_place)
-  [Function `can_lock`](#sui_kiosk_extension_can_lock)
-  [Function `extension`](#sui_kiosk_extension_extension)
-  [Function `extension_mut`](#sui_kiosk_extension_extension_mut)


<pre><code><b>use</b> <a href="../std/address.md#std_address">std::address</a>;
<b>use</b> <a href="../std/ascii.md#std_ascii">std::ascii</a>;
<b>use</b> <a href="../std/bcs.md#std_bcs">std::bcs</a>;
<b>use</b> <a href="../std/option.md#std_option">std::option</a>;
<b>use</b> <a href="../std/string.md#std_string">std::string</a>;
<b>use</b> <a href="../std/type_name.md#std_type_name">std::type_name</a>;
<b>use</b> <a href="../std/vector.md#std_vector">std::vector</a>;
<b>use</b> <a href="../sui/address.md#sui_address">sui::address</a>;
<b>use</b> <a href="../sui/bag.md#sui_bag">sui::bag</a>;
<b>use</b> <a href="../sui/balance.md#sui_balance">sui::balance</a>;
<b>use</b> <a href="../sui/coin.md#sui_coin">sui::coin</a>;
<b>use</b> <a href="../sui/config.md#sui_config">sui::config</a>;
<b>use</b> <a href="../sui/deny_list.md#sui_deny_list">sui::deny_list</a>;
<b>use</b> <a href="../sui/dynamic_field.md#sui_dynamic_field">sui::dynamic_field</a>;
<b>use</b> <a href="../sui/dynamic_object_field.md#sui_dynamic_object_field">sui::dynamic_object_field</a>;
<b>use</b> <a href="../sui/event.md#sui_event">sui::event</a>;
<b>use</b> <a href="../sui/hex.md#sui_hex">sui::hex</a>;
<b>use</b> <a href="../sui/kiosk.md#sui_kiosk">sui::kiosk</a>;
<b>use</b> <a href="../sui/object.md#sui_object">sui::object</a>;
<b>use</b> <a href="../sui/package.md#sui_package">sui::package</a>;
<b>use</b> <a href="../sui/sui.md#sui_sui">sui::sui</a>;
<b>use</b> <a href="../sui/table.md#sui_table">sui::table</a>;
<b>use</b> <a href="../sui/transfer.md#sui_transfer">sui::transfer</a>;
<b>use</b> <a href="../sui/transfer_policy.md#sui_transfer_policy">sui::transfer_policy</a>;
<b>use</b> <a href="../sui/tx_context.md#sui_tx_context">sui::tx_context</a>;
<b>use</b> <a href="../sui/types.md#sui_types">sui::types</a>;
<b>use</b> <a href="../sui/url.md#sui_url">sui::url</a>;
<b>use</b> <a href="../sui/vec_set.md#sui_vec_set">sui::vec_set</a>;
</code></pre>



<a name="sui_kiosk_extension_Extension"></a>

## Struct `Extension`

The Extension struct contains the data used by the extension and the
configuration for this extension. Stored under the <code><a href="../sui/kiosk_extension.md#sui_kiosk_extension_ExtensionKey">ExtensionKey</a></code>
dynamic field.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/kiosk_extension.md#sui_kiosk_extension_Extension">Extension</a> <b>has</b> store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code><a href="../sui/kiosk_extension.md#sui_kiosk_extension_storage">storage</a>: <a href="../sui/bag.md#sui_bag_Bag">sui::bag::Bag</a></code>
</dt>
<dd>
 Storage for the extension, an isolated Bag. By putting the extension
 into a single dynamic field, we reduce the amount of fields on the
 top level (eg items / listings) while giving extension developers
 the ability to store any data they want.
</dd>
<dt>
<code>permissions: u128</code>
</dt>
<dd>
 Bitmap of permissions that the extension has (can be revoked any
 moment). It's all or nothing policy - either the extension has the
 required permissions or no permissions at all.
 1st bit - <code><a href="../sui/kiosk_extension.md#sui_kiosk_extension_place">place</a></code> - allows to place items for sale
 2nd bit - <code><a href="../sui/kiosk_extension.md#sui_kiosk_extension_lock">lock</a></code> and <code><a href="../sui/kiosk_extension.md#sui_kiosk_extension_place">place</a></code> - allows to lock items (and place)
 For example:
 - <code>10</code> - allows to place items and lock them.
 - <code>11</code> - allows to place items and lock them (<code><a href="../sui/kiosk_extension.md#sui_kiosk_extension_lock">lock</a></code> includes <code><a href="../sui/kiosk_extension.md#sui_kiosk_extension_place">place</a></code>).
 - <code>01</code> - allows to place items, but not lock them.
 - <code>00</code> - no permissions.
</dd>
<dt>
<code><a href="../sui/kiosk_extension.md#sui_kiosk_extension_is_enabled">is_enabled</a>: bool</code>
</dt>
<dd>
 Whether the extension can call protected actions. By default, all
 extensions are enabled (on <code><a href="../sui/kiosk_extension.md#sui_kiosk_extension_add">add</a></code> call), however the Kiosk
 owner can disable them at any time.
 Disabling the extension does not limit its access to the storage.
</dd>
</dl>


</details>

<a name="sui_kiosk_extension_ExtensionKey"></a>

## Struct `ExtensionKey`

The <code><a href="../sui/kiosk_extension.md#sui_kiosk_extension_ExtensionKey">ExtensionKey</a></code> is a typed dynamic field key used to store the
extension configuration and data. <code>Ext</code> is a phantom type that is used
to identify the extension witness.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/kiosk_extension.md#sui_kiosk_extension_ExtensionKey">ExtensionKey</a>&lt;<b>phantom</b> Ext&gt; <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
</dl>


</details>

<a name="@Constants_3"></a>

## Constants


<a name="sui_kiosk_extension_EExtensionNotAllowed"></a>

Extension is trying to access a permissioned action while not having
the required permission.


<pre><code><b>const</b> <a href="../sui/kiosk_extension.md#sui_kiosk_extension_EExtensionNotAllowed">EExtensionNotAllowed</a>: u64 = 2;
</code></pre>



<a name="sui_kiosk_extension_EExtensionNotInstalled"></a>

Extension is not installed in the Kiosk.


<pre><code><b>const</b> <a href="../sui/kiosk_extension.md#sui_kiosk_extension_EExtensionNotInstalled">EExtensionNotInstalled</a>: u64 = 3;
</code></pre>



<a name="sui_kiosk_extension_ENotOwner"></a>

Trying to add an extension while not being the owner of the Kiosk.


<pre><code><b>const</b> <a href="../sui/kiosk_extension.md#sui_kiosk_extension_ENotOwner">ENotOwner</a>: u64 = 0;
</code></pre>



<a name="sui_kiosk_extension_LOCK"></a>

Value that represents the <code><a href="../sui/kiosk_extension.md#sui_kiosk_extension_lock">lock</a></code> and <code><a href="../sui/kiosk_extension.md#sui_kiosk_extension_place">place</a></code> permission in the
permissions bitmap.


<pre><code><b>const</b> <a href="../sui/kiosk_extension.md#sui_kiosk_extension_LOCK">LOCK</a>: u128 = 2;
</code></pre>



<a name="sui_kiosk_extension_PLACE"></a>

Value that represents the <code><a href="../sui/kiosk_extension.md#sui_kiosk_extension_place">place</a></code> permission in the permissions bitmap.


<pre><code><b>const</b> <a href="../sui/kiosk_extension.md#sui_kiosk_extension_PLACE">PLACE</a>: u128 = 1;
</code></pre>



<a name="sui_kiosk_extension_add"></a>

## Function `add`

Add an extension to the Kiosk. Can only be performed by the owner. The
extension witness is required to allow extensions define their set of
permissions in the custom <code><a href="../sui/kiosk_extension.md#sui_kiosk_extension_add">add</a></code> call.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/kiosk_extension.md#sui_kiosk_extension_add">add</a>&lt;Ext: drop&gt;(_ext: Ext, self: &<b>mut</b> <a href="../sui/kiosk.md#sui_kiosk_Kiosk">sui::kiosk::Kiosk</a>, cap: &<a href="../sui/kiosk.md#sui_kiosk_KioskOwnerCap">sui::kiosk::KioskOwnerCap</a>, permissions: u128, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/kiosk_extension.md#sui_kiosk_extension_add">add</a>&lt;Ext: drop&gt;(
    _ext: Ext,
    self: &<b>mut</b> Kiosk,
    cap: &KioskOwnerCap,
    permissions: u128,
    ctx: &<b>mut</b> TxContext,
) {
    <b>assert</b>!(self.has_access(cap), <a href="../sui/kiosk_extension.md#sui_kiosk_extension_ENotOwner">ENotOwner</a>);
    df::add(
        self.uid_mut_as_owner(cap),
        <a href="../sui/kiosk_extension.md#sui_kiosk_extension_ExtensionKey">ExtensionKey</a>&lt;Ext&gt; {},
        <a href="../sui/kiosk_extension.md#sui_kiosk_extension_Extension">Extension</a> {
            <a href="../sui/kiosk_extension.md#sui_kiosk_extension_storage">storage</a>: <a href="../sui/bag.md#sui_bag_new">bag::new</a>(ctx),
            permissions,
            <a href="../sui/kiosk_extension.md#sui_kiosk_extension_is_enabled">is_enabled</a>: <b>true</b>,
        },
    )
}
</code></pre>



</details>

<a name="sui_kiosk_extension_disable"></a>

## Function `disable`

Revoke permissions from the extension. While it does not remove the
extension completely, it keeps it from performing any protected actions.
The storage is still available to the extension (until it's removed).


<pre><code><b>public</b> <b>fun</b> <a href="../sui/kiosk_extension.md#sui_kiosk_extension_disable">disable</a>&lt;Ext: drop&gt;(self: &<b>mut</b> <a href="../sui/kiosk.md#sui_kiosk_Kiosk">sui::kiosk::Kiosk</a>, cap: &<a href="../sui/kiosk.md#sui_kiosk_KioskOwnerCap">sui::kiosk::KioskOwnerCap</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/kiosk_extension.md#sui_kiosk_extension_disable">disable</a>&lt;Ext: drop&gt;(self: &<b>mut</b> Kiosk, cap: &KioskOwnerCap) {
    <b>assert</b>!(self.has_access(cap), <a href="../sui/kiosk_extension.md#sui_kiosk_extension_ENotOwner">ENotOwner</a>);
    <b>assert</b>!(<a href="../sui/kiosk_extension.md#sui_kiosk_extension_is_installed">is_installed</a>&lt;Ext&gt;(self), <a href="../sui/kiosk_extension.md#sui_kiosk_extension_EExtensionNotInstalled">EExtensionNotInstalled</a>);
    <a href="../sui/kiosk_extension.md#sui_kiosk_extension_extension_mut">extension_mut</a>&lt;Ext&gt;(self).<a href="../sui/kiosk_extension.md#sui_kiosk_extension_is_enabled">is_enabled</a> = <b>false</b>;
}
</code></pre>



</details>

<a name="sui_kiosk_extension_enable"></a>

## Function `enable`

Re-enable the extension allowing it to call protected actions (eg
<code><a href="../sui/kiosk_extension.md#sui_kiosk_extension_place">place</a></code>, <code><a href="../sui/kiosk_extension.md#sui_kiosk_extension_lock">lock</a></code>). By default, all added extensions are enabled. Kiosk
owner can disable them via <code><a href="../sui/kiosk_extension.md#sui_kiosk_extension_disable">disable</a></code> call.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/kiosk_extension.md#sui_kiosk_extension_enable">enable</a>&lt;Ext: drop&gt;(self: &<b>mut</b> <a href="../sui/kiosk.md#sui_kiosk_Kiosk">sui::kiosk::Kiosk</a>, cap: &<a href="../sui/kiosk.md#sui_kiosk_KioskOwnerCap">sui::kiosk::KioskOwnerCap</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/kiosk_extension.md#sui_kiosk_extension_enable">enable</a>&lt;Ext: drop&gt;(self: &<b>mut</b> Kiosk, cap: &KioskOwnerCap) {
    <b>assert</b>!(self.has_access(cap), <a href="../sui/kiosk_extension.md#sui_kiosk_extension_ENotOwner">ENotOwner</a>);
    <b>assert</b>!(<a href="../sui/kiosk_extension.md#sui_kiosk_extension_is_installed">is_installed</a>&lt;Ext&gt;(self), <a href="../sui/kiosk_extension.md#sui_kiosk_extension_EExtensionNotInstalled">EExtensionNotInstalled</a>);
    <a href="../sui/kiosk_extension.md#sui_kiosk_extension_extension_mut">extension_mut</a>&lt;Ext&gt;(self).<a href="../sui/kiosk_extension.md#sui_kiosk_extension_is_enabled">is_enabled</a> = <b>true</b>;
}
</code></pre>



</details>

<a name="sui_kiosk_extension_remove"></a>

## Function `remove`

Remove an extension from the Kiosk. Can only be performed by the owner,
the extension storage must be empty for the transaction to succeed.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/kiosk_extension.md#sui_kiosk_extension_remove">remove</a>&lt;Ext: drop&gt;(self: &<b>mut</b> <a href="../sui/kiosk.md#sui_kiosk_Kiosk">sui::kiosk::Kiosk</a>, cap: &<a href="../sui/kiosk.md#sui_kiosk_KioskOwnerCap">sui::kiosk::KioskOwnerCap</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/kiosk_extension.md#sui_kiosk_extension_remove">remove</a>&lt;Ext: drop&gt;(self: &<b>mut</b> Kiosk, cap: &KioskOwnerCap) {
    <b>assert</b>!(self.has_access(cap), <a href="../sui/kiosk_extension.md#sui_kiosk_extension_ENotOwner">ENotOwner</a>);
    <b>assert</b>!(<a href="../sui/kiosk_extension.md#sui_kiosk_extension_is_installed">is_installed</a>&lt;Ext&gt;(self), <a href="../sui/kiosk_extension.md#sui_kiosk_extension_EExtensionNotInstalled">EExtensionNotInstalled</a>);
    <b>let</b> <a href="../sui/kiosk_extension.md#sui_kiosk_extension_Extension">Extension</a> {
        <a href="../sui/kiosk_extension.md#sui_kiosk_extension_storage">storage</a>,
        permissions: _,
        <a href="../sui/kiosk_extension.md#sui_kiosk_extension_is_enabled">is_enabled</a>: _,
    } = df::remove(self.uid_mut_as_owner(cap), <a href="../sui/kiosk_extension.md#sui_kiosk_extension_ExtensionKey">ExtensionKey</a>&lt;Ext&gt; {});
    <a href="../sui/kiosk_extension.md#sui_kiosk_extension_storage">storage</a>.destroy_empty();
}
</code></pre>



</details>

<a name="sui_kiosk_extension_storage"></a>

## Function `storage`

Get immutable access to the extension storage. Can only be performed by
the extension as long as the extension is installed.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/kiosk_extension.md#sui_kiosk_extension_storage">storage</a>&lt;Ext: drop&gt;(_ext: Ext, self: &<a href="../sui/kiosk.md#sui_kiosk_Kiosk">sui::kiosk::Kiosk</a>): &<a href="../sui/bag.md#sui_bag_Bag">sui::bag::Bag</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/kiosk_extension.md#sui_kiosk_extension_storage">storage</a>&lt;Ext: drop&gt;(_ext: Ext, self: &Kiosk): &Bag {
    <b>assert</b>!(<a href="../sui/kiosk_extension.md#sui_kiosk_extension_is_installed">is_installed</a>&lt;Ext&gt;(self), <a href="../sui/kiosk_extension.md#sui_kiosk_extension_EExtensionNotInstalled">EExtensionNotInstalled</a>);
    &<a href="../sui/kiosk_extension.md#sui_kiosk_extension_extension">extension</a>&lt;Ext&gt;(self).<a href="../sui/kiosk_extension.md#sui_kiosk_extension_storage">storage</a>
}
</code></pre>



</details>

<a name="sui_kiosk_extension_storage_mut"></a>

## Function `storage_mut`

Get mutable access to the extension storage. Can only be performed by
the extension as long as the extension is installed. Disabling the
extension does not prevent it from accessing the storage.

Potentially dangerous: extension developer can keep data in a Bag
therefore never really allowing the KioskOwner to remove the extension.
However, it is the case with any other solution (1) and this way we
prevent intentional extension freeze when the owner wants to ruin a
trade (2) - eg locking extension while an auction is in progress.

Extensions should be crafted carefully, and the KioskOwner should be
aware of the risks.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/kiosk_extension.md#sui_kiosk_extension_storage_mut">storage_mut</a>&lt;Ext: drop&gt;(_ext: Ext, self: &<b>mut</b> <a href="../sui/kiosk.md#sui_kiosk_Kiosk">sui::kiosk::Kiosk</a>): &<b>mut</b> <a href="../sui/bag.md#sui_bag_Bag">sui::bag::Bag</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/kiosk_extension.md#sui_kiosk_extension_storage_mut">storage_mut</a>&lt;Ext: drop&gt;(_ext: Ext, self: &<b>mut</b> Kiosk): &<b>mut</b> Bag {
    <b>assert</b>!(<a href="../sui/kiosk_extension.md#sui_kiosk_extension_is_installed">is_installed</a>&lt;Ext&gt;(self), <a href="../sui/kiosk_extension.md#sui_kiosk_extension_EExtensionNotInstalled">EExtensionNotInstalled</a>);
    &<b>mut</b> <a href="../sui/kiosk_extension.md#sui_kiosk_extension_extension_mut">extension_mut</a>&lt;Ext&gt;(self).<a href="../sui/kiosk_extension.md#sui_kiosk_extension_storage">storage</a>
}
</code></pre>



</details>

<a name="sui_kiosk_extension_place"></a>

## Function `place`

Protected action: place an item into the Kiosk. Can be performed by an
authorized extension. The extension must have the <code><a href="../sui/kiosk_extension.md#sui_kiosk_extension_place">place</a></code> permission or
a <code><a href="../sui/kiosk_extension.md#sui_kiosk_extension_lock">lock</a></code> permission.

To prevent non-tradable items from being placed into <code>Kiosk</code> the method
requires a <code>TransferPolicy</code> for the placed type to exist.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/kiosk_extension.md#sui_kiosk_extension_place">place</a>&lt;Ext: drop, T: key, store&gt;(_ext: Ext, self: &<b>mut</b> <a href="../sui/kiosk.md#sui_kiosk_Kiosk">sui::kiosk::Kiosk</a>, item: T, _policy: &<a href="../sui/transfer_policy.md#sui_transfer_policy_TransferPolicy">sui::transfer_policy::TransferPolicy</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/kiosk_extension.md#sui_kiosk_extension_place">place</a>&lt;Ext: drop, T: key + store&gt;(
    _ext: Ext,
    self: &<b>mut</b> Kiosk,
    item: T,
    _policy: &TransferPolicy&lt;T&gt;,
) {
    <b>assert</b>!(<a href="../sui/kiosk_extension.md#sui_kiosk_extension_is_installed">is_installed</a>&lt;Ext&gt;(self), <a href="../sui/kiosk_extension.md#sui_kiosk_extension_EExtensionNotInstalled">EExtensionNotInstalled</a>);
    <b>assert</b>!(<a href="../sui/kiosk_extension.md#sui_kiosk_extension_can_place">can_place</a>&lt;Ext&gt;(self) || <a href="../sui/kiosk_extension.md#sui_kiosk_extension_can_lock">can_lock</a>&lt;Ext&gt;(self), <a href="../sui/kiosk_extension.md#sui_kiosk_extension_EExtensionNotAllowed">EExtensionNotAllowed</a>);
    self.place_internal(item)
}
</code></pre>



</details>

<a name="sui_kiosk_extension_lock"></a>

## Function `lock`

Protected action: lock an item in the Kiosk. Can be performed by an
authorized extension. The extension must have the <code><a href="../sui/kiosk_extension.md#sui_kiosk_extension_lock">lock</a></code> permission.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/kiosk_extension.md#sui_kiosk_extension_lock">lock</a>&lt;Ext: drop, T: key, store&gt;(_ext: Ext, self: &<b>mut</b> <a href="../sui/kiosk.md#sui_kiosk_Kiosk">sui::kiosk::Kiosk</a>, item: T, _policy: &<a href="../sui/transfer_policy.md#sui_transfer_policy_TransferPolicy">sui::transfer_policy::TransferPolicy</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/kiosk_extension.md#sui_kiosk_extension_lock">lock</a>&lt;Ext: drop, T: key + store&gt;(
    _ext: Ext,
    self: &<b>mut</b> Kiosk,
    item: T,
    _policy: &TransferPolicy&lt;T&gt;,
) {
    <b>assert</b>!(<a href="../sui/kiosk_extension.md#sui_kiosk_extension_is_installed">is_installed</a>&lt;Ext&gt;(self), <a href="../sui/kiosk_extension.md#sui_kiosk_extension_EExtensionNotInstalled">EExtensionNotInstalled</a>);
    <b>assert</b>!(<a href="../sui/kiosk_extension.md#sui_kiosk_extension_can_lock">can_lock</a>&lt;Ext&gt;(self), <a href="../sui/kiosk_extension.md#sui_kiosk_extension_EExtensionNotAllowed">EExtensionNotAllowed</a>);
    self.lock_internal(item)
}
</code></pre>



</details>

<a name="sui_kiosk_extension_is_installed"></a>

## Function `is_installed`

Check whether an extension of type <code>Ext</code> is installed.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/kiosk_extension.md#sui_kiosk_extension_is_installed">is_installed</a>&lt;Ext: drop&gt;(self: &<a href="../sui/kiosk.md#sui_kiosk_Kiosk">sui::kiosk::Kiosk</a>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/kiosk_extension.md#sui_kiosk_extension_is_installed">is_installed</a>&lt;Ext: drop&gt;(self: &Kiosk): bool {
    df::exists_(self.uid(), <a href="../sui/kiosk_extension.md#sui_kiosk_extension_ExtensionKey">ExtensionKey</a>&lt;Ext&gt; {})
}
</code></pre>



</details>

<a name="sui_kiosk_extension_is_enabled"></a>

## Function `is_enabled`

Check whether an extension of type <code>Ext</code> is enabled.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/kiosk_extension.md#sui_kiosk_extension_is_enabled">is_enabled</a>&lt;Ext: drop&gt;(self: &<a href="../sui/kiosk.md#sui_kiosk_Kiosk">sui::kiosk::Kiosk</a>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/kiosk_extension.md#sui_kiosk_extension_is_enabled">is_enabled</a>&lt;Ext: drop&gt;(self: &Kiosk): bool {
    <a href="../sui/kiosk_extension.md#sui_kiosk_extension_extension">extension</a>&lt;Ext&gt;(self).<a href="../sui/kiosk_extension.md#sui_kiosk_extension_is_enabled">is_enabled</a>
}
</code></pre>



</details>

<a name="sui_kiosk_extension_can_place"></a>

## Function `can_place`

Check whether an extension of type <code>Ext</code> can <code><a href="../sui/kiosk_extension.md#sui_kiosk_extension_place">place</a></code> into Kiosk.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/kiosk_extension.md#sui_kiosk_extension_can_place">can_place</a>&lt;Ext: drop&gt;(self: &<a href="../sui/kiosk.md#sui_kiosk_Kiosk">sui::kiosk::Kiosk</a>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/kiosk_extension.md#sui_kiosk_extension_can_place">can_place</a>&lt;Ext: drop&gt;(self: &Kiosk): bool {
    <a href="../sui/kiosk_extension.md#sui_kiosk_extension_is_enabled">is_enabled</a>&lt;Ext&gt;(self) && <a href="../sui/kiosk_extension.md#sui_kiosk_extension_extension">extension</a>&lt;Ext&gt;(self).permissions & <a href="../sui/kiosk_extension.md#sui_kiosk_extension_PLACE">PLACE</a> != 0
}
</code></pre>



</details>

<a name="sui_kiosk_extension_can_lock"></a>

## Function `can_lock`

Check whether an extension of type <code>Ext</code> can <code><a href="../sui/kiosk_extension.md#sui_kiosk_extension_lock">lock</a></code> items in Kiosk.
Locking also enables <code><a href="../sui/kiosk_extension.md#sui_kiosk_extension_place">place</a></code>.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/kiosk_extension.md#sui_kiosk_extension_can_lock">can_lock</a>&lt;Ext: drop&gt;(self: &<a href="../sui/kiosk.md#sui_kiosk_Kiosk">sui::kiosk::Kiosk</a>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/kiosk_extension.md#sui_kiosk_extension_can_lock">can_lock</a>&lt;Ext: drop&gt;(self: &Kiosk): bool {
    <a href="../sui/kiosk_extension.md#sui_kiosk_extension_is_enabled">is_enabled</a>&lt;Ext&gt;(self) && <a href="../sui/kiosk_extension.md#sui_kiosk_extension_extension">extension</a>&lt;Ext&gt;(self).permissions & <a href="../sui/kiosk_extension.md#sui_kiosk_extension_LOCK">LOCK</a> != 0
}
</code></pre>



</details>

<a name="sui_kiosk_extension_extension"></a>

## Function `extension`

Internal: get a read-only access to the Extension.


<pre><code><b>fun</b> <a href="../sui/kiosk_extension.md#sui_kiosk_extension_extension">extension</a>&lt;Ext: drop&gt;(self: &<a href="../sui/kiosk.md#sui_kiosk_Kiosk">sui::kiosk::Kiosk</a>): &<a href="../sui/kiosk_extension.md#sui_kiosk_extension_Extension">sui::kiosk_extension::Extension</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/kiosk_extension.md#sui_kiosk_extension_extension">extension</a>&lt;Ext: drop&gt;(self: &Kiosk): &<a href="../sui/kiosk_extension.md#sui_kiosk_extension_Extension">Extension</a> {
    df::borrow(self.uid(), <a href="../sui/kiosk_extension.md#sui_kiosk_extension_ExtensionKey">ExtensionKey</a>&lt;Ext&gt; {})
}
</code></pre>



</details>

<a name="sui_kiosk_extension_extension_mut"></a>

## Function `extension_mut`

Internal: get a mutable access to the Extension.


<pre><code><b>fun</b> <a href="../sui/kiosk_extension.md#sui_kiosk_extension_extension_mut">extension_mut</a>&lt;Ext: drop&gt;(self: &<b>mut</b> <a href="../sui/kiosk.md#sui_kiosk_Kiosk">sui::kiosk::Kiosk</a>): &<b>mut</b> <a href="../sui/kiosk_extension.md#sui_kiosk_extension_Extension">sui::kiosk_extension::Extension</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/kiosk_extension.md#sui_kiosk_extension_extension_mut">extension_mut</a>&lt;Ext: drop&gt;(self: &<b>mut</b> Kiosk): &<b>mut</b> <a href="../sui/kiosk_extension.md#sui_kiosk_extension_Extension">Extension</a> {
    df::borrow_mut(self.uid_mut_internal(), <a href="../sui/kiosk_extension.md#sui_kiosk_extension_ExtensionKey">ExtensionKey</a>&lt;Ext&gt; {})
}
</code></pre>



</details>
