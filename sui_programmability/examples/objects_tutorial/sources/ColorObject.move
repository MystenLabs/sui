// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module Tutorial::ColorObject {
    use Sui::ID::{Self, VersionedID};
    use Sui::Transfer;
    use Sui::TxContext::{Self, TxContext};

    struct ColorObject has key {
        id: VersionedID,
        red: u8,
        green: u8,
        blue: u8,
    }

    // == Functions covered in Chapter 1 ==

    fun new(red: u8, green: u8, blue: u8, ctx: &mut TxContext): ColorObject {
        ColorObject {
            id: TxContext::new_id(ctx),
            red,
            green,
            blue,
        }
    }

    public(script) fun create(red: u8, green: u8, blue: u8, ctx: &mut TxContext) {
        let color_object = new(red, green, blue, ctx);
        Transfer::transfer(color_object, TxContext::sender(ctx))
    }

    public fun get_color(self: &ColorObject): (u8, u8, u8) {
        (self.red, self.green, self.blue)
    }

    // == Functions covered in Chapter 2 ==

    /// Copies the values of `from_object` into `into_object`.
    public(script) fun copy_into(from_object: &ColorObject, into_object: &mut ColorObject, _ctx: &mut TxContext) {
        into_object.red = from_object.red;
        into_object.green = from_object.green;
        into_object.blue = from_object.blue;
    }

    public(script) fun delete(object: ColorObject, _ctx: &mut TxContext) {
        let ColorObject { id, red: _, green: _, blue: _ } = object;
        ID::delete(id);
    }

    public(script) fun transfer(object: ColorObject, recipient: address, _ctx: &mut TxContext) {
        Transfer::transfer(object, recipient)
    }

    // == Functions covered in Chapter 3 ==

    public(script) fun freeze_object(object: ColorObject, _ctx: &mut TxContext) {
        Transfer::freeze_object(object)
    }

    public(script) fun create_immutable(red: u8, green: u8, blue: u8, ctx: &mut TxContext) {
        let color_object = new(red, green, blue, ctx);
        Transfer::freeze_object(color_object)
    }

    public(script) fun update(
        object: &mut ColorObject,
        red: u8, green: u8, blue: u8,
        _ctx: &mut TxContext,
    ) {
        object.red = red;
        object.green = green;
        object.blue = blue;
    }
}

#[test_only]
module Tutorial::ColorObjectTests {
    use Sui::TestScenario;
    use Tutorial::ColorObject::{Self, ColorObject};
    use Sui::TxContext;

    // == Tests covered in Chapter 1 ==

    #[test]
    public(script) fun test_create() {
        let owner = @0x1;
        // Create a ColorObject and transfer it to @owner.
        let scenario = &mut TestScenario::begin(&owner);
        {
            let ctx = TestScenario::ctx(scenario);
            ColorObject::create(255, 0, 255, ctx);
        };
        // Check that @not_owner does not own the just-created ColorObject.
        let not_owner = @0x2;
        TestScenario::next_tx(scenario, &not_owner);
        {
            assert!(!TestScenario::can_take_owned<ColorObject>(scenario), 0);
        };
        // Check that @owner indeed owns the just-created ColorObject.
        // Also checks the value fields of the object.
        TestScenario::next_tx(scenario, &owner);
        {
            let object = TestScenario::take_owned<ColorObject>(scenario);
            let (red, green, blue) = ColorObject::get_color(&object);
            assert!(red == 255 && green == 0 && blue == 255, 0);
            TestScenario::return_owned(scenario, object);
        };
    }

    // == Tests covered in Chapter 2 ==

