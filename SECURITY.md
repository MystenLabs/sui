# Intro

We appreciate your participation in keeping the Sui network secure. Please read all details on how to report an issue, what bug bounties are available, and who is eligible for bounties.

The program is run in partnership with [Immunefi](https://immunefi.com/). More information is available on [their site](https://immunefi.com/bounty/sui/).

If you have any questions or concerns, please contact support@immunefi.com.

# Reporting and response process

All reports must be sent using Immunefi’s [secure dashboard](https://bugs.immunefi.com/). **Please DO NOT report a security issue using GitHub, email, or Discord.**

You should receive an acknowledgement of your report within 48 hours for critical vulnerabilities and 96 hours for all other vulnerabilities.

If you do not get a response, please contact support@immunefi.com. Please DO NOT include attachments or provide detail regarding the security issue.

If you are reporting something outside the scope of Immunefi's Bug Bounty Program for Sui, please contact security@sui.io

# Bug bounties

The Sui Foundation offers bug bounties for security issues found at different levels of severity. If a vulnerability is found, please follow the process detailed above to report the issue. All bug reports must come with a testnet Proof of Concept with an end-effect impacting an asset-in-scope in order to be considered for a reward. You do not need to have a fix in order to submit a report or receive a bounty.

## Assets in scope

Impacts only apply to assets in active use by Sui.

### Blockchain/DLT

-   Sui Network Consensus
-   Sui Network
-   Sui Move

### Smart Contract

-   Sui Framework

## Impacts in scope

Only the following impacts are accepted within this bug bounty program. All other impacts are considered out-of-scope and ineligible for payout.

### Coin and object integrity bugs

CRITICAL [$500,000 USD]

-   Exceeding the maximum token supply of 10B SUI and allowing attacker to claim the excess funds
-   Unauthorized creation, copying, transfer or destruction of objects via bypass of or exploit of bugs in the Move or Sui bytecode verifier
-   Address collision—creating two distinct authentication schemes that hash to the same SUI address
-   Object ID collision—creating two distinct objects with the same ID, with financial consequences

MEDIUM [$10,000 USD]

-   Going under the maximum supply of SUI by sending a transaction that burns SUI that will not be re-minted into validator rewards or the storage fund at at the next epoch

LOW [$5,000 USD]

-   Sending a transaction that triggers invariant violation error code in unmodified validator software

Other vulnerabilities leading to theft or loss of valuable objects will have their severity determined depending upon the consequences of the issue and the preconditions for exploiting it.

### Bypassing authentication

CRITICAL [$500,000 USD]

-   Including an owned object as a transaction input without a valid signature from the object’s owner, in a manner that leads to significant loss of funds
-   Dynamically loading an object that is not directly or transitively owned by the transaction sender, in a manner that leads to significant loss of funds
-   Unauthorized upgrade of a Move package, in a manner that leads to significant loss of funds
-   Locking an owned object without a valid signature from the object’s owner

### Staking

CRITICAL [$500,000 USD]

-   Acquiring voting power vastly disproportionate to stake, not including:
    -   Voting power that is redistributed because one or more other validators already has max voting power
    -   Rounding errors that result in minor voting power discrepancies
-   Approve a transaction or checkpoint with less than ⅔ voting power
-   Stealing staking rewards that belong to another user, or claiming more than a user’s share of staking rewards, not including
    -   Rounding errors that result in a minor, financially insignificant discrepancy

Any other issue leading that undermines proof of stake governance or staking rewards, with severity depending on the consequences of the issue and the preconditions for exploiting it

### Operational

CRITICAL [$500,000 USD]

-   Cause an unrecoverable network halt that must be fixed via a hard fork
-   Arbitrary, non-Move remote code execution on unmodified validator software
-   Send a transaction that causes an uncaught panic in unmodified validator software

HIGH [$50,000 USD]

-   Any other issue leading to non-recoverable network downtime, with severity depending on the consequences of the issue and the preconditions for exploiting it

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
-   Content spoofing / Text injection issues
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

-   Participants must use the [Immunefi dashboard](http://bugs.immunefi.com) to report bugs or vulnerabilities. Reports made via email, Discord, or Twitter will not be eligible for bounties.
-   Bug reports that are disclosed publicly are not eligible for bounties.
-   If multiple reports are submitted for the same class of exploit, the first submission is eligible for the bounty.
-   Participants must complete KYC prior to distribution of a bounty.

# Prohibited activities

The following activities are prohibited by this bug bounty program. Participating in these activities can result in a temporary suspension or permanent ban from the bug bounty platform, which may also result in the forfeiture and loss of access to all bug submissions and zero payout.

There is no tolerance for spam/low-quality/incomplete bug reports, “beg bounty” behavior, and misrepresentation of assets and severity.

-   Any testing with mainnet or public testnet deployed code; all testing should be done on private testnets
-   Attempting phishing or other social engineering attacks against Sui or Immunefi employees and/or customers
-   Any testing with third party systems and applications (e.g. browser extensions) as well as websites (e.g. SSO providers, advertising networks)
-   Any denial of service attacks
-   Automated testing of services that generates significant amounts of traffic
-   Public disclosure of an unpatched vulnerability in an embargoed bounty
-   Any other actions prohibited by the [Immunefi Rules](https://immunefi.com/rules/). These rules are subject to change at any time.

---

# FAQ

**Where can I find more information on the bug bounty program?**

_All of the program details along with a link to the dashboard to report a bug are available on [Immunefi’s bounty program page for Sui](https://immunefi.com/bounty/sui/)._

**How do I join the program?**

_If you find a bug or vulnerability, report it using the [Immunefi dashboard](http://bugs.immunefi.com). You should receive an acknowledgement of your report within 48 hours for critical vulnerabilities and 96 hours for all other vulnerabilities._

**Where can I get technical questions answered?**

_Sui and Immunefi will be conducting Office Hours to answer questions. A date will be announced on Twitter by [@SuiNetwork](https://twitter.com/suinetwork). If you are not able to attend, you can email questions to support@immunefi.com._

**Who is behind this program?**

_The program is funded and managed by the Sui Foundation, in partnership with Immunefi._
