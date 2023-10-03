---
'@mysten/zklogin': patch
---

- stop exporting `ZkSignatureInputs`
- use `toBigEndianBytes` instead of `toBufferBE` that was renamed
