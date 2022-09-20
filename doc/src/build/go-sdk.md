---
title: Interact with Sui over Golang SDK
---

## Overview
The [Sui SDK](https://github.com/MystenLabs/sui/tree/main/crates/sui-sdk) is a collection of Golang language JSON-RPC wrapper and crypto utilities you can use to interact with the [Sui Devnet Gateway](../build/devnet.md) and [Sui Full Node](fullnode.md).

The [`SuiClient`](cli-client.md) can be used to create an HTTP or a WebSocket client (`SuiClient::new_rpc_client`).  
See our [JSON-RPC](json-rpc.md#sui-json-rpc-methods) doc for the list of available methods.

## References

Find the `rustdoc` output for key Sui projects at:

* Sui blockchain - https://mystenlabs.github.io/sui/
* Narwhal and Bullshark consensus engine - https://mystenlabs.github.io/narwhal/
* Mysten Labs infrastructure - https://mystenlabs.github.io/mysten-infra/

## Examples

### Example 1 - Get all objects owned by an address

```go
package main

import (
	"fmt"
	"io/ioutil"
	"net/http"
	"strings"
)

func main() {

	payload := strings.NewReader(`{
    	"jsonrpc": "2.0", 
		"id": 1, 
		"method": "sui_getObjectsOwnedByAddress", 
		"params": ["0x10b5d7b81c796c807a73d1af4b38e8b519b86106"]
	}`)

	client := &http.Client{}
	req, err := http.NewRequest("POST", "https://gateway.devnet.sui.io:443", payload)

	if err != nil {
		fmt.Println(err)
		return
	}
	req.Header.Add("Content-Type", "application/json")

	res, err := client.Do(req)
	if err != nil {
		fmt.Println(err)
		return
	}
	defer res.Body.Close()

	body, err := ioutil.ReadAll(res.Body)
	if err != nil {
		fmt.Println(err)
		return
	}
	fmt.Println(string(body))
}
```
