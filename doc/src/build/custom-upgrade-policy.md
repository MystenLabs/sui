---
title: Custom Upgrade Policies
---

The ability to upgrade Sui Move packages provides the opportunity to iterate your package development, whether to continuously improve features or address logic defects. The `sui client upgrade` command offers an approachable way to upgrade packages when the CLI active address owns the `UpgradeCap` object associated with those packages. 

Using the Sui Client CLI is useful to get started with upgrades, or in the early stages of package development, but protecting the ability to upgrade a package on chain using a single key can pose a security risk for several reasons:

- The entity owning that key might make changes that are in their own interests but not the interests of the broader community.
- Upgrades might happen without enough time for package users to consult on the change or stop using the package if they disagree.
- The key might get lost.

You can make a package *immutable* when it goes live to mitigate this risk using the Sui Move `sui::package::make_immutable` function to destroy its `UpgradeCap`. Making the package immutable, however, prevents future bug fixes and new features, which might not be practical or desired.

To address the security risk a single key poses while still providing the opportunity to upgrade live packages, Sui offers *custom upgrade policies*. These policies protect `UpgradeCap` access behind arbitrary Sui Move code and issue `UpgradeTicket` objects that authorize upgrades on a case-by-case basis.

## Upgrade overview

Package upgrades must occur end-to-end in a single transaction block and are composed of three commands:

1. **Authorization:** Get permission from the `UpgradeCap` to perform
   the upgrade, creating an `UpgradeTicket`.
2. **Execution:** Consume the `UpgradeTicket` and verify the package
   bytecode and compatibility against the previous version, and create
   the on-chain object representing the upgraded package. Return an
   `UpgradeReceipt` as a result on success.
3. **Commit:** Update the `UpgradeCap` with information about the
   newly created package.

While step 2 is a built-in command, steps 1 and 3 are implemented as Move functions. The Sui framework provides their most basic implementation:

```rust
module sui::package {
    public fun authorize_upgrade(
        cap: &mut UpgradeCap,
        policy: u8,
        digest: vector<u8>
    ): UpgradeTicket;

    public fun commit_upgrade(
        cap: &mut UpgradeCap,
        receipt: UpgradeReceipt,
    );
}
```

These are the functions that `sui client upgrade` calls for authorization and commit. Custom upgrade policies work by guarding
access to a package `UpgradeCap` (and therefore to calls of these functions) behind extra conditions that are specific to that policy
(such as voting, governance, permission lists, timelocks, and so on).

Any pair of functions that produces an `UpgradeTicket` from an `UpgradeCap` and consumes an `UpgradeReceipt` to update an
`UpgradeCap` constitutes a custom upgrade policy.

## UpgradeCap

The `UpgradeCap` is the central type responsible for coordinating package upgrades.

```rust
module sui::package {
    struct UpgradeCap has key, store {
        id: UID,
        package: ID,
        version: u64,
        policy: u8,
    }
}
```

Publishing a package creates the `UpgradeCap` object and upgrading the package updates that object. The owner of this object has permission to:

- Change the compatibility requirements for future upgrades.
- Authorize future upgrades.
- Make the package immutable (not upgradeable).

And its API guarantees the following properties:

- Only the latest version of a package can be upgraded (a linear history is guaranteed).
- Only one upgrade can be in-flight at any time (cannot authorize multiple concurrent upgrades).
- An upgrade can only be authorized for the extent of a single transaction; no one can `store` the `UpgradeTicket` that proves authorization.
- Compatibility requirements for a package can be made only more restrictive over time.

## UpgradeTicket

```rust
module sui::package {
    struct UpgradeTicket {
        cap: ID,
        package: ID,
        policy: u8,
        digest: vector<u8>,
    }
}
```

An `UpgradeTicket` is proof that an upgrade has been authorized.  This authorization is specific to:

- A particular `package: ID` to upgrade from, which must be the latest package in the family identified by the `UpgradeCap` at `cap: ID`.
- A particular `policy: u8` that attests to the kind of compatibility guarantees that the upgrade expects to adhere to.
- A particular `digest: vector<u8>` that identifies the contents of the package after the upgrade.

When you attempt to run the upgrade, the validator checks that the upgrade it is about to perform matches the upgrade that was authorized along all those lines, and does not perform the upgrade if any of these criteria are not met.

After creating an `UpgradeTicket`, you must use it within that transaction block (you cannot store it for later, drop it, or burn it), or the transaction fails.

### Package digest

The `UpgradeTicket` `digest` field comes from the `digest` parameter to `authorize_upgrade`, which the caller must supply.  While
`authorize_upgrade` does not process the `digest`, custom policies can use it to authorize only upgrades that it has seen the
bytecode or source code for ahead of time. Sui calculates the digest as follows:

- Take the bytecode for each module, represented as an array of bytes.
- Append the list of the package's transitive dependencies, each represented as an array of bytes.
- Sort this list of byte-arrays lexicographically.
- Feed each element in the sorted list, in order, into a `Blake2B` hasher.
- Compute the digest from this hash state.

