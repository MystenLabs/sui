# Custom Upgrade Policies

`sui client upgrade` offers a simple way to upgrade packages when the
CLI's active address owns the package's `UpgradeCap`.  This process is
useful to get started with upgrades, or in the early stages of a
package's development, but having the ability to upgrade a package be
protected by only one key can pose a security risk for packages that
are live in production:

- The individual owning that key may make changes that are in their
  interests but not the interests of the broader community.
- Upgrades may happen without enough time for package users to consult
  on the change or stop using the package if they disagree.
- The key may get lost.

This security risk can be eliminated by making a package **immutable**
when it goes live (using `sui::package::make_immutable` to burn its
`UpgradeCap`) but this prevents future bugfixes and new features being
added, which may not be practical.

**Custom upgrade policies** maintain safety and security for the 
package creator and its users while preserving the ability to make 
changes to live packages.  They protect `UpgradeCap` access behind
arbitrary Move code and allow upgrades to be authorized on a case-by-
-case basis by issuing `UpgradeTicket`s.

## Overview

Package upgrades must occur end-to-end in a single transaction block
and are composed of three commands:

1. **Authorization:** Get permission from the `UpgradeCap` to perform
   the upgrade, creating an `UpgradeTicket`.
2. **Execution:** Consume the `UpgradeTicket` and verify the package
   bytecode and compatibility against the previous version, and create
   the on-chain object representing the upgraded package. Return an
   `UpgradeReceipt` as a result on success.
3. **Commit:** Update the `UpgradeCap` with information about the
   newly created package.

While step 2 is a built-in command, steps 1 and 3 are implemented as
move functions.  Their most basic implementation is provided as part
of the Sui framework:

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

These are the functions that are called for authorization and commit
by `sui client upgrade`.  Custom upgrade policies work by guarding
access to a package's `UpgradeCap` (and therefore to calls of these
functions) behind extra conditions that are specific to that policy
(e.g. voting, governance, permission lists, timelocks).

Any pair of functions that produces an `UpgradeTicket` from an
`UpgradeCap` and consumes an `UpgradeReceipt` to update an
`UpgradeCap` constitutes a custom upgrade policy.

## Upgrade Cap

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

The `UpgradeCap` is the central type responsible for coordinating
package upgrades.  It is created during package publishing and updated
during upgrades.  The owner of this object has permission to:

- Change the compatibility requirements for future upgrades.
- Authorize future upgrades.
- Make the package immutable (not upgradeable).

And its API guarantees the following properties:

- Only the latest version of a package can be upgraded (a linear
  history is guaranteed).
- Only one upgrade can be in-flight at any time (cannot authorize
  multiple concurrent upgrades).
- An upgrade can only be authorized for the extent of a single
  transaction, the `UpgradeTicket` that proves authorization cannot be
  `store`d.
- Compatibility requirements for a package can only be made more
  restrictive over time.

## Upgrade Ticket

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

An `UpgradeTicket` is proof that an upgrade has been authorized.  This
authorization is specific to:

- A particular `package: ID` to upgrade from, which must be the latest
  package in the family identified by the `UpgradeCap` at `cap: ID`.
- A particular `policy: u8` that attests to the kind of compatibility
  guarantees that the upgrade expects to adhere to.
- A particular `digest: vector<u8>` which identifies the contents of
  the package after the upgrade.

When the upgrade is run, the validator checks that the upgrade it is
about to perform matches the upgrade that was authorized along all
those lines, and will not perform the upgrade if any of these criteria
are not met.

Once an `UpgradeTicket` is created, it must be used within that
transaction (it cannot be stored for later, dropped, or burned), or the
transaction will fail.

### Package Digest

The `UpgradeTicket`'s `digest` field comes from the `digest` parameter
to `authorize_upgrade`, which must be supplied by the caller.  While
`authorize_upgrade` does not process the `digest`, it can be used by
custom policies to only authorize upgrades that it has seen the
bytecode or source code for ahead of time.  The digest is calculated
as follows:

- Take the bytecode for each module, represented as an array of bytes.
- Append the list of the package's transitive dependencies, each
  represented as an array of bytes.
- Sort this list of byte-arrays lexicographically.
- Feed each element in the sorted list, in order, into a `Blake2B`
  hasher.
