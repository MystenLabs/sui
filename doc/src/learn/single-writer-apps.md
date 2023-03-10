---
title: Use Sui Single-Writer-Friendly (SWF) Apps
---

This page lists applications that can work in the single-writer model defined as [simple transactions](how-sui-works.md#simple-transactions) in Sui.

Apart from the obvious single-writer friendly applications (such as simple peer-to-peer asset transfer), note that some proposals that typically require shared objects have been transformed to variants that require only a shared object as a final step and not for every action, such as voting and lotteries and DeFi Oracle price quotes submission.

 * Regular peer-to-peer (p2p) transactions ([see how to create a new Coin with just 7 lines of Sui Move code](https://www.linkedin.com/posts/chalkiaskostas_startup-smartcontract-cryptocurrency-activity-6946006856528003072-CvI0)).
 * Public bulletin board; users store only publicly accessed data, files, links, metadata.
 * Proof of existence: similar to the above, but for time-stamped documents; it can be extended to support commitment proof of existence, i.e. publish your hash, then reveal.
 * Private decentralized repository - users store private files, encrypted under their public keys; users' public keys can be represented as NFTs.
 * Extend the above for selected disclosure CV (resume) repository, University degrees repository.
 * Decentralized or conventional Certificate Authority. Authorities publish their signatures over certs, they can revoke any time (easier revocation).
 * Messaging service: apps, Oracles and Internet of Things (IoTs) exchanging messages. Sui is probably the best platform for any messaging protocol, as typically each response and message can be encoded with a single-writer NFT.
 * Extend the above to social networks; note that each post is a single-writer NFT. See a [smart contract implementation of a fully functional decentralized Twitter with just 50 lines of Sui Move code](https://github.com/MystenLabs/sui/blob/main/sui_programmability/examples/nfts/sources/chat.move).
 * Extend the above to private messaging (i.e., decentralized WhatsApp or Signal).
 * Extend the above for any website / blog / rating platform (i.e., Yelp and Tripadvisor).
 * Personal GitHub, Overleaf LaTex editor, wish/shopping lists, etc.
 * Personal password manager.
 * Non-interactive games (i.e., advertise/evolve your SimCity, FarmVille state etc.).
 * Human vs. Computer games (i.e., chess AI that is programmed into the smart contract. The AI automatically plays back in the same transaction of user's chess move).
 * Coupons and tickets.
 * Mass minting of game assets.
 * Optimistic decentralized lottery: a new variant which needs only shared objects to declare winner but not to buy tickets; thus only one out of the million flows needs consensus.
 * Same for voting (each vote is an NFT) - only the aggregation part at the end needs to support fraud proofs with shared objects or have this happen at the application layer.
 * Same for most auction types (each bid is an NFT) - declaring a winner can be challenged by fraud proofs; thus, it’s the only step that requires a shared object.
 * Timed-release encrypted messages, including decrypting gift cards in the future.
 * Posting price quotes (i.e., from Oracles, Pyth, etc.) can be *single-writer*, and a DEX trade can utilize shared objects. So Oracles can 100% work on the single-writer model.
 * Job listing and related applications (i.e., a decentralized Workable).
 * Real estate contract repository: for tracking purposes only - payment is offline, otherwise it would be an atomic swap.
