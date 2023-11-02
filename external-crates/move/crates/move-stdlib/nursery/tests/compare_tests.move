// Tests for polymorphic comparison in Move
#[test_only]
module std::compareTests {
    use std::compare;
    use std::bcs;

    const EQUAL: u8 = 0;
    const LESS_THAN: u8 = 1;
    const GREATER_THAN: u8 = 2;


    #[test]
    fun equality_of_simple_types() {
        assert!(compare::cmp_bcs_bytes(&bcs::to_bytes(&true), &bcs::to_bytes(&true)) == EQUAL, 0);
        assert!(compare::cmp_bcs_bytes(&bcs::to_bytes(&1u8), &bcs::to_bytes(&1u8)) == EQUAL, 0);
        assert!(compare::cmp_bcs_bytes(&bcs::to_bytes(&1u16), &bcs::to_bytes(&1u16)) == EQUAL, 0);
        assert!(compare::cmp_bcs_bytes(&bcs::to_bytes(&1u32), &bcs::to_bytes(&1u32)) == EQUAL, 0);
        assert!(compare::cmp_bcs_bytes(&bcs::to_bytes(&1), &bcs::to_bytes(&1)) == EQUAL, 0);
        assert!(compare::cmp_bcs_bytes(&bcs::to_bytes(&1u128), &bcs::to_bytes(&1u128)) == EQUAL, 0);
        assert!(compare::cmp_bcs_bytes(&bcs::to_bytes(&1u256), &bcs::to_bytes(&1u256)) == EQUAL, 0);
        assert!(compare::cmp_bcs_bytes(&bcs::to_bytes(&@0x1), &bcs::to_bytes(&@0x1)) == EQUAL, 0);
        assert!(compare::cmp_bcs_bytes(&bcs::to_bytes(&x"01"), &bcs::to_bytes(&x"01")) == EQUAL, 0);
    }

    #[test]
    fun inequality_of_simple_types() {
        // inequality of simple types
        assert!(compare::cmp_bcs_bytes(&bcs::to_bytes(&true), &bcs::to_bytes(&false)) != EQUAL, 0);
        assert!(compare::cmp_bcs_bytes(&bcs::to_bytes(&1u8), &bcs::to_bytes(&0u8)) != EQUAL, 0);
        assert!(compare::cmp_bcs_bytes(&bcs::to_bytes(&1u16), &bcs::to_bytes(&0u16)) != EQUAL, 0);
        assert!(compare::cmp_bcs_bytes(&bcs::to_bytes(&1u32), &bcs::to_bytes(&0u32)) != EQUAL, 0);
        assert!(compare::cmp_bcs_bytes(&bcs::to_bytes(&1), &bcs::to_bytes(&0)) != EQUAL, 0);
        assert!(compare::cmp_bcs_bytes(&bcs::to_bytes(&1u128), &bcs::to_bytes(&0u128)) != EQUAL, 0);
        assert!(compare::cmp_bcs_bytes(&bcs::to_bytes(&1u256), &bcs::to_bytes(&0u256)) != EQUAL, 0);
        assert!(compare::cmp_bcs_bytes(&bcs::to_bytes(&@0x1), &bcs::to_bytes(&@0x0)) != EQUAL, 0);
        assert!(compare::cmp_bcs_bytes(&bcs::to_bytes(&x"01"), &bcs::to_bytes(&x"00")) != EQUAL, 0);
    }

    #[test]
    fun less_than_with_natural_ordering() {
        // less than for types with a natural ordering exposed via bytecode operations
        assert!(compare::cmp_bcs_bytes(&bcs::to_bytes(&false), &bcs::to_bytes(&true)) == LESS_THAN, 0);
        assert!(compare::cmp_bcs_bytes(&bcs::to_bytes(&0u8), &bcs::to_bytes(&1u8)) == LESS_THAN, 0);
        assert!(compare::cmp_bcs_bytes(&bcs::to_bytes(&0u16), &bcs::to_bytes(&1u16)) == LESS_THAN, 0);
        assert!(compare::cmp_bcs_bytes(&bcs::to_bytes(&0u32), &bcs::to_bytes(&1u32)) == LESS_THAN, 0);
        assert!(compare::cmp_bcs_bytes(&bcs::to_bytes(&0), &bcs::to_bytes(&1)) == LESS_THAN, 0);
        assert!(compare::cmp_bcs_bytes(&bcs::to_bytes(&0u128), &bcs::to_bytes(&1u128)) == LESS_THAN, 0);
        assert!(compare::cmp_bcs_bytes(&bcs::to_bytes(&0u256), &bcs::to_bytes(&1u256)) == LESS_THAN, 0);
    }

