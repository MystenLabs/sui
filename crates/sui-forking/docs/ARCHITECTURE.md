# Sui Forking System Architecture

## Overview
The Sui Forking tool is designed as a modular system that enables developers to fork the Sui blockchain state at any checkpoint and run a local network for testing and development.

## Core Design Principles

1. **Lazy Loading**: Objects are fetched from the network only when needed, not all at once
2. **Lock-step Execution**: Transactions execute sequentially without validators
3. **State Consistency**: Maintain checkpoint/epoch/clock consistency with forked state
4. **Caching Strategy**: Multi-level caching (memory + disk) for performance
5. **Compatibility**: Full Sui RPC API compatibility for seamless integration

## System Components

### 1. CLI Layer
- **Purpose**: User interface for all forking operations
- **Commands**:
  - `sui fork`: Start forking network
  - `sui fork advance-checkpoint/clock/epoch`: Manual state advancement
  - `sui fork status`: Query current state

### 2. Coordinator Module
Central orchestrator managing all forking operations.

#### Components:
- **Coordinator**: Main orchestration logic
- **Transaction Interceptor**: Analyzes transactions to identify required objects
- **State Manager**: Tracks and synchronizes local state with fork point

#### Responsibilities:
- Initialize local network at specified checkpoint
- Intercept and analyze transaction requests
- Coordinate between data fetching and execution
- Manage state transitions

### 3. Data Store Module
Handles all data fetching and caching operations.

#### Components:
- **Object Store**: Main storage interface with LRU eviction
- **GraphQL Client**: Network communication with batching and retry
- **Cache Manager**: Multi-level cache management

#### Features:
- On-demand object fetching
- Intelligent prefetching based on transaction analysis
- Persistent disk cache for frequently used objects
- Memory cache for hot objects
- Batch GraphQL requests for efficiency

### 4. Network Module
Manages the local forked network execution.

#### Components:
- **Forking Network**: Core execution engine (extends simulacrum)
- **Checkpoint Manager**: Handles checkpoint creation and advancement
- **Clock Manager**: Manages blockchain time
- **Epoch Manager**: Handles epoch transitions

#### Characteristics:
- No validators (lock-step mode)
- Sequential transaction execution
- Deterministic checkpoint creation
- Manual state advancement capabilities

### 5. RPC Server
Provides network interface for external clients.

#### Components:
- **JSON-RPC Server**: Sui-compatible RPC API
- **WebSocket Server**: Real-time event streaming

#### Features:
- Standard Sui RPC endpoints
- Transaction submission
- State queries
- Event subscriptions

## Data Flow

### Transaction Execution Flow
1. Client submits transaction via RPC/CLI
2. Coordinator receives and forwards to Transaction Interceptor
3. Transaction Interceptor analyzes dependencies:
   - Input objects
   - Package dependencies
   - System objects
4. Data Store checks cache hierarchy:
   - Memory cache (hot objects)
   - Disk cache (persistent)
   - Network fetch if not cached
5. GraphQL Client fetches missing objects from network
6. Objects provided to Forking Network
7. Transaction executes in lock-step mode
8. New checkpoint created
9. Result returned to client

### State Initialization Flow
1. User specifies checkpoint and network
2. Coordinator initiates fork at checkpoint
3. Data Store fetches initial state:
   - System objects
   - Genesis objects
   - Framework packages
4. Forking Network initialized with state
5. RPC Server starts listening
6. System ready for transactions

## Storage Architecture

### Cache Hierarchy
1. **L1 - Memory Cache**
   - Hot objects (recently/frequently accessed)
   - Limited size (configurable)
   - LRU eviction policy

2. **L2 - Disk Cache**
   - Persistent storage (RocksDB/SQLite)
   - Larger capacity
   - Indexed by object ID and version
   - Optional TTL for cache invalidation

3. **L3 - Network**
   - GraphQL RPC to actual Sui network
   - Batch fetching for efficiency
   - Retry with exponential backoff

### Database Schema
```
Objects Table:
- object_id: PRIMARY KEY
- version: INTEGER
- checkpoint: INTEGER
- data: BLOB
- accessed_at: TIMESTAMP
- cached_at: TIMESTAMP

Checkpoints Table:
- checkpoint_number: PRIMARY KEY
- epoch: INTEGER
- timestamp: INTEGER
- state_root: BLOB

Transactions Table:
- tx_digest: PRIMARY KEY
- checkpoint: INTEGER
- effects: BLOB
- timestamp: INTEGER
```

## Configuration

### Configuration File Structure
```yaml
network:
  type: testnet|mainnet|devnet
  graphql_endpoint: <optional override>
  
forking:
  checkpoint: 100
  port: 8123
  
cache:
  memory_size_mb: 512
  disk_path: ~/.sui-fork/cache
  disk_size_gb: 10
  ttl_hours: 24
  
advanced:
  batch_size: 100
  retry_attempts: 3
  timeout_ms: 30000
```

## Error Handling

### Error Types
1. **Network Errors**: Connection failures, timeouts
2. **Cache Errors**: Corruption, disk full
3. **Execution Errors**: Invalid transactions, state inconsistency
4. **Configuration Errors**: Invalid settings, missing data

### Recovery Strategies
- Automatic retry with exponential backoff for network errors
- Cache invalidation and rebuild for corruption
- Transaction rollback for execution errors
- Graceful degradation when possible

## Performance Considerations

### Optimizations
1. **Batch Fetching**: Group multiple object requests
2. **Predictive Prefetching**: Analyze transaction patterns
3. **Parallel Downloads**: Concurrent GraphQL requests
4. **Compression**: Compress cached objects
5. **Index Optimization**: Efficient object lookups

### Bottlenecks and Mitigations
- **Network Latency**: Aggressive caching, batch requests
- **Memory Constraints**: Tiered caching, configurable limits
- **Disk I/O**: Async writes, write batching
- **Transaction Analysis**: Caching of analysis results

## Security Considerations

1. **Data Integrity**: Verify object hashes
2. **Cache Poisoning**: Validate data from network
3. **Resource Limits**: Prevent DoS through limits
4. **Access Control**: Local-only by default

## Future Enhancements

1. **Cluster Mode**: Multiple forking nodes for load distribution
2. **State Snapshots**: Save/restore fork states
3. **Time Travel**: Fork from multiple checkpoints simultaneously
4. **Smart Prefetching**: ML-based prediction of required objects
5. **Plugin System**: Extensible interceptors and analyzers
