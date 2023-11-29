# Sui GraphQL Examples
### [Address](#0)
#### &emsp;&emsp;[Address](#0)
#### &emsp;&emsp;[Transaction Block Connection](#1)
### [Balance Connection](#1)
#### &emsp;&emsp;[Balance Connection](#65535)
### [Chain Id](#2)
#### &emsp;&emsp;[Chain Id](#131070)
### [Checkpoint](#3)
#### &emsp;&emsp;[At Digest](#196605)
#### &emsp;&emsp;[At Seq Num](#196606)
#### &emsp;&emsp;[First Two Tx Blocks For Checkpoint](#196607)
#### &emsp;&emsp;[Latest Checkpoint](#196608)
#### &emsp;&emsp;[Multiple Selections](#196609)
#### &emsp;&emsp;[With Timestamp Tx Block Live Objects](#196610)
#### &emsp;&emsp;[With Tx Sent Addr Filter](#196611)
### [Checkpoint Connection](#4)
#### &emsp;&emsp;[Ascending Fetch](#262140)
#### &emsp;&emsp;[First Ten After Checkpoint](#262141)
#### &emsp;&emsp;[Last Ten After Checkpoint](#262142)
### [Coin Connection](#5)
#### &emsp;&emsp;[Coin Connection](#327675)
### [Coin Metadata](#6)
#### &emsp;&emsp;[Coin Metadata](#393210)
### [Epoch](#7)
#### &emsp;&emsp;[Latest Epoch](#458745)
#### &emsp;&emsp;[Specific Epoch](#458746)
#### &emsp;&emsp;[With Checkpoint Connection](#458747)
#### &emsp;&emsp;[With Tx Block Connection](#458748)
#### &emsp;&emsp;[With Tx Block Connection Latest Epoch](#458749)
### [Event Connection](#8)
#### &emsp;&emsp;[Event Connection](#524280)
### [Name Service](#9)
#### &emsp;&emsp;[Name Service](#589815)
### [Object](#10)
#### &emsp;&emsp;[Object](#655350)
### [Object Connection](#11)
#### &emsp;&emsp;[Filter Object Ids](#720885)
#### &emsp;&emsp;[Filter Owner](#720886)
#### &emsp;&emsp;[Object Connection](#720887)
### [Owner](#12)
#### &emsp;&emsp;[Dynamic Field](#786420)
#### &emsp;&emsp;[Dynamic Field Connection](#786421)
#### &emsp;&emsp;[Dynamic Object Field](#786422)
#### &emsp;&emsp;[Owner](#786423)
### [Protocol Configs](#13)
#### &emsp;&emsp;[Key Value](#851955)
#### &emsp;&emsp;[Key Value Feature Flag](#851956)
#### &emsp;&emsp;[Specific Config](#851957)
#### &emsp;&emsp;[Specific Feature Flag](#851958)
### [Service Config](#14)
#### &emsp;&emsp;[Service Config](#917490)
### [Stake Connection](#15)
#### &emsp;&emsp;[Stake Connection](#983025)
### [Sui System State Summary](#16)
#### &emsp;&emsp;[Sui System State Summary](#1048560)
### [Transaction Block](#17)
#### &emsp;&emsp;[Transaction Block](#1114095)
#### &emsp;&emsp;[Transaction Block Kind](#1114096)
### [Transaction Block Connection](#18)
#### &emsp;&emsp;[Before After Checkpoint](#1179630)
#### &emsp;&emsp;[Changed Object Filter](#1179631)
#### &emsp;&emsp;[Input Object Filter](#1179632)
#### &emsp;&emsp;[Input Object Sent Addr Filter](#1179633)
#### &emsp;&emsp;[Package Filter](#1179634)
#### &emsp;&emsp;[Package Module Filter](#1179635)
#### &emsp;&emsp;[Package Module Func Filter](#1179636)
#### &emsp;&emsp;[Recv Addr Filter](#1179637)
#### &emsp;&emsp;[Sent Addr Filter](#1179638)
#### &emsp;&emsp;[Tx Ids Filter](#1179639)
#### &emsp;&emsp;[Tx Kind Filter](#1179640)
#### &emsp;&emsp;[With Defaults Ascending](#1179641)
### [Transaction Block Effects](#19)
#### &emsp;&emsp;[Transaction Block Effects](#1245165)
## <a id=0></a>
## Address
### <a id=0></a>
### Address
####  Get the address' balance and its coins' id and type

