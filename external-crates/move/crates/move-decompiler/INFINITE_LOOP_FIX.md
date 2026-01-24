# Infinite Loop Bug Fix - Package 3835ce7a...e353

## Summary

Fixed an infinite loop bug in the decompiler's `find_loop_nodes` method that caused package 223 (3835ce7a...e353) to hang indefinitely during decompilation.

## Root Cause

**File:** `src/structuring/graph.rs`
**Function:** `find_loop_nodes` (lines 106-143)
**Bug Location:** Lines 121-124

### The Bug

```rust
for latch_node in latch_nodes {
    let paths = petgraph::algo::all_simple_paths::<Vec<_>, _, RandomState>(
        &self.cfg, node_start, latch_node, 0, None,
    )
    .collect::<Vec<_>>();
    for path in paths {
        loop_nodes.extend(path);
    }
}
```

**Problem:** When `latch_node == node_start` (self-loop case), calling `all_simple_paths(node_start, latch_node, ...)` with the same node as both start and end causes `petgraph::algo::all_simple_paths` to **hang indefinitely**.

### Why This Causes an Infinite Loop

1. **Self-Loop Edge Case:** When a loop has a back-edge from a node to itself (self-loop), `latch_node` equals `node_start`
2. **all_simple_paths Hangs:** The function `petgraph::algo::all_simple_paths(N, N, ...)` (finding paths from node N to itself) enters an infinite search
3. **No Timeout:** The algorithm has no timeout or iteration limit on path enumeration
4. **Complete Hang:** The decompiler becomes completely stuck, unable to make progress

### Specific Scenario

For package 3835ce7a...e353:
- The control flow graph contained a self-loop at NodeIndex(83)
- The back_edges map recorded: `{NodeIndex(83): {NodeIndex(83)}}`
- When processing this loop, the code tried to find all simple paths from 83 to 83
- `all_simple_paths` hung indefinitely trying to enumerate these paths

Debug output showed:
```
[DEBUG] find_loop_nodes: Starting for node NodeIndex(83)
[DEBUG] find_loop_nodes: Finding paths from NodeIndex(83) to NodeIndex(83)
<hangs here forever>
```

## The Fix

Detect the self-loop case and handle it specially without calling `all_simple_paths`:

```rust
for latch_node in latch_nodes {
    // Handle self-loops: if the latch node is the same as the start node,
    // just add the start node to loop_nodes
    if latch_node == node_start {
        loop_nodes.insert(node_start);
        continue;
    }

    let paths = petgraph::algo::all_simple_paths::<Vec<_>, _, RandomState>(
        &self.cfg, node_start, latch_node, 0, None,
    )
    .collect::<Vec<_>>();
    for path in paths {
        loop_nodes.extend(path);
    }
}
```

## Verification

- ✅ Code compiles successfully
- ✅ Package 3835ce7a...e353 now decompiles successfully in 0.10s (was: timeout/hang)
- ✅ All existing tests continue to pass
- ✅ No regression in functionality

## Impact

This fix resolves the infinite loop issue for package 223 and potentially other packages with self-loop constructs. The decompiler can now handle:
- Self-referential loops (while-true with break conditions)
- Back-edges from a node to itself
- Complex nested loop patterns that include self-loops

## Performance

- **Before:** Package 3835ce7a...e353 hung indefinitely (required manual kill)
- **After:** Package 3835ce7a...e353 decompiles successfully in 0.10s
- **Success Rate:** Improved from 95.5% (213/223, 1 timeout) to 95.9% (214/223, 0 timeouts)

## Future Improvements (Optional)

1. **Add Path Limit:** Consider limiting the number of paths enumerated in `all_simple_paths` to prevent exponential blowup in highly connected graphs
2. **Add Timeout Mechanism:** Implement a per-function or per-package timeout for the entire decompilation process
3. **Add Test Case:** Create a test with a self-loop control flow pattern

## Date

2026-01-23
