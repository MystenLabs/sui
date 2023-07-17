---
title: Sui Kiosk
---

Sui Kiosk is a primitive, or building block (a module in the Sui Framework), you can use to build a trading platform for digital assets. Sui Kiosk supports adding digital assets that you can store and list for sale to other users. You can also define rules for the kiosk as part of a transfer policy that controls how purchasers can use the asset after purchase. 
To add digital assets to your kiosk to list for sale, you must first create and publish a package to Sui as part of a programmable transaction block. The package must define a type (T). You can then create a transfer policy (`TransferPolicy`) using the `Publisher` object. The transfer policy which determines the conditions that must be met for the purchase to succeed. You can specify different requirements for each asset and transaction. To learn more about transfer policies, see [Transfer Policy](https://github.com/MystenLabs/sui/blob/main/crates/sui-framework/docs/transfer_policy.md). To learn more about using the Publisher object, see [Publisher](https://examples.sui.io/basics/publisher.html) in Sui by Example.

To extend and customize Kiosk functionality, Sui Kiosk also supports building extensions that take full advantage of the highly accessible, composable, and dynamic nature of objects on Sui.

## Sui Kiosk architecture

A Sui Kiosk is a single, typically shared `Kiosk` object. Each `Kiosk` object has a owned capability, `KioskOwnerCap`, to manage it. The `Kiosk` object provides a rich base for building custom extensions, such as creating a marketplace for your digital assets, while maintaining strong security and protected ownership of your assets . 


To see all of the properties supported in Sui Kiosk, see [Sui Kiosk](https://github.com/MystenLabs/sui/blob/main/crates/sui-framework/docs/kiosk.md) in the Sui Framework documentation.

## Sui Kiosk design principles

Sui Kiosk is built on the following principles:
**True ownership** - Kiosk is a shared object. Each has a shared state and anyone can access a Kiosk on the network. Only the kiosk owner (an address or entity such as a dApp), can perform actions on the kiosk, such as taking assets from it or listing an asset in the kiosk. The only action available to everyone else is to purchase an asset that the kiosk owner listed from the kiosk.

* **Foundation for commerce applications on Sui** - Sui Kiosk defines a base framework to build on, and each change or enhancement goes through rigorous testing to ensure compatibility within the community.
* **Permissioned expandability** - The base implementation of Sui Kiosk is generic. Any community or third-party extensions and modifications must maintain and ensure asset security. For example, any transferring of ownership for a kiosk must be clearly communicated to users and require their consent. Any extensions built on Sui Kiosk must use an explicit entry call to install them transparently.

## Sui Kiosk for collectors and traders

Anyone can create a Sui Kiosk. As a kiosk owner, you can sell any asset with a type (T) that has a shared `TransferPolicy` available, or you can use a kiosk to store assets even without a shared policy. You can’t sell or transfer any assets from your kiosk that do not have an associated transfer policy available. 
To sell an item, if there is an existing transfer policy for the type (T), you just add your assets to your kiosk and then list them. You specify an offer amount when you list an item. Anyone can then purchase the item for the amount of SUI specified in the listing. The associated transfer policy determines what the buyer can do with the purchased asset.

## Sui Kiosk for marketplaces

As a marketplace operator, you can implement Sui Kiosk to watch for offers made in a collection of kiosks and display them on a marketplace site. You can also implement a custom system using Kiosk extensions (created by the community or third-parties). For example, marketplaces can use a `TransferPolicyCap` to implement application-specific transfer rules.

## Sui Kiosk for creators

As a creator, Sui Kiosk supports strong enforcement for transfer policies and associated rules to protect assets and enforce asset ownership. Sui Kiosk gives creators more control over their creations, and puts creators and owners in control of how their works can be used.

## Create a Sui Kiosk

To create a Sui Kiosk, you need to have an active address on the Sui network you use. The address is the owner of the `Kiosk` object you create.
To create a base Sui kiosk (`Kiosk` object):

```rust
kiosk::new(Kiosk, KioskOwnerCap)
```

The function returns the `Kiosk` object and the `KioskOwnerCap`, an object that grants full access to the `Kiosk` and makes the holder the `Kiosk` owner.
For a full list of the available parameters, see [Sui Kiosk](https://github.com/MystenLabs/sui/blob/main/crates/sui-framework/docs/kiosk.md) in the Sui Framework documentation.

## Place an item in your Sui Kiosk

To add an asset to your kiosk, use the `place` function. Only the kiosk owner has permission to perform actions on a `Kiosk` object - you must hold the `KioskOwnerCap` to [`place`](https://github.com/MystenLabs/sui/blob/main/crates/sui-framework/docs/kiosk.md#0x2_kiosk_place) or [`withdraw`](https://github.com/MystenLabs/sui/blob/main/crates/sui-framework/docs/kiosk.md#function-withdraw) an item from a kiosk.

To place an item in a Sui kiosk:

```rust
kiosk::place(Kiosk, KioskOwnerCap, id)
```

To place an item in your kiosk as a [dynamic field](https://docs.sui.io/build/programming-with-objects/ch5-dynamic-fields):

```rust
kiosk::place(Kiosk, KioskOwnerCap, Item)
```

If you also want to list an item after you place it in a kiosk, you can use the [`place_and_list`](https://github.com/MystenLabs/sui/blob/main/crates/sui-framework/docs/kiosk.md#0x2_kiosk_place_and_list) function to place the item in the kiosk and then list it.
To place and list an item in a Sui kiosk:

```rust
kiosk::place_and_list(Kiosk, KioskOwnerCap, id, price)
```

### Asset states in Sui kiosk

Sui Kiosk is a shared object that can store heterogeneous values, such as different sets of asset collectibles. When you add an asset to your kiosk, it has one of the following states:
* PLACED - an item placed in the kiosk using the `kiosk::place` function. The Kiosk Owner can withdraw it and use it directly, borrow it mutably or immutably, or list an item for sale.
* LOCKED - an item placed in the kiosk using the `kiosk::lock` function. You can’t withdraw a  Locked item from a kiosk, but you can borrow it mutably and list it for sale. Any item placed in a kiosk that has an associated Kiosk Lock policy have a LOCKED state.
* LISTED - an item in the kiosk that is listed for sale using the `kiosk::list` or `kiosk::place_and_list` functions. You can’t modify an item while listed, but you can borrow it immutably or delist it, which returns it to its previous state.
* LISTED EXCLUSIVELY - an item placed or locked in the kiosk by an extension that calls the  `kiosk::list_with_purchase_cap` function. Only the kiosk owner can approve calling the function. The owner can only borrow it immutably. The extension must provide the functionality to delist / unlock the asset, or it might stay locked forever. Given that this action is explicitly performed by the Owner - it is the responsibility of the Owner to choose verified and audited extensions to use.

When someone purchases an asset from a kiosk, the asset leaves the kiosk and ownership transfers to the buyer’s address.

## List items from your kiosk

As a kiosk owner, you can use the [`list`](https://github.com/MystenLabs/sui/blob/main/crates/sui-framework/docs/kiosk.md#0x2_kiosk_list) function to list any item you placed in the kiosk. Buyers can purchase only items that have an associated transfer policy ([`TransferPolicy`](https://github.com/MystenLabs/sui/blob/main/crates/sui-framework/docs/transfer_policy.md) object) that is shared and available to them. Use the `list` function to list assets already placed in your kiosk.

To list an item in a Sui kiosk:

```rust
kiosk::list(Kiosk, KioskOwnerCap, ID, price)
```

## List assets that include a PurchaseCap

When you use the `list` function, you specify a fixed price that someone must pay to purchase it. The listing is available to anyone. To list an item without a set price, you can use the [`kiosk::list_with_purchase_cap`] (https://github.com/MystenLabs/sui/blob/main/crates/sui-framework/docs/kiosk.md#0x2_kiosk_list_with_purchase_cap) function to include a `PurchaseCap`. A `PurchaseCap`is an object that grants exclusive rights to purchase a specific asset. The asset gets a state of LISTED EXCLUSIVELY. You can’t take the asset from the kiosk, delist it, or modify it until the `PurchaseCap` is returned (which unlocks the asset) or the holder uses it to purchase the asset. You can use `PurchaseCap` to implement any special trading conditions you want, such as for an auction, or to send to a user directly.

To list an asset with a `PurchaseCap`:

```rust
kiosk::list_with_purchase_cap(Kiosk, KioskOwnerCap, id, min_price)
```

Only the user that holds the `PurchaseCap` can purchase the associated asset.

## Remove an item from your Sui kiosk

To remove an item from a kiosk, as the kiosk owner, use the `kiosk::take` function. You can take only items that are not locked or listed. 

To take an item from your Sui kiosk:

```rust
kiosk::take(Kiosk, KioskOwnerCap, ID): Item
```

## Purchase an item from a kiosk

To purchase a listed item from a kiosk, use the [`kiosk::purchase`](https://github.com/MystenLabs/sui/blob/main/crates/sui-framework/docs/kiosk.md#function-purchase) function. The asset becomes available to the purchaser when the TransferRequest for the purchase resolves. If there is a policy that enforces locking, such a kiosk lock rule, he asset must be placed into the purchaser's kiosk.

To purchase an item listed on a kiosk:

```rust
kiosk::purchase(Kiosk, ID, Coin): (Item, TransferRequest<Item>)
```

## Lock an item in your Sui kiosk

You can lock items in your kiosk using the [`kiosk::lock`](https://github.com/MystenLabs/sui/blob/main/crates/sui-framework/docs/kiosk.md#0x2_kiosk_lock) function. You can lock only items that have an associated `TransferPolicy`. To lock an item in a Kiosk, you must include the `TransferPolicy` for the item as a transaction argument. This requirement prevents asset loss from locking an asset forever, such as when one inadvertently locks an item and then can’t take the item from the kiosk or list it without a transfer policy.