><pre>{
>  address(
>    address: "0x5094652429957619e6efa79a404a6714d1126e63f551f4b6c7fb76440f8118c9"
>  ) {
>    location
>    balance {
>      coinType {
>        repr
>      }
>      coinObjectCount
>      totalBalance
>    }
>    coinConnection {
>      nodes {
>        asMoveObject {
>          contents {
>            type {
>              repr
>            }
>          }
>        }
>
>      }
>    }
>  }
>}</pre>

### <a id=1></a>
### Transaction Block Connection
####  See examples in Query::transactionBlockConnection as this is
####  similar behavior to the `transactionBlockConnection` in Query but
####  supports additional `AddressTransactionBlockRelationship` filter
####  Filtering on package where the sender of the TX is the current address
####  and displaying the transaction's sender and the gas price and budget

><pre># See examples in Query::transactionBlockConnection as this is
># similar behavior to the `transactionBlockConnection` in Query but
># supports additional `AddressTransactionBlockRelationship` filter
>
># Filtering on package where the sender of the TX is the current address
># and displaying the transaction's sender and the gas price and budget
>query transaction_block_with_relation_filter {
>  address(address: "0x2") {
>    transactionBlockConnection(relation: SENT, filter: { package: "0x2" }) {
>      nodes {
>        sender {
>          location
>        }
>        gasInput {
>          gasPrice
>          gasBudget
>        }
>      }
>    }
>  }
>}</pre>

## <a id=1></a>
## Balance Connection
### <a id=65535></a>
### Balance Connection
####  Query the balance for objects of type COIN and then for each coin
####  get the coin type, the number of objects, and the total balance

><pre>{
>  address(
>    address: "0x5094652429957619e6efa79a404a6714d1126e63f551f4b6c7fb76440f8118c9"
>  ) {
>    balance(
>      type: "0xc060006111016b8a020ad5b33834984a437aaa7d3c74c18e09a95d48aceab08c::coin::COIN"
>    ) {
>      coinObjectCount
>      totalBalance
>    }
>    balanceConnection {
>      nodes {
>        coinType {
>          repr
>        }
>        coinObjectCount
>        totalBalance
>      }
>      pageInfo {
>        endCursor
>      }
>    }
>  }
>}</pre>

## <a id=2></a>
## Chain Id
### <a id=131070></a>
### Chain Id
####  Returns the chain identifier for the chain that the server is tracking

><pre>{
>  chainIdentifier
>}</pre>

## <a id=3></a>
## Checkpoint
### <a id=196605></a>
### At Digest
####  Get the checkpoint's information at a particular digest

><pre>{
>  checkpoint(id: { digest: "GaDeWEfbSQCQ8FBQHUHVdm4KjrnbgMqEZPuhStoq5njU" }) {
>    digest
>    sequenceNumber
>    validatorSignature
>    previousCheckpointDigest
>    networkTotalTransactions
>    rollingGasSummary {
>      computationCost
>      storageCost
>      storageRebate
>      nonRefundableStorageFee
>    }
>    epoch {
>      epochId
>      referenceGasPrice
>      startTimestamp
>      endTimestamp
>    }
>    endOfEpoch {
>      nextProtocolVersion
>    }
>  }
>}</pre>

### <a id=196606></a>
### At Seq Num
####  Get the checkpoint's information at a particular sequence number

><pre>{
>  checkpoint(id: { sequenceNumber: 10 }) {
>    digest
>    sequenceNumber
>    validatorSignature
>    previousCheckpointDigest
>    networkTotalTransactions
>    rollingGasSummary {
>      computationCost
>      storageCost
>      storageRebate
>      nonRefundableStorageFee
>    }
>    epoch {
>      epochId
>      referenceGasPrice
>      startTimestamp
>      endTimestamp
>    }
>    endOfEpoch {
>      nextProtocolVersion
>    }
>  }
>}</pre>

### <a id=196607></a>
### First Two Tx Blocks For Checkpoint
####  Get data for the first two transaction blocks of checkpoint at sequence number 10

><pre>{
>  checkpoint(id: { sequenceNumber: 10 }) {
>    transactionBlockConnection(first: 2) {
>      edges {
>        node {
>          kind {
>            __typename
>          }
>          digest
>          sender {
>            location
>          }
>          expiration {
>            epochId
>          }
>        }
>      }
>      pageInfo {
>        startCursor
>        hasNextPage
>        hasPreviousPage
>        endCursor
>      }
>    }
>  }
>}</pre>

### <a id=196608></a>
### Latest Checkpoint
####  Latest checkpoint's data

