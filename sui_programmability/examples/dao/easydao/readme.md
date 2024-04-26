## Module Overview

This repository implements a simple `dao` module aimed at creating a decentralized autonomous organization (DAO) for managing shared funds, community tasks, proposals, and voting functionalities.

### `dao` token
**`dao` Token**
The DAO utilizes a community token called `dao` for governing the community and incentivizing its members. Community members need to use `dao` tokens to create proposals and participate in voting to engage in community governance. The total supply of `dao` tokens was fixed upon the establishment of the DAO, and initially, all `dao` tokens will be locked in the treasury.
**How to Obtain `dao` Tokens?**
The DAO organization will release community tasks with certain `dao` token rewards. Upon completion, participants can claim `dao` tokens with corresponding credentials.
If proposals submitted by community members are accepted, `dao` token rewards will be distributed based on the proposal's level.

### Member Roles
**There are three types of member roles in the community:**

#### 1. InitCoreMember
Initial core members of the DAO organization.
Authorized roles: InitCoreMember, CoreMember.

#### 2. CoreMember
Core members of the DAO organization:
Authorized roles: Member.
Rights:
1. Publishing community tasks
2. Distributing task rewards
3. Authorizing regular community members
4. Closing proposals
5. Modifying proposal levels

#### 3. Member
Regular members of the DAO organization (need to apply for authorization from CoreMember by holding dao).
Rights:
1. Submitting proposals
2. Claiming proposal rewards
3. Participating in voting on proposals

### How to Run
1. Initial members of the DAO organization publish community tasks.
2. By participating in community tasks and holding dao tokens, one can apply to join the DAO organization.
3. After joining the DAO organization, members can submit community proposals. If accepted, they can earn certain token rewards. They can also participate in proposal voting.
