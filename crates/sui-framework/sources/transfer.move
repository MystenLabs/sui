// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::transfer {

    /// Shared an object that was previously created. Shared objects must currently
    /// be constructed in the transaction they are created.
    const ESharedNonNewObject: u64 = 0;

    /// Transfer ownership of `obj` to `recipient`. `obj` must have the
    /// `key` attribute, which (in turn) ensures that `obj` has a globally
    /// unique ID.
    public fun transfer<T: key>(obj: T, recipient: address) {
        // TODO: emit event
        transfer_internal(obj, recipient)
    }

    /// Freeze `obj`. After freezing `obj` becomes immutable and can no
    /// longer be transferred or mutated.
    public native fun freeze_object<T: key>(obj: T);

    /// Turn the given object into a mutable shared object that everyone
    /// can access and mutate. This is irreversible, i.e. once an object
    /// is shared, it will stay shared forever.
    /// Shared mutable object is not yet fully supported in Sui, which is being
    /// actively worked on and should be supported very soon.
    /// https://github.com/MystenLabs/sui/issues/633
    /// https://github.com/MystenLabs/sui/issues/681
    /// This API is exposed to demonstrate how we may be able to use it to program
    /// Move contracts that use shared objects.
    /// Aborts with `ESharedNonNewObject` of the object being shared was not created
    /// in this transaction. This restriction may be relaxed in the future.
    public native fun share_object<T: key>(obj: T);

    native fun transfer_internal<T: key>(obj: T, recipient: address);

    // Cost calibration functions
    #[test_only]
    public fun calibrate_freeze_object<T: key>(obj: T) {
        freeze_object(obj);
    }
    #[test_only]
    public fun calibrate_freeze_object_nop<T: key + drop>(obj: T) {
        let _ = obj;
    }

    #[test_only]
    public fun calibrate_share_object<T: key>(obj: T) {
        share_object(obj);
    }
    #[test_only]
    public fun calibrate_share_object_nop<T: key + drop>(obj: T) {
        let _ = obj;
    }

    #[test_only]
    public fun calibrate_transfer_internal<T: key>(obj: T, recipient: address) {
        transfer_internal(obj, recipient);
    }
    #[test_only]
    public fun calibrate_transfer_internal_nop<T: key + drop>(obj: T, recipient: address) {
        let _ = obj;
        let _ = recipient;
    }

}
