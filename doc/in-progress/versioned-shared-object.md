# Versioned Shared Objects

Packages that involve shared objects need to think about upgrades and
versioning from the start given that **all prior versions of a package
still exist on-chain**.  A useful pattern is to introduce versioning
to the shared object and guard access to functions in the package by a
version check. This allows us to limit access to the shared object to
only the latest version of a package.

Considering our earlier `counter` example, which may have started life
as follows:

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

To ensure that upgrades to this package can limit accesses of the
shared object to the latest version of the package, we need to:

1. Track the current version of the module in a constant, `VERSION`.
2. Track the current version of the shared object, `Counter`, in a new
   `version` field.
3. Introduce an `AdminCap` to protect privileged calls, and associate
   the `Counter` with its `AdminCap` with a new field (you may already
   have a similar type for shared object administration, in which case
   you can re-use that).  This cap will be used to protect calls to
   migrate the shared object from version to version.
4. Guard the entry of all functions that access the shared object with
   a check that its `version` matches the package `VERSION`.
  
An upgrade-aware `counter` module that incorporates all these ideas
looks as follows:

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

To upgrade a module using this pattern requires making two extra
changes, on top of any implementation changes your upgrade requires:

1. Bump the `VERSION` of the package
2. Introduce a `migrate` function to upgrade the shared object:

The following module is an upgraded `counter` that emits `Progress`
events as originally discussed, but also provides tools for an admin
(`AdminCap` holder) to prevent accesses to the counter from older
package versions:

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

Upgrading to this version of the package requires performing the
package upgrade, and calling the `migrate` function in a follow-up
transaction.  Note that the `migrate` function:

- Is an `entry` function and **not `public`**.  This allows it to be
  entirely changed (including changing its signature or removing it
  entirely) in later upgrades.
- Accepts an `AdminCap` and checks that its ID matches the ID of the
  counter being migrated, making it a privileged operation.
- Includes a sanity check that the version of the module is actually
  an upgrade for the object.  This helps catch errors such as failing
  to bump the module version before upgrading.
  
After a successful upgrade, calls to `increment` on the previous
version of the package will abort on the version check, while calls on
the later version should succeed.

## Extensions

This pattern forms the basis for upgradeable packages involving shared
objects but can be extended in a number of ways, depending on your
package's needs:

- The version constraints can be made more expressive:
  - Rather than using a single `u64`, versions could be specified as a
    `String`, or a pair of upper and lowerbounds.
  - Access to specific functions or sets of functions can be
    controlled by adding and removing marker typess as dynamic fields
    on the shared object.
- The `migrate` function could be made more sophisticated (modifying
  other fields in the shared object, adding/removing dynamic fields,
  migrating multiple shared objects simultaneously).
- Large migrations that need to be run over multiple transactions can
  be implemented in a three phase set-up:
  - Disable general access to the shared object by setting its version
    to a sentinel value (e.g. `U64_MAX`), using an `AdminCap`-guarded
    call.
  - Run the migration over the course of multiple transactions
    (e.g. if a large volume of objects need to be moved, it is best to
    do this in batches, to avoid hitting transaction limits).
  - Set the version of the shared object back to a usable value.

