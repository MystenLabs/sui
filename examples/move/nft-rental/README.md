# sui_nft_rental

## **Description**

NFT renting is a mechanism that allows individuals without ownership or possession of a specific NFT to temporarily utilize or experience it. The implementation of this process leverages the Kiosk Extension to establish an infrastructure conducive to facilitating rental transactions. This approach closely aligns with the ERC-4907 renting standard, making it a suitable choice for Solidity-based use cases intended for implementation on Sui.

## **Requirements**

- Enable a lender to offer their assets for renting for a specified period of time (List for renting)
- Enable a lender to define the rental duration.
    - Borrower has to comply with the renting period.
- Borrower can gain mutable or immutable access to the NFT.
    - Immutable access is read only.
    - Mutable, the lender should consider downgrade and upgrade operations and include them in the renting fee.
- After the renting period has finished the item can be sold normally.
- Royalties
    - Creator defined royalties are respected by encompassing Transfer Policy rules.