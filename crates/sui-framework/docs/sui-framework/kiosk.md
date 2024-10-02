---
title: Module `0x2::kiosk`
---

Kiosk is a primitive for building safe, decentralized and trustless trading
experiences. It allows storing and trading any types of assets as long as
the creator of these assets implements a <code>TransferPolicy</code> for them.


<a name="@Principles_and_philosophy:_0"></a>

#### Principles and philosophy:


- Kiosk provides guarantees of "true ownership"; - just like single owner
objects, assets stored in the Kiosk can only be managed by the Kiosk owner.
Only the owner can <code>place</code>, <code>take</code>, <code>list</code>, perform any other actions on
assets in the Kiosk.

- Kiosk aims to be generic - allowing for a small set of default behaviors
and not imposing any restrictions on how the assets can be traded. The only
default scenario is a <code>list</code> + <code>purchase</code> flow; any other trading logic can
be implemented on top using the <code>list_with_purchase_cap</code> (and a matching
<code>purchase_with_cap</code>) flow.

- For every transaction happening with a third party a <code>TransferRequest</code> is
created - this way creators are fully in control of the trading experience.


<a name="@Asset_states_in_the_Kiosk:_1"></a>

#### Asset states in the Kiosk:


- <code>placed</code> -  An asset is <code>place</code>d into the Kiosk and can be <code>take</code>n out by
the Kiosk owner; it's freely tradable and modifiable via the <code>borrow_mut</code>
and <code>borrow_val</code> functions.

- <code>locked</code> - Similar to <code>placed</code> except that <code>take</code> is disabled and the only
way to move the asset out of the Kiosk is to <code>list</code> it or
<code>list_with_purchase_cap</code> therefore performing a trade (issuing a
<code>TransferRequest</code>). The check on the <code>lock</code> function makes sure that the
<code>TransferPolicy</code> exists to not lock the item in a <code><a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a></code> forever.

- <code>listed</code> - A <code>place</code>d or a <code>lock</code>ed item can be <code>list</code>ed for a fixed price
allowing anyone to <code>purchase</code> it from the Kiosk. While listed, an item can
not be taken or modified. However, an immutable borrow via <code><a href="borrow.md#0x2_borrow">borrow</a></code> call is
still available. The <code>delist</code> function returns the asset to the previous
state.

- <code>listed_exclusively</code> - An item is listed via the <code>list_with_purchase_cap</code>
function (and a <code><a href="kiosk.md#0x2_kiosk_PurchaseCap">PurchaseCap</a></code> is created). While listed this way, an item
can not be <code>delist</code>-ed unless a <code><a href="kiosk.md#0x2_kiosk_PurchaseCap">PurchaseCap</a></code> is returned. All actions
available at this item state require a <code><a href="kiosk.md#0x2_kiosk_PurchaseCap">PurchaseCap</a></code>:

1. <code>purchase_with_cap</code> - to purchase the item for a price equal or higher
than the <code>min_price</code> set in the <code><a href="kiosk.md#0x2_kiosk_PurchaseCap">PurchaseCap</a></code>.
2. <code>return_purchase_cap</code> - to return the <code><a href="kiosk.md#0x2_kiosk_PurchaseCap">PurchaseCap</a></code> and return the asset
into the previous state.

When an item is listed exclusively it cannot be modified nor taken and
losing a <code><a href="kiosk.md#0x2_kiosk_PurchaseCap">PurchaseCap</a></code> would lock the item in the Kiosk forever. Therefore,
it is recommended to only use <code><a href="kiosk.md#0x2_kiosk_PurchaseCap">PurchaseCap</a></code> functionality in trusted
applications and not use it for direct trading (eg sending to another
account).


<a name="@Using_multiple_Transfer_Policies_for_different_"tracks":_2"></a>

#### Using multiple Transfer Policies for different "tracks":


Every <code>purchase</code> or <code>purchase_with_purchase_cap</code> creates a <code>TransferRequest</code>
hot potato which must be resolved in a matching <code>TransferPolicy</code> for the
transaction to pass. While the default scenario implies that there should be
a single <code>TransferPolicy&lt;T&gt;</code> for <code>T</code>; it is possible to have multiple, each
one having its own set of rules.


<a name="@Examples:_3"></a>

#### Examples:


- I create one <code>TransferPolicy</code> with "Royalty Rule" for everyone
- I create a special <code>TransferPolicy</code> for bearers of a "Club Membership"
object so they don't have to pay anything
- I create and wrap a <code>TransferPolicy</code> so that players of my game can
transfer items between <code><a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a></code>s in game without any charge (and maybe not
even paying the price with a 0 SUI PurchaseCap)

```
Kiosk -> (Item, TransferRequest)
... TransferRequest ------> Common Transfer Policy
... TransferRequest ------> In-game Wrapped Transfer Policy
... TransferRequest ------> Club Membership Transfer Policy
```

