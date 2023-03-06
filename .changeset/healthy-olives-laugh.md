---
"@mysten/sui.js": minor
---

Introduce new `Transaction` builder class, and deprecate all existing methods of sending transactions. The new builder class is designed to take full advantage of Programmable Transactions. Any transaction using the previous `SignableTransaction` interface will be converted to a `Transaction` class when possible, but this interface will be fully removed soon.
