---
title: Sui Events API
---

Sui [Full nodes](fullnode.md) support publish / subscribe using [JSON-RPC](json-rpc.md) notifications via the WebSocket
API. You can use this service with Sui client to filter and subscribe to a real-time event stream generated from Move or
from the Sui network.

The client provides an [event filter](#event-filters) to limit the scope of events. Sui returns a notification with the
event data and subscription ID for each event that matches the filter.

## Move event

Move calls emit Move events. You can [define custom events](https://examples.sui.io/basics/events.html) in Move
contracts.

### Attributes

Move event attributes:

* `packageId`
* `transactionModule`
* `sender`
* `type`
* `parsedJson`
* `bcs`

### Example Move event

```json
{
  "id": {
    "txDigest": "7GV246XCe71e7ssFVV17k2GvcPypQ5ES2DBLtucW8r3b",
    "eventSeq": "0"
  },
  "packageId": "<PACKAGE-ID>",
  "transactionModule": "devnet_nft",
  "sender": "0xce1bf7db5ad6b04bfc305c049f08b80d5ccdc5d6d65d2ded13e95516d1e9fa22",
  "type": "<PACKAGE-ID>::devnet_nft::MintNFTEvent",
  "parsedJson": {
    "creator": "0xce1bf7db5ad6b04bfc305c049f08b80d5ccdc5d6d65d2ded13e95516d1e9fa22",
    "name": "test2",
    "object_id": "0x07d16da80bd9b9859c5aa2b8656f8f4189cf57291439de61be13d0c75b0802c8"
  },
  "bcs": "Lvj1v5oebk4YsieXhwMBKP1m1vjwuLQgYry9ZAYnWB2hYEdikvji7aSs2xQideVwqj6mrXsLuKk5jY2FJWC2X4VTW49PwRT",
  "timestampMs": "1685959791871"
}
```

## Sui event query filter

You can use the `EventFilter` object to query a Sui node and retrieve events that match query criteria.

| Query       | Description                                               | JSON-RPC Parameter Example                                                                          |
|-------------|-----------------------------------------------------------|-----------------------------------------------------------------------------------------------------|
| All         | All events                                                | {"All"}                                                                                             |
| Transaction | Events emitted from the specified transaction.            | {"Transaction":"DGUe2TXiJdN3FI6MH1FwghYbiHw+NKu8Nh579zdFtUk="}                                      |
| MoveModule  | Events emitted from the specified Move module             | {"MoveModule":{"package":"<PACKAGE-ID>", "module":"nft"}}                                           |
| MoveEvent   | Move struct name of the event                             | {"MoveEvent":"<PACKAGE-ID>::nft::MintNFTEvent"}                                                     |
| EventType   | Type of event described in [Events](#event-types) section | {"EventType": "NewObject"}                                                                          |
| Sender      | Query by sender address                                   | {"Sender":"0x008e9c621f4fdb210b873aab59a1e5bf32ddb1d33ee85eb069b348c234465106"}                     |
| Object      | Return events associated with the given object            | {"Object":"0x727b37454ab13d5c1dbb22e8741bff72b145d1e660f71b275c01f24e7860e5e5"}                     |
| TimeRange   | Return events emitted in [start_time, end_time] interval  | {"TimeRange":{"startTime":1669039504014, "endTime":1669039604014}}                                  |

## Pagination

The Event Query API provides cursor-based pagination to make it easier to work with large result sets. You can provide
a `cursor` parameter in paginated query to indicate the starting position of the query. The query returns the number of
results specified by `limit`, and returns the `next_cursor` value when there are additional results. The maximum `limit`
is 1000 per query.

The following examples demonstrate how to create queries that use pagination for the results.

### 1. Get all events an nft module emits, in descending time order

**Request**

```shell
curl --location 'http://0.0.0.0:9000' \
--header 'Content-Type: application/json' \
--data '{
    "jsonrpc": "2.0",
    "id": 1,
    "method": "suix_queryEvents",
    "params": {
        "query": {
            "MoveModule":{"package":"<PACKAGE-ID>", "module":"devnet_nft"}
        },
        "descending_order": true
    }
}'
```

**Response**

```json
{
  "jsonrpc": "2.0",
  "result": {
    "data": [
      {
        "id": {
          "txDigest": "ENmjG42TE4GyqYb1fGNwJe7oxBbbXWCdNfRiQhCNLBJQ",
          "eventSeq": "0"
        },
        "packageId": "<PACKAGE-ID>",
        "transactionModule": "devnet_nft",
        "sender": "0xce1bf7db5ad6b04bfc305c049f08b80d5ccdc5d6d65d2ded13e95516d1e9fa22",
        "type": "<PACKAGE-ID>::devnet_nft::MintNFTEvent",
        "parsedJson": {
          "creator": "0xce1bf7db5ad6b04bfc305c049f08b80d5ccdc5d6d65d2ded13e95516d1e9fa22",
          "name": "test3",
          "object_id": "0xe9d44ad5eaaba6367f09ebe21a72876a6d6cb471275a3e10411a69d17d30008a"
        },
        "bcs": "BGyKDxdW5TcHksJ9nJac9pWJyTZvUmAoiDtezxe7VjfEZQVDfoXv9cAvSCXgPmUY7bpmcu11ue9xijCstUUUmgdZtp4SG7L6",
        "timestampMs": "1685959930934"
      },
      {
        "id": {
          "txDigest": "7GV246XCe71e7ssFVV17k2GvcPypQ5ES2DBLtucW8r3b",
          "eventSeq": "0"
        },
        "packageId": "<PACKAGE-ID>",
        "transactionModule": "devnet_nft",
        "sender": "0xce1bf7db5ad6b04bfc305c049f08b80d5ccdc5d6d65d2ded13e95516d1e9fa22",
        "type": "<PACKAGE-ID>::devnet_nft::MintNFTEvent",
        "parsedJson": {
          "creator": "0xce1bf7db5ad6b04bfc305c049f08b80d5ccdc5d6d65d2ded13e95516d1e9fa22",
          "name": "test2",
          "object_id": "0x07d16da80bd9b9859c5aa2b8656f8f4189cf57291439de61be13d0c75b0802c8"
        },
        "bcs": "Lvj1v5oebk4YsieXhwMBKP1m1vjwuLQgYry9ZAYnWB2hYEdikvji7aSs2xQideVwqj6mrXsLuKk5jY2FJWC2X4VTW49PwRT",
        "timestampMs": "1685959791871"
      },
      {
        "id": {
          "txDigest": "DiPyYRqS7VXYDpGWWy8P7bx1Kmd1TioD4PtZVj9RvWsT",
          "eventSeq": "0"
        },
        "packageId": "<PACKAGE-ID>",
        "transactionModule": "devnet_nft",
        "sender": "0xce1bf7db5ad6b04bfc305c049f08b80d5ccdc5d6d65d2ded13e95516d1e9fa22",
        "type": "<PACKAGE-ID>::devnet_nft::MintNFTEvent",
        "parsedJson": {
          "creator": "0xce1bf7db5ad6b04bfc305c049f08b80d5ccdc5d6d65d2ded13e95516d1e9fa22",
          "name": "test",
          "object_id": "0x433a4bb160563d0931074e5b643840576f84e4ccfaedbda4f0461a7a40da70f7"
        },
        "bcs": "fpbcEYmKNE7cYi44U217hRRKTHsSMwJYvVHx8rGMSJmd2GxmVhwUcc8UimrWEvfyboAocsNX5FMtR8brUnRCPzaDgXk3pF",
        "timestampMs": "1685959781654"
      }
    ],
    "nextCursor": {
      "txDigest": "DiPyYRqS7VXYDpGWWy8P7bx1Kmd1TioD4PtZVj9RvWsT",
      "eventSeq": "0"
    },
    "hasNextPage": false
  },
  "id": 1
}
```

### 2. Get all MintNFTEvent events

**Request**

```shell
curl --location 'http://0.0.0.0:9000' \
--header 'Content-Type: application/json' \
--data '{
    "jsonrpc": "2.0",
    "id": 1,
    "method": "suix_queryEvents",
    "params": {
        "query": {
            "MoveEventType": "<PACKAGE-ID>::devnet_nft::MintNFTEvent"
        }
    }
}'
```

**Response**

```json
{
  "jsonrpc": "2.0",
  "result": {
    "data": [
      {
        "id": {
          "txDigest": "DiPyYRqS7VXYDpGWWy8P7bx1Kmd1TioD4PtZVj9RvWsT",
          "eventSeq": "0"
        },
        "packageId": "<PACKAGE-ID>",
        "transactionModule": "devnet_nft",
        "sender": "0xce1bf7db5ad6b04bfc305c049f08b80d5ccdc5d6d65d2ded13e95516d1e9fa22",
        "type": "<PACKAGE-ID>::devnet_nft::MintNFTEvent",
        "parsedJson": {
          "creator": "0xce1bf7db5ad6b04bfc305c049f08b80d5ccdc5d6d65d2ded13e95516d1e9fa22",
          "name": "test",
          "object_id": "0x433a4bb160563d0931074e5b643840576f84e4ccfaedbda4f0461a7a40da70f7"
        },
        "bcs": "fpbcEYmKNE7cYi44U217hRRKTHsSMwJYvVHx8rGMSJmd2GxmVhwUcc8UimrWEvfyboAocsNX5FMtR8brUnRCPzaDgXk3pF",
        "timestampMs": "1685959781654"
      },
      {
        "id": {
          "txDigest": "7GV246XCe71e7ssFVV17k2GvcPypQ5ES2DBLtucW8r3b",
          "eventSeq": "0"
        },
        "packageId": "<PACKAGE-ID>",
        "transactionModule": "devnet_nft",
        "sender": "0xce1bf7db5ad6b04bfc305c049f08b80d5ccdc5d6d65d2ded13e95516d1e9fa22",
        "type": "<PACKAGE-ID>::devnet_nft::MintNFTEvent",
        "parsedJson": {
          "creator": "0xce1bf7db5ad6b04bfc305c049f08b80d5ccdc5d6d65d2ded13e95516d1e9fa22",
          "name": "test2",
          "object_id": "0x07d16da80bd9b9859c5aa2b8656f8f4189cf57291439de61be13d0c75b0802c8"
        },
        "bcs": "Lvj1v5oebk4YsieXhwMBKP1m1vjwuLQgYry9ZAYnWB2hYEdikvji7aSs2xQideVwqj6mrXsLuKk5jY2FJWC2X4VTW49PwRT",
        "timestampMs": "1685959791871"
      },
      {
        "id": {
          "txDigest": "ENmjG42TE4GyqYb1fGNwJe7oxBbbXWCdNfRiQhCNLBJQ",
          "eventSeq": "0"
        },
        "packageId": "<PACKAGE-ID>",
        "transactionModule": "devnet_nft",
        "sender": "0xce1bf7db5ad6b04bfc305c049f08b80d5ccdc5d6d65d2ded13e95516d1e9fa22",
        "type": "<PACKAGE-ID>::devnet_nft::MintNFTEvent",
        "parsedJson": {
          "creator": "0xce1bf7db5ad6b04bfc305c049f08b80d5ccdc5d6d65d2ded13e95516d1e9fa22",
          "name": "test3",
          "object_id": "0xe9d44ad5eaaba6367f09ebe21a72876a6d6cb471275a3e10411a69d17d30008a"
        },
        "bcs": "BGyKDxdW5TcHksJ9nJac9pWJyTZvUmAoiDtezxe7VjfEZQVDfoXv9cAvSCXgPmUY7bpmcu11ue9xijCstUUUmgdZtp4SG7L6",
        "timestampMs": "1685959930934"
      }
    ],
    "nextCursor": {
      "txDigest": "ENmjG42TE4GyqYb1fGNwJe7oxBbbXWCdNfRiQhCNLBJQ",
      "eventSeq": "0"
    },
    "hasNextPage": false
  },
  "id": 1
}
```

### 3. Get all events and return 2 items per page in descending time order

**Request**

```shell
curl --location 'http://0.0.0.0:9000' \
--header 'Content-Type: application/json' \
--data '{
    "jsonrpc": "2.0",
    "id": 1,
    "method": "suix_queryEvents",
    "params": {
        "query": {
            "All": []
        },
        "limit": 2,
        "descending_order": true
    }
}'
```

**Response**

```json
{
  "jsonrpc": "2.0",
  "result": {
    "data": [
      {
        "id": {
          "txDigest": "ENmjG42TE4GyqYb1fGNwJe7oxBbbXWCdNfRiQhCNLBJQ",
          "eventSeq": "0"
        },
        "packageId": "<PACKAGE-ID>",
        "transactionModule": "devnet_nft",
        "sender": "0xce1bf7db5ad6b04bfc305c049f08b80d5ccdc5d6d65d2ded13e95516d1e9fa22",
        "type": "<PACKAGE-ID>::devnet_nft::MintNFTEvent",
        "parsedJson": {
          "creator": "0xce1bf7db5ad6b04bfc305c049f08b80d5ccdc5d6d65d2ded13e95516d1e9fa22",
          "name": "test3",
          "object_id": "0xe9d44ad5eaaba6367f09ebe21a72876a6d6cb471275a3e10411a69d17d30008a"
        },
        "bcs": "BGyKDxdW5TcHksJ9nJac9pWJyTZvUmAoiDtezxe7VjfEZQVDfoXv9cAvSCXgPmUY7bpmcu11ue9xijCstUUUmgdZtp4SG7L6",
        "timestampMs": "1685959930934"
      },
      {
        "id": {
          "txDigest": "7GV246XCe71e7ssFVV17k2GvcPypQ5ES2DBLtucW8r3b",
          "eventSeq": "0"
        },
        "packageId": "<PACKAGE-ID>",
        "transactionModule": "devnet_nft",
        "sender": "0xce1bf7db5ad6b04bfc305c049f08b80d5ccdc5d6d65d2ded13e95516d1e9fa22",
        "type": "<PACKAGE-ID>::devnet_nft::MintNFTEvent",
        "parsedJson": {
          "creator": "0xce1bf7db5ad6b04bfc305c049f08b80d5ccdc5d6d65d2ded13e95516d1e9fa22",
          "name": "test2",
          "object_id": "0x07d16da80bd9b9859c5aa2b8656f8f4189cf57291439de61be13d0c75b0802c8"
        },
        "bcs": "Lvj1v5oebk4YsieXhwMBKP1m1vjwuLQgYry9ZAYnWB2hYEdikvji7aSs2xQideVwqj6mrXsLuKk5jY2FJWC2X4VTW49PwRT",
        "timestampMs": "1685959791871"
      }
    ],
    "nextCursor": {
      "txDigest": "7GV246XCe71e7ssFVV17k2GvcPypQ5ES2DBLtucW8r3b",
      "eventSeq": "0"
    },
    "hasNextPage": true
  },
  "id": 1
}
```

## Subscribe to Sui events

When you subscribe to the events described in the preceding sections, you can apply event filters to match the events
you want to filter.

## Event filters

You can use `EventFilter` to filter the events included in your subscription to the event stream. `EventFilter` supports
filtering on one attribute or a combination of attributes.

### List of attributes that support filters

| Filter          | Description                                           | JSON-RPC Parameter Example                                                                   |
|-----------------|-------------------------------------------------------|----------------------------------------------------------------------------------------------|
| Package         | Move package ID                                       | `{"Package":"<PACKAGE-ID>"}`                                                                 |
| MoveModule      | Move module where the event was emitted               | `{"MoveModule": {"package": "<PACKAGE-ID>", "module": "nft"}}`                               |
| MoveEventType   | Move event type defined in the move code              | `{"MoveEventType":"<PACKAGE-ID>::nft::MintNFTEvent"}`                                        |
| MoveEventModule | Move event module defined in the move code            | `{"MoveEventModule": {"package": "<PACKAGE-ID>", "module": "nft", "event": "MintNFTEvent"}}` |
| MoveEventField  | Filter using the data fields in the move event object | `{"MoveEventField":{ "path":"/name", "value":"NFT"}}`                                        |
| SenderAddress   | Address that started the transaction                  | `{"SenderAddress": "0x008e9c621f4fdb210b873aab59a1e5bf32ddb1d33ee85eb069b348c234465106"}`    |
| Sender          | Sender address                                        | `{"Sender":"0x008e9c621f4fdb210b873aab59a1e5bf32ddb1d33ee85eb069b348c234465106"}`            |
| Transaction     | Transaction hash                                      | `{"Transaction":"ENmjG42TE4GyqYb1fGNwJe7oxBbbXWCdNfRiQhCNLBJQ"}`                             |
| TimeRange       | Time range in millisecond                             | `{"TimeRange": {"start_time": "1685959791871", "end_time": "1685959791871"}}`                |

### Combining filters

Sui provides a few operators for combining filters:

| Operator | Description                                                             | JSON-RPC Parameter Example                                                                           |
|----------|-------------------------------------------------------------------------|------------------------------------------------------------------------------------------------------|
| And      | Combine two filters; behaves the same as boolean And operator           | `{"And":[{"Package":"<PACKAGE-ID>"}, {"MoveModule": {"package": "<PACKAGE-ID>", "module": "nft"}}]}` |
| Or       | Combine two filters; behaves the same as boolean Or operator            | `{"Or":[{"Package":"<PACKAGE-ID>"}, {"Package":"0x1"}]}`                                             |
| All      | Combine a list of filters; returns true if all filters match the event  | `{"All":[{"Package":"<PACKAGE-ID>"}, {"MoveModule": {"package": "<PACKAGE-ID>", "module": "nft"}}]}` |
| Any      | Combine a list of filters; returns true if any filter matches the event | `{"Any":[{"Package":"<PACKAGE-ID>"}, {"Package":"0x1"}]}`                                            |

### Example using a combined filter

The following example demonstrates how to subscribe to events that a `<PACKAGE-ID>::nft` package emits:

```shell
>> {"jsonrpc":"2.0", "id": 1, "method": "sui_subscribeEvent", "params": [{"All":[{"Package":"<PACKAGE-MODULE-ID>"}, {"MoveModule": {"package": "<PACKAGE-ID>", "module": "nft"}}]}]}
<< {"jsonrpc":"2.0","result":3121662727959200,"id":1}
```

To unsubscribe from this stream, use:

```shell
>> {"jsonrpc":"2.0", "id": 1, "method": "sui_unsubscribeEvent", "params": [3121662727959200]}
<< {"jsonrpc":"2.0","result":true,"id":1}
```
