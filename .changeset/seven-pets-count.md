---
'@mysten/sui': minor
---

`WaitForLocalExecution` now waits using client.waitForTransaction rather than sending requestType to the RPC node. This change will preserve readAfterWrite consistency when local execution is removed from fullnodes, at the cost of more network requests and higher latency.