><pre>{
>  checkpoint {
>    digest
>    sequenceNumber
>    validatorSignature
>    previousCheckpointDigest
>    networkTotalTransactions
>    rollingGasSummary {
>      computationCost
>      storageCost
>      storageRebate
>      nonRefundableStorageFee
>    }
>    epoch {
>      epochId
>      referenceGasPrice
>      startTimestamp
>      endTimestamp
>    }
>    endOfEpoch {
>      nextProtocolVersion
>    }
>  }
>}</pre>

### <a id=196609></a>
### Multiple Selections
####  Get the checkpoint at sequence 9769 and show
####  the new committe authority and stake units

><pre>{
>  checkpoint(id: { sequenceNumber: 9769 }) {
>    digest
>    sequenceNumber
>    timestamp
>    validatorSignature
>    previousCheckpointDigest
>    liveObjectSetDigest
>    networkTotalTransactions
>    rollingGasSummary {
>      computationCost
>      storageCost
>      storageRebate
>      nonRefundableStorageFee
>    }
>    epoch {
>      epochId
>    }
>    endOfEpoch {
>      newCommittee {
>        authorityName
>        stakeUnit
>      }
>      nextProtocolVersion
>    }
>    transactionBlockConnection {
>      edges {
>        node {
>          digest
>          sender {
>            location
>          }
>          expiration {
>            epochId
>          }
>        }
>      }
>    }
>  }
>}</pre>

### <a id=196610></a>
### With Timestamp Tx Block Live Objects
####  Latest checkpoint's timestamp, liveObjectSetDigest, and transaction block data

><pre>{
>  checkpoint {
>    digest
>    sequenceNumber
>    timestamp
>    liveObjectSetDigest
>    transactionBlockConnection {
>      edges {
>        node {
>          digest
>          sender {
>            location
>          }
>          expiration {
>            epochId
>          }
>        }
>      }
>    }
>  }
>}</pre>

### <a id=196611></a>
### With Tx Sent Addr Filter
####  Select checkpoint at sequence number 14830285 for transactions from sentAddress

><pre>{
>  checkpoint(id: { sequenceNumber: 14830285 }) {
>    digest
>    sequenceNumber
>    timestamp
>    liveObjectSetDigest
>    transactionBlockConnection(
>      filter: {
>        sentAddress: "0x0000000000000000000000000000000000000000000000000000000000000000"
>      }
>    ) {
>      edges {
>        node {
>          digest
>          sender {
>            location
>          }
>          expiration {
>            epochId
>          }
>        }
>      }
>    }
>  }
>}</pre>

## <a id=4></a>
## Checkpoint Connection
### <a id=262140></a>
### Ascending Fetch
####  Use the checkpoint connection to fetch some default amount of checkpoints in an ascending order

><pre>{
>  checkpointConnection {
>    nodes {
>      digest
>      sequenceNumber
>      validatorSignature
>      previousCheckpointDigest
>      networkTotalTransactions
>      rollingGasSummary {
>        computationCost
>        storageCost
>        storageRebate
>        nonRefundableStorageFee
>      }
>      epoch {
>        epochId
>        referenceGasPrice
>        startTimestamp
>        endTimestamp
>      }
>      endOfEpoch {
>        nextProtocolVersion
>      }
>    }
>  }
>}</pre>

### <a id=262141></a>
### First Ten After Checkpoint
####  Fetch the digest and sequence number of the first 10 checkpoints after the cursor, which in this example is set to be checkpoint 11. Note that cursor will be opaque

><pre>{
>  checkpointConnection(first: 10, after: "11") {
>    nodes {
>      sequenceNumber
>      digest
>    }
>  }
>}</pre>

### <a id=262142></a>
### Last Ten After Checkpoint
####  Fetch the digest and the sequence number of the last 20 checkpoints before the cursor

><pre>{
>  checkpointConnection(last: 20, before: "100") {
>    nodes {
>      sequenceNumber
>      digest
>    }
>  }
>}</pre>

## <a id=5></a>
## Coin Connection
### <a id=327675></a>
### Coin Connection
####  Get last 3 coins before coins at cursor 13034947

><pre>{
>  address(
>    address: "0x0000000000000000000000000000000000000000000000000000000000000000"
>  ) {
>    coinConnection(last: 3, before: "0x13034947") {
>      nodes {
>        balance
>      }
>      pageInfo {
>        endCursor
>        hasNextPage
>      }
>    }
>  }
>}</pre>

## <a id=6></a>
## Coin Metadata
### <a id=393210></a>
### Coin Metadata

