# Usage Examples - Sui Voting DApp

This document provides practical examples of using the Voting DApp through different interfaces.

## Table of Contents

- [CLI Examples](#cli-examples)
- [JavaScript/TypeScript Examples](#javascripttypescript-examples)
- [Rust SDK Examples](#rust-sdk-examples)
- [Advanced Scenarios](#advanced-scenarios)

## CLI Examples

### Setup Environment Variables

```bash
export PACKAGE_ID=0xYOUR_PACKAGE_ID
export POLL_ID=0xYOUR_POLL_ID
```

### Create a Simple Poll

```bash
sui client call \
  --package $PACKAGE_ID \
  --module voting \
  --function create_poll \
  --args "Do you like Sui?" "Yes" "No" \
  --gas-budget 10000
```

### Create a Poll with Multiple Options

```bash
sui client call \
  --package $PACKAGE_ID \
  --module voting \
  --function create_poll_multi \
  --args "Best programming language?" '["Rust","Python","JavaScript","Go"]' \
  --gas-budget 10000
```

### Cast a Vote (Option 0)

```bash
sui client call \
  --package $PACKAGE_ID \
  --module voting \
  --function vote \
  --args $POLL_ID 0 \
  --gas-budget 10000
```

### Close a Poll

```bash
sui client call \
  --package $PACKAGE_ID \
  --module voting \
  --function close_poll \
  --args $POLL_ID \
  --gas-budget 10000
```

### Reopen a Poll

```bash
sui client call \
  --package $PACKAGE_ID \
  --module voting \
  --function reopen_poll \
  --args $POLL_ID \
  --gas-budget 10000
```

### View Poll Details

```bash
# Get full object data
sui client object $POLL_ID

# Get JSON format
sui client object $POLL_ID --json

# Pretty print with jq
sui client object $POLL_ID --json | jq '.details.data.fields'
```

### Query Poll Fields

```bash
# Get question
sui client object $POLL_ID --json | jq '.details.data.fields.question'

# Get options
sui client object $POLL_ID --json | jq '.details.data.fields.options'

# Get votes
sui client object $POLL_ID --json | jq '.details.data.fields.votes'

# Get total votes
sui client object $POLL_ID --json | jq '.details.data.fields.total_votes'

# Check if active
sui client object $POLL_ID --json | jq '.details.data.fields.is_active'
```

## JavaScript/TypeScript Examples

### Setup

```typescript
import { JsonRpcProvider, devnetConnection, Ed25519Keypair, RawSigner } from '@mysten/sui.js';

const provider = new JsonRpcProvider(devnetConnection);
const keypair = Ed25519Keypair.fromSecretKey(YOUR_SECRET_KEY);
const signer = new RawSigner(keypair, provider);

const PACKAGE_ID = 'YOUR_PACKAGE_ID';
const MODULE = 'voting';
```

### Create a Poll

```typescript
async function createPoll(question: string, options: string[]) {
  const tx = {
    packageObjectId: PACKAGE_ID,
    module: MODULE,
    function: options.length === 2 ? 'create_poll' : 'create_poll_multi',
    typeArguments: [],
    arguments: options.length === 2
      ? [question, options[0], options[1]]
      : [question, options],
    gasBudget: 10000,
  };

  const result = await signer.executeMoveCall(tx);
  console.log('Poll created:', result.certificate.transactionDigest);

  // Find the created poll object
  const createdObjects = result.effects.created;
  const pollObject = createdObjects?.find(obj => obj.owner === 'Shared');

  return pollObject?.reference.objectId;
}

// Usage
const pollId = await createPoll(
  "What's your favorite blockchain?",
  ["Sui", "Ethereum", "Solana"]
);
console.log('Poll ID:', pollId);
```

### Vote on a Poll

```typescript
async function vote(pollId: string, optionIndex: number) {
  const tx = {
    packageObjectId: PACKAGE_ID,
    module: MODULE,
    function: 'vote',
    typeArguments: [],
    arguments: [pollId, optionIndex],
    gasBudget: 10000,
  };

  const result = await signer.executeMoveCall(tx);
  console.log('Vote cast:', result.certificate.transactionDigest);

  return result;
}

// Usage
await vote(pollId, 0); // Vote for option 0
```

### Get Poll Data

```typescript
async function getPollData(pollId: string) {
  const object = await provider.getObject(pollId);

  if (!object.details || object.status !== 'Exists') {
    throw new Error('Poll not found');
  }

  const fields = object.details.data.fields;

  return {
    question: fields.question,
    options: fields.options,
    votes: fields.votes.map(v => parseInt(v)),
    totalVotes: parseInt(fields.total_votes),
    creator: fields.creator,
    isActive: fields.is_active,
  };
}

// Usage
const pollData = await getPollData(pollId);
console.log('Question:', pollData.question);
console.log('Options:', pollData.options);
console.log('Votes:', pollData.votes);
console.log('Total:', pollData.totalVotes);
```

### Get Poll Results with Percentages

```typescript
async function getPollResults(pollId: string) {
  const data = await getPollData(pollId);

  const results = data.options.map((option, index) => {
    const votes = data.votes[index];
    const percentage = data.totalVotes > 0
      ? (votes / data.totalVotes * 100).toFixed(2)
      : '0.00';

    return {
      option,
      votes,
      percentage: `${percentage}%`,
    };
  });

  return {
    question: data.question,
    results,
    totalVotes: data.totalVotes,
    isActive: data.isActive,
  };
}

// Usage
const results = await getPollResults(pollId);
console.log('Results:', JSON.stringify(results, null, 2));
```

### Close/Reopen Poll

```typescript
async function closePoll(pollId: string) {
  const tx = {
    packageObjectId: PACKAGE_ID,
    module: MODULE,
    function: 'close_poll',
    typeArguments: [],
    arguments: [pollId],
    gasBudget: 10000,
  };

  const result = await signer.executeMoveCall(tx);
  console.log('Poll closed:', result.certificate.transactionDigest);
}

async function reopenPoll(pollId: string) {
  const tx = {
    packageObjectId: PACKAGE_ID,
    module: MODULE,
    function: 'reopen_poll',
    typeArguments: [],
    arguments: [pollId],
    gasBudget: 10000,
  };

  const result = await signer.executeMoveCall(tx);
  console.log('Poll reopened:', result.certificate.transactionDigest);
}

// Usage
await closePoll(pollId);
await reopenPoll(pollId);
```

### Listen to Events

```typescript
async function listenToEvents() {
  // Subscribe to all events from the package
  const subscription = await provider.subscribeEvent({
    filter: { Package: PACKAGE_ID },
    onMessage: (event) => {
      const eventType = event.type.split('::').pop();

      if (eventType === 'PollCreated') {
        console.log('New poll created:', {
          pollId: event.parsedJson.poll_id,
          question: event.parsedJson.question,
          creator: event.parsedJson.creator,
        });
      } else if (eventType === 'VoteCast') {
        console.log('Vote cast:', {
          pollId: event.parsedJson.poll_id,
          voter: event.parsedJson.voter,
          option: event.parsedJson.option_index,
        });
      }
    },
  });

  return subscription;
}

// Usage
const subscription = await listenToEvents();

// Later, to unsubscribe:
// subscription.unsubscribe();
```

### Query Historical Events

```typescript
async function getPolls() {
  const events = await provider.queryEvents({
    query: { MoveEventType: `${PACKAGE_ID}::${MODULE}::PollCreated` },
  });

  return events.data.map(event => ({
    pollId: event.parsedJson.poll_id,
    question: event.parsedJson.question,
    creator: event.parsedJson.creator,
    timestamp: event.timestamp,
  }));
}

// Usage
const polls = await getPolls();
console.log('All polls:', polls);
```

## Rust SDK Examples

### Setup

```rust
use sui_sdk::{SuiClient, types::base_types::ObjectID};
use sui_keys::keystore::{AccountKeystore, FileBasedKeystore};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sui = SuiClient::new_devnet_client().await?;
    let keystore = FileBasedKeystore::new(&sui_config_dir()?.join("sui.keystore"))?;
    let active_address = keystore.addresses()[0];

    // Your code here

    Ok(())
}
```

### Create a Poll

```rust
async fn create_poll(
    sui: &SuiClient,
    signer: SuiAddress,
    package_id: ObjectID,
    question: String,
    option1: String,
    option2: String,
) -> Result<ObjectID, Box<dyn std::error::Error>> {
    let tx = sui.transaction_builder()
        .move_call(
            signer,
            package_id,
            "voting",
            "create_poll",
            vec![],
            vec![
                SuiJsonValue::from_str(&question)?,
                SuiJsonValue::from_str(&option1)?,
                SuiJsonValue::from_str(&option2)?,
            ],
            None,
            10000,
        )
        .await?;

    let response = sui.execute_transaction(tx, &keystore, signer).await?;

    // Extract poll ID from created objects
    let poll_id = response.effects
        .created()
        .iter()
        .find(|obj| matches!(obj.owner, Owner::Shared { .. }))
        .map(|obj| obj.reference.object_id)
        .ok_or("Poll object not found")?;

    Ok(poll_id)
}
```

### Vote on a Poll

```rust
async fn vote(
    sui: &SuiClient,
    signer: SuiAddress,
    package_id: ObjectID,
    poll_id: ObjectID,
    option_index: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    let tx = sui.transaction_builder()
        .move_call(
            signer,
            package_id,
            "voting",
            "vote",
            vec![],
            vec![
                SuiJsonValue::new(poll_id.to_string().into())?,
                SuiJsonValue::new(option_index.into())?,
            ],
            None,
            10000,
        )
        .await?;

    sui.execute_transaction(tx, &keystore, signer).await?;

    Ok(())
}
```

## Advanced Scenarios

### Batch Voting (Multiple Accounts)

```typescript
async function batchVote(pollId: string, voters: Ed25519Keypair[], options: number[]) {
  const promises = voters.map(async (keypair, index) => {
    const signer = new RawSigner(keypair, provider);
    const optionIndex = options[index % options.length];

    const tx = {
      packageObjectId: PACKAGE_ID,
      module: MODULE,
      function: 'vote',
      typeArguments: [],
      arguments: [pollId, optionIndex],
      gasBudget: 10000,
    };

    return await signer.executeMoveCall(tx);
  });

  const results = await Promise.all(promises);
  console.log(`Cast ${results.length} votes`);

  return results;
}
```

### Auto-refresh Results

```typescript
async function watchPollResults(pollId: string, intervalMs: number = 5000) {
  const update = async () => {
    const results = await getPollResults(pollId);
    console.clear();
    console.log('\n=== Live Poll Results ===\n');
    console.log(`Question: ${results.question}\n`);

    results.results.forEach(r => {
      console.log(`${r.option}: ${r.votes} votes (${r.percentage})`);
    });

    console.log(`\nTotal Votes: ${results.totalVotes}`);
    console.log(`Status: ${results.isActive ? 'Active' : 'Closed'}`);
  };

  // Initial update
  await update();

  // Set up interval
  const interval = setInterval(update, intervalMs);

  // Return cleanup function
  return () => clearInterval(interval);
}

// Usage
const stopWatching = await watchPollResults(pollId, 3000);

// Later, to stop watching:
// stopWatching();
```

### Create Multiple Polls

```typescript
async function createMultiplePolls(pollsData: Array<{question: string, options: string[]}>) {
  const promises = pollsData.map(data =>
    createPoll(data.question, data.options)
  );

  const pollIds = await Promise.all(promises);

  console.log('Created polls:', pollIds);
  return pollIds;
}

// Usage
const pollIds = await createMultiplePolls([
  { question: "Favorite color?", options: ["Red", "Blue", "Green"] },
  { question: "Favorite food?", options: ["Pizza", "Sushi", "Tacos"] },
  { question: "Favorite season?", options: ["Summer", "Winter"] },
]);
```

### Analyze Poll Statistics

```typescript
async function analyzePoll(pollId: string) {
  const data = await getPollData(pollId);

  // Find winner
  const maxVotes = Math.max(...data.votes);
  const winnerIndex = data.votes.indexOf(maxVotes);
  const winner = data.options[winnerIndex];

  // Calculate statistics
  const avgVotesPerOption = data.totalVotes / data.options.length;
  const participation = data.totalVotes > 0 ? 100 : 0;

  // Find options with no votes
  const optionsWithNoVotes = data.options.filter((_, i) => data.votes[i] === 0);

  return {
    winner: {
      option: winner,
      votes: maxVotes,
      percentage: ((maxVotes / data.totalVotes) * 100).toFixed(2) + '%',
    },
    statistics: {
      totalVotes: data.totalVotes,
      averageVotesPerOption: avgVotesPerOption.toFixed(2),
      participation: `${participation}%`,
      optionsCount: data.options.length,
      optionsWithNoVotes: optionsWithNoVotes.length,
    },
    status: data.isActive ? 'Active' : 'Closed',
  };
}

// Usage
const analysis = await analyzePoll(pollId);
console.log('Poll Analysis:', JSON.stringify(analysis, null, 2));
```

## Error Handling

```typescript
async function safeVote(pollId: string, optionIndex: number) {
  try {
    await vote(pollId, optionIndex);
    console.log('Vote successful!');
  } catch (error) {
    if (error.message.includes('EInvalidOption')) {
      console.error('Invalid option selected');
    } else if (error.message.includes('EAlreadyVoted')) {
      console.error('You have already voted on this poll');
    } else if (error.message.includes('EPollNotActive')) {
      console.error('This poll is no longer active');
    } else {
      console.error('Unexpected error:', error.message);
    }
  }
}
```

## Testing

```typescript
// Complete test flow
async function testVotingFlow() {
  console.log('1. Creating poll...');
  const pollId = await createPoll("Test poll?", ["Yes", "No"]);
  console.log('Poll created:', pollId);

  console.log('2. Voting...');
  await vote(pollId, 0);
  console.log('Vote cast');

  console.log('3. Getting results...');
  const results = await getPollResults(pollId);
  console.log('Results:', results);

  console.log('4. Closing poll...');
  await closePoll(pollId);
  console.log('Poll closed');

  console.log('Test completed successfully!');
}

// Run test
testVotingFlow().catch(console.error);
```

---

For more examples and up-to-date code, check the [Sui Examples Repository](https://github.com/MystenLabs/sui/tree/main/sui_programmability/examples).
