package main

import (
	"fmt"
	"io/ioutil"
	"net/http"
	"strings"
)

func main() {

	url := "https://gateway.devnet.sui.io:443"
	method := "POST"

	payload := strings.NewReader(`{
    "jsonrpc": "2.0", "id": 1, "method": "sui_getObjectsOwnedByAddress", "params": ["0x10b5d7b81c796c807a73d1af4b38e8b519b86106"]}
	`)

	client := &http.Client{}
	req, err := http.NewRequest(method, url, payload)

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