><pre>query CoinMetadata {
>  coinMetadata(coinType: "0x2::sui::SUI") {
>    decimals
>    name
>    symbol
>    description
>    iconUrl
>    supply
>    asMoveObject {
>      hasPublicTransfer
>    }
>  }
>}</pre>

## <a id=7></a>
## Epoch
### <a id=458745></a>
### Latest Epoch
####  Latest epoch, since epoch omitted

><pre>{
>  epoch {
>    protocolConfigs {
>      protocolVersion
>    }
>    epochId
>    referenceGasPrice
>    startTimestamp
>    endTimestamp
>    validatorSet {
>      totalStake
>      pendingActiveValidatorsSize
>      stakePoolMappingsSize
>      inactivePoolsSize
>      validatorCandidatesSize
>      activeValidators {
>        name
>        description
>        imageUrl
>        projectUrl
>        exchangeRates {
>          asObject {
>            storageRebate
>            bcs
>            kind
>          }
>          hasPublicTransfer
>        }
>        exchangeRatesSize
>        stakingPoolActivationEpoch
>        stakingPoolSuiBalance
>        rewardsPool
>        poolTokenBalance
>        pendingStake
>        pendingTotalSuiWithdraw
>        pendingPoolTokenWithdraw
>        votingPower
>        gasPrice
>        commissionRate
>        nextEpochStake
>        nextEpochGasPrice
>        nextEpochCommissionRate
>        atRisk
>      }
>    }
>  }
>}</pre>

### <a id=458746></a>
### Specific Epoch
####  Selecting all fields for epoch 100

><pre>{
>  epoch(id: 100) {
>    protocolConfigs {
>      protocolVersion
>    }
>    epochId
>    referenceGasPrice
>    startTimestamp
>    endTimestamp
>    validatorSet {
>      totalStake
>      pendingActiveValidatorsSize
>      stakePoolMappingsSize
>      inactivePoolsSize
>      validatorCandidatesSize
>      activeValidators {
>        name
>        description
>        imageUrl
>        projectUrl
>        exchangeRates {
>          asObject {
>            storageRebate
>            bcs
>            kind
>          }
>          hasPublicTransfer
>        }
>        exchangeRatesSize
>        stakingPoolActivationEpoch
>        stakingPoolSuiBalance
>        rewardsPool
>        poolTokenBalance
>        pendingStake
>        pendingTotalSuiWithdraw
>        pendingPoolTokenWithdraw
>        votingPower
>        gasPrice
>        commissionRate
>        nextEpochStake
>        nextEpochGasPrice
>        nextEpochCommissionRate
>        atRisk
>      }
>    }
>  }
>}</pre>

### <a id=458747></a>
### With Checkpoint Connection

><pre>{
>  epoch {
>    checkpointConnection {
>      nodes {
>        transactionBlockConnection(first: 10) {
>          pageInfo {
>            hasNextPage
>            endCursor
>          }
>          edges {
>            cursor
>            node {
>              sender {
>                location
>              }
>              effects {
>                gasEffects {
>                  gasObject {
>                    location
>                  }
>                }
>              }
>              gasInput {
>                gasPrice
>                gasBudget
>              }
>            }
>          }
>        }
>      }
>    }
>  }
>}</pre>

### <a id=458748></a>
### With Tx Block Connection
####  Fetch the first 20 transactions after 231220100 for epoch 97

><pre>{
>  epoch(id:97) {
>    transactionBlockConnection(first: 20, after:"231220100") {
>      pageInfo {
>        hasNextPage
>        endCursor
>      }
>      edges {
>        cursor
>        node {
>          digest
>          sender {
>            location
>          }
>          effects {
>            gasEffects {
>              gasObject {
>                location
>              }
>            }
>          }
>          gasInput {
>            gasPrice
>            gasBudget
>          }
>        }
>      }
>    }
>  }
>}</pre>

### <a id=458749></a>
### With Tx Block Connection Latest Epoch
####  the last checkpoint of epoch 97 is 8097645
####  last tx number of the checkpoint is 261225985

><pre>{
>  epoch {
>    transactionBlockConnection(first: 20, after: "261225985") {
>      pageInfo {
>        hasNextPage
>        endCursor
>      }
>      edges {
>        cursor
>        node {
>          sender {
>            location
>          }
>          effects {
>            gasEffects {
>              gasObject {
>                location
>              }
>            }
>          }
>          gasInput {
>            gasPrice
>            gasBudget
>          }
>        }
>      }
>    }
>  }
>}</pre>

## <a id=8></a>
## Event Connection
### <a id=524280></a>
### Event Connection

