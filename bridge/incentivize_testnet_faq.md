---
title: Sui Bridge Testnet Incentive Program FAQ
---

# Sui Bridge Testnet Incentive Program FAQ

Q: How long will the Incentive Program last?
A: It will run for at least two weeks. The exact end date is TBD but will be announced when it is decided.

Q: What’s the total amount of rewards for this program?
A: 100k Sui.

Q: How will the rewards be distributed?
A: After the program ends, the rewards will be sent to eligible participants’ Sui addresses on Mainnet that they used to test Sui Bridge on testnet.

Q: Am I eligible for rewards if I test the bridge by directly calling the contract/package?
A: No, only actions happened on https://bridge.testnet.sui.io/ are eligible for rewards.

Q: Can I use a bot to bulk test https://bridge.testnet.sui.io/? 
A: We expect real valuable signals and feedback to come from human testers. Using a bot for testing is discouraged and will negatively impact your final rewards.

Q: How do I get test tokens?
A: Refer to the “How-to Guide”.

Q: What is a roundtrip bridge? Do I have to do a round trip bridge to be eligible for rewards? What if I do three Ethereum to Sui transfers and one Sui to Ethereum transfer? 
A: A roundtrip consists of bridging assets from Sepolia to Sui Testnet, then bridging from Sui Testnet back to Sepolia. Only a round trip is eligible for rewards. If an address does three Ethereum to Sui transfers and one Sui to Ethereum transfer, it will be counted as one eligible test activity.

Q: If I use different Ethereum addresses, does it still count for a roundtrip?
A: Yes. We track eligible test activities by Sui address.

Q: Does a roundtrip require the same token and amount?
A: No it doesn’t. Namely transferring 1 Native Sepolia Eth from Etheruem to Sui and 500 USDC from Sui to Ethereum is considered an eligible test acitivity.

Q: I see my transfer is “delayed”. What does it mean?
A: Checker “What is the limiter?” on the FAQ section of https://bridge.testnet.sui.io/. On Mainnet we expect the limiter to be hit rarely. However during the incentivize program we may intentionally trigger this scenario more often to thoroughly test it.

Q: Is there a point system or dashboard for this program?
A: No, eligible test activities are not calculated in real time.

Q: Does the bridge amount matter?
A: No. However, testing different amounts may help surface edge cases. Reporting these bugs will be appreciated and make you eligible for potentially more rewards.

Q: Does the token type matter?
A: No. However, testing different tokens may help surface edge cases. Reporting these bugs will be appreciated and make you eligible for potentially more rewards.

Q: I can successfully bridge but the UI seems to be a bit slow to reflect the latest state. Should I report the issue?
A: No for UI slowness unless it’s extreme. Sepolia network is relatively unstable so it’s common for the front end to take longer to get the latest status from Fullnode. This will be a non-issue on Mainnet.

Q: How do I know how much reward I will get for my reported issues?
A: Due to the operation difficulties involving triaging and investigation, we are not able to immediately share how much reward you will get from the reported issues. In general, the higher the quality and the earlier the submission of the report, the more helpful it is to us, which means the potential for greater rewards.

Q: How do I report issues?
A: Prepare a video clip or screenshots to clearly reproduce the issue. Go to the “sui-bridge-forum” channel https://discord.com/channels/916379725201563759/1249826301972316190 and make a post.
