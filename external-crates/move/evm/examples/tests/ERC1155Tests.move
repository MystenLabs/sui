module Evm::ERC1155Tests {
    use Evm::Evm::sender;
    use Evm::U256::{zero, one, u256_from_u128};
    use std::vector;
    use Evm::ERC1155;
    use std::ascii::{string};

    const Alice: address = @0x8877;
    const Bob: address = @0x8888;

    #[test]
    fun test_create() {
        ERC1155::create(string(vector::empty<u8>()));
    }

    #[test]
    fun test_balance_of() {
        ERC1155::create(string(vector::empty<u8>()));
        let id1 = one();
        let id2 = u256_from_u128(22);
        assert!(ERC1155::balanceOf(sender(), id1) == zero(), 100);
        assert!(ERC1155::balanceOf(sender(), id2) == zero(), 101);
        assert!(ERC1155::balanceOf(Alice, id1) == zero(), 102);
        assert!(ERC1155::balanceOf(Alice, id2) == zero(), 103);
        assert!(ERC1155::balanceOf(Bob, id1) == zero(), 104);
        assert!(ERC1155::balanceOf(Bob, id2) == zero(), 105);
    }

    #[test]
    fun test_mint() {
        ERC1155::create(string(vector::empty<u8>()));

        let id1 = one();
        let id2 = u256_from_u128(22);
        let dummy_data = vector::empty<u8>();

        ERC1155::mint(Alice, id1, u256_from_u128(100), copy dummy_data);
        ERC1155::mint(Bob, id2, u256_from_u128(1000), copy dummy_data);

        assert!(ERC1155::balanceOf(sender(), id1) == zero(), 106);
        assert!(ERC1155::balanceOf(sender(), id2) == zero(), 107);
        assert!(ERC1155::balanceOf(Alice, id1) == u256_from_u128(100), 108);
        assert!(ERC1155::balanceOf(Alice, id2) == zero(), 109);
        assert!(ERC1155::balanceOf(Bob, id1) == zero(), 110);
        assert!(ERC1155::balanceOf(Bob, id2) == u256_from_u128(1000), 111);
    }

    #[test]
    #[expected_failure(abort_code = 124)]
    fun test_transfer() {
        ERC1155::create(string(vector::empty<u8>()));

        let id1 = one();
        let id2 = u256_from_u128(22);
        let dummy_data = vector::empty<u8>();

        ERC1155::mint(sender(), id1, u256_from_u128(100), copy dummy_data);
        ERC1155::mint(sender(), id2, u256_from_u128(1000), copy dummy_data);

        assert!(ERC1155::balanceOf(sender(), id1) == u256_from_u128(100), 112);
        assert!(ERC1155::balanceOf(sender(), id2) == u256_from_u128(1000), 113);
        assert!(ERC1155::balanceOf(Alice, id1) == zero(), 114);
        assert!(ERC1155::balanceOf(Alice, id2) == zero(), 115);
        assert!(ERC1155::balanceOf(Bob, id1) == zero(), 116);
        assert!(ERC1155::balanceOf(Bob, id2) == zero(), 117);

        ERC1155::safeTransferFrom(sender(), Alice, id1, u256_from_u128(20), copy dummy_data);
        ERC1155::safeTransferFrom(sender(), Bob, id1, u256_from_u128(50), copy dummy_data);
        ERC1155::safeTransferFrom(sender(), Alice, id2, u256_from_u128(300), copy dummy_data);
        ERC1155::safeTransferFrom(sender(), Bob, id2, u256_from_u128(200), copy dummy_data);

        assert!(ERC1155::balanceOf(sender(), id1) == u256_from_u128(30), 118);
        assert!(ERC1155::balanceOf(sender(), id2) == u256_from_u128(500), 119);
        assert!(ERC1155::balanceOf(Alice, id1) == u256_from_u128(20), 120);
        assert!(ERC1155::balanceOf(Alice, id2) == u256_from_u128(300), 121);
        assert!(ERC1155::balanceOf(Bob, id1) == u256_from_u128(50), 122);
        assert!(ERC1155::balanceOf(Bob, id2) == u256_from_u128(200), 123);

        assert!(ERC1155::balanceOf(sender(), id1) == u256_from_u128(100), 124); // expected to fail.
    }
}