    #[test]
    public(script) fun test_copy_into() {
        let owner = @0x1;
        let scenario = &mut TestScenario::begin(&owner);
        // Create two ColorObjects owned by `owner`, and obtain their IDs.
        let (id1, id2) = {
            let ctx = TestScenario::ctx(scenario);
            ColorObject::create(255, 255, 255, ctx);
            let id1 = TxContext::last_created_object_id(ctx);
            ColorObject::create(0, 0, 0, ctx);
            let id2 = TxContext::last_created_object_id(ctx);
            (id1, id2)
        };
        TestScenario::next_tx(scenario, &owner);
        {
            let obj1 = TestScenario::take_owned_by_id<ColorObject>(scenario, id1);
            let obj2 = TestScenario::take_owned_by_id<ColorObject>(scenario, id2);
            let (red, green, blue) = ColorObject::get_color(&obj1);
            assert!(red == 255 && green == 255 && blue == 255, 0);

            let ctx = TestScenario::ctx(scenario);
            ColorObject::copy_into(&obj2, &mut obj1, ctx);
            TestScenario::return_owned(scenario, obj1);
            TestScenario::return_owned(scenario, obj2);
        };
        TestScenario::next_tx(scenario, &owner);
        {
            let obj1 = TestScenario::take_owned_by_id<ColorObject>(scenario, id1);
            let (red, green, blue) = ColorObject::get_color(&obj1);
            assert!(red == 0 && green == 0 && blue == 0, 0);
            TestScenario::return_owned(scenario, obj1);
        }
    }

    #[test]
    public(script) fun test_delete() {
        let owner = @0x1;
        // Create a ColorObject and transfer it to @owner.
        let scenario = &mut TestScenario::begin(&owner);
        {
            let ctx = TestScenario::ctx(scenario);
            ColorObject::create(255, 0, 255, ctx);
        };
        // Delete the ColorObject we just created.
        TestScenario::next_tx(scenario, &owner);
        {
            let object = TestScenario::take_owned<ColorObject>(scenario);
            let ctx = TestScenario::ctx(scenario);
            ColorObject::delete(object, ctx);
        };
        // Verify that the object was indeed deleted.
        TestScenario::next_tx(scenario, &owner);
        {
            assert!(!TestScenario::can_take_owned<ColorObject>(scenario), 0);
        }
    }

    #[test]
    public(script) fun test_transfer() {
        let owner = @0x1;
        // Create a ColorObject and transfer it to @owner.
        let scenario = &mut TestScenario::begin(&owner);
        {
            let ctx = TestScenario::ctx(scenario);
            ColorObject::create(255, 0, 255, ctx);
        };
        // Transfer the object to recipient.
        let recipient = @0x2;
        TestScenario::next_tx(scenario, &owner);
        {
            let object = TestScenario::take_owned<ColorObject>(scenario);
            let ctx = TestScenario::ctx(scenario);
            ColorObject::transfer(object, recipient, ctx);
        };
        // Check that owner no longer owns the object.
        TestScenario::next_tx(scenario, &owner);
        {
            assert!(!TestScenario::can_take_owned<ColorObject>(scenario), 0);
        };
        // Check that recipient now owns the object.
        TestScenario::next_tx(scenario, &recipient);
        {
            assert!(TestScenario::can_take_owned<ColorObject>(scenario), 0);
        };
    }

    // == Tests covered in Chapter 3 ==

    #[test]
    public(script) fun test_immutable() {
        let sender1 = @0x1;
        let scenario = &mut TestScenario::begin(&sender1);
        {
            let ctx = TestScenario::ctx(scenario);
            ColorObject::create_immutable(255, 0, 255, ctx);
        };
        TestScenario::next_tx(scenario, &sender1);
        {
            assert!(!TestScenario::can_take_owned<ColorObject>(scenario), 0);
        };
        let sender2 = @0x2;
        TestScenario::next_tx(scenario, &sender2);
        {
            let object_wrapper = TestScenario::take_immutable<ColorObject>(scenario);
            let object = TestScenario::borrow(&object_wrapper);
            let (red, green, blue) = ColorObject::get_color(object);
            assert!(red == 255 && green == 0 && blue == 255, 0);
            TestScenario::return_immutable(scenario, object_wrapper);
        };
    }
}