- Compute the digest from this hash state.

The reference implementation for digest calculation can be found
[here](https://github.com/MystenLabs/sui/blob/d8cb153d886d54752763fbdab631b062da7d894b/crates/sui-types/src/move_package.rs#L232-L251),
but in most cases, package creators can rely on the Move toolchain to
output the digest as part of the build, when passing the
`--dump-bytecode-as-base64` flag:

```
$ sui move build --dump-bytecode-as-base64
FETCHING GIT DEPENDENCY https://github.com/MystenLabs/sui.git
INCLUDING DEPENDENCY Sui
INCLUDING DEPENDENCY MoveStdlib
BUILDING test
{"modules":[<MODULE-BYTES-BASE64>],"dependencies":[<DEPENDENCY-IDS>],"digest":[59,43,173,195,216,88,176,182,18,8,24,200,200,192,196,197,248,35,118,184,207,205,33,59,228,109,184,230,50,31,235,201]}
```

## Upgrade Receipt

```rust
module sui::package {
    struct UpgradeReceipt {
        cap: ID,
        package: ID,
    }
}
```

The `UpgradeReceipt` is proof that the `Upgrade` command ran
successfully, and the new package has been added to the set of created
objects for the transaction.  It is used to update its `UpgradeCap`
(identified by `cap: ID`) with the ID of the latest package in its
family (`package: ID`).

Once an `UpgradeReceipt` is created, it must be used to update its
`UpgradeCap` within the same transaction (it cannot be stored for
later, dropped, or burned), or the transaction will fail.

## Isolating Policies

When writing custom upgrade policies, prefer: 

- separating them into their own package, (i.e. not co-located with
  the code they govern the upgradeability of)
- making that package immutable (not upgradeable),
- locking in the `UpgradeCap`, so that a package's upgrade policy
  cannot be made less restrictive, once one is chosen.

These best practices help uphold **informed user consent** and
**bounded risk** by making it clear what a package's upgrade policy is
at the moment a user locks value into it, and ensuring that the policy
does not evolve to be more permissive with time, without the package
user realising and choosing to accept the new terms.

## Example: "Day of the Week" Upgrade Policy

We will put everything into practice by writing a toy upgrade policy that
only authorizes upgrades on a particular day of the week (of the
package creator's choosing).

Start by creating a new move package for the upgrade policy:

```
$ sui move new policy
```

Add a source file, at `policy/sources/day_of_week.move`, where we
define the object type for our custom upgrade policy, and a
constructor:

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

We then need to add a function to authorize an upgrade, if we are on
the correct day of the week:

```rust
module policy::day_of_week {
    use sui::package;
    use sui::tx_context::{Self, TxContext};

    /// Request to authorize upgrade on the wrong day of the week.
    const ENotAllowedDay: u64 = 2;

    const MS_IN_DAY: u64 = 24 * 60 * 60 * 1000;

    public fun authorize_upgrade(
        cap: &mut UpgradeCap,
        policy: u8,
        digest: vector<u8>,
        ctx: &TxContext,
    ): package::UpgradeTicket {
        assert!(week_day(ctx) == cap.day, ENotAllowedDay);
        package::authorize_upgrade(&mut cap.cap, policy, digest)
    }

    fun week_day(ctx: &TxContext): u8 {
        let days_since_unix_epoch = 
            tx_context::epoch_timestamp_ms(clock) / MS_IN_DAY;
        // The unix epoch (1st Jan 1970) was a Thursday so shift days
        // since the epoch by 3 so that 0 = Monday.
        ((days_since_unix_epoch + 3) % 7 as u8)
    }
}
```

Note that: 

- The signature of our custom `authorize_upgrade` can be different
  from the signature of `sui::package::authorize_upgrade` as long as
  it returns an `UpgradeTicket`.
- We use the epoch timestamp from `TxContext` rather than `Clock`
  because we only need daily granularity, and this means our
  upgrade transactions don't need to go through consensus.
  
We also need to provide implementations of `commit_upgrade` and
`make_immutable`, but these simply delegate to their respective
functions in `sui::package`:

```rust
module policy::day_of_week {
    use sui::object::{Self, UID};
    use sui::package;

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

Next we publish the policy code:

```
$ sui client publish --gas-budget 100000000
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

Sender: <SENDER>
Gas Payment: Object ID: <GAS>, version: 0x5, digest: E3tu6NE34ZDzVRtQUmXdnSTyQL2ZTm5NnhQSn1sgeUZ6
Gas Owner: <SENDER>
Gas Price: 1000
Gas Budget: 100000000

----- Transaction Effects ----
Status : Success
Created Objects:
  - ID: <POLICY-UPGRADE-CAP> , Owner: Account Address ( <SENDER> )
  - ID: <POLICY-PACKAGE> , Owner: Immutable
Mutated Objects:
  - ID: <GAS> , Owner: Account Address ( <SENDER> )

----- Events ----
Array []
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
        "version": String("6"),
        "previousVersion": String("5"),
        "digest": String("2x4rn2NNa9K5TKcSku17MMEc2JZTr4RZhkJqWAmmiU1u"),
    },
    Object {
        "type": String("created"),
        "sender": String("<SENDER>"),
        "owner": Object {
            "AddressOwner": String("<SENDER>"),
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
            "AddressOwner": String("<SENDER>"),
        },
        "coinType": String("0x2::sui::SUI"),
        "amount": String("-10773600"),
    },
]
```

Following best practices, we make the policy code immutable by calling
`sui::package::make_immutable` on its `UpgradeCap`:

```
$ sui client call --gas-budget 10000000 \
    --package 0x2                       \
    --module 'package'                  \
    --function 'make_immutable'         \
    --args <POLICY-UPGRADE-CAP>         \
