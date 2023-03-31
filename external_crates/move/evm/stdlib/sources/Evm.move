/// Module which provides access to EVM functionality, including information about the executing transaction.
////
/// This currently only represents a basic subset of what we may want to expose.
module Evm::Evm {
    use std::vector;
    use Evm::U256::{Self, U256};

    /// Returns the address of the executing contract.
    public native fun self(): address;

    /// An alias for `msg_sender`.
    public fun sender(): address {
        msg_sender()
    }

    /// An alias for `msg_value`.
    public fun value(): U256 {
        msg_value()
    }

    /// Returns the balance, in Wei, of any account.
    public native fun balance(addr: address): U256;

    /// Transfers the given amount to the target account.
    public native fun transfer(addr: address, amount: U256);

    /// Emits an event. The type passed for `E` must be annotated with #[event].
    public native fun emit<E>(e: E);

    /// Creates a signer for the contract's address space.
    public native fun sign(addr: address): signer;

    /// Returns the keccak256 hash for the input `data`.
    /// The length of the resulting vector should be 32.
    public native fun keccak256(data: vector<u8>): vector<u8>;

    /// Returns the first four bytes of `data`.
    /// This is the equivalent to the type casting operation `bytes4` in Solidity.
    public fun bytes4(data: vector<u8>): vector<u8> {
        assert!(vector::length(&data) >= 4, 0);
        let res = vector::empty<u8>();
        vector::push_back(&mut res, *vector::borrow(&data, 0));
        vector::push_back(&mut res, *vector::borrow(&data, 1));
        vector::push_back(&mut res, *vector::borrow(&data, 2));
        vector::push_back(&mut res, *vector::borrow(&data, 3));
        res
    }

    /// Returns the result of (`v1` xor `v2`).
    // TODO: implment this in Move.
    public native fun bytes_xor(v1: vector<u8>, v2: vector<u8>): vector<u8>;

    /// Returns true iff `addr` is a contract address.
    // This can be implemented in Solidity as follows:
    // ```
    // uint32 size;
    // assembly {
    //   size := extcodesize(_addr)
    // }
    // return (size > 0);
    // ```
    public fun isContract(addr: address): bool {
        U256::gt(extcodesize(addr), U256::zero())
    }

    /// Returns the size of the code at address `addr`.
    public native fun extcodesize(addr: address): U256;

    /// Define the unit (null or void) type.
    struct Unit has drop {}

    /// Get tokenURI with base URI.
    // This is implemented in Solidity as follows:
    //   bytes(baseURI).length > 0 ? string(abi.encodePacked(baseURI, tokenId.toString())) : "";\
    // TODO: this probably belongs in ERC721.move?
    public fun tokenURI_with_baseURI(baseURI: vector<u8>, tokenId: U256): vector<u8> {
        if (vector::length(&baseURI) == 0) {
            vector::empty()
        } else {
            concat(baseURI, to_string(tokenId))
        }
    }

    /// Abort with an error message.
    public native fun abort_with(message: vector<u8>);

    /// If `cond` is false, it aborts with `message`.
    public fun require(cond: bool, message: vector<u8>) {
        if (!cond) { abort_with(message); }
    }

    // --------------------------------
    // Block and Transaction Properties
    // --------------------------------

    /// Return the hash of the given block when blocknumber is one of the 256
    /// most recent blocks; otherwise returns zero. The return value is 32 bytes.
    public native fun blockhash(block_number: U256): vector<u8>;

    /// Return the current block's base fee (EIP-3198 and EIP-1559).
    public native fun block_basefee(): U256;

    /// Return the current chain id.
    public native fun block_chainid(): U256;

    /// Return the current block miner's address.
    public native fun block_coinbase(): address;

    /// Return the current block difficulty.
    public native fun block_difficulty(): U256;

    /// Return the current block gaslimit.
    public native fun block_gaslimit(): U256;

    /// Return the current block number.
    public native fun block_number(): U256;

    /// Return the current block timestamp as seconds since unix epoch.
    public native fun block_timestamp(): U256;

    /// Return the remaining gas.
    public native fun gasleft(): U256;

    /// Return the complete calldata.
    public native fun msg_data(): vector<u8>;

    /// Return the sender of the message (current call).
    public native fun msg_sender(): address;

    /// Return the first four bytes of the calldata (i.e. function identifier).
    public native fun msg_sig(): vector<u8>;

    /// Return the number of wei sent with the message.
    public native fun msg_value(): U256;

    /// Return the gas price of the transaction.
    public native fun tx_gasprice(): U256;

    /// Return the sender of the transaction (full call chain).
    public native fun tx_origin(): address;

    // --------------------------------
    // String Operations
    // --------------------------------

    /// Add s2 to the end of s1.
    public native fun concat(s1: vector<u8>, s2: vector<u8>): vector<u8>;

    /// Return the string representation of a U256.
    public native fun to_string(x: U256): vector<u8>;
}
