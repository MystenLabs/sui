/// An example of a module that uses Shared Objects and ID linking/access.
///
/// This module allows any content to be locked inside a 'virtual chest' and later
/// be accessed by putting a 'key' into the 'lock'. Lock is shared and is visible
/// and discoverable by the key owner. 
/// 
/// Possible additions:
/// - make it reusable, since Key is already transferable
/// - improve error codes
///
module Basics::Lock {
    use Sui::ID::{Self, ID, VersionedID};
    use Sui::Transfer;
    use Sui::TxContext::{Self, TxContext};
    use Std::Option::{Self, Option};

    /// Lock that stores any content inside it. 
    struct Lock<T: store + key> has key {
        id: VersionedID,
        locked: Option<T>
    }

    /// A key that is created with a Lock; is transferable
    /// and contains all the needed information to open the Lock. 
    struct Key<phantom T: store + key> has key {
        id: VersionedID,
        for: ID
    }

    /// Lock some content inside a shared object. A key is created and is 
    /// sent to the transaction sender.
    public fun lock<T: store + key>(obj: T, ctx: &mut TxContext) {
        let id = TxContext::new_id(ctx);
        let for = *ID::inner(&id);

        Transfer::share_object(Lock<T> {
            id,
            locked: Option::some(obj),
        });

        Transfer::transfer(Key<T> {
            for,
            id: TxContext::new_id(ctx)
        }, TxContext::sender(ctx));
    }

    /// Unlock the Lock with a Key and trasfer its contents to the owner. 
    /// Can only be called if both conditions are met:
    /// - key matches the lock
    /// - lock is not empty
    public fun unlock<T: store + key>(lock: &mut Lock<T>, key: Key<T>, ctx: &mut TxContext) {
        let Key { id, for } = key;

        assert!(Option::is_some(&lock.locked), 0);
        assert!(ID::bytes(&for) == ID::id_bytes(lock), 0);

        ID::delete(id);

        let content = Option::extract(&mut lock.locked);
        Transfer::transfer(content, TxContext::sender(ctx));
    }
}
