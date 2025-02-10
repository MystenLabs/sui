# Introduction

We appreciate your participation in keeping the Sui network secure. This doc includes details on Sui's Bug Bounty Program, including information on how to report an issue, what bug bounties are available, and who is eligible for bounties.

The program is run in partnership with [HackenProof](https://hackenproof.com/). More information is available on [their site](https://hackenproof.com/programs/sui-protocol).

If you have any questions or concerns, contact [support@hackenproof.com](mailto:info@hackenproof.com).

# Reporting and response process

All reports must be sent using HackenProof’s [secure dashboard](https://hackenproof.com/programs/sui-protocol). **DO NOT report a security issue using GitHub, email, or Discord.**

You should receive an acknowledgement of your report within 48 hours for critical vulnerabilities and 96 hours for all other vulnerabilities.

If you do not get a response, contact info@hackenproof.com. DO NOT include attachments or provide detail regarding the security issue.

If you are reporting something outside the scope of HackenProof's Bug Bounty Program for Sui, contact [security@sui.io](mailto:security@sui.io)

# Bug bounties

The Sui Foundation offers bug bounties for security issues found at different levels of severity. If a vulnerability is found, follow the process detailed above to report the issue. All bug reports must come with a runnable [testnet](https://github.com/MystenLabs/sui/tree/testnet) Proof-of-Concept with an end-effect impacting an asset-in-scope in order to be considered for a reward. You do not need to have a fix in order to submit a report or receive a bounty. By your participation in Sui Foundation’s bug bounty program you agree to be bound and abide by our [terms of service](https://sui.io/terms) and [privacy policy](https://sui.io/policy) .

## Assets in scope

Impacts only apply to assets in active use by Sui.

### Blockchain/DLT

-   Sui Network Consensus
-   Sui Network
-   Sui Move

### Smart Contract

-   Sui Framework

## Impacts in Scope


The following impacts are accepted within this bug bounty program--refer to [Sui's HackenProof Bug Bounty Program Page](https://hackenproof.com/sui/sui/) for an official and up-to-date listing.
All other impacts are considered out-of-scope and ineligible for payout.



CRITICAL [$100,000-$500,000 USD]
1. Exceeding the maximum supply of 10 billion SUI + allowing the attacker to claim the excess funds
2. Loss of Funds which includes
    * Unauthorized creation, copying, transfer or destruction of objects via bypass of or exploit of bugs in the Move or Sui bytecode verifier
    * Address Collision – creating two distinct authentication schemes that hash to the same SUI address in a manner that lead to significant loss of funds
    * Object ID collision—creating two distinct objects with the same ID in a manner that leads to significant loss of funds.
    * Unauthorized use of an owned object as a transaction input, resulting in significant loss of funds due to the inability to verify ownership and permission to transfer
    * Dynamically loading an object that is not directly or transitively owned by the transaction sender, in a manner that leads to significant loss of funds
    * Unauthorized upgrade of a Move package, in a manner that leads to significant loss of funds
    * Stealing staking rewards that belong to another user, or claiming more than a user’s share of staking rewards, not including rounding errors that result in a minor, financially insignificant discrepancy
3. Violating BFT assumptions, acquiring voting power vastly disproportionate to stake, or any other issue that can meaningfully compromise the integrity of the blockchain’s proof of stake governance does not include the following: 
    * Voting power that is redistributed because one or more other validators already has max voting power
    * Rounding errors that result in minor voting power discrepancies
4. Unintended permanent chain split requiring hard fork (network partition requiring hard fork)
5. Network not being able to confirm new transactions (total network shutdown) requiring a hard fork to resolve
6. Arbitrary, non-Move remote code execution on unmodified validator software


HIGH [$50,000 USD]

1. Temporary Total Network Shutdown (greater than 10 minutes of network downtime)

MEDIUM [$10,000 USD]

1. A bug that results in unintended and harmful smart contract behavior with no concrete funds at direct risk
2. Unintended, permanent burning of SUI under the max cap.
3. Shutdown of greater than or equal to 30% of network processing nodes without brute force actions, but does not shut down the network


LOW [$5,000 USD]

1. Sending a transaction that triggers invariant violation error code in unmodified validator software
2. A remote call that crashes a Sui fullnode

# Audit Discoveries and Known Issues
Bug reports covering previously-discovered bugs are not eligible for any reward through the bug bounty program. If a bug report covers a known issue, it may be rejected together with proof of the issue being known before escalation of the bug report via HackenProof. 

**Previous audits and known issues can be found at:**
| Known Issue  | Related Impact-in-Scope |
| ------------- | ------------- |
| In our staking contract, we have the concept of pool tokens and keep track of exchange rates between pool tokens and SUI tokens of all epochs, which increase as more rewards are added to the staking pools. When a user withdraws their stake, we retrieve from that record both the exchange rate at staking time and the current exchange rate (at withdrawing time), and calculate the rewards to be paid out based on the difference in exchange rates. While doing this calculation, we do conversions both ways: pool tokens -> SUI and SUI -> pool tokens. Rounding may happen along the way due to integer division. The exchange rate between the two tokens should stay roughly the same since we will be burning a proportional amount of pool tokens as SUI is withdrawn. However, in the extreme case where a user is unstaking 1 MIST, this rounding error may cause ZERO pool tokens to be burnt, causing the pool token to effectively depreciate. If an attacker has a lot of 1 MIST stakes, they can withdraw them one by one, causing the pool token exchange rate to drop and other takers to “lose” their staking rewards. I put quotation marks around “lose” because the attacker themselves won’t get any of that rewards so this attacker doesn’t actually make economic sense. Rather the rewards stay in the rewards pool and will become dust. This issue is mitigated by enforcing a minimum staking amount of 1 SUI or 10^9 MIST in this PR: https://github.com/MystenLabs/sui/pull/9961 | Critical - Any other issue leading to theft or loss of valuable objects, with severity depending on the consequences of the issue and the preconditions for exploiting it  |
| Excessive storage rebate on 0x5 object right after epoch change:
Each on-chain object is associated with a storage rebate, which would be refunded to the owner if it ever gets deleted. Epoch change transactions are special in that they are system transactions without a sender, hence any storage rebate generated in that transaction is kept in the 0x5 object. This means that the first person touching the 0x5 object in each epoch may be able to obtain these storage rebates by simply touching this object (e.g. a failed staking request). We will look into a way to evenly distribute any these rebates such that it does not lead to any undesired behaviors.  | |
| Crash Validator by providing a gas price of u64:max | Network not being able to confirm new transactions (total network shutdown) |


# Payment of bounties

-   Bounties are awarded on a rolling basis and eligibility to receive such awards shall be determined by Sui Foundation in its sole discretion.
-   Once a bug report is accepted as legitimate, assuming KYC has been completed successfully, bounties will be paid out on or about a time that is 14 days thereafter or less.
-   Bounties are denominated in USD but will be paid in USDC; provided if a bounty award winner is not eligible to receive USDC in compliance with applicable law, the bounty award shall be paid out in USD.
-   Multiple vulnerabilities of a similar root cause will be paid out as one report, as a single bounty award, determined by the severity of the vulnerability as described above.

# Out of scope

The following vulnerabilities are excluded from the bug bounty program:

-   Attacks that the reporter has already exploited themselves, leading to damage
-   Attacks requiring access to leaked keys/credentials
-   Attacks requiring access to privileged addresses (governance, strategist), except in such cases where the contracts are intended to have no privileged access to functions that make the attack possible
-   Broken link hijacking

### Smart Contracts and Blockchain/DLT

-   Basic economic governance attacks (e.g. 51% attack)
-   Lack of liquidity
-   Best practice critiques
-   Sybil attacks
-   Centralization risks

### Websites and Apps

-   Theoretical impacts without any proof or demonstration
-   Content spoofing / text injection issues
-   Self-XSS
-   Captcha bypass using OCR
-   CSRF with no security impact (logout CSRF, change language, etc.)
-   Missing HTTP Security Headers (such as X-FRAME-OPTIONS) or cookie security flags (such as “httponly”)
-   Server-side information disclosure such as IPs, server names, and most stack traces
-   Vulnerabilities used to enumerate or confirm the existence of users or tenants
-   Vulnerabilities requiring unlikely user actions
-   URL Redirects (unless combined with another vulnerability to produce a more severe vulnerability)
-   Lack of SSL/TLS best practices
-   Attacks involving DDoS
-   Attacks requiring privileged access from within the organization
-   SPF records for email domains
-   Feature requests
-   Best practices

# Eligibility

-   Participants must use the [HackenProof dashboard](https://hackenproof.com/sui/sui) to report bugs or vulnerabilities. Reports made via email, Discord, or Twitter will not be eligible for bounties.
-   Bug reports that are disclosed publicly are not eligible for bounties.
-   If multiple reports are submitted for the same class of exploit, the first submission is eligible for the bounty.
-   Participants must complete KYC prior to distribution of a bounty.

# Prohibited activities

The following activities are prohibited by this bug bounty program. Participating in these activities can result in a temporary suspension or permanent ban from the bug bounty platform, which may also result in the forfeiture and loss of access to all bug submissions and zero payout.

There is no tolerance for spam/low-quality/incomplete bug reports, “beg bounty” behavior, and misrepresentation of assets and severity.

-   Any testing with mainnet or public testnet deployed code; all testing should be done on private testnets
-   Attempting phishing or other social engineering attacks against Sui or HackenProof employees and/or customers
-   Any testing with third party systems and applications (e.g. browser extensions) as well as websites (e.g. SSO providers, advertising networks)
-   Any denial of service attacks
-   Automated testing of services that generates significant amounts of traffic
-   Public disclosure of an unpatched vulnerability in an embargoed bounty
-   Any other actions prohibited by the HackenProof Rules. These rules are subject to change at any time.

---

# FAQ

**Where can I find more information on the bug bounty program?**

_All of the program details along with a link to the dashboard to report a bug are available on [HackenProof’s bounty program page for Sui](https://hackenproof.com/sui/sui/)._

**How do I join the program?**

_If you find a bug or vulnerability, report it using the [HackenProof dashboard](https://hackenproof.com/sui/sui). You should receive an acknowledgement of your report within 48 hours for critical vulnerabilities and 96 hours for all other vulnerabilities._

**Where can I get technical questions answered?**

_Sui and HackenProof will be conducting Office Hours to answer questions. A date will be announced on Twitter by [@SuiNetwork](https://x.com/suinetwork). If you are not able to attend, you can email questions to info@hackenproof.com._ with questions regarding the Bug Bounty Program.

For additional security concerns/questions/comments outside the scope of the HackenProof Bug Bounty Program, reach out to Sui's Community [Discord](https://discord.gg/sui), [Forums](https://forums.sui.io/), or e-mail us at _security@sui.io_

**Who is behind this program?**

_The program is funded and managed by the Sui Foundation, in partnership with HackenProof._