----- Transaction Digest ----
FqTdsEgFnyVqc3sFeu5EnBUziEDYbxhLUAaLv4FDjN6d
----- Transaction Data ----
Transaction Signature: [Signature(Ed25519SuiSignature(Ed25519SuiSignature([0, 123, 97, 9, 252, 127, 238, 10, 88, 175, 157, 155, 98, 11, 23, 234, 52, 167, 230, 45, 218, 171, 31, 174, 87, 107, 174, 117, 236, 65, 117, 18, 42, 74, 56, 149, 82, 107, 216, 199, 223, 142, 135, 165, 200, 80, 151, 32, 110, 75, 133, 128, 150, 66, 13, 40, 173, 228, 211, 94, 222, 201, 248, 221, 10, 92, 225, 85, 204, 230, 61, 45, 147, 106, 193, 13, 195, 116, 230, 99, 61, 161, 251, 251, 68, 154, 46, 172, 143, 122, 101, 212, 120, 80, 164, 214, 54])))]
Transaction Kind : Programmable
Inputs: [Object(ImmOrOwnedObject { object_id: <POLICY-UPGRADE-CAP>, version: SequenceNumber(6), digest: o#DG1CABxqdHNhjBDzt7K4VKiJdLfnrW9qnCx8yr4jVP4 })]
Commands: [
  MoveCall(0x0000000000000000000000000000000000000000000000000000000000000002::package::make_immutable(Input(0))),
]

Sender: <SENDER>
Gas Payment: Object ID: <GAS>, version: 0x6, digest: 2x4rn2NNa9K5TKcSku17MMEc2JZTr4RZhkJqWAmmiU1u
Gas Owner: <SENDER>
Gas Price: 1000
Gas Budget: 10000000

----- Transaction Effects ----
Status : Success
Mutated Objects:
  - ID: <GAS> , Owner: Account Address ( <SENDER> )
Deleted Objects:
  - ID: <POLICY-UPGRADE-CAP>

----- Events ----
Array []
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
        "version": String("7"),
        "previousVersion": String("6"),
        "digest": String("2Awa8KHrP4wo33iLNKCeLVQ8HrKj1hrd2LigkLiacJVg"),
    },
]
----- Balance changes ----
Array [
    Object {
        "owner": Object {
            "AddressOwner": String("<SENDER>"),
        },
        "coinType": String("0x2::sui::SUI"),
        "amount": String("607780"),
    },
]
```

Now we need a package to upgrade with our new policy, any package will
do, but if you don't have one handy, try the following:

```
$ sui move new example
```

With a single module `example/sources/example.move`:

```
module example::example {
    struct Event has copy, drop { x: u64 }
    entry fun nudge() {
        sui::event::emit(Event { x: 41 })
    }
}
```

We will publish this and upgrade it to change the value in the `Event`
that is emitted.

Because we are using a custom upgrade policy, we will need to use the
TypeScript SDK to build the package's publish and upgrade commands.
Create a node project with the following `package.json`:

```JSON
{ "type": "module" }

```

And run the following in its directory to add the Sui TypeScript SDK
as a dependency:

```
$ npm install @mysten/sui.js
```

And create a script, `publish.js` to perform the upgrade, starting by
defining some constants: `SUI` the location of the `sui` CLI binary,
and `POLICY_PACKAGE_ID`, the ID of our published `day_of_week`
package:

```js
const SUI = 'sui';
const POLICY_PACKAGE_ID = '<POLICY-PACKAGE>';
```

Then some boiler-plate to get the keypair for the currently active
address in the `sui` CLI:

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

Then define the path of the package to be published.  The following
snippet assumes that the package is found in a sibling directory to
`publish.js`, called `example`:

```js
import path from 'path';
import { fileToURLPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const packagePath = path.join(__dirname, 'example');
```

Then we can build the package:

```js
const { modules, dependencies } = JSON.parse(
    execSync(
        `${SUI} move build --dump-bytecode-as-base64 --path ${packagePath}`,
        { encoding: 'utf-8'},
    ),
);
```

Construct the transaction to publish it, wrap its `UpgradeCap` in a
"day of the week" policy, which permits upgrades on Tuesdays, and send
the new policy back to us:

```js
import { TransactionBlock } from '@mysten/sui.js';

const tx = new TransactionBlock();
const packageUpgradeCap = tx.publish({ modules, dependencies });
const tuesdayUpgradeCap = tx.moveCall({
    target: `${POLICY_PACKAGE_ID}::day_of_week::new_policy`,
    arguments: [
        packageUpgradeCap,
        tx.pure(1), // Tuesday
    ],
});

tx.transferObjects([tuesdayUpgradeCap], tx.pure(sender));
```

And finally, execute that transaction, and display its effects (The
following snippet assumes that you are running your examples against a
local network.  Replace `localnetConnection` with `devnetConnection`
or `testnetConnection` everywhere as appropriate to run on devnet or
testnet respectively):

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

Then, run the publish script:

```
$ node publish.js
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

We can test that our newly published package works using the CLI:

```
$ sui client call --gas-budget 10000000 \
    --package '<EXAMPLE-PACKAGE>'       \
    --module 'example'                  \
    --function 'nudge'                  \
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

Note that the Event produced contains a field `x` with value `41`.

Now we can prepare our `upgrade.js` script to perform an upgrade using
our new policy.  It behaves identically to `publish.js` up until
building the package, at which point it diverges.  When building the
package, we also capture its `digest`, and the transaction now
performs the three upgrade commands (authorize, execute, commit).  The
full script for `upgrade.js` follows:

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

The next step, is to wait until next Tuesday, when your policy allows
you to perform upgrades.  At that point, update your `example.move` so
the event is emitted with a different constant and run the upgrade script:

```
$ node upgrade.js
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

And test the upgraded package on the CLI (Note that its Package ID
will be **different** from the original version of your example
package):

```
$ sui client call --gas-budget 10000000    \
    --package '<UPGRADED-EXAMPLE-PACKAGE>' \
    --module 'example'                     \
    --function 'nudge'                     \
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

Now, the emitted Event's `x` field has value `42` (changed from the
original `41`).

If you wait a further day, change the constant again, and re-run the
script, you will receive an output similar to the following, which
indicates that the upgrade aborted with code `2` (`EDayNotAllowed`):

```
$ node upgrade.js
INCLUDING DEPENDENCY Sui
INCLUDING DEPENDENCY MoveStdlib
BUILDING example
... [snip] ...
  [cause]: {
    effects: {
      messageVersion: 'v1',
      status: {
        status: 'failure',
        error: 'MoveAbort(MoveLocation { module: ModuleId { address: <POLICY-PACKAGE>, name: Identifier("day_of_week") }, function: 1, instruction: 11, function_name: Some("authorize_upgrade") }, 2) in command 0'
      },
... [snip] ...
}
```
