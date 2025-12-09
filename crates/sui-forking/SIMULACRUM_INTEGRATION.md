# Simulacrum Integration for Sui Forking

## Overview
The simulacrum crate provides the core execution engine for Sui Forking. It offers lock-step transaction execution without validators, perfect for our forking needs.

## Key Components in Simulacrum

### 1. Core Structure
- **`Simulacrum<R, Store>`**: Main orchestrator at ../simulacrum/src/lib.rs
- **`SimulatorStore` trait**: Storage interface at ../simulacrum/src/store/mod.rs
- **`InMemoryStore`**: Default implementation at ../simulacrum/src/store/in_mem_store.rs
- **`EpochState`**: Epoch-specific execution state at ../simulacrum/src/epoch_state.rs

### 2. Object Management

The simulacrum stores objects with full versioning support:

```rust
struct InMemoryStore {
    // Latest version of each object
    live_objects: HashMap<ObjectID, SequenceNumber>,
    // All versions of all objects
    objects: HashMap<ObjectID, BTreeMap<SequenceNumber, Object>>,
}
```

## How to Add Downloaded Objects

### Primary Method: `update_objects()`

```rust
fn update_objects(
    &mut self,
    written_objects: BTreeMap<ObjectID, Object>,
    deleted_objects: Vec<(ObjectID, SequenceNumber, ObjectDigest)>,
);
```

This is the main API for injecting objects downloaded from RPC into the local network.

### Integration Approach

## 1. Custom Store Implementation

Create a custom store that wraps InMemoryStore and adds lazy downloading:

```rust
use simulacrum::{SimulatorStore, InMemoryStore};
use sui_types::base_types::{ObjectID, SequenceNumber};
use sui_types::object::Object;

pub struct ForkingStore {
    // Base storage
    base_store: InMemoryStore,
    
    // Download capability
    data_downloader: Arc<DataStore>,
    
    // Fork configuration
    target_checkpoint: CheckpointSequenceNumber,
    network: Network,
}

impl ForkingStore {
    pub fn new(
        genesis: &Genesis,
        checkpoint: CheckpointSequenceNumber,
        network: Network,
        data_downloader: Arc<DataStore>,
    ) -> Self {
        let mut base_store = InMemoryStore::default();
        base_store.init_with_genesis(genesis);
        
        Self {
            base_store,
            data_downloader,
            target_checkpoint: checkpoint,
            network,
        }
    }
    
    /// Pre-load objects into the store
    pub fn inject_objects(&mut self, objects: BTreeMap<ObjectID, Object>) {
        self.base_store.update_objects(objects, vec![]);
    }
}

impl SimulatorStore for ForkingStore {
    fn get_object(&self, id: &ObjectID) -> Option<Object> {
        // First check local cache
        if let Some(obj) = self.base_store.get_object(id) {
            return Some(obj);
        }
        
        // If not found, download from network
        if let Ok(obj) = self.data_downloader.fetch_object_blocking(
            id, 
            self.target_checkpoint
        ) {
            // Insert into local store for future use
            let mut objects = BTreeMap::new();
            objects.insert(*id, obj.clone());
            self.base_store.update_objects(objects, vec![]);
            
            return Some(obj);
        }
        
        None
    }
    
    fn get_object_at_version(
        &self, 
        id: &ObjectID, 
        version: SequenceNumber
    ) -> Option<Object> {
        // Similar pattern with version-specific downloading
        if let Some(obj) = self.base_store.get_object_at_version(id, version) {
            return Some(obj);
        }
        
        // Download specific version if needed
        if let Ok(obj) = self.data_downloader.fetch_object_at_version_blocking(
            id, 
            version,
            self.target_checkpoint
        ) {
            let mut objects = BTreeMap::new();
            objects.insert(*id, obj.clone());
            self.base_store.update_objects(objects, vec![]);
            return Some(obj);
        }
        
        None
    }
    
    // Delegate other methods to base_store
    fn owned_objects(&self, owner: Owner) -> Vec<ObjectID> {
        self.base_store.owned_objects(owner)
    }
    
    fn insert_executed_transaction(&mut self, tx: ExecutedTransaction) {
        self.base_store.insert_executed_transaction(tx)
    }
    
    // ... other delegated methods
}
```

## 2. Initialize Forking Network

