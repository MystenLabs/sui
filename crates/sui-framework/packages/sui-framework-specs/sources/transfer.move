// module specs::transfer_spec;

// use prover::prover::ensures;
// use prover::ghost;
// use sui::transfer::{freeze_object_impl, share_object_impl, transfer_impl, receive_impl};
// use sui::object::ID;

// public struct SpecTransferAddress {}
// public struct SpecTransferAddressExists {}

// #[spec(target = sui::transfer::freeze_object_impl)]
// fun freeze_object_impl_spec<T: key>(obj: T) {
//     freeze_object_impl(obj)
// }

// #[spec(target = sui::transfer::share_object_impl)]
// fun share_object_impl_spec<T: key>(obj: T) {
//     share_object_impl(obj)
// }

// #[spec(target = sui::transfer::transfer_impl)]
// fun transfer_impl_spec<T: key>(obj: T, recipient: address) {
//     ghost::declare_global_mut<SpecTransferAddressExists, bool>();
//     ghost::declare_global_mut<SpecTransferAddress, address>();

//     transfer_impl(obj, recipient);

//     ensures(ghost::global<SpecTransferAddressExists, bool>() == true);
//     ensures(ghost::global<SpecTransferAddress, address>() == recipient);
// }

// #[spec(target = sui::transfer::receive_impl)]
// fun receive_impl_spec<T: key>(parent: address, to_receive: ID, version: u64): T {
//     receive_impl(parent, to_receive, version)
// }


