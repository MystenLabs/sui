package main

import (
	"context"
	"fmt"
	"log"
	"math/big"

	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/core/types"
	"github.com/ethereum/go-ethereum/ethclient"
)

// RPCClient wraps go-ethereum client with utility methods
type RPCClient struct {
	client  *ethclient.Client
	chainID *big.Int
}

// NewRPCClient creates a new RPC client
func NewRPCClient(rpcURL string) (*RPCClient, error) {
	client, err := ethclient.Dial(rpcURL)
	if err != nil {
		return nil, fmt.Errorf("failed to connect to RPC: %w", err)
	}

	ctx := context.Background()
	chainID, err := client.ChainID(ctx)
	if err != nil {
		return nil, fmt.Errorf("failed to get chain ID: %w", err)
	}

	fmt.Printf("✅ Connected to network (Chain ID: %s)\n", chainID.String())

	return &RPCClient{
		client:  client,
		chainID: chainID,
	}, nil
}

// GetBalance returns the ETH balance of an address
func (rc *RPCClient) GetBalance(address common.Address) (*big.Int, error) {
	ctx := context.Background()
	balance, err := rc.client.BalanceAt(ctx, address, nil)
	if err != nil {
		return nil, fmt.Errorf("failed to get balance: %w", err)
	}
	return balance, nil
}

// GetBlockNumber returns the latest block number
func (rc *RPCClient) GetBlockNumber() (uint64, error) {
	ctx := context.Background()
	blockNumber, err := rc.client.BlockNumber(ctx)
	if err != nil {
		return 0, fmt.Errorf("failed to get block number: %w", err)
	}
	return blockNumber, nil
}

// GetBlock returns block information
func (rc *RPCClient) GetBlock(blockNumber *big.Int) (*types.Block, error) {
	ctx := context.Background()
	block, err := rc.client.BlockByNumber(ctx, blockNumber)
	if err != nil {
		return nil, fmt.Errorf("failed to get block: %w", err)
	}
	return block, nil
}

// GetTransaction returns transaction details
func (rc *RPCClient) GetTransaction(txHash common.Hash) (*types.Transaction, bool, error) {
	ctx := context.Background()
	tx, isPending, err := rc.client.TransactionByHash(ctx, txHash)
	if err != nil {
		return nil, false, fmt.Errorf("failed to get transaction: %w", err)
	}
	return tx, isPending, nil
}

// GetTransactionReceipt returns transaction receipt
func (rc *RPCClient) GetTransactionReceipt(txHash common.Hash) (*types.Receipt, error) {
	ctx := context.Background()
	receipt, err := rc.client.TransactionReceipt(ctx, txHash)
	if err != nil {
		return nil, fmt.Errorf("failed to get receipt: %w", err)
	}
	return receipt, nil
}

// GetGasPrice returns current gas price
func (rc *RPCClient) GetGasPrice() (*big.Int, error) {
	ctx := context.Background()
	gasPrice, err := rc.client.SuggestGasPrice(ctx)
	if err != nil {
		return nil, fmt.Errorf("failed to get gas price: %w", err)
	}
	return gasPrice, nil
}

// GetNonce returns the nonce for an address
func (rc *RPCClient) GetNonce(address common.Address) (uint64, error) {
	ctx := context.Background()
	nonce, err := rc.client.PendingNonceAt(ctx, address)
	if err != nil {
		return 0, fmt.Errorf("failed to get nonce: %w", err)
	}
	return nonce, nil
}

// EstimateGas estimates gas for a transaction
func (rc *RPCClient) EstimateGas(msg types.CallMsg) (uint64, error) {
	ctx := context.Background()
	gasLimit, err := rc.client.EstimateGas(ctx, msg)
	if err != nil {
		return 0, fmt.Errorf("failed to estimate gas: %w", err)
	}
	return gasLimit, nil
}

// Close closes the client connection
func (rc *RPCClient) Close() {
	rc.client.Close()
}

// PrintNetworkInfo displays network information
func (rc *RPCClient) PrintNetworkInfo() error {
	blockNumber, err := rc.GetBlockNumber()
	if err != nil {
		return err
	}

	gasPrice, err := rc.GetGasPrice()
	if err != nil {
		return err
	}

	weiPerEth := big.NewInt(1000000000000000000)
	gasPriceGwei := new(big.Int).Div(gasPrice, big.NewInt(1000000000))

	fmt.Println("\n🌐 Network Information")
	fmt.Printf("   Chain ID: %s\n", rc.chainID.String())
	fmt.Printf("   Latest Block: %d\n", blockNumber)
	fmt.Printf("   Gas Price: %s Gwei\n", gasPriceGwei.String())

	return nil
}

func main() {
	// Connect to Ethereum mainnet
	client, err := NewRPCClient("https://eth.llamarpc.com")
	if err != nil {
		log.Fatal(err)
	}
	defer client.Close()

	// Print network info
	if err := client.PrintNetworkInfo(); err != nil {
		log.Fatal(err)
	}

	// Check Vitalik's balance
	fmt.Println("\n💰 Balance Check")
	vitalikAddr := common.HexToAddress("0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045")
	balance, err := client.GetBalance(vitalikAddr)
	if err != nil {
		log.Fatal(err)
	}

	balanceEth := new(big.Float).Quo(
		new(big.Float).SetInt(balance),
		new(big.Float).SetInt(big.NewInt(1000000000000000000)),
	)
	fmt.Printf("   Address: %s\n", vitalikAddr.Hex())
	fmt.Printf("   Balance: %s ETH\n", balanceEth.String())

	// Get latest block
	fmt.Println("\n🔷 Latest Block")
	block, err := client.GetBlock(nil)
	if err != nil {
		log.Fatal(err)
	}
	fmt.Printf("   Number: %d\n", block.Number().Uint64())
	fmt.Printf("   Hash: %s\n", block.Hash().Hex())
	fmt.Printf("   Transactions: %d\n", len(block.Transactions()))
	fmt.Printf("   Timestamp: %d\n", block.Time())
}