See <code><a href="transfer_policy.md#0x2_transfer_policy">transfer_policy</a></code> module for more details on how they function.


        -  [Principles and philosophy:](#@Principles_and_philosophy:_0)
        -  [Asset states in the Kiosk:](#@Asset_states_in_the_Kiosk:_1)
        -  [Using multiple Transfer Policies for different "tracks":](#@Using_multiple_Transfer_Policies_for_different_"tracks":_2)
        -  [Examples:](#@Examples:_3)
-  [Resource `Kiosk`](#0x2_kiosk_Kiosk)
-  [Resource `KioskOwnerCap`](#0x2_kiosk_KioskOwnerCap)
-  [Resource `PurchaseCap`](#0x2_kiosk_PurchaseCap)
-  [Struct `Borrow`](#0x2_kiosk_Borrow)
-  [Struct `Item`](#0x2_kiosk_Item)
-  [Struct `Listing`](#0x2_kiosk_Listing)
-  [Struct `Lock`](#0x2_kiosk_Lock)
-  [Struct `ItemListed`](#0x2_kiosk_ItemListed)
-  [Struct `ItemPurchased`](#0x2_kiosk_ItemPurchased)
-  [Struct `ItemDelisted`](#0x2_kiosk_ItemDelisted)
-  [Constants](#@Constants_4)
-  [Function `default`](#0x2_kiosk_default)
-  [Function `new`](#0x2_kiosk_new)
-  [Function `close_and_withdraw`](#0x2_kiosk_close_and_withdraw)
-  [Function `set_owner`](#0x2_kiosk_set_owner)
-  [Function `set_owner_custom`](#0x2_kiosk_set_owner_custom)
-  [Function `place`](#0x2_kiosk_place)
-  [Function `lock`](#0x2_kiosk_lock)
-  [Function `take`](#0x2_kiosk_take)
-  [Function `list`](#0x2_kiosk_list)
-  [Function `place_and_list`](#0x2_kiosk_place_and_list)
-  [Function `delist`](#0x2_kiosk_delist)
-  [Function `purchase`](#0x2_kiosk_purchase)
-  [Function `list_with_purchase_cap`](#0x2_kiosk_list_with_purchase_cap)
-  [Function `purchase_with_cap`](#0x2_kiosk_purchase_with_cap)
-  [Function `return_purchase_cap`](#0x2_kiosk_return_purchase_cap)
-  [Function `withdraw`](#0x2_kiosk_withdraw)
-  [Function `lock_internal`](#0x2_kiosk_lock_internal)
-  [Function `place_internal`](#0x2_kiosk_place_internal)
-  [Function `uid_mut_internal`](#0x2_kiosk_uid_mut_internal)
-  [Function `has_item`](#0x2_kiosk_has_item)
-  [Function `has_item_with_type`](#0x2_kiosk_has_item_with_type)
-  [Function `is_locked`](#0x2_kiosk_is_locked)
-  [Function `is_listed`](#0x2_kiosk_is_listed)
-  [Function `is_listed_exclusively`](#0x2_kiosk_is_listed_exclusively)
-  [Function `has_access`](#0x2_kiosk_has_access)
-  [Function `uid_mut_as_owner`](#0x2_kiosk_uid_mut_as_owner)
-  [Function `set_allow_extensions`](#0x2_kiosk_set_allow_extensions)
-  [Function `uid`](#0x2_kiosk_uid)
-  [Function `uid_mut`](#0x2_kiosk_uid_mut)
-  [Function `owner`](#0x2_kiosk_owner)
-  [Function `item_count`](#0x2_kiosk_item_count)
-  [Function `profits_amount`](#0x2_kiosk_profits_amount)
-  [Function `profits_mut`](#0x2_kiosk_profits_mut)
-  [Function `borrow`](#0x2_kiosk_borrow)
-  [Function `borrow_mut`](#0x2_kiosk_borrow_mut)
-  [Function `borrow_val`](#0x2_kiosk_borrow_val)
-  [Function `return_val`](#0x2_kiosk_return_val)
-  [Function `kiosk_owner_cap_for`](#0x2_kiosk_kiosk_owner_cap_for)
-  [Function `purchase_cap_kiosk`](#0x2_kiosk_purchase_cap_kiosk)
-  [Function `purchase_cap_item`](#0x2_kiosk_purchase_cap_item)
-  [Function `purchase_cap_min_price`](#0x2_kiosk_purchase_cap_min_price)


<pre><code><b>use</b> <a href="../move-stdlib/option.md#0x1_option">0x1::option</a>;
<b>use</b> <a href="balance.md#0x2_balance">0x2::balance</a>;
<b>use</b> <a href="coin.md#0x2_coin">0x2::coin</a>;
<b>use</b> <a href="dynamic_field.md#0x2_dynamic_field">0x2::dynamic_field</a>;
<b>use</b> <a href="dynamic_object_field.md#0x2_dynamic_object_field">0x2::dynamic_object_field</a>;
<b>use</b> <a href="event.md#0x2_event">0x2::event</a>;
<b>use</b> <a href="object.md#0x2_object">0x2::object</a>;
<b>use</b> <a href="sui.md#0x2_sui">0x2::sui</a>;
<b>use</b> <a href="transfer.md#0x2_transfer">0x2::transfer</a>;
<b>use</b> <a href="transfer_policy.md#0x2_transfer_policy">0x2::transfer_policy</a>;
<b>use</b> <a href="tx_context.md#0x2_tx_context">0x2::tx_context</a>;
</code></pre>



<a name="0x2_kiosk_Kiosk"></a>

## Resource `Kiosk`

An object which allows selling collectibles within "kiosk" ecosystem.
By default gives the functionality to list an item openly - for anyone
to purchase providing the guarantees for creators that every transfer
needs to be approved via the <code>TransferPolicy</code>.


<pre><code><b>struct</b> <a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a> <b>has</b> store, key
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
<code>profits: <a href="balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="sui.md#0x2_sui_SUI">sui::SUI</a>&gt;</code>
</dt>
<dd>
 Balance of the Kiosk - all profits from sales go here.
</dd>
<dt>
<code>owner: <b>address</b></code>
</dt>
<dd>
 Always point to <code>sender</code> of the transaction.
 Can be changed by calling <code>set_owner</code> with Cap.
</dd>
<dt>
<code>item_count: u32</code>
</dt>
<dd>
 Number of items stored in a Kiosk. Used to allow unpacking
 an empty Kiosk if it was wrapped or has a single owner.
</dd>
<dt>
<code>allow_extensions: bool</code>
</dt>
<dd>
 [DEPRECATED] Please, don't use the <code>allow_extensions</code> and the matching
 <code>set_allow_extensions</code> function - it is a legacy feature that is being
 replaced by the <code><a href="kiosk_extension.md#0x2_kiosk_extension">kiosk_extension</a></code> module and its Extensions API.

 Exposes <code>uid_mut</code> publicly when set to <code><b>true</b></code>, set to <code><b>false</b></code> by default.
</dd>
</dl>


</details>

<a name="0x2_kiosk_KioskOwnerCap"></a>

## Resource `KioskOwnerCap`

A Capability granting the bearer a right to <code>place</code> and <code>take</code> items
from the <code><a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a></code> as well as to <code>list</code> them and <code>list_with_purchase_cap</code>.


<pre><code><b>struct</b> <a href="kiosk.md#0x2_kiosk_KioskOwnerCap">KioskOwnerCap</a> <b>has</b> store, key
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
<code>for: <a href="object.md#0x2_object_ID">object::ID</a></code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x2_kiosk_PurchaseCap"></a>

## Resource `PurchaseCap`

A capability which locks an item and gives a permission to
purchase it from a <code><a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a></code> for any price no less than <code>min_price</code>.

Allows exclusive listing: only bearer of the <code><a href="kiosk.md#0x2_kiosk_PurchaseCap">PurchaseCap</a></code> can
purchase the asset. However, the capability should be used
carefully as losing it would lock the asset in the <code><a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a></code>.

The main application for the <code><a href="kiosk.md#0x2_kiosk_PurchaseCap">PurchaseCap</a></code> is building extensions
on top of the <code><a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a></code>.


<pre><code><b>struct</b> <a href="kiosk.md#0x2_kiosk_PurchaseCap">PurchaseCap</a>&lt;T: store, key&gt; <b>has</b> store, key
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
<code>kiosk_id: <a href="object.md#0x2_object_ID">object::ID</a></code>
</dt>
<dd>
 ID of the <code><a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a></code> the cap belongs to.
</dd>
<dt>
<code>item_id: <a href="object.md#0x2_object_ID">object::ID</a></code>
</dt>
<dd>
 ID of the listed item.
</dd>
<dt>
<code>min_price: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>
 Minimum price for which the item can be purchased.
</dd>
</dl>


</details>

<a name="0x2_kiosk_Borrow"></a>

## Struct `Borrow`

Hot potato to ensure an item was returned after being taken using
the <code>borrow_val</code> call.


<pre><code><b>struct</b> <a href="kiosk.md#0x2_kiosk_Borrow">Borrow</a>
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>kiosk_id: <a href="object.md#0x2_object_ID">object::ID</a></code>
</dt>
<dd>

</dd>
<dt>
<code>item_id: <a href="object.md#0x2_object_ID">object::ID</a></code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x2_kiosk_Item"></a>

## Struct `Item`

Dynamic field key for an item placed into the kiosk.


<pre><code><b>struct</b> <a href="kiosk.md#0x2_kiosk_Item">Item</a> <b>has</b> <b>copy</b>, drop, store
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

<a name="0x2_kiosk_Listing"></a>

## Struct `Listing`

Dynamic field key for an active offer to purchase the T. If an
item is listed without a <code><a href="kiosk.md#0x2_kiosk_PurchaseCap">PurchaseCap</a></code>, exclusive is set to <code><b>false</b></code>.


<pre><code><b>struct</b> <a href="kiosk.md#0x2_kiosk_Listing">Listing</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>id: <a href="object.md#0x2_object_ID">object::ID</a></code>
</dt>
<dd>

</dd>
<dt>
<code>is_exclusive: bool</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x2_kiosk_Lock"></a>

## Struct `Lock`

Dynamic field key which marks that an item is locked in the <code><a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a></code> and
can't be <code>take</code>n. The item then can only be listed / sold via the PurchaseCap.
Lock is released on <code>purchase</code>.


<pre><code><b>struct</b> <a href="kiosk.md#0x2_kiosk_Lock">Lock</a> <b>has</b> <b>copy</b>, drop, store
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

<a name="0x2_kiosk_ItemListed"></a>

## Struct `ItemListed`

Emitted when an item was listed by the safe owner. Can be used
to track available offers anywhere on the network; the event is
type-indexed which allows for searching for offers of a specific <code>T</code>


<pre><code><b>struct</b> <a href="kiosk.md#0x2_kiosk_ItemListed">ItemListed</a>&lt;T: store, key&gt; <b>has</b> <b>copy</b>, drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code><a href="kiosk.md#0x2_kiosk">kiosk</a>: <a href="object.md#0x2_object_ID">object::ID</a></code>
</dt>
<dd>

</dd>
<dt>
<code>id: <a href="object.md#0x2_object_ID">object::ID</a></code>
</dt>
<dd>

</dd>
<dt>
<code>price: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x2_kiosk_ItemPurchased"></a>

## Struct `ItemPurchased`

Emitted when an item was purchased from the <code><a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a></code>. Can be used
to track finalized sales across the network. The event is emitted
in both cases: when an item is purchased via the <code><a href="kiosk.md#0x2_kiosk_PurchaseCap">PurchaseCap</a></code> or
when it's purchased directly (via <code>list</code> + <code>purchase</code>).

The <code>price</code> is also emitted and might differ from the <code>price</code> set
in the <code><a href="kiosk.md#0x2_kiosk_ItemListed">ItemListed</a></code> event. This is because the <code><a href="kiosk.md#0x2_kiosk_PurchaseCap">PurchaseCap</a></code> only
sets a minimum price for the item, and the actual price is defined
by the trading module / extension.


<pre><code><b>struct</b> <a href="kiosk.md#0x2_kiosk_ItemPurchased">ItemPurchased</a>&lt;T: store, key&gt; <b>has</b> <b>copy</b>, drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code><a href="kiosk.md#0x2_kiosk">kiosk</a>: <a href="object.md#0x2_object_ID">object::ID</a></code>
</dt>
<dd>

</dd>
<dt>
<code>id: <a href="object.md#0x2_object_ID">object::ID</a></code>
</dt>
<dd>

</dd>
<dt>
<code>price: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x2_kiosk_ItemDelisted"></a>

## Struct `ItemDelisted`

Emitted when an item was delisted by the safe owner. Can be used
to close tracked offers.


<pre><code><b>struct</b> <a href="kiosk.md#0x2_kiosk_ItemDelisted">ItemDelisted</a>&lt;T: store, key&gt; <b>has</b> <b>copy</b>, drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code><a href="kiosk.md#0x2_kiosk">kiosk</a>: <a href="object.md#0x2_object_ID">object::ID</a></code>
</dt>
<dd>

</dd>
<dt>
<code>id: <a href="object.md#0x2_object_ID">object::ID</a></code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="@Constants_4"></a>

## Constants


<a name="0x2_kiosk_ENotEnough"></a>

Trying to withdraw higher amount than stored.


<pre><code><b>const</b> <a href="kiosk.md#0x2_kiosk_ENotEnough">ENotEnough</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 2;
</code></pre>



<a name="0x2_kiosk_ENotOwner"></a>

Trying to withdraw profits and sender is not owner.


<pre><code><b>const</b> <a href="kiosk.md#0x2_kiosk_ENotOwner">ENotOwner</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 0;
</code></pre>



<a name="0x2_kiosk_EAlreadyListed"></a>

Trying to exclusively list an already listed item.


<pre><code><b>const</b> <a href="kiosk.md#0x2_kiosk_EAlreadyListed">EAlreadyListed</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 6;
</code></pre>



<a name="0x2_kiosk_EIncorrectAmount"></a>

Coin paid does not match the offer price.


<pre><code><b>const</b> <a href="kiosk.md#0x2_kiosk_EIncorrectAmount">EIncorrectAmount</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 1;
</code></pre>



<a name="0x2_kiosk_EItemIsListed"></a>

Taking or mutably borrowing an item that is listed.


<pre><code><b>const</b> <a href="kiosk.md#0x2_kiosk_EItemIsListed">EItemIsListed</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 9;
</code></pre>



<a name="0x2_kiosk_EItemLocked"></a>

Attempt to <code>take</code> an item that is locked.


<pre><code><b>const</b> <a href="kiosk.md#0x2_kiosk_EItemLocked">EItemLocked</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 8;
</code></pre>



<a name="0x2_kiosk_EItemMismatch"></a>

Item does not match <code><a href="kiosk.md#0x2_kiosk_Borrow">Borrow</a></code> in <code>return_val</code>.


<pre><code><b>const</b> <a href="kiosk.md#0x2_kiosk_EItemMismatch">EItemMismatch</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 10;
</code></pre>



<a name="0x2_kiosk_EItemNotFound"></a>

An is not found while trying to borrow.


<pre><code><b>const</b> <a href="kiosk.md#0x2_kiosk_EItemNotFound">EItemNotFound</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 11;
</code></pre>



<a name="0x2_kiosk_EListedExclusively"></a>

Attempt to take an item that has a <code><a href="kiosk.md#0x2_kiosk_PurchaseCap">PurchaseCap</a></code> issued.


<pre><code><b>const</b> <a href="kiosk.md#0x2_kiosk_EListedExclusively">EListedExclusively</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 4;
</code></pre>



<a name="0x2_kiosk_ENotEmpty"></a>

Trying to close a Kiosk and it has items in it.


<pre><code><b>const</b> <a href="kiosk.md#0x2_kiosk_ENotEmpty">ENotEmpty</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 3;
</code></pre>



<a name="0x2_kiosk_ENotListed"></a>

Delisting an item that is not listed.


<pre><code><b>const</b> <a href="kiosk.md#0x2_kiosk_ENotListed">ENotListed</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 12;
</code></pre>



<a name="0x2_kiosk_EUidAccessNotAllowed"></a>

Trying to call <code>uid_mut</code> when <code>allow_extensions</code> set to false.


<pre><code><b>const</b> <a href="kiosk.md#0x2_kiosk_EUidAccessNotAllowed">EUidAccessNotAllowed</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 7;
</code></pre>



<a name="0x2_kiosk_EWrongKiosk"></a>

<code><a href="kiosk.md#0x2_kiosk_PurchaseCap">PurchaseCap</a></code> does not match the <code><a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a></code>.


<pre><code><b>const</b> <a href="kiosk.md#0x2_kiosk_EWrongKiosk">EWrongKiosk</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 5;
</code></pre>



<a name="0x2_kiosk_default"></a>

## Function `default`

Creates a new Kiosk in a default configuration: sender receives the
<code><a href="kiosk.md#0x2_kiosk_KioskOwnerCap">KioskOwnerCap</a></code> and becomes the Owner, the <code><a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a></code> is shared.


<pre><code>entry <b>fun</b> <a href="kiosk.md#0x2_kiosk_default">default</a>(ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code>entry <b>fun</b> <a href="kiosk.md#0x2_kiosk_default">default</a>(ctx: &<b>mut</b> TxContext) {
    <b>let</b> (<a href="kiosk.md#0x2_kiosk">kiosk</a>, cap) = <a href="kiosk.md#0x2_kiosk_new">new</a>(ctx);
    sui::transfer::transfer(cap, ctx.sender());
    sui::transfer::share_object(<a href="kiosk.md#0x2_kiosk">kiosk</a>);
}
</code></pre>



</details>

<a name="0x2_kiosk_new"></a>

## Function `new`

Creates a new <code><a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a></code> with a matching <code><a href="kiosk.md#0x2_kiosk_KioskOwnerCap">KioskOwnerCap</a></code>.


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_new">new</a>(ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): (<a href="kiosk.md#0x2_kiosk_Kiosk">kiosk::Kiosk</a>, <a href="kiosk.md#0x2_kiosk_KioskOwnerCap">kiosk::KioskOwnerCap</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_new">new</a>(ctx: &<b>mut</b> TxContext): (<a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a>, <a href="kiosk.md#0x2_kiosk_KioskOwnerCap">KioskOwnerCap</a>) {
    <b>let</b> <a href="kiosk.md#0x2_kiosk">kiosk</a> = <a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a> {
        id: <a href="object.md#0x2_object_new">object::new</a>(ctx),
        profits: <a href="balance.md#0x2_balance_zero">balance::zero</a>(),
        owner: ctx.sender(),
        item_count: 0,
        allow_extensions: <b>false</b>,
    };

    <b>let</b> cap = <a href="kiosk.md#0x2_kiosk_KioskOwnerCap">KioskOwnerCap</a> {
        id: <a href="object.md#0x2_object_new">object::new</a>(ctx),
        `for`: <a href="object.md#0x2_object_id">object::id</a>(&<a href="kiosk.md#0x2_kiosk">kiosk</a>),
    };

    (<a href="kiosk.md#0x2_kiosk">kiosk</a>, cap)
}
</code></pre>



</details>

<a name="0x2_kiosk_close_and_withdraw"></a>

## Function `close_and_withdraw`

Unpacks and destroys a Kiosk returning the profits (even if "0").
Can only be performed by the bearer of the <code><a href="kiosk.md#0x2_kiosk_KioskOwnerCap">KioskOwnerCap</a></code> in the
case where there's no items inside and a <code><a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a></code> is not shared.


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_close_and_withdraw">close_and_withdraw</a>(self: <a href="kiosk.md#0x2_kiosk_Kiosk">kiosk::Kiosk</a>, cap: <a href="kiosk.md#0x2_kiosk_KioskOwnerCap">kiosk::KioskOwnerCap</a>, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="coin.md#0x2_coin_Coin">coin::Coin</a>&lt;<a href="sui.md#0x2_sui_SUI">sui::SUI</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_close_and_withdraw">close_and_withdraw</a>(self: <a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a>, cap: <a href="kiosk.md#0x2_kiosk_KioskOwnerCap">KioskOwnerCap</a>, ctx: &<b>mut</b> TxContext): Coin&lt;SUI&gt; {
    <b>let</b> <a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a> { id, profits, owner: _, item_count, allow_extensions: _ } = self;
    <b>let</b> <a href="kiosk.md#0x2_kiosk_KioskOwnerCap">KioskOwnerCap</a> { id: cap_id, `for` } = cap;

    <b>assert</b>!(id.to_inner() == `for`, <a href="kiosk.md#0x2_kiosk_ENotOwner">ENotOwner</a>);
    <b>assert</b>!(item_count == 0, <a href="kiosk.md#0x2_kiosk_ENotEmpty">ENotEmpty</a>);

    cap_id.delete();
    id.delete();

    profits.into_coin(ctx)
}
</code></pre>



</details>

<a name="0x2_kiosk_set_owner"></a>

## Function `set_owner`

Change the <code>owner</code> field to the transaction sender.
The change is purely cosmetical and does not affect any of the
basic kiosk functions unless some logic for this is implemented
in a third party module.


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_set_owner">set_owner</a>(self: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">kiosk::Kiosk</a>, cap: &<a href="kiosk.md#0x2_kiosk_KioskOwnerCap">kiosk::KioskOwnerCap</a>, ctx: &<a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_set_owner">set_owner</a>(self: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a>, cap: &<a href="kiosk.md#0x2_kiosk_KioskOwnerCap">KioskOwnerCap</a>, ctx: &TxContext) {
    <b>assert</b>!(self.<a href="kiosk.md#0x2_kiosk_has_access">has_access</a>(cap), <a href="kiosk.md#0x2_kiosk_ENotOwner">ENotOwner</a>);
    self.owner = ctx.sender();
}
</code></pre>



</details>

<a name="0x2_kiosk_set_owner_custom"></a>

## Function `set_owner_custom`

Update the <code>owner</code> field with a custom address. Can be used for
implementing a custom logic that relies on the <code><a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a></code> owner.


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_set_owner_custom">set_owner_custom</a>(self: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">kiosk::Kiosk</a>, cap: &<a href="kiosk.md#0x2_kiosk_KioskOwnerCap">kiosk::KioskOwnerCap</a>, owner: <b>address</b>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_set_owner_custom">set_owner_custom</a>(self: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a>, cap: &<a href="kiosk.md#0x2_kiosk_KioskOwnerCap">KioskOwnerCap</a>, owner: <b>address</b>) {
    <b>assert</b>!(self.<a href="kiosk.md#0x2_kiosk_has_access">has_access</a>(cap), <a href="kiosk.md#0x2_kiosk_ENotOwner">ENotOwner</a>);
    self.owner = owner
}
</code></pre>



</details>

<a name="0x2_kiosk_place"></a>

## Function `place`

Place any object into a Kiosk.
Performs an authorization check to make sure only owner can do that.


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_place">place</a>&lt;T: store, key&gt;(self: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">kiosk::Kiosk</a>, cap: &<a href="kiosk.md#0x2_kiosk_KioskOwnerCap">kiosk::KioskOwnerCap</a>, item: T)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_place">place</a>&lt;T: key + store&gt;(self: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a>, cap: &<a href="kiosk.md#0x2_kiosk_KioskOwnerCap">KioskOwnerCap</a>, item: T) {
    <b>assert</b>!(self.<a href="kiosk.md#0x2_kiosk_has_access">has_access</a>(cap), <a href="kiosk.md#0x2_kiosk_ENotOwner">ENotOwner</a>);
    self.<a href="kiosk.md#0x2_kiosk_place_internal">place_internal</a>(item)
}
</code></pre>



</details>

<a name="0x2_kiosk_lock"></a>

## Function `lock`

Place an item to the <code><a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a></code> and issue a <code><a href="kiosk.md#0x2_kiosk_Lock">Lock</a></code> for it. Once placed this
way, an item can only be listed either with a <code>list</code> function or with a
<code>list_with_purchase_cap</code>.

Requires policy for <code>T</code> to make sure that there's an issued <code>TransferPolicy</code>
and the item can be sold, otherwise the asset might be locked forever.


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_lock">lock</a>&lt;T: store, key&gt;(self: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">kiosk::Kiosk</a>, cap: &<a href="kiosk.md#0x2_kiosk_KioskOwnerCap">kiosk::KioskOwnerCap</a>, _policy: &<a href="transfer_policy.md#0x2_transfer_policy_TransferPolicy">transfer_policy::TransferPolicy</a>&lt;T&gt;, item: T)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_lock">lock</a>&lt;T: key + store&gt;(
    self: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a>,
    cap: &<a href="kiosk.md#0x2_kiosk_KioskOwnerCap">KioskOwnerCap</a>,
    _policy: &TransferPolicy&lt;T&gt;,
    item: T,
) {
    <b>assert</b>!(self.<a href="kiosk.md#0x2_kiosk_has_access">has_access</a>(cap), <a href="kiosk.md#0x2_kiosk_ENotOwner">ENotOwner</a>);
    self.<a href="kiosk.md#0x2_kiosk_lock_internal">lock_internal</a>(item)
}
</code></pre>



</details>

<a name="0x2_kiosk_take"></a>

## Function `take`

Take any object from the Kiosk.
Performs an authorization check to make sure only owner can do that.


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_take">take</a>&lt;T: store, key&gt;(self: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">kiosk::Kiosk</a>, cap: &<a href="kiosk.md#0x2_kiosk_KioskOwnerCap">kiosk::KioskOwnerCap</a>, id: <a href="object.md#0x2_object_ID">object::ID</a>): T
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_take">take</a>&lt;T: key + store&gt;(self: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a>, cap: &<a href="kiosk.md#0x2_kiosk_KioskOwnerCap">KioskOwnerCap</a>, id: ID): T {
    <b>assert</b>!(self.<a href="kiosk.md#0x2_kiosk_has_access">has_access</a>(cap), <a href="kiosk.md#0x2_kiosk_ENotOwner">ENotOwner</a>);
    <b>assert</b>!(!self.<a href="kiosk.md#0x2_kiosk_is_locked">is_locked</a>(id), <a href="kiosk.md#0x2_kiosk_EItemLocked">EItemLocked</a>);
    <b>assert</b>!(!self.<a href="kiosk.md#0x2_kiosk_is_listed_exclusively">is_listed_exclusively</a>(id), <a href="kiosk.md#0x2_kiosk_EListedExclusively">EListedExclusively</a>);
    <b>assert</b>!(self.<a href="kiosk.md#0x2_kiosk_has_item">has_item</a>(id), <a href="kiosk.md#0x2_kiosk_EItemNotFound">EItemNotFound</a>);

    self.item_count = self.item_count - 1;
    df::remove_if_exists&lt;<a href="kiosk.md#0x2_kiosk_Listing">Listing</a>, <a href="../move-stdlib/u64.md#0x1_u64">u64</a>&gt;(&<b>mut</b> self.id, <a href="kiosk.md#0x2_kiosk_Listing">Listing</a> { id, is_exclusive: <b>false</b> });
    dof::remove(&<b>mut</b> self.id, <a href="kiosk.md#0x2_kiosk_Item">Item</a> { id })
}
</code></pre>



</details>

<a name="0x2_kiosk_list"></a>

## Function `list`

List the item by setting a price and making it available for purchase.
Performs an authorization check to make sure only owner can sell.


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_list">list</a>&lt;T: store, key&gt;(self: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">kiosk::Kiosk</a>, cap: &<a href="kiosk.md#0x2_kiosk_KioskOwnerCap">kiosk::KioskOwnerCap</a>, id: <a href="object.md#0x2_object_ID">object::ID</a>, price: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_list">list</a>&lt;T: key + store&gt;(self: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a>, cap: &<a href="kiosk.md#0x2_kiosk_KioskOwnerCap">KioskOwnerCap</a>, id: ID, price: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>) {
    <b>assert</b>!(self.<a href="kiosk.md#0x2_kiosk_has_access">has_access</a>(cap), <a href="kiosk.md#0x2_kiosk_ENotOwner">ENotOwner</a>);
    <b>assert</b>!(self.<a href="kiosk.md#0x2_kiosk_has_item_with_type">has_item_with_type</a>&lt;T&gt;(id), <a href="kiosk.md#0x2_kiosk_EItemNotFound">EItemNotFound</a>);
    <b>assert</b>!(!self.<a href="kiosk.md#0x2_kiosk_is_listed_exclusively">is_listed_exclusively</a>(id), <a href="kiosk.md#0x2_kiosk_EListedExclusively">EListedExclusively</a>);

    df::add(&<b>mut</b> self.id, <a href="kiosk.md#0x2_kiosk_Listing">Listing</a> { id, is_exclusive: <b>false</b> }, price);
    <a href="event.md#0x2_event_emit">event::emit</a>(<a href="kiosk.md#0x2_kiosk_ItemListed">ItemListed</a>&lt;T&gt; { <a href="kiosk.md#0x2_kiosk">kiosk</a>: <a href="object.md#0x2_object_id">object::id</a>(self), id, price })
}
</code></pre>



</details>

<a name="0x2_kiosk_place_and_list"></a>

## Function `place_and_list`

Calls <code>place</code> and <code>list</code> together - simplifies the flow.


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_place_and_list">place_and_list</a>&lt;T: store, key&gt;(self: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">kiosk::Kiosk</a>, cap: &<a href="kiosk.md#0x2_kiosk_KioskOwnerCap">kiosk::KioskOwnerCap</a>, item: T, price: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_place_and_list">place_and_list</a>&lt;T: key + store&gt;(
    self: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a>,
    cap: &<a href="kiosk.md#0x2_kiosk_KioskOwnerCap">KioskOwnerCap</a>,
    item: T,
    price: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>,
) {
    <b>let</b> id = <a href="object.md#0x2_object_id">object::id</a>(&item);
    self.<a href="kiosk.md#0x2_kiosk_place">place</a>(cap, item);
    self.<a href="kiosk.md#0x2_kiosk_list">list</a>&lt;T&gt;(cap, id, price)
}
</code></pre>



</details>

<a name="0x2_kiosk_delist"></a>

## Function `delist`

Remove an existing listing from the <code><a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a></code> and keep the item in the
user Kiosk. Can only be performed by the owner of the <code><a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a></code>.


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_delist">delist</a>&lt;T: store, key&gt;(self: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">kiosk::Kiosk</a>, cap: &<a href="kiosk.md#0x2_kiosk_KioskOwnerCap">kiosk::KioskOwnerCap</a>, id: <a href="object.md#0x2_object_ID">object::ID</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_delist">delist</a>&lt;T: key + store&gt;(self: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a>, cap: &<a href="kiosk.md#0x2_kiosk_KioskOwnerCap">KioskOwnerCap</a>, id: ID) {
    <b>assert</b>!(self.<a href="kiosk.md#0x2_kiosk_has_access">has_access</a>(cap), <a href="kiosk.md#0x2_kiosk_ENotOwner">ENotOwner</a>);
    <b>assert</b>!(self.<a href="kiosk.md#0x2_kiosk_has_item_with_type">has_item_with_type</a>&lt;T&gt;(id), <a href="kiosk.md#0x2_kiosk_EItemNotFound">EItemNotFound</a>);
    <b>assert</b>!(!self.<a href="kiosk.md#0x2_kiosk_is_listed_exclusively">is_listed_exclusively</a>(id), <a href="kiosk.md#0x2_kiosk_EListedExclusively">EListedExclusively</a>);
    <b>assert</b>!(self.<a href="kiosk.md#0x2_kiosk_is_listed">is_listed</a>(id), <a href="kiosk.md#0x2_kiosk_ENotListed">ENotListed</a>);

    df::remove&lt;<a href="kiosk.md#0x2_kiosk_Listing">Listing</a>, <a href="../move-stdlib/u64.md#0x1_u64">u64</a>&gt;(&<b>mut</b> self.id, <a href="kiosk.md#0x2_kiosk_Listing">Listing</a> { id, is_exclusive: <b>false</b> });
    <a href="event.md#0x2_event_emit">event::emit</a>(<a href="kiosk.md#0x2_kiosk_ItemDelisted">ItemDelisted</a>&lt;T&gt; { <a href="kiosk.md#0x2_kiosk">kiosk</a>: <a href="object.md#0x2_object_id">object::id</a>(self), id })
}
</code></pre>



</details>

<a name="0x2_kiosk_purchase"></a>

## Function `purchase`

Make a trade: pay the owner of the item and request a Transfer to the <code>target</code>
kiosk (to prevent item being taken by the approving party).

Received <code>TransferRequest</code> needs to be handled by the publisher of the T,
if they have a method implemented that allows a trade, it is possible to
request their approval (by calling some function) so that the trade can be
finalized.


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_purchase">purchase</a>&lt;T: store, key&gt;(self: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">kiosk::Kiosk</a>, id: <a href="object.md#0x2_object_ID">object::ID</a>, payment: <a href="coin.md#0x2_coin_Coin">coin::Coin</a>&lt;<a href="sui.md#0x2_sui_SUI">sui::SUI</a>&gt;): (T, <a href="transfer_policy.md#0x2_transfer_policy_TransferRequest">transfer_policy::TransferRequest</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_purchase">purchase</a>&lt;T: key + store&gt;(
    self: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a>,
    id: ID,
    payment: Coin&lt;SUI&gt;,
): (T, TransferRequest&lt;T&gt;) {
    <b>let</b> price = df::remove&lt;<a href="kiosk.md#0x2_kiosk_Listing">Listing</a>, <a href="../move-stdlib/u64.md#0x1_u64">u64</a>&gt;(&<b>mut</b> self.id, <a href="kiosk.md#0x2_kiosk_Listing">Listing</a> { id, is_exclusive: <b>false</b> });
    <b>let</b> inner = dof::remove&lt;<a href="kiosk.md#0x2_kiosk_Item">Item</a>, T&gt;(&<b>mut</b> self.id, <a href="kiosk.md#0x2_kiosk_Item">Item</a> { id });

    self.item_count = self.item_count - 1;
    <b>assert</b>!(price == payment.value(), <a href="kiosk.md#0x2_kiosk_EIncorrectAmount">EIncorrectAmount</a>);
    df::remove_if_exists&lt;<a href="kiosk.md#0x2_kiosk_Lock">Lock</a>, bool&gt;(&<b>mut</b> self.id, <a href="kiosk.md#0x2_kiosk_Lock">Lock</a> { id });
    <a href="coin.md#0x2_coin_put">coin::put</a>(&<b>mut</b> self.profits, payment);

    <a href="event.md#0x2_event_emit">event::emit</a>(<a href="kiosk.md#0x2_kiosk_ItemPurchased">ItemPurchased</a>&lt;T&gt; { <a href="kiosk.md#0x2_kiosk">kiosk</a>: <a href="object.md#0x2_object_id">object::id</a>(self), id, price });

    (inner, <a href="transfer_policy.md#0x2_transfer_policy_new_request">transfer_policy::new_request</a>(id, price, <a href="object.md#0x2_object_id">object::id</a>(self)))
}
</code></pre>



</details>

<a name="0x2_kiosk_list_with_purchase_cap"></a>

## Function `list_with_purchase_cap`

Creates a <code><a href="kiosk.md#0x2_kiosk_PurchaseCap">PurchaseCap</a></code> which gives the right to purchase an item
for any price equal or higher than the <code>min_price</code>.


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_list_with_purchase_cap">list_with_purchase_cap</a>&lt;T: store, key&gt;(self: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">kiosk::Kiosk</a>, cap: &<a href="kiosk.md#0x2_kiosk_KioskOwnerCap">kiosk::KioskOwnerCap</a>, id: <a href="object.md#0x2_object_ID">object::ID</a>, min_price: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="kiosk.md#0x2_kiosk_PurchaseCap">kiosk::PurchaseCap</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_list_with_purchase_cap">list_with_purchase_cap</a>&lt;T: key + store&gt;(
    self: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a>,
    cap: &<a href="kiosk.md#0x2_kiosk_KioskOwnerCap">KioskOwnerCap</a>,
    id: ID,
    min_price: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>,
    ctx: &<b>mut</b> TxContext,
): <a href="kiosk.md#0x2_kiosk_PurchaseCap">PurchaseCap</a>&lt;T&gt; {
    <b>assert</b>!(self.<a href="kiosk.md#0x2_kiosk_has_access">has_access</a>(cap), <a href="kiosk.md#0x2_kiosk_ENotOwner">ENotOwner</a>);
    <b>assert</b>!(self.<a href="kiosk.md#0x2_kiosk_has_item_with_type">has_item_with_type</a>&lt;T&gt;(id), <a href="kiosk.md#0x2_kiosk_EItemNotFound">EItemNotFound</a>);
    <b>assert</b>!(!self.<a href="kiosk.md#0x2_kiosk_is_listed">is_listed</a>(id), <a href="kiosk.md#0x2_kiosk_EAlreadyListed">EAlreadyListed</a>);

    df::add(&<b>mut</b> self.id, <a href="kiosk.md#0x2_kiosk_Listing">Listing</a> { id, is_exclusive: <b>true</b> }, min_price);

    <a href="kiosk.md#0x2_kiosk_PurchaseCap">PurchaseCap</a>&lt;T&gt; {
        min_price,
        item_id: id,
        id: <a href="object.md#0x2_object_new">object::new</a>(ctx),
        kiosk_id: <a href="object.md#0x2_object_id">object::id</a>(self),
    }
}
</code></pre>



</details>

<a name="0x2_kiosk_purchase_with_cap"></a>

## Function `purchase_with_cap`

Unpack the <code><a href="kiosk.md#0x2_kiosk_PurchaseCap">PurchaseCap</a></code> and call <code>purchase</code>. Sets the payment amount
as the price for the listing making sure it's no less than <code>min_amount</code>.


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_purchase_with_cap">purchase_with_cap</a>&lt;T: store, key&gt;(self: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">kiosk::Kiosk</a>, purchase_cap: <a href="kiosk.md#0x2_kiosk_PurchaseCap">kiosk::PurchaseCap</a>&lt;T&gt;, payment: <a href="coin.md#0x2_coin_Coin">coin::Coin</a>&lt;<a href="sui.md#0x2_sui_SUI">sui::SUI</a>&gt;): (T, <a href="transfer_policy.md#0x2_transfer_policy_TransferRequest">transfer_policy::TransferRequest</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_purchase_with_cap">purchase_with_cap</a>&lt;T: key + store&gt;(
    self: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a>,
    purchase_cap: <a href="kiosk.md#0x2_kiosk_PurchaseCap">PurchaseCap</a>&lt;T&gt;,
    payment: Coin&lt;SUI&gt;,
): (T, TransferRequest&lt;T&gt;) {
    <b>let</b> <a href="kiosk.md#0x2_kiosk_PurchaseCap">PurchaseCap</a> { id, item_id, kiosk_id, min_price } = purchase_cap;
    id.delete();

    <b>let</b> id = item_id;
    <b>let</b> paid = payment.value();
    <b>assert</b>!(paid &gt;= min_price, <a href="kiosk.md#0x2_kiosk_EIncorrectAmount">EIncorrectAmount</a>);
    <b>assert</b>!(<a href="object.md#0x2_object_id">object::id</a>(self) == kiosk_id, <a href="kiosk.md#0x2_kiosk_EWrongKiosk">EWrongKiosk</a>);

    df::remove&lt;<a href="kiosk.md#0x2_kiosk_Listing">Listing</a>, <a href="../move-stdlib/u64.md#0x1_u64">u64</a>&gt;(&<b>mut</b> self.id, <a href="kiosk.md#0x2_kiosk_Listing">Listing</a> { id, is_exclusive: <b>true</b> });

    <a href="coin.md#0x2_coin_put">coin::put</a>(&<b>mut</b> self.profits, payment);
    self.item_count = self.item_count - 1;
    df::remove_if_exists&lt;<a href="kiosk.md#0x2_kiosk_Lock">Lock</a>, bool&gt;(&<b>mut</b> self.id, <a href="kiosk.md#0x2_kiosk_Lock">Lock</a> { id });
    <b>let</b> item = dof::remove&lt;<a href="kiosk.md#0x2_kiosk_Item">Item</a>, T&gt;(&<b>mut</b> self.id, <a href="kiosk.md#0x2_kiosk_Item">Item</a> { id });

    (item, <a href="transfer_policy.md#0x2_transfer_policy_new_request">transfer_policy::new_request</a>(id, paid, <a href="object.md#0x2_object_id">object::id</a>(self)))
}
</code></pre>



</details>

<a name="0x2_kiosk_return_purchase_cap"></a>

## Function `return_purchase_cap`

Return the <code><a href="kiosk.md#0x2_kiosk_PurchaseCap">PurchaseCap</a></code> without making a purchase; remove an active offer and
allow the item for taking. Can only be returned to its <code><a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a></code>, aborts otherwise.


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_return_purchase_cap">return_purchase_cap</a>&lt;T: store, key&gt;(self: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">kiosk::Kiosk</a>, purchase_cap: <a href="kiosk.md#0x2_kiosk_PurchaseCap">kiosk::PurchaseCap</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_return_purchase_cap">return_purchase_cap</a>&lt;T: key + store&gt;(self: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a>, purchase_cap: <a href="kiosk.md#0x2_kiosk_PurchaseCap">PurchaseCap</a>&lt;T&gt;) {
    <b>let</b> <a href="kiosk.md#0x2_kiosk_PurchaseCap">PurchaseCap</a> { id, item_id, kiosk_id, min_price: _ } = purchase_cap;

    <b>assert</b>!(<a href="object.md#0x2_object_id">object::id</a>(self) == kiosk_id, <a href="kiosk.md#0x2_kiosk_EWrongKiosk">EWrongKiosk</a>);
    df::remove&lt;<a href="kiosk.md#0x2_kiosk_Listing">Listing</a>, <a href="../move-stdlib/u64.md#0x1_u64">u64</a>&gt;(&<b>mut</b> self.id, <a href="kiosk.md#0x2_kiosk_Listing">Listing</a> { id: item_id, is_exclusive: <b>true</b> });
    id.delete()
}
</code></pre>



</details>

<a name="0x2_kiosk_withdraw"></a>

## Function `withdraw`

Withdraw profits from the Kiosk.


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_withdraw">withdraw</a>(self: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">kiosk::Kiosk</a>, cap: &<a href="kiosk.md#0x2_kiosk_KioskOwnerCap">kiosk::KioskOwnerCap</a>, amount: <a href="../move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;<a href="../move-stdlib/u64.md#0x1_u64">u64</a>&gt;, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="coin.md#0x2_coin_Coin">coin::Coin</a>&lt;<a href="sui.md#0x2_sui_SUI">sui::SUI</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_withdraw">withdraw</a>(
    self: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a>,
    cap: &<a href="kiosk.md#0x2_kiosk_KioskOwnerCap">KioskOwnerCap</a>,
    amount: Option&lt;<a href="../move-stdlib/u64.md#0x1_u64">u64</a>&gt;,
    ctx: &<b>mut</b> TxContext,
): Coin&lt;SUI&gt; {
    <b>assert</b>!(self.<a href="kiosk.md#0x2_kiosk_has_access">has_access</a>(cap), <a href="kiosk.md#0x2_kiosk_ENotOwner">ENotOwner</a>);

    <b>let</b> amount = <b>if</b> (amount.is_some()) {
        <b>let</b> amt = amount.destroy_some();
        <b>assert</b>!(amt &lt;= self.profits.value(), <a href="kiosk.md#0x2_kiosk_ENotEnough">ENotEnough</a>);
        amt
    } <b>else</b> {
        self.profits.value()
    };

    <a href="coin.md#0x2_coin_take">coin::take</a>(&<b>mut</b> self.profits, amount, ctx)
}
</code></pre>



</details>

<a name="0x2_kiosk_lock_internal"></a>

## Function `lock_internal`

Internal: "lock" an item disabling the <code>take</code> action.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="kiosk.md#0x2_kiosk_lock_internal">lock_internal</a>&lt;T: store, key&gt;(self: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">kiosk::Kiosk</a>, item: T)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="package.md#0x2_package">package</a>) <b>fun</b> <a href="kiosk.md#0x2_kiosk_lock_internal">lock_internal</a>&lt;T: key + store&gt;(self: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a>, item: T) {
    df::add(&<b>mut</b> self.id, <a href="kiosk.md#0x2_kiosk_Lock">Lock</a> { id: <a href="object.md#0x2_object_id">object::id</a>(&item) }, <b>true</b>);
    self.<a href="kiosk.md#0x2_kiosk_place_internal">place_internal</a>(item)
}
</code></pre>



</details>

<a name="0x2_kiosk_place_internal"></a>

## Function `place_internal`

Internal: "place" an item to the Kiosk and increment the item count.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="kiosk.md#0x2_kiosk_place_internal">place_internal</a>&lt;T: store, key&gt;(self: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">kiosk::Kiosk</a>, item: T)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="package.md#0x2_package">package</a>) <b>fun</b> <a href="kiosk.md#0x2_kiosk_place_internal">place_internal</a>&lt;T: key + store&gt;(self: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a>, item: T) {
    self.item_count = self.item_count + 1;
    dof::add(&<b>mut</b> self.id, <a href="kiosk.md#0x2_kiosk_Item">Item</a> { id: <a href="object.md#0x2_object_id">object::id</a>(&item) }, item)
}
</code></pre>



</details>

<a name="0x2_kiosk_uid_mut_internal"></a>

## Function `uid_mut_internal`

Internal: get a mutable access to the UID.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="kiosk.md#0x2_kiosk_uid_mut_internal">uid_mut_internal</a>(self: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">kiosk::Kiosk</a>): &<b>mut</b> <a href="object.md#0x2_object_UID">object::UID</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="package.md#0x2_package">package</a>) <b>fun</b> <a href="kiosk.md#0x2_kiosk_uid_mut_internal">uid_mut_internal</a>(self: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a>): &<b>mut</b> UID {
    &<b>mut</b> self.id
}
</code></pre>



</details>

<a name="0x2_kiosk_has_item"></a>

## Function `has_item`

Check whether the <code>item</code> is present in the <code><a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a></code>.


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_has_item">has_item</a>(self: &<a href="kiosk.md#0x2_kiosk_Kiosk">kiosk::Kiosk</a>, id: <a href="object.md#0x2_object_ID">object::ID</a>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_has_item">has_item</a>(self: &<a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a>, id: ID): bool {
    dof::exists_(&self.id, <a href="kiosk.md#0x2_kiosk_Item">Item</a> { id })
}
</code></pre>



</details>

<a name="0x2_kiosk_has_item_with_type"></a>

## Function `has_item_with_type`

Check whether the <code>item</code> is present in the <code><a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a></code> and has type T.


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_has_item_with_type">has_item_with_type</a>&lt;T: store, key&gt;(self: &<a href="kiosk.md#0x2_kiosk_Kiosk">kiosk::Kiosk</a>, id: <a href="object.md#0x2_object_ID">object::ID</a>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_has_item_with_type">has_item_with_type</a>&lt;T: key + store&gt;(self: &<a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a>, id: ID): bool {
    dof::exists_with_type&lt;<a href="kiosk.md#0x2_kiosk_Item">Item</a>, T&gt;(&self.id, <a href="kiosk.md#0x2_kiosk_Item">Item</a> { id })
}
</code></pre>



</details>

<a name="0x2_kiosk_is_locked"></a>

## Function `is_locked`

Check whether an item with the <code>id</code> is locked in the <code><a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a></code>. Meaning
that the only two actions that can be performed on it are <code>list</code> and
<code>list_with_purchase_cap</code>, it cannot be <code>take</code>n out of the <code><a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a></code>.


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_is_locked">is_locked</a>(self: &<a href="kiosk.md#0x2_kiosk_Kiosk">kiosk::Kiosk</a>, id: <a href="object.md#0x2_object_ID">object::ID</a>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_is_locked">is_locked</a>(self: &<a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a>, id: ID): bool {
    df::exists_(&self.id, <a href="kiosk.md#0x2_kiosk_Lock">Lock</a> { id })
}
</code></pre>



</details>

<a name="0x2_kiosk_is_listed"></a>

## Function `is_listed`

Check whether an <code>item</code> is listed (exclusively or non exclusively).


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_is_listed">is_listed</a>(self: &<a href="kiosk.md#0x2_kiosk_Kiosk">kiosk::Kiosk</a>, id: <a href="object.md#0x2_object_ID">object::ID</a>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_is_listed">is_listed</a>(self: &<a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a>, id: ID): bool {
    df::exists_(&self.id, <a href="kiosk.md#0x2_kiosk_Listing">Listing</a> { id, is_exclusive: <b>false</b> })
        || self.<a href="kiosk.md#0x2_kiosk_is_listed_exclusively">is_listed_exclusively</a>(id)
}
</code></pre>



</details>

<a name="0x2_kiosk_is_listed_exclusively"></a>

## Function `is_listed_exclusively`

Check whether there's a <code><a href="kiosk.md#0x2_kiosk_PurchaseCap">PurchaseCap</a></code> issued for an item.


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_is_listed_exclusively">is_listed_exclusively</a>(self: &<a href="kiosk.md#0x2_kiosk_Kiosk">kiosk::Kiosk</a>, id: <a href="object.md#0x2_object_ID">object::ID</a>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_is_listed_exclusively">is_listed_exclusively</a>(self: &<a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a>, id: ID): bool {
    df::exists_(&self.id, <a href="kiosk.md#0x2_kiosk_Listing">Listing</a> { id, is_exclusive: <b>true</b> })
}
</code></pre>



</details>

<a name="0x2_kiosk_has_access"></a>

## Function `has_access`

Check whether the <code><a href="kiosk.md#0x2_kiosk_KioskOwnerCap">KioskOwnerCap</a></code> matches the <code><a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a></code>.


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_has_access">has_access</a>(self: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">kiosk::Kiosk</a>, cap: &<a href="kiosk.md#0x2_kiosk_KioskOwnerCap">kiosk::KioskOwnerCap</a>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_has_access">has_access</a>(self: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a>, cap: &<a href="kiosk.md#0x2_kiosk_KioskOwnerCap">KioskOwnerCap</a>): bool {
    <a href="object.md#0x2_object_id">object::id</a>(self) == cap.`for`
}
</code></pre>



</details>

<a name="0x2_kiosk_uid_mut_as_owner"></a>

## Function `uid_mut_as_owner`

Access the <code>UID</code> using the <code><a href="kiosk.md#0x2_kiosk_KioskOwnerCap">KioskOwnerCap</a></code>.


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_uid_mut_as_owner">uid_mut_as_owner</a>(self: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">kiosk::Kiosk</a>, cap: &<a href="kiosk.md#0x2_kiosk_KioskOwnerCap">kiosk::KioskOwnerCap</a>): &<b>mut</b> <a href="object.md#0x2_object_UID">object::UID</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_uid_mut_as_owner">uid_mut_as_owner</a>(self: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a>, cap: &<a href="kiosk.md#0x2_kiosk_KioskOwnerCap">KioskOwnerCap</a>): &<b>mut</b> UID {
    <b>assert</b>!(self.<a href="kiosk.md#0x2_kiosk_has_access">has_access</a>(cap), <a href="kiosk.md#0x2_kiosk_ENotOwner">ENotOwner</a>);
    &<b>mut</b> self.id
}
</code></pre>



</details>

<a name="0x2_kiosk_set_allow_extensions"></a>

## Function `set_allow_extensions`

[DEPRECATED]
Allow or disallow <code>uid</code> and <code>uid_mut</code> access via the <code>allow_extensions</code>
setting.


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_set_allow_extensions">set_allow_extensions</a>(self: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">kiosk::Kiosk</a>, cap: &<a href="kiosk.md#0x2_kiosk_KioskOwnerCap">kiosk::KioskOwnerCap</a>, allow_extensions: bool)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_set_allow_extensions">set_allow_extensions</a>(self: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a>, cap: &<a href="kiosk.md#0x2_kiosk_KioskOwnerCap">KioskOwnerCap</a>, allow_extensions: bool) {
    <b>assert</b>!(self.<a href="kiosk.md#0x2_kiosk_has_access">has_access</a>(cap), <a href="kiosk.md#0x2_kiosk_ENotOwner">ENotOwner</a>);
    self.allow_extensions = allow_extensions;
}
</code></pre>



</details>

<a name="0x2_kiosk_uid"></a>

## Function `uid`

Get the immutable <code>UID</code> for dynamic field access.
Always enabled.

Given the &UID can be used for reading keys and authorization,
its access


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_uid">uid</a>(self: &<a href="kiosk.md#0x2_kiosk_Kiosk">kiosk::Kiosk</a>): &<a href="object.md#0x2_object_UID">object::UID</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_uid">uid</a>(self: &<a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a>): &UID {
    &self.id
}
</code></pre>



</details>

<a name="0x2_kiosk_uid_mut"></a>

## Function `uid_mut`

Get the mutable <code>UID</code> for dynamic field access and extensions.
Aborts if <code>allow_extensions</code> set to <code><b>false</b></code>.


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_uid_mut">uid_mut</a>(self: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">kiosk::Kiosk</a>): &<b>mut</b> <a href="object.md#0x2_object_UID">object::UID</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_uid_mut">uid_mut</a>(self: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a>): &<b>mut</b> UID {
    <b>assert</b>!(self.allow_extensions, <a href="kiosk.md#0x2_kiosk_EUidAccessNotAllowed">EUidAccessNotAllowed</a>);
    &<b>mut</b> self.id
}
</code></pre>



</details>

<a name="0x2_kiosk_owner"></a>

## Function `owner`

Get the owner of the Kiosk.


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_owner">owner</a>(self: &<a href="kiosk.md#0x2_kiosk_Kiosk">kiosk::Kiosk</a>): <b>address</b>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_owner">owner</a>(self: &<a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a>): <b>address</b> {
    self.owner
}
</code></pre>



</details>

<a name="0x2_kiosk_item_count"></a>

## Function `item_count`

Get the number of items stored in a Kiosk.


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_item_count">item_count</a>(self: &<a href="kiosk.md#0x2_kiosk_Kiosk">kiosk::Kiosk</a>): u32
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_item_count">item_count</a>(self: &<a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a>): u32 {
    self.item_count
}
</code></pre>



</details>

<a name="0x2_kiosk_profits_amount"></a>

## Function `profits_amount`

Get the amount of profits collected by selling items.


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_profits_amount">profits_amount</a>(self: &<a href="kiosk.md#0x2_kiosk_Kiosk">kiosk::Kiosk</a>): <a href="../move-stdlib/u64.md#0x1_u64">u64</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_profits_amount">profits_amount</a>(self: &<a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a>): <a href="../move-stdlib/u64.md#0x1_u64">u64</a> {
    self.profits.value()
}
</code></pre>



</details>

<a name="0x2_kiosk_profits_mut"></a>

## Function `profits_mut`

Get mutable access to <code>profits</code> - owner only action.


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_profits_mut">profits_mut</a>(self: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">kiosk::Kiosk</a>, cap: &<a href="kiosk.md#0x2_kiosk_KioskOwnerCap">kiosk::KioskOwnerCap</a>): &<b>mut</b> <a href="balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="sui.md#0x2_sui_SUI">sui::SUI</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_profits_mut">profits_mut</a>(self: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a>, cap: &<a href="kiosk.md#0x2_kiosk_KioskOwnerCap">KioskOwnerCap</a>): &<b>mut</b> Balance&lt;SUI&gt; {
    <b>assert</b>!(self.<a href="kiosk.md#0x2_kiosk_has_access">has_access</a>(cap), <a href="kiosk.md#0x2_kiosk_ENotOwner">ENotOwner</a>);
    &<b>mut</b> self.profits
}
</code></pre>



</details>

<a name="0x2_kiosk_borrow"></a>

## Function `borrow`

Immutably borrow an item from the <code><a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a></code>. Any item can be <code><a href="borrow.md#0x2_borrow">borrow</a></code>ed
at any time.


<pre><code><b>public</b> <b>fun</b> <a href="borrow.md#0x2_borrow">borrow</a>&lt;T: store, key&gt;(self: &<a href="kiosk.md#0x2_kiosk_Kiosk">kiosk::Kiosk</a>, cap: &<a href="kiosk.md#0x2_kiosk_KioskOwnerCap">kiosk::KioskOwnerCap</a>, id: <a href="object.md#0x2_object_ID">object::ID</a>): &T
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="borrow.md#0x2_borrow">borrow</a>&lt;T: key + store&gt;(self: &<a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a>, cap: &<a href="kiosk.md#0x2_kiosk_KioskOwnerCap">KioskOwnerCap</a>, id: ID): &T {
    <b>assert</b>!(<a href="object.md#0x2_object_id">object::id</a>(self) == cap.`for`, <a href="kiosk.md#0x2_kiosk_ENotOwner">ENotOwner</a>);
    <b>assert</b>!(self.<a href="kiosk.md#0x2_kiosk_has_item">has_item</a>(id), <a href="kiosk.md#0x2_kiosk_EItemNotFound">EItemNotFound</a>);

    dof::borrow(&self.id, <a href="kiosk.md#0x2_kiosk_Item">Item</a> { id })
}
</code></pre>



</details>

<a name="0x2_kiosk_borrow_mut"></a>

## Function `borrow_mut`

Mutably borrow an item from the <code><a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a></code>.
Item can be <code>borrow_mut</code>ed only if it's not <code>is_listed</code>.


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_borrow_mut">borrow_mut</a>&lt;T: store, key&gt;(self: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">kiosk::Kiosk</a>, cap: &<a href="kiosk.md#0x2_kiosk_KioskOwnerCap">kiosk::KioskOwnerCap</a>, id: <a href="object.md#0x2_object_ID">object::ID</a>): &<b>mut</b> T
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_borrow_mut">borrow_mut</a>&lt;T: key + store&gt;(self: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a>, cap: &<a href="kiosk.md#0x2_kiosk_KioskOwnerCap">KioskOwnerCap</a>, id: ID): &<b>mut</b> T {
    <b>assert</b>!(self.<a href="kiosk.md#0x2_kiosk_has_access">has_access</a>(cap), <a href="kiosk.md#0x2_kiosk_ENotOwner">ENotOwner</a>);
    <b>assert</b>!(self.<a href="kiosk.md#0x2_kiosk_has_item">has_item</a>(id), <a href="kiosk.md#0x2_kiosk_EItemNotFound">EItemNotFound</a>);
    <b>assert</b>!(!self.<a href="kiosk.md#0x2_kiosk_is_listed">is_listed</a>(id), <a href="kiosk.md#0x2_kiosk_EItemIsListed">EItemIsListed</a>);

    dof::borrow_mut(&<b>mut</b> self.id, <a href="kiosk.md#0x2_kiosk_Item">Item</a> { id })
}
</code></pre>



</details>

<a name="0x2_kiosk_borrow_val"></a>

## Function `borrow_val`

Take the item from the <code><a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a></code> with a guarantee that it will be returned.
Item can be <code>borrow_val</code>-ed only if it's not <code>is_listed</code>.


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_borrow_val">borrow_val</a>&lt;T: store, key&gt;(self: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">kiosk::Kiosk</a>, cap: &<a href="kiosk.md#0x2_kiosk_KioskOwnerCap">kiosk::KioskOwnerCap</a>, id: <a href="object.md#0x2_object_ID">object::ID</a>): (T, <a href="kiosk.md#0x2_kiosk_Borrow">kiosk::Borrow</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_borrow_val">borrow_val</a>&lt;T: key + store&gt;(self: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a>, cap: &<a href="kiosk.md#0x2_kiosk_KioskOwnerCap">KioskOwnerCap</a>, id: ID): (T, <a href="kiosk.md#0x2_kiosk_Borrow">Borrow</a>) {
    <b>assert</b>!(self.<a href="kiosk.md#0x2_kiosk_has_access">has_access</a>(cap), <a href="kiosk.md#0x2_kiosk_ENotOwner">ENotOwner</a>);
    <b>assert</b>!(self.<a href="kiosk.md#0x2_kiosk_has_item">has_item</a>(id), <a href="kiosk.md#0x2_kiosk_EItemNotFound">EItemNotFound</a>);
    <b>assert</b>!(!self.<a href="kiosk.md#0x2_kiosk_is_listed">is_listed</a>(id), <a href="kiosk.md#0x2_kiosk_EItemIsListed">EItemIsListed</a>);

    (dof::remove(&<b>mut</b> self.id, <a href="kiosk.md#0x2_kiosk_Item">Item</a> { id }), <a href="kiosk.md#0x2_kiosk_Borrow">Borrow</a> { kiosk_id: <a href="object.md#0x2_object_id">object::id</a>(self), item_id: id })
}
</code></pre>



</details>

<a name="0x2_kiosk_return_val"></a>

## Function `return_val`

Return the borrowed item to the <code><a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a></code>. This method cannot be avoided
if <code>borrow_val</code> is used.


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_return_val">return_val</a>&lt;T: store, key&gt;(self: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">kiosk::Kiosk</a>, item: T, <a href="borrow.md#0x2_borrow">borrow</a>: <a href="kiosk.md#0x2_kiosk_Borrow">kiosk::Borrow</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_return_val">return_val</a>&lt;T: key + store&gt;(self: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a>, item: T, <a href="borrow.md#0x2_borrow">borrow</a>: <a href="kiosk.md#0x2_kiosk_Borrow">Borrow</a>) {
    <b>let</b> <a href="kiosk.md#0x2_kiosk_Borrow">Borrow</a> { kiosk_id, item_id } = <a href="borrow.md#0x2_borrow">borrow</a>;

    <b>assert</b>!(<a href="object.md#0x2_object_id">object::id</a>(self) == kiosk_id, <a href="kiosk.md#0x2_kiosk_EWrongKiosk">EWrongKiosk</a>);
    <b>assert</b>!(<a href="object.md#0x2_object_id">object::id</a>(&item) == item_id, <a href="kiosk.md#0x2_kiosk_EItemMismatch">EItemMismatch</a>);

    dof::add(&<b>mut</b> self.id, <a href="kiosk.md#0x2_kiosk_Item">Item</a> { id: item_id }, item);
}
</code></pre>



</details>

<a name="0x2_kiosk_kiosk_owner_cap_for"></a>

## Function `kiosk_owner_cap_for`

Get the <code>for</code> field of the <code><a href="kiosk.md#0x2_kiosk_KioskOwnerCap">KioskOwnerCap</a></code>.


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_kiosk_owner_cap_for">kiosk_owner_cap_for</a>(cap: &<a href="kiosk.md#0x2_kiosk_KioskOwnerCap">kiosk::KioskOwnerCap</a>): <a href="object.md#0x2_object_ID">object::ID</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_kiosk_owner_cap_for">kiosk_owner_cap_for</a>(cap: &<a href="kiosk.md#0x2_kiosk_KioskOwnerCap">KioskOwnerCap</a>): ID {
    cap.`for`
}
</code></pre>



</details>

<a name="0x2_kiosk_purchase_cap_kiosk"></a>

## Function `purchase_cap_kiosk`

Get the <code>kiosk_id</code> from the <code><a href="kiosk.md#0x2_kiosk_PurchaseCap">PurchaseCap</a></code>.


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_purchase_cap_kiosk">purchase_cap_kiosk</a>&lt;T: store, key&gt;(self: &<a href="kiosk.md#0x2_kiosk_PurchaseCap">kiosk::PurchaseCap</a>&lt;T&gt;): <a href="object.md#0x2_object_ID">object::ID</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_purchase_cap_kiosk">purchase_cap_kiosk</a>&lt;T: key + store&gt;(self: &<a href="kiosk.md#0x2_kiosk_PurchaseCap">PurchaseCap</a>&lt;T&gt;): ID {
    self.kiosk_id
}
</code></pre>



</details>

<a name="0x2_kiosk_purchase_cap_item"></a>

## Function `purchase_cap_item`

Get the <code>Item_id</code> from the <code><a href="kiosk.md#0x2_kiosk_PurchaseCap">PurchaseCap</a></code>.


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_purchase_cap_item">purchase_cap_item</a>&lt;T: store, key&gt;(self: &<a href="kiosk.md#0x2_kiosk_PurchaseCap">kiosk::PurchaseCap</a>&lt;T&gt;): <a href="object.md#0x2_object_ID">object::ID</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_purchase_cap_item">purchase_cap_item</a>&lt;T: key + store&gt;(self: &<a href="kiosk.md#0x2_kiosk_PurchaseCap">PurchaseCap</a>&lt;T&gt;): ID {
    self.item_id
}
</code></pre>



</details>

<a name="0x2_kiosk_purchase_cap_min_price"></a>

## Function `purchase_cap_min_price`

Get the <code>min_price</code> from the <code><a href="kiosk.md#0x2_kiosk_PurchaseCap">PurchaseCap</a></code>.


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_purchase_cap_min_price">purchase_cap_min_price</a>&lt;T: store, key&gt;(self: &<a href="kiosk.md#0x2_kiosk_PurchaseCap">kiosk::PurchaseCap</a>&lt;T&gt;): <a href="../move-stdlib/u64.md#0x1_u64">u64</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_purchase_cap_min_price">purchase_cap_min_price</a>&lt;T: key + store&gt;(self: &<a href="kiosk.md#0x2_kiosk_PurchaseCap">PurchaseCap</a>&lt;T&gt;): <a href="../move-stdlib/u64.md#0x1_u64">u64</a> {
    self.min_price
}
</code></pre>



</details>
