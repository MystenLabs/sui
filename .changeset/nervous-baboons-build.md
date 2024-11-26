---
'@mysten/prettier-plugin-move': minor
---

- parser rename "move-parser" -> "move"
- adds `prettier-move` bin when installed globally
- better comments handling in empty blocks
- sorts abilities alphabetically, but `key` always first
- no longer inserts block in `if_expression` if expression breaks
