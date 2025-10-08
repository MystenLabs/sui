# @mysten/prettier-plugin-move

## 0.3.3

### Patch Changes

- fix incorrect label assignment when extend + regular modules are mixed in the same file

## 0.3.2

### Patch Changes

- fix issue with clearing out comments after last variant in an `enum_definition`
- switch to newer tree-sitter with support for `extend` keyword

## 0.3.1

### Patch Changes

- migrate to better tree-sitter version

## 0.3.0

### Minor Changes

- `dot_expression` now supports trailing comment in a list
- a single element of an `arg_list` can have **trailing** line comment
- a single element of an `arg_list` can have **leading** line comment
- trailing comment no longer breaks `vector_expression`

`if_expression`:
- indentation fixed in certain places
- `if-else` chain is a special behavior

## 0.2.2

### Patch Changes

-   3e5cf29: abort + address improvements

## 0.2.1

### Patch Changes

-   360e9a2: Fixes missing parser for move-parser error

## 0.2.0

### Minor Changes

-   53387ff: - parser rename "move-parser" -> "move"
    -   adds `prettier-move` bin when installed globally
    -   better comments handling in empty blocks
    -   sorts abilities alphabetically, but `key` always first
    -   no longer inserts block in `if_expression` if expression breaks

## 0.1.1

### Patch Changes

-   e1a85c2: fixes publishing issue, compiles prepublish

## 0.1.0

### Minor Changes

-   9521492: Initial version of the prettier-plugin-move
