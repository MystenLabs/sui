---
title: Use Sui Single-Writer-Friendly (SWF) Apps
---

This page lists applications that can work in the single-writer model defined as [simple transactions](how-sui-works.md#simple-transactions) in Sui. 
Apart from the obvious single-writer friendly applications (such as simple peer to peer asset transfer), note that some 
proposals that typically require shared objects have been transformed to variants that require only a shared object as a
final step and not for every action, such as voting and lotteries and DeFi Oracle price quotes submission.

1. Regular peer-to-peer (p2p) transactions ([see how to create a new Coin with just 7 lines of Sui Move code](https://www.linkedin.com/posts/chalkiaskostas_startup-smartcontract-cryptocurrency-activity-6946006856528003072-CvI0)).
1. Confidential p2p Txs: same as FastPay but with pedersen commitments to hide amounts transferred; this still ensures input amount = output amount - we can set amount limits, i.e., N transfers up to $1,000 can be confidential.
1. Public bulletin board; users store only publicly accessed data, files, links, metadata.
1. Proof of existence: similar to the above, but for time-stamped documents; it can be extended to support commitment proof of existence, i.e. publish your hash, then reveal.
1. Private decentralized repository (users store private files, encrypted under their public keys; users' public keys can be represented as NFTs.
1. Extend the above for selected disclosure CV (resume) repository, University degrees repository.
1. Decentralized or conventional Certificate Authority. Authorities publish their signatures over certs, they can revoke any time (easier revocation).
1. Messaging service: apps, Oracles and Internet of Things (IoTs) exchanging messages. Sui is probably the best platform for any messaging protocol, as typically each response and message can be encoded with a single-writer NFT.
1. Extend the above to social networks; note that each post is a single-writer NFT. See a [smart contract implementation of a fully functional decentralized Twitter with just 50 lines of Sui Move code](https://github.com/MystenLabs/sui/blob/main/sui_programmability/examples/nfts/sources/chat.move).
1. Extend the above to private messaging (i.e., decentralized WhatsApp or Signal).
1. Extend the above for any website / blog / rating platform (i.e., Yelp and Tripadvisor).
1. Personal GitHub, Overleaf LaTex editor, wish/shopping lists, etc.
1. Personal password manager.
1. Non-interactive games (i.e., advertise/evolve your SimCity, FarmVille state etc.).
1. Coupons and tickets. See a [full dApp demo and installation instructions on how to build a mass-minting coupon platform with Sui](https://github.com/MystenLabs/sui/blob/sui-coupon-v0/examples/coupons/README.md).
1. Mass minting of game assets.
1. Optimistic decentralized lottery: a new variant which needs only shared objects to declare winner but not to buy tickets; thus only one out of the million flows needs consensus.
1. Same for voting (each vote is an NFT) - only the aggregation part at the end needs to support fraud proofs with shared objects or have this happen at the application layer.
1. Same for most auction types (each bid is an NFT) - declaring a winner can be challenged by fraud proofs; thus, it’s the only step that requires a shared object.
1. Timed-release encrypted messages, including decrypting gift cards in the future.
1. Posting price quotes (i.e., from Oracles, Pyth, etc.) can be *single-writer*, and a DEX trade can utilize shared objects. So Oracles can 100% work on the single-writer model.
1. Job listing and related applications (i.e., a decentralized Workable).
1. Real estate contract repository: for tracking purposes only - payment is offline, otherwise it would be an atomic swap.
