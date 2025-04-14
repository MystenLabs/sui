module specs::kiosk_spec;

use sui::kiosk::{Kiosk, KioskOwnerCap, has_access, has_item_with_type};
use sui::object::ID;

#[spec(target = sui::kiosk::has_item_with_type)]
public fun has_item_with_type_spec<T: key + store>(self: &Kiosk, id: ID): bool {
    has_item_with_type<T>(self, id)
}

#[spec(target = sui::kiosk::has_access)]
public fun has_access_spec(self: &mut Kiosk, cap: &KioskOwnerCap): bool {
    has_access(self, cap)
}