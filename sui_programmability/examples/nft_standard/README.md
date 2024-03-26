# NFT Standards, Philosophy, and Requirements

# Basics

Sui’s object-centric data model gives Sui objects many of the features of a basic ERC721-style NFT: an owner, the ability to be transferred, a globally unique identifier.

### Individual NFT’s

An NFT is a Sui object (i.e., a struct value whose declared type has the `key` ability, which in turn means the struct value has a field named `id` holding a globally unique ID). All NFT’s are Sui objects, but not every Sui object is an NFT.

### Collections

An NFT collection is a Move struct type with the `key` ability. Every NFT collection has a distinct Move struct type, but not every Move struct type corresponds to an NFT collection.

Collection metadata is a singleton object of type `sui::collection::Collection<T>` created via the [module initializer](https://examples.sui.io/basics/init-function.html) of the module that defines the NFT type `T`. Initializing a collection gives the creator a `MintCap<T>` granting permission to mint NFT's of the given type, and a `RoyaltyCap<T>` granting permission to mint `RoyaltyReceipt<T>`'s (which will be required upon sale--more on this below).

### Transfers

Every Sui object whose declared type has the `store` ability can be freely transferred via the polymorphic `transfer::transfer` API in the Sui Framework, or the special `TransferObject` transaction type. Objects without the `store` ability may be freely transferrable, transferrable with restrictions, or non-transferrable—the implementer of the module declaring the type can decide.

Beyond the basics, we broadly view NFT standards in two separate parts: display and commerce.

- Display standards give programmers the power to tell clients (wallets, explorers, marketplaces, …) how to display and organize NFT’s. Clients have a “default” scheme for displaying Sui objects (show a JSON representation of all field values); these standards allow programmers to make this representation more visually appealing and end-user friendly.
    - Clients are the only audience of these standards. They are not intended to be used by Sui smart contracts that work with NFT’s
- Commerce standards allow Sui Move programmers to use libraries for NFT pricing, marketplace listing, royalties, etc. These standards are designed to be compositional building blocks for key NFT-related functionality that can be used either in isolation, or together
    - The intended audience for these standards is both clients and Sui Move programmers.
    - Clients are aware of the key functions and events in these libraries. They know how to craft transactions using these functions, use read functions for (e.g.) checking ownership of an NFT, and interpret events emitted by the library to inform the user of on-chain happenings.
    - Sui Move programmers use these libraries both for creating NFT’s (e.g., designing an NFT drop using an auction library) and writing NFT infrastructure that build on top of the core libraries (e.g., creating a new royalty policy that plugs into the extensible libraries)

# Commerce

These standards do not yet exist, but here are:

- the requirements we have in mind
- the philosophy behind these requirements
- some implementation ideas

### Requirements

- **Simple proof of ownership for clients:** We must support checking NFT ownership in a uniform way.
    - This mechanism is intended for off-chain ownership checking: e.g., for a wallet to show a user which NFT’s they own, allowing access to tokengated content, etc. On-chain ownership checking will likely use a separate mechanism that is more direct (e.g., “function `f` checks ownership of a `T` by asking for a parameter of type `&T`).
    - If all NFT’s were single-owner objects, this would be easy (just look at the object ownership metadata!), but this is too restrictive—many NFT’s will need to be shared or quasi-shared (e.g., an NFT listed for sale on a marketplace will be quasi-shared, but will still have an owner).
    - This should be simple and broadly accessible: either a single function call, or small number of direct object reads encapsulated behind a single API call. This should *not* require an indexer or special API’s not supported by an ordinary full node
- **Listing without lockup**. An ****NFT listed for sale (e.g., on a marketplace) must retain most (but not all) of its functionality
    - NFT’s that are listed for sale should still have an owner, and should be usable for tokengating, games, etc.
    - However, it is important that listed NFT’s cannot be mutated by the owner (or if it can, this fact + the risks should be very clear to the buyer)—e.g., the owner of a `Hero` listed for sale at a high price should not be able to remove items from the `Hero`'s inventory just before a sale happens.
    - An NFT should be able to be listed on multiple marketplaces at the same time. Note that the natural way of implementing marketplace listing (make the NFT object a child of a shared object marketplace) does not support this.
- **Low-level royalty enforcement**. We believe royalties are the raison d'être of NFT’s from the creator perspective—this is a feature physical or web2 digital creations cannot easily have.
    - Ironclad royalty enforcement is impossible (e.g., can give away an NFT for free and do a side payment). But we want to make it highly inconvenient (e.g., you’d have to go outside the accepted standards) and socially unacceptable (e.g., you will be shunned by users, creators, even other marketplaces) to bypass the standards
    - We want to enforce royalties on *sales*, not transfers. A user should be able to transfer NFT’s between their own addresses, or give an NFT to another user.
- **Low-level commission enforcement for sale facilitator**. Same as above, but for the party (e.g., marketplace, wallet, explorer) that facilitates the sale. We need to give both creators (via royalties) and facilitators (via commissions) a business model.
- **Borrowing**. Another distinguishing feature of NFT’s compared to physical or web2 digital assets is the ability to implementing trusted or conditional borrowing .
    - Allowing immutable borrowing of assets is straightforward
    - Allowing mutable borrowing of assets is possible, but should be done with care (e.g., the borrower of a `Hero` probably needs to be mutable so using the `Hero` in a game lets the `Hero` level up, but we don’t want to allow the borrower to sell off all of the `Hero`'s items)
- **Arbitrary customization of royalties, commission, minting/sale method (e.g., kind of auction used).** The mechanisms for each of these is rapidly evolving, and we do not think a standard that enforces a fixed menu of choices will age well.
    - Here, “arbitrary” means that a creator or marketplace can implement a policy in arbitrary Move code with no restrictions (e.g., the policy might involve touching 10 shared objects, and that is ok).
    - The recommended technical mechanism for this sort of extensibility is the “receipt” pattern; e.g., a `buy<T>(royalty: RoyaltyReceipt<T>)`, where the creator of the `T` collection gets to write arbitrary code that decides when a `RoyaltyReceipt<T>` can be created.
    - We should have safe, convenient libraries for common royalty policies (e.g., no royalty, royalty split among set of addresses), commissions, and auctions
- **Price discovery aka “NFT orderbook”**. The standards for listing NFT’s should provide (via event queries or other mechanisms) a common framework for discovering the price for a given NFT, the floor price of a collection, etc.

### Implementation thoughts

One approach to satisfying a number of these requirements is to define a shared `NFTSafe` that is associated with a specific address (similar to the `Safe` for coins proposed here: ‣).

- The owner of the `NFTSafe` has an `OwnerCap` that allows them to mint `TransferCap`'s, which give the holder permission to transfer the NFT between two different `NFTSafe`'s.
    - When an owner lists an NFT on a marketplace, they would actually list the `TransferCap`--the NFT continues to sit in the safe.
    - A `TransferCap` doesn’t give the holder unconditional permission to transfer the NFT—they must also satisfy the royalty and commission policies. This is how royalty enforcement happens.
    - If you have an `OwnerCap`, you can take an NFT out of the safe (i.e., make it a single-owner object), or transfer it to a different safe without paying royalties or commissions. This satisfies the “let a user transfer NFT between addresses” requirements
- Borrowing an NFT can be implemented through a `BorrowCap` that is similar to `TransferCap`, but only allows the holder to temporarily take the NFT out of a safe during a transaction—they must return it before the transaction ends
    - The “hot potato” design pattern is the natural implementation approach for this
- An `OwnerCap` also allows the owner to
    - transfer all of their NFT’s to a different user (by transferring the `OwnerCap`),
    - gate access to NFT’s via a multisig (by putting the `OwnerCap` in a multisig smart contract wallet)
    - Use [KELP](https://eprint.iacr.org/2021/289.pdf)-like social recovery schemes to protect the user against key loss (by putting the `OwnerCap` in a social recovery smart contract wallet)
- NFT minting can either transfer the freshly created NFT directly to a user address, or put it into the user’s `NFTSafe`. Some users might want to allow arbitrary deposits of NFT’s into their safe, whereas others might only want to give themselves permission to pull an NFT into the safe (e.g., as a means of curation or spam prevention)
- A single, heterogenous `NFTSafe` is probably the simplest/most convenient, but one could also imagine `NFTSafe<T>` that partitions by type. This draft PR goes for the latter.
- A reasonable implementation of the `NFTSafe` idea will rely heavily on the forthcoming “dynamic child object access” feature: https://github.com/MystenLabs/sui/issues/4203.

# Display

The display standard is organized as:

- A set of (field name, type) pairs with a special interpretation on the client side. When a non-wrapped Sui object has a field with the given name and type, these rules will be applied.

| Field Name | Move type | Description |
| --- | --- | --- |
| name | std::string::String, std::ascii::String | Name of the NFT. Shown at the top of the NFT view |
| description | std::string::String, std::ascii::String | Description of the NFT. Shown in the NFT properties view. |
| url | sui::url::Url, sui::url::UrlCommitment, vector<sui::url::Url>, vector<sui::url::UrlCommitmen>t> | URL containing the content for an NFT, or a vector containing several such URLs. If the URL contains an image, it will be displayed on the left of the NFT view |
- A set of types with special display interpretation on the client side. When *any* Sui object (wrapped or non-wrapped) has a field of the given type that does not match one of the rules in the previous table, these rules will be applied.

| Move type | Description |
| --- | --- |
| std::string::String, std::ascii::String | Displayed as a UTF8-encoded string for std::string::String or a vector<u8> if the underlying bytes are not valid ASCII., and an ASCII-encoded string for std::ascii::String. Displayed as a vector<u8> |
| sui::url::Url, sui::url::UrlCommitment | Displayed as a clickable hyperlink |
| sui::object::ID,
sui::object::UID | Displayed in hex with a leading 0x (e.g., 0xabc..), with a clickable hyperlink to the object view page in the Sui explorer |
| std::option::Option<T> | Displayed as None if the Option  does not contain a value, and Some(_) with display rules applied to the contents if the Option contains a value. |

## Philosophy

The aim of this design is to gives Sui programmers maximum flexibility—they don’t have to use special wrapper types (e.g., `NFT<T>`) to implement NFT’s with visual elements. They can simply define ordinary Sui objects that use the special field and type names, and clients will understand them.

## Status

- The standards above are supported by several (perhaps all?) Sui wallets and the explorer
- The standards above are fairly bare-bones, are we are very open to tweaks and extensions as long as they preserve the core philosophy of the display standards. For example:
    - other media types
    - distinguishing between different kinds of URL’s (e.g. an image URL vs the URL for the website of a collection)
    - a distinguished type for collection metadata (as opposed to NFT metadata)
    - a distinguished field/type to identify the collection creator(s)
    - storing media directly in an NFT (rather than at a URL)
    - finalizing the standards for `UrlCommitment`
- There should be a way to “opt out” of being shown as an NFT for an object that happens to define some or all of the special (name, field) pairs