><pre>{
>  eventConnection(
>    filter: {
>      eventType: "0x3164fcf73eb6b41ff3d2129346141bd68469964c2d95a5b1533e8d16e6ea6e13::Market::ChangePriceEvent<0x2::sui::SUI>"
>    }
>  ) {
>    nodes {
>      sendingModule {
>        name
>        package {
>          asObject {
>            digest
>          }
>        }
>      }
>      eventType {
>        repr
>      }
>      senders {
>        location
>      }
>      timestamp
>      json
>      bcs
>    }
>  }
>}</pre>

## <a id=9></a>
## Name Service
### <a id=589815></a>
### Name Service

><pre>{
>  resolveNameServiceAddress(name: "example.sui") {
>    location
>  }
>  address(
>    address: "0x0b86be5d779fac217b41d484b8040ad5145dc9ba0cba099d083c6cbda50d983e"
>  ) {
>    location
>    balance(type: "0x2::sui::SUI") {
>      coinType {
>        repr
>      }
>      coinObjectCount
>      totalBalance
>    }
>    defaultNameServiceName
>  }
>}</pre>

## <a id=10></a>
## Object
### <a id=655350></a>
### Object

><pre>{
>  object(
>    address: "0x04e20ddf36af412a4096f9014f4a565af9e812db9a05cc40254846cf6ed0ad91"
>  ) {
>    location
>    version
>    digest
>    storageRebate
>    owner {
>      defaultNameServiceName
>    }
>    previousTransactionBlock {
>      digest
>    }
>    kind
>  }
>}</pre>

## <a id=11></a>
## Object Connection
### <a id=720885></a>
### Filter Object Ids
####  Filter on objectIds

><pre>{
>  objectConnection(
>    filter: {
>      objectIds: [
>        "0x4bba2c7b9574129c272bca8f58594eba933af8001257aa6e0821ad716030f149"
>      ]
>    }
>  ) {
>    edges {
>      node {
>        storageRebate
>        kind
>      }
>    }
>  }
>}</pre>

### <a id=720886></a>
### Filter Owner
####  Filter on owner

><pre>{
>  objectConnection(
>    filter: {
>      owner: "0x23b7b0e2badb01581ba9b3ab55587d8d9fdae087e0cfc79f2c72af36f5059439"
>    }
>  ) {
>    edges {
>      node {
>        storageRebate
>        kind
>      }
>    }
>  }
>}</pre>

### <a id=720887></a>
### Object Connection

><pre>{
>  objectConnection {
>    nodes {
>      version
>      digest
>      storageRebate
>      previousTransactionBlock {
>        digest
>        sender {
>          defaultNameServiceName
>        }
>        gasInput {
>          gasPrice
>          gasBudget
>        }
>      }
>    }
>    pageInfo {
>      endCursor
>    }
>  }
>}</pre>

## <a id=12></a>
## Owner
### <a id=786420></a>
### Dynamic Field

><pre>fragment DynamicFieldValueSelection on DynamicFieldValue {
>  ... on MoveValue {
>    type {
>      repr
>    }
>    data
>    __typename
>  }
>  ... on MoveObject {
>    hasPublicTransfer
>    contents {
>      type {
>        repr
>      }
>      data
>    }
>    __typename
>  }
>}
>
>fragment DynamicFieldNameSelection on MoveValue {
>  type {
>    repr
>  }
>  data
>  bcs
>}
>
>fragment DynamicFieldSelect on DynamicField {
>  name {
>    ...DynamicFieldNameSelection
>  }
>  value {
>    ...DynamicFieldValueSelection
>  }
>}
>
>query DynamicField {
>  object(
>    address: "0xb57fba584a700a5bcb40991e1b2e6bf68b0f3896d767a0da92e69de73de226ac"
>  ) {
>    dynamicField(
>      name: {
>        type: "0x2::kiosk::Listing",
>        bcs: "NLArx1UJguOUYmXgNG8Pv8KbKXLjWtCi6i0Yeq1VhfwA",
>      }
>    ) {
>      ...DynamicFieldSelect
>    }
>  }
>}</pre>

### <a id=786421></a>
### Dynamic Field Connection

