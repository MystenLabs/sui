# RPC Client Examples

## Quick Start

```go
package main

import (
    "fmt"
    "log"
)

func main() {
    // Connect to Ethereum
    client, err := NewRPCClient("https://eth.llamarpc.com")
    if err != nil {
        log.Fatal(err)
    }
    defer client.Close()

    // Get latest block
    blockNum, _ := client.GetBlockNumber()
    fmt.Printf("Latest block: %d\n", blockNum)
}
```

## More Examples

See client.go for complete implementation.