    #[test]
    fun less_than_without_natural_ordering() {
        // less then for types without a natural ordering exposed by bytecode operations
        assert!(compare::cmp_bcs_bytes(&bcs::to_bytes(&@0x0), &bcs::to_bytes(&@0x1)) == LESS_THAN, 0); // sensible
        assert!(compare::cmp_bcs_bytes(&bcs::to_bytes(&@0x01), &bcs::to_bytes(&@0x10)) == LESS_THAN, 0); // sensible
        assert!(compare::cmp_bcs_bytes(&bcs::to_bytes(&@0x100), &bcs::to_bytes(&@0x001)) == LESS_THAN, 0); // potentially confusing
        assert!(compare::cmp_bcs_bytes(&bcs::to_bytes(&x"00"), &bcs::to_bytes(&x"01")) == LESS_THAN, 0); // sensible
        assert!(compare::cmp_bcs_bytes(&bcs::to_bytes(&x"01"), &bcs::to_bytes(&x"10")) == LESS_THAN, 0); // sensible
        assert!(compare::cmp_bcs_bytes(&bcs::to_bytes(&x"0000"), &bcs::to_bytes(&x"01")) == LESS_THAN, 0); //
        assert!(compare::cmp_bcs_bytes(&bcs::to_bytes(&x"0100"), &bcs::to_bytes(&x"0001")) == LESS_THAN, 0); // potentially confusing
    }

    #[test]
    fun greater_than_with_natural_ordering() {
        // greater than for types with a natural ordering exposed by bytecode operations
        assert!(compare::cmp_bcs_bytes(&bcs::to_bytes(&true), &bcs::to_bytes(&false)) == GREATER_THAN, 0);
        assert!(compare::cmp_bcs_bytes(&bcs::to_bytes(&1u8), &bcs::to_bytes(&0u8)) == GREATER_THAN, 0);
        assert!(compare::cmp_bcs_bytes(&bcs::to_bytes(&1u16), &bcs::to_bytes(&0u16)) == GREATER_THAN, 0);
        assert!(compare::cmp_bcs_bytes(&bcs::to_bytes(&1u32), &bcs::to_bytes(&0u32)) == GREATER_THAN, 0);
        assert!(compare::cmp_bcs_bytes(&bcs::to_bytes(&1), &bcs::to_bytes(&0)) == GREATER_THAN, 0);
        assert!(compare::cmp_bcs_bytes(&bcs::to_bytes(&1u128), &bcs::to_bytes(&0u128)) == GREATER_THAN, 0);
        assert!(compare::cmp_bcs_bytes(&bcs::to_bytes(&1u256), &bcs::to_bytes(&0u256)) == GREATER_THAN, 0);
    }

    #[test]
    fun greater_than_without_natural_ordering() {
        // greater than for types without a natural ordering exposed by by bytecode operations
        assert!(compare::cmp_bcs_bytes(&bcs::to_bytes(&@0x1), &bcs::to_bytes(&@0x0)) == GREATER_THAN, 0); // sensible
        assert!(compare::cmp_bcs_bytes(&bcs::to_bytes(&@0x10), &bcs::to_bytes(&@0x01)) == GREATER_THAN, 0); // sensible
        assert!(compare::cmp_bcs_bytes(&bcs::to_bytes(&@0x001), &bcs::to_bytes(&@0x100)) == GREATER_THAN, 0); // potentially confusing
        assert!(compare::cmp_bcs_bytes(&bcs::to_bytes(&x"01"), &bcs::to_bytes(&x"00")) == GREATER_THAN, 0); // sensible
        assert!(compare::cmp_bcs_bytes(&bcs::to_bytes(&x"10"), &bcs::to_bytes(&x"01")) == GREATER_THAN, 0); // sensible
        assert!(compare::cmp_bcs_bytes(&bcs::to_bytes(&x"01"), &bcs::to_bytes(&x"0000")) == GREATER_THAN, 0); // sensible
        assert!(compare::cmp_bcs_bytes(&bcs::to_bytes(&x"0001"), &bcs::to_bytes(&x"0100")) == GREATER_THAN, 0); // potentially confusing
    }
}
