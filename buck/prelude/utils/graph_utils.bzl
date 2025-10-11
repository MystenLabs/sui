# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

load("@prelude//utils:utils.bzl", "expect")

def pre_order_traversal(
        graph: dict[typing.Any, list[typing.Any]],
        node_formatter: typing.Callable = str) -> list[typing.Any]:
    """
    Perform a pre-order (topologically sorted) traversal of `graph` and return the ordered nodes
    """

    in_degrees = {node: 0 for node in graph}
    for _node, deps in graph.items():
        for dep in dedupe(deps):
            in_degrees[dep] += 1

    queue = []

    for node, in_degree in in_degrees.items():
        if in_degree == 0:
            queue.append(node)

    ordered = []

    for _ in range(len(in_degrees)):
        if len(queue) == 0:
            fail_cycle(graph, node_formatter)

        node = queue.pop()
        ordered.append(node)

        for dep in graph[node]:
            in_degrees[dep] -= 1
            if in_degrees[dep] == 0:
                queue.append(dep)

    expect(not queue, "finished before processing nodes: {}".format([node_formatter(node) for node in queue]))
    expect(len(ordered) == len(graph), "missing or duplicate nodes in sort")

    return ordered

def post_order_traversal(
        graph: dict[typing.Any, list[typing.Any]],
        node_formatter: typing.Callable = str) -> list[typing.Any]:
    """
    Performs a post-order traversal of `graph`.
    """

    out_degrees = {node: 0 for node in graph}
    rdeps = {node: [] for node in graph}
    for node, deps in graph.items():
        for dep in dedupe(deps):
            out_degrees[node] += 1
            rdeps[dep].append(node)

    queue = []

    for node, out_degree in out_degrees.items():
        if out_degree == 0:
            queue.append(node)

    ordered = []

    for _ in range(len(out_degrees)):
        if len(queue) == 0:
            fail_cycle(graph, node_formatter)

        node = queue.pop()
        ordered.append(node)

        for dep in rdeps[node]:
            out_degrees[dep] -= 1
            if out_degrees[dep] == 0:
                queue.append(dep)

    expect(not queue, "finished before processing nodes: {}".format([node_formatter(node) for node in queue]))
    expect(len(ordered) == len(graph), "missing or duplicate nodes in sort")

    return ordered

def fail_cycle(
        graph: dict[typing.Any, list[typing.Any]],
        node_formatter: typing.Callable) -> typing.Never:
    cycle = find_cycle(graph)
    if cycle:
        fail(
            "cycle in graph detected: {}".format(
                " -> ".join(
                    [node_formatter(c) for c in cycle],
                ),
            ),
        )
    fail("expected cycle, but found none")

def find_cycle(graph: dict[typing.Any, list[typing.Any]]) -> list[typing.Any] | None:
    visited = {}
    OUTPUT = 1
    VISIT = 2
    current_parents = []
    work = [(VISIT, n) for n in graph.keys()]
    for _ in range(2000000000):
        if not work:
            break
        kind, node = work.pop()
        if kind == VISIT:
            if node not in visited:
                visited[node] = True
                current_parents.append(node)

                work.append((OUTPUT, node))
                for dep in graph[node]:
                    if dep in current_parents:
                        return current_parents + [dep]
                    if dep not in visited:
                        work.append((VISIT, dep))
        else:
            current_parents.pop()

    return None

def post_order_traversal_by(
        roots: list[typing.Any],
        get_nodes_to_traverse_func) -> list[typing.Any]:
    """
    Returns the post-order sorted list of the nodes in the traversal.

    This implementation simply performs a dfs. We maintain a work stack here.
    When visiting a node, we first add an item to the work stack to output that
    node, and then add items to visit all the children. While a work item for a
    child will not be added if it has already been visited, if there's an item in
    the stack for that child it will still be added. When popping the visit, if
    the node had been visited, it's ignored. This ensures that a node's children are
    all visited before we output that node.
    """
    ordered = []
    visited = {}
    OUTPUT = 1
    VISIT = 2
    queue = [(VISIT, n) for n in roots]
    for _ in range(2000000000):
        if not queue:
            break

        kind, node = queue.pop()
        if kind == VISIT:
            if not node in visited:
                queue.append((OUTPUT, node))
                for dep in get_nodes_to_traverse_func(node):
                    if dep not in visited:
                        queue.append((VISIT, dep))
        else:
            visited[node] = True
            ordered.append(node)
    return ordered

def pre_order_traversal_by(
        roots: list[typing.Any],
        get_nodes_to_traverse_func) -> list[typing.Any]:
    """
    Returns a topological sorted list of the nodes from a pre-order traversal.

    Note this gives a different order from `pre_order_traversal` above (to simplify the implementation).
    """
    ordered = post_order_traversal_by(roots, get_nodes_to_traverse_func)
    return ordered[::-1]

def breadth_first_traversal(
        graph_nodes: dict[typing.Any, list[typing.Any]],
        roots: list[typing.Any]) -> list[typing.Any]:
    """
    Like `breadth_first_traversal_by` but the nodes are stored in the graph.
    """

    def lookup(x):
        return graph_nodes[x]

    return breadth_first_traversal_by(graph_nodes, roots, lookup)

def breadth_first_traversal_by(
        graph_nodes: [dict[typing.Any, typing.Any], None],
        roots: list[typing.Any],
        get_nodes_to_traverse_func: typing.Callable,
        node_formatter: typing.Callable = str) -> list[typing.Any]:
    """
    Performs a breadth first traversal of `graph_nodes`, beginning
    with the `roots` and queuing the nodes returned by`get_nodes_to_traverse_func`.
    Returns a list of all visisted nodes.

    get_nodes_to_traverse_func(node: '_a') -> ['_a']:

    Starlark does not offer while loops, so this implementation
    must make use of a for loop. We pop from the end of the queue
    as a matter of performance.
    """

    # Dictify for O(1) lookup
    visited = {k: None for k in roots}

    queue = visited.keys()

    for _ in range(len(graph_nodes) if graph_nodes else 2000000000):
        if not queue:
            break
        node = queue.pop()
        if graph_nodes:
            expect(node in graph_nodes, "Expected node {} in graph nodes", node_formatter(node))
        nodes_to_visit = get_nodes_to_traverse_func(node)
        for node in nodes_to_visit:
            if node not in visited:
                visited[node] = None
                queue.append(node)

    expect(not queue, "Expected to be done with graph traversal queue.")

    return visited.keys()
