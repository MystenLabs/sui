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

This will print a list of object summaries owned by the address `"0x10b5d7b81c796c807a73d1af4b38e8b519b86106"`:

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

You can verify the result with the [Sui Explorer](https://explorer.devnet.sui.io/) if you are using the Sui Devnet Gateway.

### Example 2 - Subscribe to JSON-RPC Real-Time Events

Subscribe to a real-time event stream generated from Move or from the Sui network. More information [here](https://docs.sui.io/devnet/build/pubsub)
```go
package main

import (
	"log"
	"net/url"
	"os"
	"os/signal"
	"github.com/gorilla/websocket"
)

func writeInChan(channel chan string, c *websocket.Conn) {
	//edit req var for change method
	req := `{"jsonrpc":"2.0", "id": 1, "method": "sui_subscribeEvent", "params": [{"All":[]}]}`
	log.Print("Sent: ", req)
	err := c.WriteMessage(websocket.TextMessage, []byte(req))
	if err != nil {
		log.Println("write:", err)
		return
	}

	for {
		_, message, err := c.ReadMessage()
		if err != nil {
			log.Println("read:", err)
			return
		}
		channel <- string(message)
	}
}

func main() {
	interrupt := make(chan os.Signal, 1)
	income := make(chan string)

	signal.Notify(interrupt, os.Interrupt)
	u := url.URL{Scheme: "ws", Host: "136.243.36.109:9001"}
	log.Printf("connecting to %s", u.String())
	c, resp, err := websocket.DefaultDialer.Dial(u.String(), nil)

	if err != nil {
		log.Printf("handshake failed with status %d", resp.StatusCode)
		log.Fatal("dial:", err)
	}
	//When the program closes close the connection
	defer c.Close()

	go writeInChan(income, c)

	for {
		select {
		case msg := <-income:
			log.Println(msg)

		case <-interrupt:
			log.Println("interrupt")
			err := c.WriteMessage(websocket.CloseMessage, websocket.FormatCloseMessage(websocket.CloseNormalClosure, ""))
			if err != nil {
				log.Println("write close:", err)
				return
			}
			return
		}
	}
}
```