Refer to the [implementation for digest calculation](https://github.com/MystenLabs/sui/blob/d8cb153d886d54752763fbdab631b062da7d894b/crates/sui-types/src/move_package.rs#L232-L251) for more information, but in most cases, you can rely on the Move toolchain to output the digest as part of the build, when passing the `--dump-bytecode-as-base64` flag:

```
$ sui move build --dump-bytecode-as-base64
FETCHING GIT DEPENDENCY https://github.com/MystenLabs/sui.git
INCLUDING DEPENDENCY Sui
INCLUDING DEPENDENCY MoveStdlib
BUILDING test
{"modules":[<MODULE-BYTES-BASE64>],"dependencies":[<DEPENDENCY-IDS>],"digest":[59,43,173,195,216,88,176,182,18,8,24,200,200,192,196,197,248,35,118,184,207,205,33,59,228,109,184,230,50,31,235,201]}
```

## UpgradeReceipt

```rust
module sui::package {
    struct UpgradeReceipt {
        cap: ID,
        package: ID,
    }
}
```

The `UpgradeReceipt` is proof that the `Upgrade` command ran successfully, and Sui added the new package to the set of created
objects for the transaction. It is used to update its `UpgradeCap` (identified by `cap: ID`) with the ID of the latest package in its
family (`package: ID`).

After Sui creates an `UpgradeReceipt`, you must use it to update its `UpgradeCap` within the same transaction block (you cannot store it for later, drop it, or burn it), or the transaction fails.

## Isolating policies

When writing custom upgrade policies, prefer: 

- separating them into their own package, (i.e. not co-located with the code they govern the upgradeability of),
- making that package immutable (not upgradeable), and
- locking in the policy of the `UpgradeCap`, so that the policy cannot be made less restrictive later.

These best practices help uphold **informed user consent** and **bounded risk** by making it clear what a package's upgrade policy is
at the moment a user locks value into it, and ensuring that the policy does not evolve to be more permissive with time, without the package user realizing and choosing to accept the new terms.

## Example: "Day of the Week" upgrade policy

Time to put everything into practice by writing a toy upgrade policy that only authorizes upgrades on a particular day of the week (of the package creator's choosing).

### Creating an upgrade policy

Start by creating a new Move package for the upgrade policy:

```
$ sui move new policy
```

The command creates a `policy` directory with a `sources` folder and `Move.toml` manifest.

In the `sources` folder, create a source file named `day_of_week.move`. Copy and paste the following code into the file:

```rust
module policy::day_of_week {
    use sui::object::{Self, UID};
    use sui::package;
    use sui::tx_context::TxContext;

    struct UpgradeCap has key, store {
        id: UID,
        cap: package::UpgradeCap,
        day: u8,
    }

    /// Day is not a week day (number in range 0 <= day < 7).
    const ENotWeekDay: u64 = 1;

    public fun new_policy(
        cap: package::UpgradeCap,
        day: u8,
        ctx: &mut TxContext,
    ): UpgradeCap {
        assert!(day < 7, ENotWeekDay);
        UpgradeCap { id: object::new(ctx), cap, day }
    }
}
```

This code includes a constructor and defines the object type for the custom upgrade policy.

You then need to add a function to authorize an upgrade, if on the correct day of the week. First, define a couple of constants, one for the error code that identifies an attempted upgrade on a day the policy doesn't allow, and another to define the number of milliseconds in a day (to be used shortly). Add these definitions directly under the current `ENotWeekDay` one.

```rust
// Request to authorize upgrade on the wrong day of the week.
const ENotAllowedDay: u64 = 2;

const MS_IN_DAY: u64 = 24 * 60 * 60 * 1000;
```

After the `new_policy` function, add a `week_day` function to get the current weekday. As promised, the function uses the `MS_IN_DAY` constant you defined earlier.

```rust
fun week_day(ctx: &TxContext): u8 {
    let days_since_unix_epoch = 
        tx_context::epoch_timestamp_ms(ctx) / MS_IN_DAY;
    // The unix epoch (1st Jan 1970) was a Thursday so shift days
    // since the epoch by 3 so that 0 = Monday.
    ((days_since_unix_epoch + 3) % 7 as u8)
}

```
This function uses the epoch timestamp from `TxContext` rather than `Clock` because it needs only daily granularity, which means the upgrade transactions don't require consensus.

Next, add an `authorize_upgrade` function that calls the previous function to get the current day of the week, then checks whether that value violates the policy, returning the `ENotAllowedDay` error value if it does.

```rust
public fun authorize_upgrade(
    cap: &mut UpgradeCap,
    policy: u8,
    digest: vector<u8>,
    ctx: &TxContext,
): package::UpgradeTicket {
    assert!(week_day(ctx) == cap.day, ENotAllowedDay);
    package::authorize_upgrade(&mut cap.cap, policy, digest)
}
```

The signature of a custom `authorize_upgrade` can be different from the signature of `sui::package::authorize_upgrade` as long as it returns an `UpgradeTicket`.


  
Finally, provide implementations of `commit_upgrade` and `make_immutable` that delegate to their respective functions in `sui::package`:

```rust
public fun commit_upgrade(
    cap: &mut UpgradeCap,
    receipt: package::UpgradeReceipt,
) {
    package::commit_upgrade(&mut cap.cap, receipt)
}

public entry fun make_immutable(cap: UpgradeCap) {
    let UpgradeCap { id, cap, day: _ } = cap;
    object::delete(id);
    package::make_immutable(cap);
}
```

The final code in your `day_of_week.move` file should resemble the following:
```rust
module policy::day_of_week {
    use sui::object::{Self, UID};
    use sui::package;
    use sui::tx_context::TxContext;

    struct UpgradeCap has key, store {
        id: UID,
        cap: package::UpgradeCap,
        day: u8,
    }

    // Day is not a week day (number in range 0 <= day < 7).
    const ENotWeekDay: u64 = 1;
    const ENotAllowedDay: u64 = 2;
    const MS_IN_DAY: u64 = 24 * 60 * 60 * 1000;

    public fun new_policy(
        cap: package::UpgradeCap,
        day: u8,
        ctx: &mut TxContext,
    ): UpgradeCap {
        assert!(day < 7, ENotWeekDay);
        UpgradeCap { id: object::new(ctx), cap, day }
    }

    fun week_day(ctx: &TxContext): u8 {
        let days_since_unix_epoch = 
            sui::tx_context::epoch_timestamp_ms(ctx) / MS_IN_DAY;
        // The unix epoch (1st Jan 1970) was a Thursday so shift days
        // since the epoch by 3 so that 0 = Monday.
        ((days_since_unix_epoch + 3) % 7 as u8)
    }

    public fun authorize_upgrade(
        cap: &mut UpgradeCap,
        policy: u8,
        digest: vector<u8>,
        ctx: &TxContext,
    ): package::UpgradeTicket {
        assert!(week_day(ctx) == cap.day, ENotAllowedDay);
        package::authorize_upgrade(&mut cap.cap, policy, digest)
    }

    public fun commit_upgrade(
        cap: &mut UpgradeCap,
        receipt: package::UpgradeReceipt,
    ) {
        package::commit_upgrade(&mut cap.cap, receipt)
    }

    public entry fun make_immutable(cap: UpgradeCap) {
        let UpgradeCap { id, cap, day: _ } = cap;
        object::delete(id);
        package::make_immutable(cap);
    }
}
```

### Publishing an upgrade policy

Use the `sui client publish` command to publish the policy.

```sh
sui client publish --gas-budget 100000000
```
A successful publish returns the following:

```sh
INCLUDING DEPENDENCY Sui
INCLUDING DEPENDENCY MoveStdlib
BUILDING policy
Successfully verified dependencies on-chain against source.
----- Transaction Digest ----
CAFFD2HHnULQMCycL9xgad5JJpjFu2nuftf2xyugQu4t
----- Transaction Data ----
Transaction Signature: [Signature(Ed25519SuiSignature(Ed25519SuiSignature([0, 251, 96, 164, 70, 48, 195, 251, 181, 82, 206, 254, 167, 84, 165, 40, 29, 254, 102, 165, 152, 81, 244, 203, 199, 97, 33, 107, 29, 95, 120, 212, 34, 19, 233, 109, 179, 72, 246, 219, 23, 254, 108, 222, 210, 250, 166, 172, 208, 133, 108, 252, 36, 165, 71, 97, 210, 206, 144, 138, 237, 169, 15, 218, 13, 92, 225, 85, 204, 230, 61, 45, 147, 106, 193, 13, 195, 116, 230, 99, 61, 161, 251, 251, 68, 154, 46, 172, 143, 122, 101, 212, 120, 80, 164, 214, 54])))]
Transaction Kind : Programmable
Inputs: [Pure(SuiPureValue { value_type: Some(Address), value: "<SENDER>" })]
Commands: [
  Publish(_,0x0000000000000000000000000000000000000000000000000000000000000001,0x0000000000000000000000000000000000000000000000000000000000000002),
  TransferObjects([Result(0)],Input(0)),
]

Sender: <SENDER-ADDRESS>
Gas Payment: Object ID: <GAS>, version: 0x5, digest: E3tu6NE34ZDzVRtQUmXdnSTyQL2ZTm5NnhQSn1sgeUZ6
Gas Owner: <SENDER-ADDRESS>
Gas Price: 1000
Gas Budget: 100000000

----- Transaction Effects ----
Status : Success
Created Objects:
  - ID: <POLICY-UPGRADE-CAP> , Owner: Account Address ( <SENDER-ADDRESS> )
  - ID: <POLICY-PACKAGE> , Owner: Immutable
Mutated Objects:
  - ID: <GAS> , Owner: Account Address ( <SENDER-ADDRESS> )

----- Events ----
Array []
----- Object changes ----
Array [
    Object {
        "type": String("mutated"),
        "sender": String("<SENDER-ADDRESS>"),
        "owner": Object {
            "AddressOwner": String("<SENDER-ADDRESS>"),
        },
        "objectType": String("0x2::coin::Coin<0x2::sui::SUI>"),
        "objectId": String("<GAS>"),
        "version": String("6"),
        "previousVersion": String("5"),
        "digest": String("2x4rn2NNa9K5TKcSku17MMEc2JZTr4RZhkJqWAmmiU1u"),
    },
    Object {
        "type": String("created"),
        "sender": String("<SENDER-ADDRESS>"),
        "owner": Object {
            "AddressOwner": String("<SENDER-ADDRESS>"),
        },
        "objectType": String("0x2::package::UpgradeCap"),
        "objectId": String("<POLICY-UPGRADE-CAP>"),
        "version": String("6"),
        "digest": String("DG1CABxqdHNhjBDzt7K4VKiJdLfnrW9qnCx8yr4jVP4"),
    },
    Object {
        "type": String("published"),
        "packageId": String("<POLICY-PACKAGE>"),
        "version": String("1"),
        "digest": String("XehdKX2WCyMFFds53bd5xDT1okBwczE3ajW9E1h5zgh"),
        "modules": Array [
            String("day_of_week"),
        ],
    },
]
----- Balance changes ----
Array [
    Object {
        "owner": Object {
            "AddressOwner": String("<SENDER-ADDRESS>"),
        },
        "coinType": String("0x2::sui::SUI"),
        "amount": String("-10773600"),
    },
]
```

Following best practices, use the Sui Client CLI to call `sui::package::make_immutable` on the `UpgradeCap` to make the policy immutable.

```sh
sui client call --gas-budget 10000000 \
    --package 0x2 \
    --module 'package' \
    --function 'make_immutable' \
    --args '<POLICY-UPGRADE-CAP>'
```

```sh
----- Transaction Digest ----
FqTdsEgFnyVqc3sFeu5EnBUziEDYbxhLUAaLv4FDjN6d
----- Transaction Data ----
Transaction Signature: [Signature(Ed25519SuiSignature(Ed25519SuiSignature([0, 123, 97, 9, 252, 127, 238, 10, 88, 175, 157, 155, 98, 11, 23, 234, 52, 167, 230, 45, 218, 171, 31, 174, 87, 107, 174, 117, 236, 65, 117, 18, 42, 74, 56, 149, 82, 107, 216, 199, 223, 142, 135, 165, 200, 80, 151, 32, 110, 75, 133, 128, 150, 66, 13, 40, 173, 228, 211, 94, 222, 201, 248, 221, 10, 92, 225, 85, 204, 230, 61, 45, 147, 106, 193, 13, 195, 116, 230, 99, 61, 161, 251, 251, 68, 154, 46, 172, 143, 122, 101, 212, 120, 80, 164, 214, 54])))]
Transaction Kind : Programmable
Inputs: [Object(ImmOrOwnedObject { object_id: <POLICY-UPGRADE-CAP>, version: SequenceNumber(6), digest: o#DG1CABxqdHNhjBDzt7K4VKiJdLfnrW9qnCx8yr4jVP4 })]
Commands: [
  MoveCall(0x0000000000000000000000000000000000000000000000000000000000000002::package::make_immutable(Input(0))),
]

Sender: <SENDER-ADDRESS>
Gas Payment: Object ID: <GAS>, version: 0x6, digest: 2x4rn2NNa9K5TKcSku17MMEc2JZTr4RZhkJqWAmmiU1u
Gas Owner: <SENDER-ADDRESS>
Gas Price: 1000
Gas Budget: 10000000

----- Transaction Effects ----
Status : Success
Mutated Objects:
  - ID: <GAS> , Owner: Account Address ( <SENDER-ADDRESS> )
Deleted Objects:
  - ID: <POLICY-UPGRADE-CAP>

----- Events ----
Array []
----- Object changes ----
Array [
    Object {
        "type": String("mutated"),
        "sender": String("<SENDER-ADDRESS>"),
        "owner": Object {
            "AddressOwner": String("<SENDER-ADDRESS>"),
        },
        "objectType": String("0x2::coin::Coin<0x2::sui::SUI>"),
        "objectId": String("<GAS>"),
        "version": String("7"),
        "previousVersion": String("6"),
        "digest": String("2Awa8KHrP4wo33iLNKCeLVQ8HrKj1hrd2LigkLiacJVg"),
    },
]
----- Balance changes ----
Array [
    Object {
        "owner": Object {
            "AddressOwner": String("<SENDER-ADDRESS>"),
        },
        "coinType": String("0x2::sui::SUI"),
        "amount": String("607780"),
    },
]
```

### Creating a package for testing

With a policy now available on chain, you need a package to upgrade. This topic creates a basic package and references it in the following scenarios, but you can use any package you might have available instead of creating a new one.

If you don't have a package available, use the `sui move new` command to create the template for a new package called `example`.

```
$ sui move new example
```

In the `example/sources` directory, create an `example.move` file with the following code: 

```rust
module example::example {
    struct Event has copy, drop { x: u64 }
    entry fun nudge() {
        sui::event::emit(Event { x: 41 })
    }
}
```

The instruction that follows publishes this example package and then upgrades it to change the value in the `Event` it emits. Because you are using a custom upgrade policy, you need to use the TypeScript SDK to build the package's publish and upgrade commands.

### Using TypeScript SDK

Create a new directory to store a Node.js project. You can use the `npm init` function to create the `package.json`, or manually create the file. Depending on your approach to creating `package.json`, populate or add the following JSON to it:

```JSON
{ "type": "module" }

```

Open a terminal or console to the root of your Node.js project. Run the following command to add the Sui TypeScript SDK as a dependency:

```
$ npm install @mysten/sui.js
```

### Publishing a package with custom policy

In the root of your Node.js project, create a script file named `publish.js`. Open the file for editing and define some constants: 
* `SUI`: the location of the `sui` CLI binary.
* `POLICY_PACKAGE_ID`: the ID of our published `day_of_week` package.

```js
const SUI = 'sui';
const POLICY_PACKAGE_ID = '<POLICY-PACKAGE>';
```

Next, add boilerplate code to get the keypair for the currently active address in the Sui Client CLI:

```js
import { execSync } from 'child_process';
import { readFileSync } from 'fs';
import { homedir } from 'os';
import path from 'path';

import {
    Ed25519Keypair,
    fromB64,
} from '@mysten/sui.js';

const sender = execSync(`${SUI} client active-address`, { encoding: 'utf8' }).trim();
const keyPair = (() => {
    const keystore = JSON.parse(
        readFileSync(
            path.join(homedir(), '.sui', 'sui_config', 'sui.keystore'),
            'utf8',
        )
    );

    for (const priv of keystore) {
        const raw = fromB64(priv);
        if (raw[0] !== 0) {
            continue;
        }

        const pair = Ed25519Keypair.fromSecretKey(raw.slice(1));
        if (pair.getPublicKey().toSuiAddress() === sender) {
            return pair;
        }
    }
    
    throw new Error(`keypair not found for sender: ${sender}`);
})();
```

Next, define the path of the package you are publishing. The following snippet assumes that the package is in a sibling directory to
`publish.js`, called `example`:

```js
import path from 'path';
import { fileToURLPath } from 'url';

const __dirname = path.dirname(fileToURLPath(import.meta.url));
// Location of package relative to current directory
const packagePath = path.join(__dirname, 'example');
```

Next, build the package:

```js
const { modules, dependencies } = JSON.parse(
    execSync(
        `${SUI} move build --dump-bytecode-as-base64 --path ${packagePath}`,
        { encoding: 'utf-8'},
    ),
);
```

Next, construct the transaction to publish the package. Wrap its `UpgradeCap` in a "day of the week" policy, which permits upgrades on Tuesdays, and send the new policy back:

```js
import { TransactionBlock } from '@mysten/sui.js';

const tx = new TransactionBlock();
const packageUpgradeCap = tx.publish({ modules, dependencies });
const tuesdayUpgradeCap = tx.moveCall({
    target: `${POLICY_PACKAGE_ID}::day_of_week::new_policy`,
    arguments: [
        packageUpgradeCap,
        tx.pure(1), // 1 = Tuesday
    ],
});

tx.transferObjects([tuesdayUpgradeCap], tx.pure(sender));
```

And finally, execute that transaction and display its effects to the console. The following snippet assumes that you're running your examples against a
local network. Replace all `localnetConnection` references with `devnetConnection` or `testnetConnection` to run on Devnet or Testnet respectively:

```js
import { JsonRpcProvider, RawSigner, localnetConnection }
const provider = new JsonRpcProvider(localnetConnection);
const signer = new RawSigner(keyPair, provider);

const result = await signer.signAndExecuteTransactionBlock({
    transactionBlock: tx,
    options: {
        showEffects: true,
        showObjectChanges: true,
    }
});

console.log(result)
```

Save your `publish.js` file, and then use Node.js to run the script:

```sh
$ node publish.js
```

If the script is successful, the console prints the following response:

```sh
INCLUDING DEPENDENCY Sui
INCLUDING DEPENDENCY MoveStdlib
BUILDING example
{
  digest: '9NBLe61sRqe7wS6y8mMVt6vhwA9W5Sz5YVEmuCwNMT64',
  effects: {
    messageVersion: 'v1',
    status: { status: 'success' },
    executedEpoch: '0',
    gasUsed: {
      computationCost: '1000000',
      storageCost: '6482800',
      storageRebate: '978120',
      nonRefundableStorageFee: '9880'
    },
    modifiedAtVersions: [ [Object] ],
    transactionDigest: '9NBLe61sRqe7wS6y8mMVt6vhwA9W5Sz5YVEmuCwNMT64',
    created: [ [Object], [Object] ],
    mutated: [ [Object] ],
    gasObject: { owner: [Object], reference: [Object] },
    dependencies: [
      'BMVXjS7GG3d5W4Prg7gMVyvKTzEk1Hazx7Tq4WCcbcz9',
      'CAFFD2HHnULQMCycL9xgad5JJpjFu2nuftf2xyugQu4t',
      'GGDUeVkDoNFcyGibGNeiaGSiKsxf9QLzbjqPzdqi3dNJ'
    ]
  },
  objectChanges: [
    {
      type: 'mutated',
      sender: '<SENDER>',
      owner: [Object],
      objectType: '0x2::coin::Coin<0x2::sui::SUI>',
      objectId: '<GAS>',
      version: '10',
      previousVersion: '9',
      digest: 'Dz38faAzFsRzKQyT7JTkVydCcvNNxbUdZiutGmA2Eyy6'
    },
    {
      type: 'published',
      packageId: '<EXAMPLE-PACKAGE>',
      version: '1',
      digest: '5JdU8hkFTjyqg4fHyC8JtdHBV11yCCKdFuyf9j4kKY3o',
      modules: [Array]
    },
    {
      type: 'created',
      sender: '<SENDER>',
      owner: [Object],
      objectType: '<POLICY-PACKAGE>::day_of_week::UpgradeCap',
      objectId: '<EXAMPLE-UPGRADE-CAP>',
      version: '10',
      digest: '3uAMFHFKunX9XrufMe27MHDbeLpgHBSsCPN3gSa93H3v'
    }
  ],
  confirmedLocalExecution: true
}
```

**Note:** If you receive a `ReferenceError: fetch is not defined` error, use Node.js version 18 or greater.

Use the CLI to test that your newly published package works:

```sh
$ sui client call --gas-budget 10000000 \
    --package '<EXAMPLE-PACKAGE-ID>' \
    --module 'example' \
    --function 'nudge' \
```

A successful call responds with the following:

```sh
----- Transaction Digest ----
Bx1GA8EsBjoLKvXV2GG92DC5Jt58dbytf6jFcLg18dDR
----- Transaction Data ----
Transaction Signature: [Signature(Ed25519SuiSignature(Ed25519SuiSignature([0, 92, 22, 253, 150, 35, 134, 140, 185, 239, 72, 194, 25, 250, 153, 98, 134, 26, 219, 232, 199, 122, 56, 189, 186, 56, 126, 184, 147, 148, 184, 4, 17, 177, 156, 231, 198, 74, 118, 28, 187, 132, 94, 141, 44, 55, 70, 207, 157, 143, 182, 83, 59, 156, 116, 226, 22, 65, 211, 179, 187, 18, 76, 245, 4, 92, 225, 85, 204, 230, 61, 45, 147, 106, 193, 13, 195, 116, 230, 99, 61, 161, 251, 251, 68, 154, 46, 172, 143, 122, 101, 212, 120, 80, 164, 214, 54])))]
Transaction Kind : Programmable
Inputs: []
Commands: [
  MoveCall(<EXAMPLE-PACKAGE>::example::nudge()),
]

Sender: <SENDER>
Gas Payment: Object ID: <GAS>, version: 0xb, digest: 93nZ3uLmLfJdHWoSHMuHsjFstEf45EM2pfovu3ibo4iH
Gas Owner: <SENDER>
Gas Price: 1000
Gas Budget: 10000000

----- Transaction Effects ----
Status : Success
Mutated Objects:
  - ID: <GAS> , Owner: Account Address ( <SENDER> )

----- Events ----
Array [
    Object {
        "id": Object {
            "txDigest": String("Bx1GA8EsBjoLKvXV2GG92DC5Jt58dbytf6jFcLg18dDR"),
            "eventSeq": String("0"),
        },
        "packageId": String("<EXAMPLE-PACKAGE>"),
        "transactionModule": String("example"),
        "sender": String("<SENDER>"),
        "type": String("<EXAMPLE-PACKAGE>::example::Event"),
        "parsedJson": Object {
            "x": String("41"),
        },
        "bcs": String("7rkaa6aDvyD"),
    },
]
----- Object changes ----
Array [
    Object {
        "type": String("mutated"),
        "sender": String("<SENDER>"),
        "owner": Object {
            "AddressOwner": String("<SENDER>"),
        },
        "objectType": String("0x2::coin::Coin<0x2::sui::SUI>"),
        "objectId": String("<GAS>"),
        "version": String("12"),
        "previousVersion": String("11"),
        "digest": String("9aNuZF63uBVaWF9L6cVmk7geimmpP9h9StigdNDPSiy3"),
    },
]
----- Balance changes ----
Array [
    Object {
        "owner": Object {
            "AddressOwner": String("<SENDER>"),
        },
        "coinType": String("0x2::sui::SUI"),
        "amount": String("-1009880"),
    },
]
```

If you used the example package provided, notice you have an `Events` section that contains a field `x` with value `41`.

### Upgrading a package with custom policy

With your package published, you can prepare an `upgrade.js` script to perform an upgrade using the new policy. It behaves identically to `publish.js` up until building the package. When building the package, the script also captures its `digest`, and the transaction now performs the three upgrade commands (authorize, execute, commit). The full script for `upgrade.js` follows:

```js
import { execSync } from 'child_process';
import { readFileSync } from 'fs';
import { homedir } from 'os';
import path from 'path';
import { fileURLToPath } from 'url';

import {
    Ed25519Keypair,
    JsonRpcProvider,
    RawSigner,
    TransactionBlock,
    UpgradePolicy,
    fromB64,
    localnetConnection,
} from '@mysten/sui.js';

const SUI = 'sui';
const POLICY_PACKAGE_ID = '<POLICY-PACKAGE>';
const EXAMPLE_PACKAGE_ID = '<EXAMPLE-PACKAGE>';
const CAP_ID = '<EXAMPLE-UPGRADE-CAP>';

const sender = execSync(`${SUI} client active-address`, { encoding: 'utf8' }).trim();
const keyPair = (() => {
    const keystore = JSON.parse(
        readFileSync(
            path.join(homedir(), '.sui', 'sui_config', 'sui.keystore'),
            'utf8',
        )
    );

    for (const priv of keystore) {
        const raw = fromB64(priv);
        if (raw[0] !== 0) {
            continue;
        }

        const pair = Ed25519Keypair.fromSecretKey(raw.slice(1));
        if (pair.getPublicKey().toSuiAddress() === sender) {
            return pair;
        }
    }
    
    throw new Error(`keypair not found for sender: ${sender}`);
})();

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const packagePath = path.join(__dirname, 'example');

const { modules, dependencies, digest } = JSON.parse(
    execSync(
        `${SUI} move build --dump-bytecode-as-base64 --path ${packagePath}`,
        { encoding: 'utf-8'},
    ),
);

const tx = new TransactionBlock();
const cap = tx.object(CAP_ID);
const ticket = tx.moveCall({
    target: `${POLICY_PACKAGE_ID}::day_of_week::authorize_upgrade`,
    arguments: [
        cap,
        tx.pure(UpgradePolicy.COMPATIBLE),
        tx.pure(digest),
    ],
});

const receipt = tx.upgrade({
    modules,
    dependencies,
    packageId: EXAMPLE_PACKAGE_ID,
    ticket,
});

tx.moveCall({
    target: `${POLICY_PACKAGE_ID}::day_of_week::commit_upgrade`,
    arguments: [cap, receipt],
})

const provider = new JsonRpcProvider(localnetConnection);
const signer = new RawSigner(keyPair, provider);

const result = await signer.signAndExecuteTransactionBlock({
    transactionBlock: tx,
    options: {
        showEffects: true,
        showObjectChanges: true,
    }
});

console.log(result)
```

If today is not Tuesday, wait until next Tuesday to run the script, when your policy allows you to perform upgrades. At that point, update your `example.move` so the event is emitted with a different constant and use Node.js to run the upgrade script:

```sh
node upgrade.js
```

If the script is successful (and today is Tuesday), your console displays the following response:

```sh
INCLUDING DEPENDENCY Sui
INCLUDING DEPENDENCY MoveStdlib
BUILDING example
{
  digest: 'EzJyH6BX231sw4jY6UZ6r9Dr28SKsiB2hg3zw4Jh4D5P',
  effects: {
    messageVersion: 'v1',
    status: { status: 'success' },
    executedEpoch: '0',
    gasUsed: {
      computationCost: '1000000',
      storageCost: '6482800',
      storageRebate: '2874168',
      nonRefundableStorageFee: '29032'
    },
    modifiedAtVersions: [ [Object], [Object] ],
    transactionDigest: 'EzJyH6BX231sw4jY6UZ6r9Dr28SKsiB2hg3zw4Jh4D5P',
    created: [ [Object] ],
    mutated: [ [Object], [Object] ],
    gasObject: { owner: [Object], reference: [Object] },
    dependencies: [
      '62BxVq24tgaRrFTXR3i944RRZ6x8sgTGbjFzpFDe2RAB',
      'BMVXjS7GG3d5W4Prg7gMVyvKTzEk1Hazx7Tq4WCcbcz9',
      'Bx1GA8EsBjoLKvXV2GG92DC5Jt58dbytf6jFcLg18dDR',
      'CAFFD2HHnULQMCycL9xgad5JJpjFu2nuftf2xyugQu4t'
    ]
  },
  objectChanges: [
    {
      type: 'mutated',
      sender: '<SENDER>',
      owner: [Object],
      objectType: '0x2::coin::Coin<0x2::sui::SUI>',
      objectId: '<GAS>',
      version: '13',
      previousVersion: '12',
      digest: 'DF4aebHRYrVdxtfAaFfET3hLHn5hqsoty4joMYxLDBuc'
    },
    {
      type: 'mutated',
      sender: '<SENDER>',
      owner: [Object],
      objectType: '<POLICY-PACKAGE>::day_of_week::UpgradeCap',
      objectId: '<EXAMPLE-UPGRADE-CAP>',
      version: '13',
      previousVersion: '11',
      digest: '5Wtuw9mAGBuP5qFdTzDCRxBF9LqJ7uZbpxk2UXhAkrXL'
    },
    {
      type: 'published',
      packageId: '<UPGRADED-EXAMPLE-PACKAGE>',
      version: '2',
      digest: '7mvnMEXezAGcWqYSt6R4QUpPjY8nqTSmb5Dv2SqkVq7a',
      modules: [Array]
    }
  ],
  confirmedLocalExecution: true
}
```

Use the Sui Client CLI to test the upgraded package (the package ID is **different** from the original version of your example package):

```sh
sui client call --gas-budget 10000000 \
    --package '<UPGRADED-EXAMPLE-PACKAGE>' \
    --module 'example' \
    --function 'nudge'
```

If successful, the console prints the following response:

```sh
----- Transaction Digest ----
EF2rQzWHmtjPvkqzFGyFvANA8e4ETULSBqDMkzqVoshi
----- Transaction Data ----
Transaction Signature: [Signature(Ed25519SuiSignature(Ed25519SuiSignature([0, 88, 98, 118, 173, 218, 55, 4, 48, 166, 42, 106, 193, 210, 159, 75, 233, 95, 77, 201, 38, 0, 234, 183, 77, 252, 178, 22, 221, 106, 202, 42, 166, 29, 130, 164, 97, 110, 201, 153, 91, 149, 50, 72, 6, 213, 183, 70, 83, 55, 5, 190, 182, 5, 98, 212, 134, 103, 181, 204, 247, 90, 28, 125, 14, 92, 225, 85, 204, 230, 61, 45, 147, 106, 193, 13, 195, 116, 230, 99, 61, 161, 251, 251, 68, 154, 46, 172, 143, 122, 101, 212, 120, 80, 164, 214, 54])))]
Transaction Kind : Programmable
Inputs: []
Commands: [
  MoveCall(<UPGRADE-EXAMPLE-PACKAGE>::example::nudge()),
]

Sender: <SENDER>
Gas Payment: Object ID: <GAS>, version: 0xd, digest: DF4aebHRYrVdxtfAaFfET3hLHn5hqsoty4joMYxLDBuc
Gas Owner: <SENDER>
Gas Price: 1000
Gas Budget: 10000000

----- Transaction Effects ----
Status : Success
Mutated Objects:
  - ID: <GAS> , Owner: Account Address ( <SENDER> )

----- Events ----
Array [
    Object {
        "id": Object {
            "txDigest": String("EF2rQzWHmtjPvkqzFGyFvANA8e4ETULSBqDMkzqVoshi"),
            "eventSeq": String("0"),
        },
        "packageId": String("<UPGRADE-EXAMPLE-PACKAGE>"),
        "transactionModule": String("example"),
        "sender": String("<SENDER>"),
        "type": String("<EXAMPLE-PACKAGE>::example::Event"),
        "parsedJson": Object {
            "x": String("42"),
        },
        "bcs": String("82TFauPiYEj"),
    },
]
----- Object changes ----
Array [
    Object {
        "type": String("mutated"),
        "sender": String("<SENDER>"),
        "owner": Object {
            "AddressOwner": String("<SENDER>"),
        },
        "objectType": String("0x2::coin::Coin<0x2::sui::SUI>"),
        "objectId": String("<GAS>"),
        "version": String("14"),
        "previousVersion": String("13"),
        "digest": String("AmGocCxy6cHvCuGG3izQ8a7afp6qWWt14yhowAzBYa44"),
    },
]
----- Balance changes ----
Array [
    Object {
        "owner": Object {
            "AddressOwner": String("<SENDER>"),
        },
        "coinType": String("0x2::sui::SUI"),
        "amount": String("-1009880"),
    },
]
```

Now, the `Events` section emitted for the `x` field has a value of `42` (changed from the original `41`).

If you attempt the first upgrade before Tuesday or you change the constant again and try the upgrade the following day, the script receives a response that includes an error similar to the following, which indicates that the upgrade aborted with code `2` (`ENotAllowedDay`):

```
...
status: {
        status: 'failure',
        error: 'MoveAbort(MoveLocation { module: ModuleId { address: <POLICY-PACKAGE>, name: Identifier("day_of_week") }, function: 1, instruction: 11, function_name: Some("authorize_upgrade") }, 2) in command 0'
      },
...
```
