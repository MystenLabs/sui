# Sui GraphQL Examples
### [Transaction Block Effects](#0)
#### &emsp;&emsp;[Transaction Block Effects](#0)
### [Coin Connection](#1)
#### &emsp;&emsp;[Coin Connection](#65535)
### [Balance Connection](#2)
#### &emsp;&emsp;[Balance Connection](#131070)
### [Checkpoint](#3)
#### &emsp;&emsp;[At Seq Num](#196605)
#### &emsp;&emsp;[At Digest](#196606)
#### &emsp;&emsp;[With Timestamp Tx Block Live Objects](#196607)
#### &emsp;&emsp;[Multiple Selections](#196608)
#### &emsp;&emsp;[First Two Tx Blocks For Checkpoint](#196609)
#### &emsp;&emsp;[Latest Checkpoint](#196610)
#### &emsp;&emsp;[With Tx Sent Addr Filter](#196611)
### [Chain Id](#4)
#### &emsp;&emsp;[Chain Id](#262140)
### [Name Service](#5)
#### &emsp;&emsp;[Name Service](#327675)
### [Owner](#6)
#### &emsp;&emsp;[Owner](#393210)
### [Checkpoint Connection](#7)
#### &emsp;&emsp;[First Ten After Checkpoint](#458745)
#### &emsp;&emsp;[Ascending Fetch](#458746)
#### &emsp;&emsp;[Last Ten After Checkpoint](#458747)
### [Event Connection](#8)
#### &emsp;&emsp;[Event Connection](#524280)
### [Epoch](#9)
#### &emsp;&emsp;[Specific Epoch](#589815)
#### &emsp;&emsp;[With Checkpoint Connection](#589816)
#### &emsp;&emsp;[With Tx Block Connection](#589817)
#### &emsp;&emsp;[With Tx Block Connection Latest Epoch](#589818)
#### &emsp;&emsp;[Latest Epoch](#589819)
### [Transaction Block](#10)
#### &emsp;&emsp;[Transaction Block Kind](#655350)
### [Object Connection](#11)
#### &emsp;&emsp;[Object Connection](#720885)
#### &emsp;&emsp;[Filter Owner](#720886)
#### &emsp;&emsp;[Filter Object Ids](#720887)
### [Object](#12)
#### &emsp;&emsp;[Object](#786420)
### [Sui System State Summary](#13)
#### &emsp;&emsp;[Sui System State Summary](#851955)
### [Address](#14)
#### &emsp;&emsp;[Transaction Block Connection](#917490)
### [Stake Connection](#15)
#### &emsp;&emsp;[Stake Connection](#983025)
### [Protocol Configs](#16)
#### &emsp;&emsp;[Specific Feature Flag](#1048560)
#### &emsp;&emsp;[Key Value Feature Flag](#1048561)
#### &emsp;&emsp;[Specific Config](#1048562)
#### &emsp;&emsp;[Key Value](#1048563)
### [Transaction Block Connection](#17)
#### &emsp;&emsp;[Input Object Filter](#1114095)
#### &emsp;&emsp;[Input Object Sent Addr Filter](#1114096)
#### &emsp;&emsp;[Sent Addr Filter](#1114097)
#### &emsp;&emsp;[Package Module Filter](#1114098)
#### &emsp;&emsp;[Recv Addr Filter](#1114099)
#### &emsp;&emsp;[Tx Kind Filter](#1114100)
#### &emsp;&emsp;[Before After Checkpoint](#1114101)
#### &emsp;&emsp;[With Defaults Ascending](#1114102)
#### &emsp;&emsp;[Tx Ids Filter](#1114103)
#### &emsp;&emsp;[Package Module Func Filter](#1114104)
#### &emsp;&emsp;[Changed Object Filter](#1114105)
#### &emsp;&emsp;[Package Filter](#1114106)
## <a id=0></a>
## Transaction Block Effects
### <a id=0></a>
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
>          gasSummary{
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

## <a id=1></a>
## Coin Connection
### <a id=65535></a>
### Coin Connection
####  Get last 3 coins before coins at cursor 13034947

><pre>{
>  address(address:"0x0000000000000000000000000000000000000000000000000000000000000000") {
>    coinConnection(last: 3, before:"0x13034947") {
>      nodes {
>        id, balance
>      },
>      pageInfo {
>        endCursor, hasNextPage
>      }
>    }
>  }
>}</pre>

## <a id=2></a>
## Balance Connection
### <a id=131070></a>
### Balance Connection

><pre>{
>  address(address:"0x5094652429957619e6efa79a404a6714d1126e63f551f4b6c7fb76440f8118c9") {
>    balance(type:"0xc060006111016b8a020ad5b33834984a437aaa7d3c74c18e09a95d48aceab08c::coin::COIN") {
>      coinObjectCount totalBalance
>    }
>    balanceConnection {
>      nodes {
>        coinObjectCount
>        totalBalance
>      }
>      pageInfo {endCursor}
>    }
>  }
>}</pre>

## <a id=3></a>
## Checkpoint
### <a id=196605></a>
### At Seq Num
####  At a particular sequence number

><pre>{
>  checkpoint(id: {
>    sequenceNumber:10
>  }) {
>    digest, sequenceNumber, validatorSignature, previousCheckpointDigest, networkTotalTransactions, rollingGasSummary {
>      computationCost
>      storageCost
>      storageRebate
>      nonRefundableStorageFee
>    }, epoch {
>      referenceGasPrice
>      startTimestamp
>    }, endOfEpoch {
>      nextProtocolVersion
>    }
>  }
>}</pre>

### <a id=196606></a>
### At Digest
####  At a particular digest

><pre>{
>  checkpoint(id: {
>    digest:"GaDeWEfbSQCQ8FBQHUHVdm4KjrnbgMqEZPuhStoq5njU"
>  }) {
>    digest, sequenceNumber, validatorSignature, previousCheckpointDigest, networkTotalTransactions, rollingGasSummary {
>      computationCost
>      storageCost
>      storageRebate
>      nonRefundableStorageFee
>    }, epoch {
>      referenceGasPrice
>      startTimestamp
>    }, endOfEpoch {
>      nextProtocolVersion
>    }
>  }
>}</pre>

### <a id=196607></a>
### With Timestamp Tx Block Live Objects
####  with timestamp, liveObjectSetDigest, and transaction block data for the latest checkpoint

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

### <a id=196608></a>
### Multiple Selections

><pre>{
>  checkpoint(id:{
>    sequenceNumber: 9769
>  }) {
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
>    transactionBlockConnection{
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

### <a id=196609></a>
### First Two Tx Blocks For Checkpoint
####  Get data for the first two transaction blocks of checkpoint at sequence number 10

><pre>{
>  checkpoint(id: {sequenceNumber: 10}) {
>    transactionBlockConnection(first: 2) {
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
>      pageInfo {
>        startCursor
>      }
>    }
>  }
>}</pre>

### <a id=196610></a>
### Latest Checkpoint
####  Latest checkpoint

><pre>{
>  checkpoint {
>    digest, sequenceNumber, validatorSignature, previousCheckpointDigest, networkTotalTransactions, rollingGasSummary {
>      computationCost
>      storageCost
>      storageRebate
>      nonRefundableStorageFee
>    }, epoch {
>      referenceGasPrice
>      startTimestamp
>    }, endOfEpoch {
>      nextProtocolVersion
>    }
>  }
>}</pre>

### <a id=196611></a>
### With Tx Sent Addr Filter
####  Select checkpoint at sequence number 14830285 for transactions from sentAddress

><pre>{
>  checkpoint(id:{
>    sequenceNumber: 14830285
>  }) {
>    digest
>    sequenceNumber
>    timestamp
>    liveObjectSetDigest
>    transactionBlockConnection(filter: {
>      sentAddress: "0x0000000000000000000000000000000000000000000000000000000000000000"
>    }) {
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
## Chain Id
### <a id=262140></a>
### Chain Id
####  Returns the chain identifier for the chain that the server is tracking

><pre>{
>  chainIdentifier
>}</pre>

## <a id=5></a>
## Name Service
### <a id=327675></a>
### Name Service

><pre>{
>  resolveNameServiceAddress(name:"example.sui") {
>    location
>  }
>  address(address:"0x0b86be5d779fac217b41d484b8040ad5145dc9ba0cba099d083c6cbda50d983e"){
>    location
>    balance(type:"0x2::sui::SUI") {
>      coinType
>      coinObjectCount
>      totalBalance
>    }
>    defaultNameServiceName
>  }
>}</pre>

## <a id=6></a>
## Owner
### <a id=393210></a>
### Owner

><pre>{
>  owner(address:"0x931f293ce7f65fd5ebe9542653e1fd92fafa03dda563e13b83be35da8a2eecbe"){
>    location
>  }
>}</pre>

## <a id=7></a>
## Checkpoint Connection
### <a id=458745></a>
### First Ten After Checkpoint
####  Fetch first 10 after the cursor - note that cursor will be opaque

><pre>{
>  checkpointConnection(first:10, after:"11") {
>    nodes {
>      digest
>    }
>  }
>}</pre>

### <a id=458746></a>
### Ascending Fetch
####  Fetch some default amount of checkpoints, ascending

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
>        referenceGasPrice
>        startTimestamp
>      }
>      endOfEpoch {
>        nextProtocolVersion
>      }
>    }
>  }
>}</pre>

### <a id=458747></a>
### Last Ten After Checkpoint
####  Fetch last 20 before the cursor

><pre>{
>  checkpointConnection(last:20, before:"100") {
>    nodes {
>      digest
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
>      eventType: "0x3164fcf73eb6b41ff3d2129346141bd68469964c2d95a5b1533e8d16e6ea6e13::Market::ChangePriceEvent<0x2::sui::SUI>"}
>  ) {
>    nodes {
>      id
>      sendingModuleId {
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
>      senders {location}
>      timestamp
>      json
>      bcs
>    }
>  }
>}</pre>

## <a id=9></a>
## Epoch
### <a id=589815></a>
### Specific Epoch
####  Selecting all fields for epoch 100

><pre>{
>  epoch(id: 100){
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

### <a id=589816></a>
### With Checkpoint Connection

><pre>{
>  epoch {
>    checkpointConnection {
>        nodes {
>          transactionBlockConnection(first: 10) {
>            pageInfo {
>              hasNextPage
>              endCursor
>            }
>            edges {
>              cursor
>              node {
>                sender {
>                  location
>                }
>                effects {
>                  gasEffects {
>                    gasObject {
>                      location
>                    }
>                  }
>                }
>                gasInput {
>                  gasPrice
>                  gasBudget
>                }
>              }
>            }
>          }
>        }
>      }
>    }
>  }</pre>

### <a id=589817></a>
### With Tx Block Connection

><pre>{
>  epoch(id:97) {
>    transactionBlockConnection(first: 20, after:"261225985") {
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

### <a id=589818></a>
### With Tx Block Connection Latest Epoch
####  the last checkpoint of epoch 97 is 8097645
####  last tx number of the checkpoint is 261225985

><pre>{
>  epoch {
>    transactionBlockConnection(first: 20, after:"261225985") {
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

### <a id=589819></a>
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

## <a id=10></a>
## Transaction Block
### <a id=655350></a>
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

## <a id=11></a>
## Object Connection
### <a id=720885></a>
### Object Connection

><pre>{
>  objectConnection {
>    nodes {
>      version
>      digest
>      storageRebate
>      previousTransactionBlock {
>        digest, sender {
>          defaultNameServiceName
>        }, gasInput {
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

### <a id=720886></a>
### Filter Owner
####  Filter on owner

><pre>{
>  objectConnection(filter:{
>    owner: "0x23b7b0e2badb01581ba9b3ab55587d8d9fdae087e0cfc79f2c72af36f5059439"
>  }) {
>    edges {
>      node {
>        storageRebate
>        kind
>      }
>    }
>  }
>}
></pre>

### <a id=720887></a>
### Filter Object Ids
####  Filter on objectIds

><pre>{
>  objectConnection(filter:{
>    objectIds: ["0x4bba2c7b9574129c272bca8f58594eba933af8001257aa6e0821ad716030f149"]
>  }) {
>    edges {
>      node {
>        storageRebate
>        kind
>      }
>    }
>  }
>}</pre>

## <a id=12></a>
## Object
### <a id=786420></a>
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

## <a id=13></a>
## Sui System State Summary
### <a id=851955></a>
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

## <a id=14></a>
## Address
### <a id=917490></a>
### Transaction Block Connection
####  See examples in Query::transactionBlockConnection as this is
####  similar behavior to the `transactionBlockConnection` in Query but
####  supports additional `AddressTransactionBlockRelationship` filter
####  Filtering on package where the sender of the TX is the current address

><pre># See examples in Query::transactionBlockConnection as this is
># similar behavior to the `transactionBlockConnection` in Query but 
># supports additional `AddressTransactionBlockRelationship` filter
>
># Filtering on package where the sender of the TX is the current address
>query transaction_block_with_relation_filter {
>  address(address: "0x2") {
>    transactionBlockConnection(
>      relation: SENT
>      filter: {package: "0x2"}
>    ) {
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

## <a id=15></a>
## Stake Connection
### <a id=983025></a>
### Stake Connection

><pre>query stake_connection {
>  address(
>    address: "0x341fa71e4e58d63668034125c3152f935b00b0bb5c68069045d8c646d017fae1"
>  ) {
>    location
>    balance(type: "0x2::sui::SUI") {
>      coinType
>      totalBalance
>    }
>    stakeConnection {
>      nodes {
>        status
>        principal
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
## Protocol Configs
### <a id=1048560></a>
### Specific Feature Flag

><pre>{
>  protocolConfig {
>    protocolVersion
>    featureFlag(key:"advance_epoch_start_time_in_safe_mode") {
>      value
>    }
>  }
>}</pre>

### <a id=1048561></a>
### Key Value Feature Flag
####  Select the key and value of the feature flag

><pre>{
>  protocolConfig {
>    featureFlags {
>      key
>	    value
>    }
>  }
>}</pre>

### <a id=1048562></a>
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

### <a id=1048563></a>
### Key Value
####  Select the key and value of the protocol configuration

><pre>{
>  protocolConfig {
>    configs {
>      key
>	    value
>    }
>  }
>}</pre>

## <a id=17></a>
## Transaction Block Connection
### <a id=1114095></a>
### Input Object Filter
####  Filter on inputObject

><pre>{
>  transactionBlockConnection(
>    filter: {
>      inputObject:"0x0000000000000000000000000000000000000000000000000000000000000006"
>    }
>  ) {
>    nodes {
>      sender {
>        location
>      },
>      gasInput {
>        gasPrice
>        gasBudget
>      }
>    }
>  }
>}</pre>

### <a id=1114096></a>
### Input Object Sent Addr Filter
####  multiple filters

><pre>{
>  transactionBlockConnection(
>    filter: {
>      inputObject:"0x0000000000000000000000000000000000000000000000000000000000000006",
>      sentAddress:"0x0000000000000000000000000000000000000000000000000000000000000000"
>    }
>  ) {
>    nodes {
>      sender {
>        location
>      },
>      effects {
>        gasEffects {
>          gasObject { location}
>        }
>      }
>      gasInput {
>        gasPrice
>        gasBudget
>      }
>    }
>  }
>}</pre>

### <a id=1114097></a>
### Sent Addr Filter
####  Filter on sign or sentAddress

><pre>{
>  transactionBlockConnection(
>    filter: {
>      sentAddress:"0x0000000000000000000000000000000000000000000000000000000000000000"
>    }
>  ) {
>    nodes {
>      sender {
>        location
>      },
>      gasInput {
>        gasPrice
>        gasBudget
>      }
>    }
>  }
>}</pre>

### <a id=1114098></a>
### Package Module Filter
####  Filtering on package and module

><pre>{
>  transactionBlockConnection(
>    filter: {
>      package: "0x0000000000000000000000000000000000000000000000000000000000000003",
>      module:"sui_system"
>    }
>  ) {
>    nodes {
>      sender {
>        location
>      },
>      gasInput {
>        gasPrice
>        gasBudget
>      }
>    }
>  }
>}</pre>

### <a id=1114099></a>
### Recv Addr Filter
####  Filter on recvAddress

><pre>{
>  transactionBlockConnection(
>    filter: {
>      recvAddress:"0x0000000000000000000000000000000000000000000000000000000000000000"
>    }
>  ) {
>    nodes {
>      sender {
>        location
>      },
>      gasInput {
>        gasPrice
>        gasBudget
>      }
>    }
>  }
>}</pre>

### <a id=1114100></a>
### Tx Kind Filter
####  Filter on TransactionKind (only SYSTEM_TX or PROGRAMMABLE_TX)

><pre>{
>  transactionBlockConnection(
>    filter: {
>      kind:SYSTEM_TX
>    }
>  ) {
>    nodes {
>      sender {
>        location
>      },
>      gasInput {
>        gasPrice
>        gasBudget
>      }
>    }
>  }
>}</pre>

### <a id=1114101></a>
### Before After Checkpoint
####  Filter on before_ and after_checkpoint. If both are provided, before must be greater than after

><pre>{
>  transactionBlockConnection(
>    filter: {
>      afterCheckpoint: 10,
>      beforeCheckpoint: 20
>    }
>  ) {
>    nodes {
>      sender {
>        location
>      },
>      gasInput {
>        gasPrice
>        gasBudget
>      }
>    }
>  }
>}</pre>

### <a id=1114102></a>
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

### <a id=1114103></a>
### Tx Ids Filter
####  Filter on transactionIds

><pre>{
>  transactionBlockConnection(
>    filter: {
>      transactionIds:["DtQ6v6iJW4wMLgadENPUCEUS5t8AP7qvdG5jX84T1akR"]
>    }
>  ) {
>    nodes {
>      sender {
>        location
>      },
>      gasInput {
>        gasPrice
>        gasBudget
>      }
>    }
>  }
>}</pre>

### <a id=1114104></a>
### Package Module Func Filter
####  Filtering on package, module and function

><pre>{
>  transactionBlockConnection(
>    filter: {
>      package: "0x0000000000000000000000000000000000000000000000000000000000000003",
>      module:"sui_system",
>      function:"request_withdraw_stake"
>    }
>  ) {
>    nodes {
>      sender {
>        location
>      },
>      gasInput {
>        gasPrice
>        gasBudget
>      }
>    }
>  }
>}</pre>

### <a id=1114105></a>
### Changed Object Filter
####  Filter on changedObject

><pre>{
>  transactionBlockConnection(
>    filter: {
>      changedObject:"0x0000000000000000000000000000000000000000000000000000000000000006"
>    }
>  ) {
>    nodes {
>      sender {
>        location
>      },
>      gasInput {
>        gasPrice
>        gasBudget
>      }
>    }
>  }
>}</pre>

### <a id=1114106></a>
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
>      },
>      gasInput {
>        gasPrice
>        gasBudget
>      }
>    }
>  }
>}</pre>