><pre>fragment DynamicFieldValueSelection on DynamicFieldValue {
>  ... on MoveValue {
>    type {
>      repr
>    }
>    data
>  }
>  ... on MoveObject {
>    hasPublicTransfer
>    contents {
>      type {
>        repr
>      }
>      data
>    }
>  }
>}
>
>fragment DynamicFieldNameSelection on MoveValue {
>  type {
>    repr
>  }
>  data
>  bcs
>}
>
>fragment DynamicFieldSelect on DynamicField {
>  name {
>    ...DynamicFieldNameSelection
>  }
>  value {
>    ...DynamicFieldValueSelection
>  }
>}
>
>query DynamicFieldConnection {
>  object(
>    address: "0xb57fba584a700a5bcb40991e1b2e6bf68b0f3896d767a0da92e69de73de226ac"
>  ) {
>    dynamicFieldConnection {
>      pageInfo {
>        hasNextPage
>        endCursor
>      }
>      edges {
>        cursor
>        node {
>          ...DynamicFieldSelect
>        }
>      }
>    }
>  }
>}</pre>

### <a id=786422></a>
### Dynamic Object Field

><pre>fragment DynamicFieldValueSelection on DynamicFieldValue {
>  ... on MoveValue {
>    type {
>      repr
>    }
>    data
>    __typename
>  }
>  ... on MoveObject {
>    hasPublicTransfer
>    contents {
>      type {
>        repr
>      }
>      data
>    }
>    __typename
>  }
>}
>
>fragment DynamicFieldNameSelection on MoveValue {
>  type {
>    repr
>  }
>  data
>  bcs
>}
>
>fragment DynamicFieldSelect on DynamicField {
>  name {
>    ...DynamicFieldNameSelection
>  }
>  value {
>    ...DynamicFieldValueSelection
>  }
>}
>
>query DynamicObjectField {
>  object(
>    address: "0xb57fba584a700a5bcb40991e1b2e6bf68b0f3896d767a0da92e69de73de226ac"
>  ) {
>    dynamicObjectField(
>      name: {type: "0x2::kiosk::Item", bcs: "NLArx1UJguOUYmXgNG8Pv8KbKXLjWtCi6i0Yeq1Vhfw="}
>    ) {
>      ...DynamicFieldSelect
>    }
>  }
>}</pre>

### <a id=786423></a>
### Owner

><pre>{
>  owner(
>    address: "0x931f293ce7f65fd5ebe9542653e1fd92fafa03dda563e13b83be35da8a2eecbe"
>  ) {
>    location
>  }
>}</pre>

## <a id=13></a>
## Protocol Configs
### <a id=851955></a>
### Key Value
####  Select the key and value of the protocol configuration

><pre>{
>  protocolConfig {
>    configs {
>      key
>      value
>    }
>  }
>}</pre>

### <a id=851956></a>
### Key Value Feature Flag
####  Select the key and value of the feature flag

><pre>{
>  protocolConfig {
>    featureFlags {
>      key
>      value
>    }
>  }
>}</pre>

### <a id=851957></a>
### Specific Config
####  Select the key and value of the specific protocol configuration, in this case `max_move_identifier_len`

><pre>{
>  protocolConfig {
>    config(key: "max_move_identifier_len") {
>      key
>      value
>    }
>  }
>}</pre>

### <a id=851958></a>
### Specific Feature Flag

><pre>{
>  protocolConfig {
>    protocolVersion
>    featureFlag(key: "advance_epoch_start_time_in_safe_mode") {
>      value
>    }
>  }
>}</pre>

## <a id=14></a>
## Service Config
### <a id=917490></a>
### Service Config
####  Get the configuration of the running service

><pre>{
>  serviceConfig {
>    isEnabled(feature: ANALYTICS)
>    enabledFeatures
>    maxQueryDepth
>    maxQueryNodes
>    maxDbQueryCost
>    defaultPageSize
>    maxPageSize
>    requestTimeoutMs
>    maxQueryPayloadSize
>  }
>}</pre>

## <a id=15></a>
## Stake Connection
### <a id=983025></a>
### Stake Connection
####  Get all the staked objects for this address and all the active validators at the epoch when the stake became active

><pre>{
>  address(
>    address: "0xc0a5b916d0e406ddde11a29558cd91b29c49e644eef597b7424a622955280e1e"
>  ) {
>    location
>    balance(type: "0x2::sui::SUI") {
>      coinType {
>        repr
>      }
>      totalBalance
>    }
>    stakedSuiConnection {
>      nodes {
>        status
>        principal
>        estimatedReward
>        activeEpoch {
>          epochId
>          referenceGasPrice
>          validatorSet {
>            activeValidators {
>              name
>              description
>              exchangeRatesSize
>            }
>            totalStake
>          }
>        }
>        requestEpoch {
>          epochId
>        }
>      }
>    }
>  }
>}</pre>

## <a id=16></a>
## Sui System State Summary
### <a id=1048560></a>
### Sui System State Summary

