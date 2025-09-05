# Description

A basic faucet that is intended to be run locally. It does not support heavy loads as it's designed to work locally in a simple and basic way: request a coin, get a coin.

# Quick start

**Prerequisites**

You need to have a key with sufficient SUI.

**Starting the faucet as a standalone service**

When starting the faucet as a standalone service, you will need to ensure that the active key in `~/.sui/sui_config/sui.keystore` has enough SUI.

**Starting as part of a local network**

If you're starting this as part of a local network by using `sui start`, then it should automatically find the coins in the configured wallet. If `--force-regenesis` is passed, the wallet
will be funded when the network starts and should have plenty of SUI to get you started.


# Response
The faucet will respond with a JSON object containing the following fields:
```json
{
 "status":"Success",
 "coins_sent": {
   "amount":5500000000,
   "id":"0xac8b8afbc9074465bf799d0f590e17176b7a05514b9434b338e38f49be14d574",
   "transferTxDigest":"DSHGocWx57BtYDE5Xv4AefeRBBMizPb3LTqRMegx14Ym"
  }
}
```

In case of error, the response will contain the following fields:
```json
{
 "status":{
   "Failure": {
     "ErrorType": "message"
	 }
 },
 "coins_sent": null
}
```

where `ErrorType` is `Internal`.


The response status codes are:
`Success` --> `200 OK`
`Internal` --> `500` error code
