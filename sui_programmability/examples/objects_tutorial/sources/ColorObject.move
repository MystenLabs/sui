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
            assert!(!TestScenario::can_take_object<ColorObject>(scenario), 0);
        };
        // Check that @owner indeed owns the just-created ColorObject.
        // Also checks the value fields of the object.
        TestScenario::next_tx(scenario, &owner);
        {
            let object = TestScenario::take_object<ColorObject>(scenario);
            let (red, green, blue) = ColorObject::get_color(&object);
            assert!(red == 255 && green == 0 && blue == 255, 0);
            TestScenario::return_object(scenario, object);
        };
    }

    // == Tests covered in Chapter 2 ==

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
            let object = TestScenario::take_object<ColorObject>(scenario);
            let ctx = TestScenario::ctx(scenario);
            ColorObject::delete(object, ctx);
        };
        // Verify that the object was indeed deleted.
        TestScenario::next_tx(scenario, &owner);
        {
            assert!(!TestScenario::can_take_object<ColorObject>(scenario), 0);
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
            let object = TestScenario::take_object<ColorObject>(scenario);
            let ctx = TestScenario::ctx(scenario);
            ColorObject::transfer(object, recipient, ctx);
        };
        // Check that owner no longer owns the object.
        TestScenario::next_tx(scenario, &owner);
        {
            assert!(!TestScenario::can_take_object<ColorObject>(scenario), 0);
        };
        // Check that recipient now owns the object.
        TestScenario::next_tx(scenario, &recipient);
        {
            assert!(TestScenario::can_take_object<ColorObject>(scenario), 0);
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
            assert!(TestScenario::can_take_object<ColorObject>(scenario), 0);
        };
        let sender2 = @0x2;
        TestScenario::next_tx(scenario, &sender2);
        {
            assert!(TestScenario::can_take_object<ColorObject>(scenario), 0);
        };
    }

    #[test]
    #[expected_failure(abort_code = 101)]
    public(script) fun test_mutate_immutable() {
        let sender1 = @0x1;
        let scenario = &mut TestScenario::begin(&sender1);
        {
            let ctx = TestScenario::ctx(scenario);
            ColorObject::create_immutable(255, 0, 255, ctx);
        };
        TestScenario::next_tx(scenario, &sender1);
        {
            let object = TestScenario::take_object<ColorObject>(scenario);
            let ctx = TestScenario::ctx(scenario);
            ColorObject::update(&mut object, 0, 0, 0, ctx);
            TestScenario::return_object(scenario, object);
        };
    }
}