><pre>{
>  latestSuiSystemState {
>    systemStateVersion
>    referenceGasPrice
>    startTimestamp
>    validatorSet {
>      totalStake
>      pendingActiveValidatorsSize
>      stakePoolMappingsSize
>      inactivePoolsSize
>      validatorCandidatesSize
>      activeValidators {
>        name
>        description
>        imageUrl
>        projectUrl
>        exchangeRates {
>          asObject {
>            storageRebate
>            bcs
>            kind
>          }
>          hasPublicTransfer
>        }
>        exchangeRatesSize
>        stakingPoolActivationEpoch
>        stakingPoolSuiBalance
>        rewardsPool
>        poolTokenBalance
>        pendingStake
>        pendingTotalSuiWithdraw
>        pendingPoolTokenWithdraw
>        votingPower
>        gasPrice
>        commissionRate
>        nextEpochStake
>        nextEpochGasPrice
>        nextEpochCommissionRate
>        atRisk
>      }
>    }
>  }
>}</pre>

## <a id=17></a>
## Transaction Block
### <a id=1114095></a>
### Transaction Block
####  Get the data for a TransactionBlock by its digest

><pre>{
>  transactionBlock(digest: "HvTjk3ELg8gRofmB1GgrpLHBFeA53QKmUKGEuhuypezg") {
>    sender {
>      location
>    }
>    gasInput {
>      gasSponsor {
>        location
>      }
>      gasPayment {
>        nodes {
>          location
>        }
>      }
>      gasPrice
>      gasBudget
>    }
>    kind {
>      __typename
>    }
>    signatures {
>      base64Sig
>    }
>    digest
>    expiration {
>      epochId
>    }
>    effects {
>      timestamp
>    }
>  }
>}</pre>

### <a id=1114096></a>
### Transaction Block Kind

><pre>{
>  object(
>    address: "0xd6b9c261ab53d636760a104e4ab5f46c2a3e9cda58bd392488fc4efa6e43728c"
>  ) {
>    previousTransactionBlock {
>      sender {
>        location
>      }
>      kind {
>        __typename
>        ... on ConsensusCommitPrologueTransaction {
>          timestamp
>          round
>          epoch {
>            epochId
>            referenceGasPrice
>          }
>        }
>        ... on ChangeEpochTransaction {
>          computationCharge
>          storageCharge
>          timestamp
>          storageRebate
>        }
>        ... on GenesisTransaction {
>          objects
>        }
>      }
>    }
>  }
>}</pre>

## <a id=18></a>
## Transaction Block Connection
### <a id=1179630></a>
### Before After Checkpoint
####  Filter on before_ and after_checkpoint. If both are provided, before must be greater than after

><pre>{
>  transactionBlockConnection(
>    filter: { afterCheckpoint: 10, beforeCheckpoint: 20 }
>  ) {
>    nodes {
>      sender {
>        location
>      }
>      gasInput {
>        gasPrice
>        gasBudget
>      }
>    }
>  }
>}</pre>

### <a id=1179631></a>
### Changed Object Filter
####  Filter on changedObject

><pre>{
>  transactionBlockConnection(
>    filter: {
>      changedObject: "0x0000000000000000000000000000000000000000000000000000000000000006"
>    }
>  ) {
>    nodes {
>      sender {
>        location
>      }
>      gasInput {
>        gasPrice
>        gasBudget
>      }
>    }
>  }
>}</pre>

### <a id=1179632></a>
### Input Object Filter
####  Filter on inputObject

><pre>{
>  transactionBlockConnection(
>    filter: {
>      inputObject: "0x0000000000000000000000000000000000000000000000000000000000000006"
>    }
>  ) {
>    nodes {
>      sender {
>        location
>      }
>      gasInput {
>        gasPrice
>        gasBudget
>      }
>    }
>  }
>}</pre>

### <a id=1179633></a>
### Input Object Sent Addr Filter
####  multiple filters

><pre>{
>  transactionBlockConnection(
>    filter: {
>      inputObject: "0x0000000000000000000000000000000000000000000000000000000000000006"
>      sentAddress: "0x0000000000000000000000000000000000000000000000000000000000000000"
>    }
>  ) {
>    nodes {
>      sender {
>        location
>      }
>      effects {
>        gasEffects {
>          gasObject {
>            location
>          }
>        }
>      }
>      gasInput {
>        gasPrice
>        gasBudget
>      }
>    }
>  }
>}</pre>

### <a id=1179634></a>
### Package Filter
####  Filtering on package

