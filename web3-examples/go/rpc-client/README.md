# Ethereum RPC Client (Go)

High-performance RPC client for Ethereum using go-ethereum.

## Features

- ✅ Connect to any Ethereum RPC endpoint
- ✅ Query balances and account info
- ✅ Get blocks and transactions
- ✅ Fetch transaction receipts
- ✅ Gas price estimation
- ✅ Nonce management
- ✅ Type-safe API

## Setup

```bash
go mod download
```

## Build

```bash
go build -o rpc-client client.go
```

## Run

```bash
go run client.go
```

## Usage

### Connect to Network

```go
client, err := NewRPCClient("https://eth.llamarpc.com")
if err != nil {
    log.Fatal(err)
}
defer client.Close()
```

### Get Balance

```go
address := common.HexToAddress("0x...")
balance, err := client.GetBalance(address)
if err != nil {
    log.Fatal(err)
}
```

### Get Latest Block

```go
blockNumber, err := client.GetBlockNumber()
block, err := client.GetBlock(nil) // nil = latest
```

### Get Transaction

```go
txHash := common.HexToHash("0x...")
tx, isPending, err := client.GetTransaction(txHash)
```

### Get Receipt

```go
receipt, err := client.GetTransactionReceipt(txHash)
if receipt.Status == 1 {
    fmt.Println("Transaction successful")
}
```

### Get Gas Price

```go
gasPrice, err := client.GetGasPrice()
gasPriceGwei := new(big.Int).Div(gasPrice, big.NewInt(1e9))
```

### Estimate Gas

```go
msg := types.CallMsg{
    From: fromAddr,
    To:   &toAddr,
    Value: value,
    Data: data,
}
gasLimit, err := client.EstimateGas(msg)
```

## Example Output

```
✅ Connected to network (Chain ID: 1)

🌐 Network Information
   Chain ID: 1
   Latest Block: 18500000
   Gas Price: 25 Gwei

💰 Balance Check
   Address: 0xd8dA...6045
   Balance: 123.456 ETH

🔷 Latest Block
   Number: 18500000
   Hash: 0x1234...
   Transactions: 150
   Timestamp: 1693838400
```

## API Reference

### RPCClient Methods

- `NewRPCClient(rpcURL)` - Create new client
- `GetBalance(address)` - Get ETH balance
- `GetBlockNumber()` - Get latest block number
- `GetBlock(blockNumber)` - Get block info
- `GetTransaction(txHash)` - Get transaction
- `GetTransactionReceipt(txHash)` - Get receipt
- `GetGasPrice()` - Get current gas price
- `GetNonce(address)` - Get account nonce
- `EstimateGas(msg)` - Estimate gas limit
- `PrintNetworkInfo()` - Display network info
- `Close()` - Close connection

## Supported Networks

Works with any Ethereum-compatible RPC:

- Ethereum Mainnet
- Polygon
- BSC (Binance Smart Chain)
- Arbitrum
- Optimism
- Avalanche C-Chain
- Local nodes (Hardhat, Ganache)

## Common RPC Endpoints

```go
// Ethereum Mainnet
"https://eth.llamarpc.com"
"https://rpc.ankr.com/eth"

// Polygon
"https://polygon-rpc.com"

// BSC
"https://bsc-dataseed.binance.org"

// Arbitrum
"https://arb1.arbitrum.io/rpc"
```

## Error Handling

```go
balance, err := client.GetBalance(address)
if err != nil {
    log.Printf("Error getting balance: %v", err)
    return
}
```

## Performance Tips

1. Reuse client connections
2. Use batch requests for multiple queries
3. Cache frequently accessed data
4. Handle rate limiting

## Dependencies

- `go-ethereum` v1.13.5

## Resources

- [Go Ethereum Documentation](https://geth.ethereum.org/docs/dapp/native)
- [Ethereum JSON-RPC](https://ethereum.org/en/developers/docs/apis/json-rpc/)