```rust
use simulacrum::Simulacrum;
use rand::rngs::OsRng;

pub async fn initialize_forking_network(
    checkpoint: CheckpointSequenceNumber,
    network: Network,
) -> Result<Simulacrum<OsRng, ForkingStore>, ForkingError> {
    // 1. Download checkpoint data
    let data_store = Arc::new(DataStore::new(network));
    let checkpoint_data = data_store.fetch_checkpoint(checkpoint).await?;
    
    // 2. Download essential objects (system objects, packages)
    let mut initial_objects = BTreeMap::new();
    
    // System objects
    let system_state = data_store.fetch_system_state(checkpoint).await?;
    initial_objects.insert(SUI_SYSTEM_STATE_OBJECT_ID, system_state);
    
    // Clock
    let clock = data_store.fetch_clock(checkpoint).await?;
    initial_objects.insert(SUI_CLOCK_OBJECT_ID, clock);
    
    // Framework packages
    for package_id in [MOVE_STDLIB_PACKAGE_ID, SUI_FRAMEWORK_PACKAGE_ID, SUI_SYSTEM_PACKAGE_ID] {
        let package = data_store.fetch_package(package_id, checkpoint).await?;
        initial_objects.insert(package_id, package);
    }
    
    // 3. Create custom store
    let mut store = ForkingStore::new(
        &minimal_genesis(),
        checkpoint,
        network,
        data_store.clone(),
    );
    
    // 4. Inject initial objects
    store.inject_objects(initial_objects);
    
    // 5. Create network config
    let config = create_network_config_for_checkpoint(checkpoint, network)?;
    
    // 6. Create simulacrum with custom store
    Ok(Simulacrum::new_with_network_config_store(&config, OsRng, store))
}
```

## 3. Transaction Execution with Pre-loading

```rust
impl ForkingCoordinator {
    pub async fn execute_transaction(
        &mut self,
        tx: Transaction,
    ) -> Result<TransactionEffects, ForkingError> {
        // 1. Analyze transaction dependencies
        let input_objects = tx.input_objects()?;
        let receiving_objects = tx.receiving_objects();
        
        // 2. Pre-download all required objects
        let mut objects_to_download = Vec::new();
        objects_to_download.extend(input_objects);
        objects_to_download.extend(receiving_objects);
        
        // Batch download for efficiency
        let downloaded_objects = self.data_store
            .fetch_objects_batch(&objects_to_download, self.checkpoint)
            .await?;
        
        // 3. Inject into store
        self.simulacrum.store_mut().inject_objects(downloaded_objects);
        
        // 4. Execute transaction - all objects now available
        let (effects, _) = self.simulacrum.execute_transaction(tx)?;
        
        // 5. Create checkpoint if needed
        if self.auto_checkpoint {
            self.simulacrum.create_checkpoint();
        }
        
        Ok(effects)
    }
}
```

## 4. Manual State Control

The simulacrum provides methods for manual control:

```rust
// Advance checkpoint
pub fn advance_checkpoint(&mut self) -> CheckpointSummary {
    self.simulacrum.create_checkpoint()
}

// Advance clock by duration
pub fn advance_clock(&mut self, duration: Duration) {
    self.simulacrum.advance_clock(duration);
}

// Advance to next epoch
pub fn advance_epoch(&mut self) {
    self.simulacrum.advance_epoch();
}

// Get current state
pub fn get_status(&self) -> ForkingStatus {
    ForkingStatus {
        checkpoint: self.simulacrum.store().get_highest_checkpoint_sequence_number(),
        epoch: self.simulacrum.epoch(),
        timestamp: self.simulacrum.clock_instant_for_testing(),
        transaction_count: self.simulacrum.store().transaction_count(),
    }
}
```

## Key Benefits of This Approach

1. **Lazy Loading**: Objects are downloaded only when needed
2. **Caching**: Downloaded objects are cached in the store
3. **Compatibility**: Full compatibility with simulacrum's execution engine
4. **Deterministic**: Lock-step execution ensures reproducible results
5. **Flexible**: Can pre-load objects or download on-demand

## Implementation Notes

### Object Downloading Strategy

1. **Essential Objects** (download at initialization):
   - System state object
   - Clock object
   - Framework packages (Move stdlib, Sui framework, Sui system)

2. **On-Demand Objects** (download when needed):
   - User objects referenced in transactions
   - Package dependencies
   - Shared objects

3. **Batch Optimization**:
   - Analyze transactions to identify all required objects
   - Download in batches to reduce RPC calls
   - Cache aggressively to avoid re-downloading

### Error Handling

```rust
pub enum ObjectFetchError {
    NetworkError(String),
    ObjectNotFound(ObjectID),
    InvalidCheckpoint,
    DeserializationError(String),
}

impl ForkingStore {
    fn handle_fetch_error(&self, id: &ObjectID, err: ObjectFetchError) -> Option<Object> {
        match err {
            ObjectFetchError::ObjectNotFound(_) => {
                // Object might not exist at this checkpoint
                None
            }
            ObjectFetchError::NetworkError(_) => {
                // Retry logic could go here
                None
            }
            _ => None
        }
    }
}
```

## Testing the Integration

```rust
#[tokio::test]
async fn test_forking_with_simulacrum() {
    // Initialize forking at checkpoint 100
    let mut fork = initialize_forking_network(100, Network::Testnet).await.unwrap();
    
    // Create a test transaction
    let recipient = SuiAddress::random_for_testing_only();
    let (tx, _) = fork.transfer_txn(recipient);
    
    // Execute - should download required objects automatically
    let effects = fork.execute_transaction(tx).unwrap().0;
    assert!(effects.status().is_ok());
    
    // Verify checkpoint was created
    let checkpoint = fork.create_checkpoint();
    assert_eq!(checkpoint.sequence_number, 101);
}
```

This integration provides a clean separation between the simulacrum's execution engine and the forking tool's data management, while maintaining full compatibility with Sui's transaction model.