><pre>{
>  transactionBlockConnection(
>    filter: {
>      package: "0x0000000000000000000000000000000000000000000000000000000000000003"
>    }
>  ) {
>    nodes {
>      sender {
>        location
>      }
>      gasInput {
>        gasPrice
>        gasBudget
>      }
>    }
>  }
>}</pre>

### <a id=1179635></a>
### Package Module Filter
####  Filtering on package and module

><pre>{
>  transactionBlockConnection(
>    filter: {
>      package: "0x0000000000000000000000000000000000000000000000000000000000000003"
>      module: "sui_system"
>    }
>  ) {
>    nodes {
>      sender {
>        location
>      }
>      gasInput {
>        gasPrice
>        gasBudget
>      }
>    }
>  }
>}</pre>

### <a id=1179636></a>
### Package Module Func Filter
####  Filtering on package, module and function

><pre>{
>  transactionBlockConnection(
>    filter: {
>      package: "0x0000000000000000000000000000000000000000000000000000000000000003"
>      module: "sui_system"
>      function: "request_withdraw_stake"
>    }
>  ) {
>    nodes {
>      sender {
>        location
>      }
>      gasInput {
>        gasPrice
>        gasBudget
>      }
>    }
>  }
>}</pre>

### <a id=1179637></a>
### Recv Addr Filter
####  Filter on recvAddress

><pre>{
>  transactionBlockConnection(
>    filter: {
>      recvAddress: "0x0000000000000000000000000000000000000000000000000000000000000000"
>    }
>  ) {
>    nodes {
>      sender {
>        location
>      }
>      gasInput {
>        gasPrice
>        gasBudget
>      }
>    }
>  }
>}</pre>

### <a id=1179638></a>
### Sent Addr Filter
####  Filter on sign or sentAddress

><pre>{
>  transactionBlockConnection(
>    filter: {
>      sentAddress: "0x0000000000000000000000000000000000000000000000000000000000000000"
>    }
>  ) {
>    nodes {
>      sender {
>        location
>      }
>      gasInput {
>        gasPrice
>        gasBudget
>      }
>    }
>  }
>}</pre>

### <a id=1179639></a>
### Tx Ids Filter
####  Filter on transactionIds

><pre>{
>  transactionBlockConnection(
>    filter: { transactionIds: ["DtQ6v6iJW4wMLgadENPUCEUS5t8AP7qvdG5jX84T1akR"] }
>  ) {
>    nodes {
>      sender {
>        location
>      }
>      gasInput {
>        gasPrice
>        gasBudget
>      }
>    }
>  }
>}</pre>

### <a id=1179640></a>
### Tx Kind Filter
####  Filter on TransactionKind (only SYSTEM_TX or PROGRAMMABLE_TX)

><pre>{
>  transactionBlockConnection(filter: { kind: SYSTEM_TX }) {
>    nodes {
>      sender {
>        location
>      }
>      gasInput {
>        gasPrice
>        gasBudget
>      }
>    }
>  }
>}</pre>

### <a id=1179641></a>
### With Defaults Ascending
####  Fetch some default amount of transactions, ascending

><pre>{
>  transactionBlockConnection {
>    nodes {
>      digest
>      effects {
>        gasEffects {
>          gasObject {
>            version
>            digest
>          }
>          gasSummary {
>            computationCost
>            storageCost
>            storageRebate
>            nonRefundableStorageFee
>          }
>        }
>        errors
>      }
>      sender {
>        location
>      }
>      gasInput {
>        gasPrice
>        gasBudget
>      }
>    }
>    pageInfo {
>      endCursor
>    }
>  }
>}</pre>

## <a id=19></a>
## Transaction Block Effects
### <a id=1245165></a>
### Transaction Block Effects

><pre>{
>  object(
>    address: "0x0bba1e7d907dc2832edfc3bf4468b6deacd9a2df435a35b17e640e135d2d5ddc"
>  ) {
>    version
>    kind
>    previousTransactionBlock {
>      effects {
>        status
>        checkpoint {
>          sequenceNumber
>        }
>        lamportVersion
>        gasEffects {
>          gasSummary {
>            computationCost
>            storageCost
>            storageRebate
>            nonRefundableStorageFee
>          }
>        }
>        balanceChanges {
>          owner {
>            location
>            balance(type: "0x2::sui::SUI") {
>              totalBalance
>            }
>          }
>          amount
>          coinType {
>            repr
>            signature
>            layout
>          }
>        }
>        dependencies {
>          sender {
>            location
>          }
>        }
>      }
>    }
>  }
>}</pre>

