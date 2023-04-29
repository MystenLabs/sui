---
title: Package Upgrades
---

Sui smart contracts are represented by immutable package objects consisting of a collection of Move modules. Because the packages are immutable, transactions can safely access smart contracts without full consensus (fast-path transactions). If someone could change these packages, they would become [shared objects](../learn/objects.md#shared), which would require full consensus before completing a transaction. 

The inability to change package objects, however, becomes a problem when considering the iterative nature of code development. Builders require the ability to update their code and pull changes from other developers while still being able to reap the benefits of fast-path transactions. Fortunately, the Sui network provides a method of upgrading your packages while still retaining their immutable properties.   

This topic examines how to upgrade packages using the Sui Client CLI. Move developers can reference the [package module documentation](https://github.com/MystenLabs/sui/blob/main/crates/sui-framework/docs/package.md) for options on working with package upgrades on a code level.  

## Upgrade considerations

There are some details of the process that you should consider before upgrading your packages.

For example, module initializers do not re-run with package upgrades. When you publish your initial package, Sui Move runs the `init` function you define for the package once (and only once) at the time of the publish event. Any `init` functions you might include in subsequent versions of your package are ignored.

As alluded to previously, all packages on the Sui network are immutable. Because of this fact, you cannot delete old packages from the chain. As a result, there is nothing that prevents other packages from accessing the methods and types defined in the old versions of your upgraded packages. By default, users can choose to keep using the old version of a package, as well. As a package developer, you must be aware of and account for this possibility.

For example, you might define an `increment` function in your original package:

```rust
public entry fun increment(c: &mut Counter) {
    c.value = c.value + 1;
}
```

Then, your package upgrade might add an emit event to the `increment` function:

```rust
struct Progress has copy, drop {
    reached: u64
}

public entry fun increment(c: &mut Counter) {
    c.value = c.value + 1;

    if (c.value % 100 == 0) {
        event::emit(Progress { reached: c.value });
    }
}
```

If there is a mix of callers for both the old and upgraded `increment` function, then the process fails because the old function is not aware of the `Progress` event. 

Similar to mismatched function definitions, you might also run into issues maintaining dynamic fields that need to remain in sync with a struct's original fields. To address these issues, you can introduce a new type as part of the upgrade and port users over to it, breaking backwards compatibility. For example, if you're using owned objects to demonstrate proof, like proof of ownership, and you develop a new version of your package to address problematic code, you can introduce a new type in the upgraded package. You can then add a function to your package that trades old objects for new ones. Because your logic only recognizes objects with the new type, you effectively force users to update.

Another example of having users update to the latest package is when you have a bookkeeping shared object in your package that you discover has flawed logic so is not functioning as expected. As in the previous example, you want users to use only the object defined in the upgraded package with the correct logic, so you add a new type and migration function to your package upgrade. This process requires a couple of transactions, one for the upgrade and another that you call from the upgraded package to set up the new shared object that replaces the existing one. To protect the setup function, you would need to create an `AdminCap` object or similar as part of your package to make sure you, as the package owner, are the only authorized initiator of that function. Perhaps even more useful, you might include a flag in the shared object that allows you, as the package owner, to toggle the enabled state of that shared object. You can add a check for the enabled state to prevent access to that object from the on-chain public while you perform the migration. Of course, you would probably create this flag only if you expected to perform this migration at some point in the future, not because you're intentionally developing objects with flawed logic. 

### Versioned shared objects

When you create packages that involve shared objects, you need to think about upgrades and versioning from the start given that **all prior versions of a package still exist on-chain**. A useful pattern is to introduce versioning to the shared object and using a version check to guard access to functions in the package. This enables you to limit access to the shared object to only the latest version of a package.

Considering the earlier `counter` example, which might have started life as follows:

```rust
module example::counter {
    use sui::object::{Self, UID};
    use sui::transfer;
    use sui::tx_context::TxContext;

    struct Counter has key {
        id: UID,
        value: u64,
    }

    fun init(ctx: &mut TxContext) {
        transfer::share_object(Counter {
            id: object::new(ctx),
            value: 0,
        })
    }

    public entry fun increment(c: &mut Counter) {
        c.value = c.value + 1;
    }
}
```

To ensure that upgrades to this package can limit access of the shared object to the latest version of the package, you need to:

1. Track the current version of the module in a constant, `VERSION`.
2. Track the current version of the shared object, `Counter`, in a new `version` field.
3. Introduce an `AdminCap` to protect privileged calls, and associate the `Counter` with its `AdminCap` with a new field (you might already
   have a similar type for shared object administration, in which case you can re-use that). This cap is used to protect calls to migrate the shared object from version to version.
4. Guard the entry of all functions that access the shared object with a check that its `version` matches the package `VERSION`.
  
An upgrade-aware `counter` module that incorporates all these ideas looks as follows:

```rust
module example::counter {
    use sui::object::{Self, ID, UID};
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};

    // 1. Track the current version of the module 
    const VERSION: u64 = 1;

    struct Counter has key {
        id: UID,
        // 2. Track the current version of the shared object
        version: u64,
        // 3. Associate the `Counter` with its `AdminCap`
        admin: ID,
        value: u64,
    }

    struct AdminCap has key {
        id: UID,
    }

    /// Not the right admin for this counter
    const ENotAdmin: u64 = 0;

    /// Calling functions from the wrong package version
    const EWrongVersion: u64 = 1;

    fun init(ctx: &mut TxContext) {
        let admin = AdminCap {
            id: object::new(ctx),
        };

        transfer::share_object(Counter {
            id: object::new(ctx),
            version: VERSION,
            admin: object::id(&admin),
            value: 0,
        });

        transfer::transfer(admin, tx_context::sender(ctx));
    }

    public entry fun increment(c: &mut Counter) {
        // 4. Guard the entry of all functions that access the shared object 
        //    with a version check.
        assert!(c.version == VERSION, EWrongVersion);
        c.value = c.value + 1;
    }
}
```

To upgrade a module using this pattern requires making two extra changes, on top of any implementation changes your upgrade requires:

1. Bump the `VERSION` of the package
2. Introduce a `migrate` function to upgrade the shared object:

The following module is an upgraded `counter` that emits `Progress` events as originally discussed, but also provides tools for an admin
(`AdminCap` holder) to prevent accesses to the counter from older package versions:

```rust
module example::counter {
    use sui::event;
    use sui::object::{Self, ID, UID};
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};
    // 1. Bump the `VERSION` of the package.
    const VERSION: u64 = 2;
    struct Counter has key {
        id: UID,
        version: u64,
        admin: ID,
        value: u64,
    }
    struct AdminCap has key {
        id: UID,
    }
    struct Progress has copy, drop {
        reached: u64,
    }
    /// Not the right admin for this counter
    const ENotAdmin: u64 = 0;
    /// Migration is not an upgrade
    const ENotUpgrade: u64 = 1;
    /// Calling functions from the wrong package version
    const EWrongVersion: u64 = 2;
    fun init(ctx: &mut TxContext) {
        let admin = AdminCap {
            id: object::new(ctx),
        };
        transfer::share_object(Counter {
            id: object::new(ctx),
            version: VERSION,
            admin: object::id(&admin),
            value: 0,
        });
        transfer::transfer(admin, tx_context::sender(ctx));
    }
    public entry fun increment(c: &mut Counter) {
        assert!(c.version == VERSION, EWrongVersion);
        c.value = c.value + 1;
        if (c.value % 100 == 0) {
            event::emit(Progress { reached: c.value })
        }
    }
    // 2. Introduce a migrate function
    entry fun migrate(c: &mut Counter, a: &AdminCap) {
        assert!(c.admin == object::id(a), ENotAdmin);
        assert!(c.version < VERSION, ENotUpgrade);
        c.version = VERSION;
    }
}
```

Upgrading to this version of the package requires performing the package upgrade, and calling the `migrate` function in a follow-up
transaction.  Note that the `migrate` function:

- Is an `entry` function and **not `public`**.  This allows it to be entirely changed (including changing its signature or removing it
  entirely) in later upgrades.
- Accepts an `AdminCap` and checks that its ID matches the ID of the counter being migrated, making it a privileged operation.
- Includes a sanity check that the version of the module is actually an upgrade for the object. This helps catch errors such as failing to bump the module version before upgrading.

After a successful upgrade, calls to `increment` on the previous version of the package aborts on the version check, while calls on
the later version should succeed.

### Extensions

This pattern forms the basis for upgradeable packages involving shared objects, but you can extend it in a number of ways, depending on your
package's needs:

- The version constraints can be made more expressive:
  - Rather than using a single `u64`, versions could be specified as a `String`, or a pair of upper and lowerbounds.
  - You can control access to specific functions or sets of functions by adding and removing marker types as dynamic fields
    on the shared object.
- The `migrate` function could be made more sophisticated (modifying other fields in the shared object, adding/removing dynamic fields, migrating multiple shared objects simultaneously).
- You can implement large migrations that need to run over multiple transactions in a three phase set-up:
  - Disable general access to the shared object by setting its version to a sentinel value (e.g. `U64_MAX`), using an `AdminCap`-guarded
    call.
  - Run the migration over the course of multiple transactions (e.g. if a large volume of objects need to be moved, it is best to
    do this in batches, to avoid hitting transaction limits).
  - Set the version of the shared object back to a usable value.

## Requirements

To upgrade a package, your package must satisfy the following requirements:
* You must have an `UpgradeTicket` for the package you want to upgrade. The Sui network issues `UpgradeCap`s when you publish a package, then you can issue `UpgradeTicket`s as the owner of that `UpgradeCap`. The Sui Client CLI handles this requirement automatically.
* Your changes must be layout-compatible with the previous version. 
    * Existing `public` function signatures and struct layouts must remain the same.
    * You can add new structs and functions.
    * You can add abilities to existing structs.
    * You can remove generic type constraints from existing functions (public or otherwise).
    * You can change function implementations.
    * You can change non-`public` function signatures, including `friend` and `entry` function signatures.

**Note:** If you have a package with a dependency, and that dependency is upgraded, your package does not automatically depend on the newer version. You must explicitly upgrade your own package to point to the new dependency.

## Upgrading

Use the `sui client upgrade` command to upgrade packages that meet the previous requirements, providing values for the following flags:

* `--gas-budget`: The maximum number of gas units that can be expended before the network cancels the transaction.
* `--cap`: The ID of the `UpgradeCap`. You receive this ID as a return from the publish command.

Developers upgrading packages using Move code have access to types and functions to define custom upgrade policies. For example, a Move developer might want to disallow upgrading a package, regardless of the current package owner. The [`make_immutable` function](https://github.com/MystenLabs/sui/blob/main/crates/sui-framework/docs/package.md#0x2_package_make_immutable) is available to them to create this behavior. More advanced policies using available types like `UpgradeTicket` and `Upgrade Receipt` are also possible. For an example, see this [custom upgrade policy](https://github.com/MystenLabs/sui/issues/2045#:~:text=Implement%20a%20custom%20upgrade%20policy) on GitHub.

When you use the Sui Client CLI, the `upgrade` command handles generating the upgrade digest, authorizing the upgrade with the `UpgradeCap` to get an `UpgradeTicket`, and updating the `UpgradeCap` with the `UpgradeReceipt` after a successful upgrade. To learn more about these processes, see the Move documentation for the [package module](https://github.com/MystenLabs/sui/blob/main/crates/sui-framework/docs/package.md). 

## Example

You develop a package named `sui_package`. Its manifest looks like the following:

```move
[package]
name = "sui_package"
version = "0.0.0"

[addresses]
sui_package = "0x0"
```

When your package is ready, you publish it:

```shell
sui client publish --gas-budget <GAS-BUDGET-AMOUNT>
```
And receive the response:

```shell
INCLUDING DEPENDENCY Sui
INCLUDING DEPENDENCY MoveStdlib
BUILDING MyFirstPackage
Successfully verified dependencies on-chain against source.
----- Transaction Digest ----
2bn3EtHvbVY4bM1887MvFiGWnqq1YZ2RKmbeK7TrRbLL
----- Transaction Data ----
Transaction Signature: [Signature(Ed25519SuiSignature(Ed25519SuiSignature([0, 156, 133, 71, 156, 44, 204, 30, 31, 250, 204, 247, 60, 212, 249, 61, 112, 249, 148, 180, 83, 207, 236, 58, 99, 134, 5, 174, 115, 226, 41, 139, 192, 1, 183, 133, 38, 73, 254, 205, 190, 54, 210, 112, 144, 204, 137, 3, 8, 30, 165, 147, 120, 199, 227, 119, 53, 208, 28, 101, 34, 239, 102, 210, 1, 103, 111, 108, 165, 156, 100, 95, 13, 236, 27, 13, 127, 150, 50, 47, 155, 217, 27, 164, 61, 245, 254, 81, 182, 121, 231, 58, 150, 214, 46, 27, 222])))]
Transaction Kind : Programmable
Inputs: [Pure(SuiPureValue { value_type: Some(Address), value: "<PUBLISHER-ID>" })]
Commands: [
  Publish(_,,0x00000000000000000000000000000000000000000000000000000000000000010x0000000000000000000000000000000000000000000000000000000000000002),
  TransferObjects([Result(0)],Input(0)),
]

Sender: <PUBLISHER-ID>
Gas Payment: Object ID:, version: 0x6, digest: HLAcq3SFPZm4xvcPryXk5MjA718xGVnTYCdtWbFsaJpe 
Gas Owner: <PUBLISHER-ID>
Gas Price: 1
Gas Budget: <GAS-BUDGET-AMOUNT>

----- Transaction Effects ----
Status : Success
Created Objects:
  - ID: <ORIGINAL-PACKAGE-ID> , Owner: Immutable
  - ID: <UPGRADE-CAP-ID> , Owner: Account Address ( <PUBLISHER-ID> )
  - ID: <PUBLISHER-ID> , Owner: Account Address ( <PUBLISHER-ID> )
Mutated Objects:
  - ID: <GAS-COIN-ID> , Owner: Account Address ( <PUBLISHER-ID> )

----- Events ----
Array []
----- Object changes ----
Array [
    Object {
        "type": String("mutated"),
        "sender": String("<PUBLISHER-ID>"),
        "owner": Object {
            "AddressOwner": String("<PUBLISHER-ID>"),
        },
        "objectType": String("0x2::coin::Coin<0x2::sui::SUI>"),
        "objectId": String("<GAS-COIN-ID>"),
        "version": Number(7),
        "previousVersion": Number(6),
        "digest": String("6R39f68p4tGqJWJTakKCyL8tz2w2XTvJ3Mu5nGwxadda"),
    },
    Object {
        "type": String("published"),
        "packageId": String("<ORIGINAL-PACKAGE-ID>"),
        "version": Number(1),
        "digest": String("FrBhLF2Rn4jP3SUsss7aXqwDDRtoKxgGbPm8eVkH7jrQ"),
        "modules": Array [
            String("sui_package"),
        ],
    },
    Object {
        "type": String("created"),
        "sender": String("<PUBLISHER-ID>"),
        "owner": Object {
            "AddressOwner": String("<PUBLISHER-ID>"),
        },
        "objectType": String("0x2::package::UpgradeCap"),
        "objectId": String("<UPGRADE-CAP-ID>"),
        "version": Number(7),
        "digest": String("BoGQ63r27DFZDMC8p7YwRcDpToFpbZ9rG1R4o4uXkaUw"),
    },
    Object {
        "type": String("created"),
        "sender": String("<PUBLISHER-ID>"),
        "owner": Object {
            "AddressOwner": String("<PUBLISHER-ID>"),
        },
        "objectType": String("<ORIGINAL-PACKAGE-ID>::sui_package::<MODULE-NAME>"),
        "objectId": String("<PACKAGE-ID>"),
        "version": Number(7),
        "digest": String("BC3KeuATKJozLNipbUz2GWzoDXbodXH4HLm59TxJSmVd"),
    },
]
----- Balance changes ----
Array [
    Object {
        "owner": Object {
            "AddressOwner": String("<PUBLISHER-ID>"),
        },
        "coinType": String("0x2::sui::SUI"),
        "amount": String("-9328480"),
    },
]
```

The result includes an **Object changes** section with two pieces of information you need for upgrading, an `UpgradeCap` ID and your package ID. 

You can identify the different objects using the `Object.objectType` value in the response. The `UpgradeCap` entry has a value of `String("0x2::package::UpgradeCap")` and the `objectType` for the package reads `String("<PACKAGE-ID>::sui_package::<MODULE-NAME>")`

To make sure your other packages can use this package as a dependency, you must update the manifest (Move.toml file) for your package to include this information. 

Update the alias address and add a new `published-at` entry in the `[package]` section, both pointing to the value of the on-chain ID:

```toml
[package]
name = "sui_package"
version = "0.0.0"
published-at = "<ORIGINAL-PACKAGE-ID>"

[addresses]
sui_package = "<ORIGINAL-PACKAGE-ID>"
```

The `published-at` value allows the Move compiler to verify dependencies against on-chain versions of those packages.

After a while, you decide to upgrade your `sui_package` to include some requested features. Before running the `upgrade` command, you need to edit the manifest again. In the `[addresses]` section, you update the `sui_package` address value to `0x0` again so the validator issues a new address for the upgrade package. You can leave the `published-at` value the same, because it is only read by the toolchain when publishing a dependent package. The saved manifest now resembles the following:

```toml
[package]
name = "sui_package"
version = "0.0.1"
published-at = "<ORIGINAL-PACKAGE-ID>"

[addresses]
sui_package = "0x0"
```    

With the new manifest and code in place, it's time to use the `sui client upgrade` command to upgrade your package. Pass the `UpgradeCap` ID (the `<UPGRADE-CAP-ID>` value from the example) to the `--upgrade-capability` flag.

```shell
sui client upgrade --gas-budget <GAS-BUDGET-AMOUNT> --upgrade-capability <UPGRADE-CAP-ID>
```

The console alerts you if the new package doesn't satisfy [requirements](#requirements), otherwise the compiler publishes the upgraded package to the network and returns its result:

```shell
INCLUDING DEPENDENCY Sui
INCLUDING DEPENDENCY MoveStdlib
BUILDING MyFirstPackage
Successfully verified dependencies on-chain against source.
----- Transaction Digest ----
HZdnGWE2VoqDWwBhoBwe17tDFn7uYgfBpK5nk75Rmh5z
----- Transaction Data ----
Transaction Signature: [Signature(Ed25519SuiSignature(Ed25519SuiSignature([0, 108, 166, 235, 244, 238, 72, 232, 143, 49, 225, 180, 55, 63, 131, 155, 146, 126, 50, 158, 138, 213, 174, 71, 162, 222, 62, 198, 245, 219, 224, 171, 82, 43, 197, 56, 16, 252, 186, 83, 154, 109, 104, 90, 212, 236, 122, 78, 175, 173, 107, 9, 2, 10, 30, 74, 101, 138, 228, 251, 170, 39, 25, 242, 8, 103, 111, 108, 165, 156, 100, 95, 13, 236, 27, 13, 127, 150, 50, 47, 155, 217, 27, 164, 61, 245, 254, 81, 182, 121, 231, 58, 150, 214, 46, 27, 222])))]
Transaction Kind : Programmable
Inputs: [Object(ImmOrOwnedObject { object_id: <UPGRADE-CAP-ID>, version: SequenceNumber(9), digest: o#Bvy7R33o4ogLuyfzM76nmM1RqKnEALQrbd34CLWZhf5Y }), Pure(SuiPureValue { value_type: Some(U8), value: 0 }), Pure(SuiPureValue { value_type: Some(Vector(U8)), value: [202,122,179,32,64,155,14,236,160,5,75,17,159,202,125,114,234,36,182,41,159,84,56,222,99,121,250,82,206,19,212,5] })]
Commands: [
  MoveCall(0x0000000000000000000000000000000000000000000000000000000000000002::package::authorize_upgrade(,Input(0),Input(1)Input(2))),
  Upgrade(Result(0),,0x00000000000000000000000000000000000000000000000000000000000000010x0000000000000000000000000000000000000000000000000000000000000002, <ORIGINAL-PACKAGE-ID>, _)),
  MoveCall(0x0000000000000000000000000000000000000000000000000000000000000002::package::commit_upgrade(,Input(0)Result(1))),
]

Sender: <PUBLISHER-ID>
Gas Payment: Object ID: <GAS-COIN-ID>, version: 0x9, digest: 84ZKQcZZLTCmyAoRp9QhDrxxZ7nzGtdoBw18UbNm26ad 
Gas Owner: <PUBLISHER-ID>
Gas Price: 1
Gas Budget: <GAS-BUDGET-AMOUNT>

----- Transaction Effects ----
Status : Success
Created Objects:
  - ID: <MODULE-ID> , Owner: Immutable
Mutated Objects:
  - ID: <GAS-COIN-ID> , Owner: Account Address ( <PUBLISHER-ID> )
  - ID: <UPGRADE-CAP-ID> , Owner: Account Address ( <PUBLISHER-ID> )

----- Events ----
Array []
----- Object changes ----
Array [
    Object {
        "type": String("mutated"),
        "sender": String("<PUBLISHER-ID>"),
        "owner": Object {
            "AddressOwner": String("<PUBLISHER-ID>"),
        },
        "objectType": String("0x2::coin::Coin<0x2::sui::SUI>"),
        "objectId": String("<GAS-COIN-ID>"),
        "version": Number(10),
        "previousVersion": Number(9),
        "digest": String("EvfMLHBDXFRUeMd7vgmAMaacnwZbGFHg8d7Kov3fTt9L"),
    },
    Object {
        "type": String("mutated"),
        "sender": String("<PUBLISHER-ID>"),
        "owner": Object {
            "AddressOwner": String("<PUBLISHER-ID>"),
        },
        "objectType": String("0x2::package::UpgradeCap"),
        "objectId": String("<UPGRADE-CAP-ID>"),
        "version": Number(10),
        "previousVersion": Number(9),
        "digest": String("FZ9AruCAnhjW8zrozUMgtsY79SggTiHr3suwZNe5eMnM"),
    },
    Object {
        "type": String("published"),
        "packageId": String("<UPGRADED-PACKAGE-ID>"),
        "version": Number(2),
        "digest": String("8RDsE6kFND2V2gxGiytwxa815mctwxNh7A8YqRS4AJME"),
        "modules": Array [
            String("<MODULE-NAME>"),
        ],
    },
]
----- Balance changes ----
Array [
    Object {
        "owner": Object {
            "AddressOwner": String("<PUBLISHER-ID>"),
        },
        "coinType": String("0x2::sui::SUI"),
        "amount": String("-6350420"),
    },
]


```

The result provides a new ID for the upgraded package. As was the case before the upgrade, you need to include that information in your manifest so any of your other packages that depend on your `sui_package` know where to find the on-chain bytecode for verification. Edit your manifest once again to provide the upgraded package ID for the `published-at` value, and return the original `sui_package` ID value in the `[addresses]` section:


```move
[package]
name = "sui_package"
version = "0.0.1"
published-at = "<UPGRADED-PACKAGE-ID>"

[addresses]
sui_package = "<ORIGINAL-PACKAGE-ID>"
```

The `published-at` value changes with every upgrade. The ID for the `sui_package` in the `[addresses]` section always points to the original package ID after upgrading. You must always change that value back to `0x0`, however, before running the `upgrade` command so the validator knows to create a new ID for the upgrade. 
