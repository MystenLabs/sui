// module specs::object_spec;

// use sui::object::{borrow_uid, delete_impl, record_new_uid};

// #[spec(target = sui::object::borrow_uid)]
// fun borrow_uid_spec<T: key>(obj: &T): &UID {
//     borrow_uid(obj)
// }

// #[spec(target = sui::object::delete_impl)]
// fun delete_impl_spec(id: address) {
//     delete_impl(id)
// }

// #[spec(target = sui::object::record_new_uid)]
// fun record_new_uid_spec(id: address) {
//     record_new_uid(id)
// }