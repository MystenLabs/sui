---
"@mysten/sui.js": patch
---

Make arguments field optional for MoveCall to match Rust definition. This fixes a bug where the Explorer page does not load for transactions with no argument.
