# Enoki Gas Pool
This is a high-level summary of how Enoki Gas Pool works.

## Architecture
There are 5 components in the Gas Pool:
### Storage
The storage layer stores the global gas pool information.
Any implementation must support the following two API functions:
- `fn reserve_gas_coins(&self, sponsor_address: SuiAddress, target_budget: u64) -> Vec<GasCoin>;`
- `fn update_gas_coins(&self, sponsor_address: SuiAddress, released_gas_coins: Vec<GasCoin>, deleted_gas_coins: Vec<ObjectID>);
Where `GasCoin` is defined as:
```rust
pub struct GasCoin {
    pub object_ref: ObjectRef,
    pub balance: u64,
}
```
`reserve_gas_coins` is able to take off potentially more than 1 gas coins from an internal queue atomically until the total accumulated balance reaches `target_budget`, or give up if it cannot find enough coins to meet the requirement even after searching for N coins (N can be implementation-specific, but must be less than 256 since it's our gas payment size cap).
This also takes advantage of the fact that gas smashing is able to merge gas coins during transaction execution. So if we have coins that have low balance, by returning multiple of them we are doing automatic dust collection.
It supports multiple sponsors, and each sponsor has its own queue of gas coins.
Caller must specify which sponsor address to use.

The storage layer is expected to keep track the list of coins that are currently reserved. This is important because although rare, the gas station server may crash after reserving a coin and never come back up. This means over time some coins will be reserved but never released. If we have a table in the storage that keeps track of reserved coins along with their reservation time, we can then run a GC process from time to time to recollect gas coins that were reserved a long time ago.

`update_gas_coins` is able to add back gas coins to the gas pool, or mark certain gas coin permanently deleted. It can be called either when we are returning gas coins after using them, or adding new coins to the pool.
Note that the released gas coins are the critical part to ensure protocol correctness, while the deleted_gas_coins is used to remove coins from the currently reserved coin list, so that we don't have to keep track of them forever.

### Gas Pool Core
The Gas Pool Core implements the core gas pool logic that is able to process RPC requests and communicate with the Storage layer.
It has the following features:
1. Upon requesting gas coins, it's able to obtain gas coins from the storage layer, remember it in memory, and return them to the caller.
2. It's able to automatically release reserved gas coins back to the storage after the requested duration expires.
3. Caller can then follow up with a transaction execution request that uses a previously reserved coin list, and the gas station core will drive the execution of the transaction, automatically release the coins back to the storage layer after the transaction is executed.

### Gas Pool Initializer
A Gas Pool Initializer is able to initialize the global gas pool.
When we are setting up the gas pool, we will need to run the initialization exactly once. It is able to look at all the SUI coins currently owned by the sponsor address, and split them into gas coins with a specified target balance.
This is done by splitting coins into smaller coins in parallel to minimize the amount of time to split all.

### RPC Server
An HTTP server is implemented to take the following 3 requests:
- GET("/"): Checks the health of the server
- POST("/v1/reserve_gas"): Takes a [`ReserveGasRequest`](src/rpc/rpc_types.rs) parameter in JSON form, and returns [`ReserveGasResponse`](src/rpc/rpc_types.rs).
- POST("/v1/execute_tx"): Takes a [`ExecuteTxRequest`](src/rpc/rpc_types.rs) parameter in JSON form, and returns [`ExecuteTxResponse`](src/rpc/rpc_types.rs).

```rust
pub struct ReserveGasRequest {
    /// Desired gas budget. The response will contain gas coins that have total balance >= gas_budget.
    pub gas_budget: u64,
    /// If request_sponsor is None, the station will pick one automatically.
    pub request_sponsor: Option<SuiAddress>,
    /// The reserved gas coins will be released back to the pool after this duration expires.
    pub reserve_duration_secs: u64,
}

pub struct ReserveGasResponse {
    /// The sponsor address and the list of gas coins that are reserved.
    pub gas_coins: Option<(SuiAddress, Vec<SuiObjectRef>)>,
    pub error: Option<String>,
}

pub struct ExecuteTxRequest {
    /// BCS serialized transaction data bytes without its type tag, as base-64 encoded string.
    pub tx_bytes: Base64,
    /// User signature (`flag || signature || pubkey` bytes, as base-64 encoded string). Signature is committed to the intent message of the transaction data, as base-64 encoded string.
    pub user_sig: Base64,
}

pub struct ExecuteTxResponse {
    pub effects: Option<SuiTransactionBlockEffects>,
    pub error: Option<String>,
}
```

### CLI
The `sui-gas-staiton` binary currently supports 3 commands:
1. `init`: This invokes the Production Initializer and initialize the global gas pool using real network data.
2. `start-storage-server`: This starts a storage server that contains the RPC server and connection to the Storage layer. There should be only one storage server instance running.
3. `start-station-server`: This starts a gas pool instance that contains the RPC server, Gas Station Core, and connection to the Storage layer.
4. `benchmark`: This starts a stress benchmark that continuously send gas reservation request to the gas station server, and measures number of requests processed per second. Each reservation expires automatically after 1 second so the unused gas are put back to the pool.

### Deployment
Step 1:
Start a storage server instance somewhere:
```bash
GAS_STATION_AUTH=<some-secret> sui-gas-station start-storage-server --db-path <gas-pool-db-path> --ip <ip> --rpc-port <port>
```
where "some-secret" is a secret that would be shared among the storage server, station servers and Enoki servers. They need to be kept internal such that external servers cannot make meaningful request to these servers even if they discover their RPC service.
The "ip" and "port" are used to start a RPC server for the storage layer.

Step 2:
Put up a config file.
First of all one can run
```bash
sui-gas-station generate-sample-config --config-path config.yaml
```
to generate a sample config file.
Then one can edit the config file to fill in the fields.
The keypair field is the serialized SuiKeyPair that can be found in a typical .keystore file.

Step 3:
Initialize the gas pool:
```bash
GAS_STATION_AUTH=<some-secret> sui-gas-station init --config-path ./config.yaml --target-init-coin-balance <initial-per-coin-balance>
```

Step 4:
Start a gas pool server instance somewhere:
```bash
GAS_STATION_AUTH=<some-secret> sui-gas-station start-station-server --config-path ./config.yaml
```
It's safe to start multiple gas station server instances in the network, as long as they all share the same storage server instance.

## TODOs
1. Add latency metrics
2. Add ability to add more coins to the pool latter 3
3. Think about load balancing and how to pair reservation and release.
4. Move some of the commands out to separate binaries. Keep the main binary for the two servers only.
5. Fix storage metrics: number of available coins require iterating through the db at startup.