---
title: Module `sui::priority_queue`
---

Priority queue implemented using a max heap.


-  [Struct `PriorityQueue`](#sui_priority_queue_PriorityQueue)
-  [Struct `Entry`](#sui_priority_queue_Entry)
-  [Constants](#@Constants_0)
-  [Function `new`](#sui_priority_queue_new)
-  [Function `pop_max`](#sui_priority_queue_pop_max)
-  [Function `insert`](#sui_priority_queue_insert)
-  [Function `new_entry`](#sui_priority_queue_new_entry)
-  [Function `create_entries`](#sui_priority_queue_create_entries)
-  [Function `restore_heap_recursive`](#sui_priority_queue_restore_heap_recursive)
-  [Function `max_heapify_recursive`](#sui_priority_queue_max_heapify_recursive)
-  [Function `priorities`](#sui_priority_queue_priorities)


<pre><code><b>use</b> <a href="../std/vector.md#std_vector">std::vector</a>;
</code></pre>



<a name="sui_priority_queue_PriorityQueue"></a>

## Struct `PriorityQueue`

Struct representing a priority queue. The <code>entries</code> vector represents a max
heap structure, where entries[0] is the root, entries[1] and entries[2] are the
left child and right child of the root, etc. More generally, the children of
entries[i] are at i * 2 + 1 and i * 2 + 2. The max heap should have the invariant
that the parent node's priority is always higher than its child nodes' priorities.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/priority_queue.md#sui_priority_queue_PriorityQueue">PriorityQueue</a>&lt;T: drop&gt; <b>has</b> drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>entries: vector&lt;<a href="../sui/priority_queue.md#sui_priority_queue_Entry">sui::priority_queue::Entry</a>&lt;T&gt;&gt;</code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="sui_priority_queue_Entry"></a>

## Struct `Entry`



<pre><code><b>public</b> <b>struct</b> <a href="../sui/priority_queue.md#sui_priority_queue_Entry">Entry</a>&lt;T: drop&gt; <b>has</b> drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>priority: u64</code>
</dt>
<dd>
</dd>
<dt>
<code>value: T</code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="sui_priority_queue_EPopFromEmptyHeap"></a>

For when heap is empty and there's no data to pop.


<pre><code><b>const</b> <a href="../sui/priority_queue.md#sui_priority_queue_EPopFromEmptyHeap">EPopFromEmptyHeap</a>: u64 = 0;
</code></pre>



<a name="sui_priority_queue_new"></a>

## Function `new`

Create a new priority queue from the input entry vectors.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/priority_queue.md#sui_priority_queue_new">new</a>&lt;T: drop&gt;(entries: vector&lt;<a href="../sui/priority_queue.md#sui_priority_queue_Entry">sui::priority_queue::Entry</a>&lt;T&gt;&gt;): <a href="../sui/priority_queue.md#sui_priority_queue_PriorityQueue">sui::priority_queue::PriorityQueue</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/priority_queue.md#sui_priority_queue_new">new</a>&lt;T: drop&gt;(<b>mut</b> entries: vector&lt;<a href="../sui/priority_queue.md#sui_priority_queue_Entry">Entry</a>&lt;T&gt;&gt;): <a href="../sui/priority_queue.md#sui_priority_queue_PriorityQueue">PriorityQueue</a>&lt;T&gt; {
    <b>let</b> len = entries.length();
    <b>let</b> <b>mut</b> i = len / 2;
    // Max heapify from the first node that is a parent (node at len / 2).
    <b>while</b> (i &gt; 0) {
        i = i - 1;
        <a href="../sui/priority_queue.md#sui_priority_queue_max_heapify_recursive">max_heapify_recursive</a>(&<b>mut</b> entries, len, i);
    };
    <a href="../sui/priority_queue.md#sui_priority_queue_PriorityQueue">PriorityQueue</a> { entries }
}
</code></pre>



</details>

<a name="sui_priority_queue_pop_max"></a>

## Function `pop_max`

Pop the entry with the highest priority value.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/priority_queue.md#sui_priority_queue_pop_max">pop_max</a>&lt;T: drop&gt;(pq: &<b>mut</b> <a href="../sui/priority_queue.md#sui_priority_queue_PriorityQueue">sui::priority_queue::PriorityQueue</a>&lt;T&gt;): (u64, T)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/priority_queue.md#sui_priority_queue_pop_max">pop_max</a>&lt;T: drop&gt;(pq: &<b>mut</b> <a href="../sui/priority_queue.md#sui_priority_queue_PriorityQueue">PriorityQueue</a>&lt;T&gt;): (u64, T) {
    <b>let</b> len = pq.entries.length();
    <b>assert</b>!(len &gt; 0, <a href="../sui/priority_queue.md#sui_priority_queue_EPopFromEmptyHeap">EPopFromEmptyHeap</a>);
    // Swap the max element with the last element in the entries and remove the max element.
    <b>let</b> <a href="../sui/priority_queue.md#sui_priority_queue_Entry">Entry</a> { priority, value } = pq.entries.swap_remove(0);
    // Now the max heap property <b>has</b> been violated at the root node, but nowhere <b>else</b>
    // so we call max heapify on the root node.
    <a href="../sui/priority_queue.md#sui_priority_queue_max_heapify_recursive">max_heapify_recursive</a>(&<b>mut</b> pq.entries, len - 1, 0);
    (priority, value)
}
</code></pre>



</details>

<a name="sui_priority_queue_insert"></a>

## Function `insert`

Insert a new entry into the queue.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/priority_queue.md#sui_priority_queue_insert">insert</a>&lt;T: drop&gt;(pq: &<b>mut</b> <a href="../sui/priority_queue.md#sui_priority_queue_PriorityQueue">sui::priority_queue::PriorityQueue</a>&lt;T&gt;, priority: u64, value: T)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/priority_queue.md#sui_priority_queue_insert">insert</a>&lt;T: drop&gt;(pq: &<b>mut</b> <a href="../sui/priority_queue.md#sui_priority_queue_PriorityQueue">PriorityQueue</a>&lt;T&gt;, priority: u64, value: T) {
    pq.entries.push_back(<a href="../sui/priority_queue.md#sui_priority_queue_Entry">Entry</a> { priority, value });
    <b>let</b> index = pq.entries.length() - 1;
    <a href="../sui/priority_queue.md#sui_priority_queue_restore_heap_recursive">restore_heap_recursive</a>(&<b>mut</b> pq.entries, index);
}
</code></pre>



</details>

<a name="sui_priority_queue_new_entry"></a>

## Function `new_entry`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/priority_queue.md#sui_priority_queue_new_entry">new_entry</a>&lt;T: drop&gt;(priority: u64, value: T): <a href="../sui/priority_queue.md#sui_priority_queue_Entry">sui::priority_queue::Entry</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/priority_queue.md#sui_priority_queue_new_entry">new_entry</a>&lt;T: drop&gt;(priority: u64, value: T): <a href="../sui/priority_queue.md#sui_priority_queue_Entry">Entry</a>&lt;T&gt; {
    <a href="../sui/priority_queue.md#sui_priority_queue_Entry">Entry</a> { priority, value }
}
</code></pre>



</details>

<a name="sui_priority_queue_create_entries"></a>

## Function `create_entries`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/priority_queue.md#sui_priority_queue_create_entries">create_entries</a>&lt;T: drop&gt;(p: vector&lt;u64&gt;, v: vector&lt;T&gt;): vector&lt;<a href="../sui/priority_queue.md#sui_priority_queue_Entry">sui::priority_queue::Entry</a>&lt;T&gt;&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/priority_queue.md#sui_priority_queue_create_entries">create_entries</a>&lt;T: drop&gt;(<b>mut</b> p: vector&lt;u64&gt;, <b>mut</b> v: vector&lt;T&gt;): vector&lt;<a href="../sui/priority_queue.md#sui_priority_queue_Entry">Entry</a>&lt;T&gt;&gt; {
    <b>let</b> len = p.length();
    <b>assert</b>!(v.length() == len, 0);
    <b>let</b> <b>mut</b> res = vector[];
    <b>let</b> <b>mut</b> i = 0;
    <b>while</b> (i &lt; len) {
        <b>let</b> priority = p.remove(0);
        <b>let</b> value = v.remove(0);
        res.push_back(<a href="../sui/priority_queue.md#sui_priority_queue_Entry">Entry</a> { priority, value });
        i = i + 1;
    };
    res
}
</code></pre>



</details>

<a name="sui_priority_queue_restore_heap_recursive"></a>

## Function `restore_heap_recursive`



<pre><code><b>fun</b> <a href="../sui/priority_queue.md#sui_priority_queue_restore_heap_recursive">restore_heap_recursive</a>&lt;T: drop&gt;(v: &<b>mut</b> vector&lt;<a href="../sui/priority_queue.md#sui_priority_queue_Entry">sui::priority_queue::Entry</a>&lt;T&gt;&gt;, i: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/priority_queue.md#sui_priority_queue_restore_heap_recursive">restore_heap_recursive</a>&lt;T: drop&gt;(v: &<b>mut</b> vector&lt;<a href="../sui/priority_queue.md#sui_priority_queue_Entry">Entry</a>&lt;T&gt;&gt;, i: u64) {
    <b>if</b> (i == 0) {
        <b>return</b>
    };
    <b>let</b> parent = (i - 1) / 2;
    // If <a href="../sui/priority_queue.md#sui_priority_queue_new">new</a> elem is greater than its parent, swap them and recursively
    // do the restoration upwards.
    <b>if</b> (*&v[i].priority &gt; *&v[parent].priority) {
        v.swap(i, parent);
        <a href="../sui/priority_queue.md#sui_priority_queue_restore_heap_recursive">restore_heap_recursive</a>(v, parent);
    }
}
</code></pre>



</details>

<a name="sui_priority_queue_max_heapify_recursive"></a>

## Function `max_heapify_recursive`

Max heapify the subtree whose root is at index <code>i</code>. That means after this function
finishes, the subtree should have the property that the parent node has higher priority
than both child nodes.
This function assumes that all the other nodes in the subtree (nodes other than the root)
do satisfy the max heap property.


<pre><code><b>fun</b> <a href="../sui/priority_queue.md#sui_priority_queue_max_heapify_recursive">max_heapify_recursive</a>&lt;T: drop&gt;(v: &<b>mut</b> vector&lt;<a href="../sui/priority_queue.md#sui_priority_queue_Entry">sui::priority_queue::Entry</a>&lt;T&gt;&gt;, len: u64, i: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/priority_queue.md#sui_priority_queue_max_heapify_recursive">max_heapify_recursive</a>&lt;T: drop&gt;(v: &<b>mut</b> vector&lt;<a href="../sui/priority_queue.md#sui_priority_queue_Entry">Entry</a>&lt;T&gt;&gt;, len: u64, i: u64) {
    <b>if</b> (len == 0) {
        <b>return</b>
    };
    <b>assert</b>!(i &lt; len, 1);
    <b>let</b> left = i * 2 + 1;
    <b>let</b> right = left + 1;
    <b>let</b> <b>mut</b> max = i;
    // Find the node with highest priority among node `i` and its two children.
    <b>if</b> (left &lt; len && *&v[left].priority &gt; *&v[max].priority) {
        max = left;
    };
    <b>if</b> (right &lt; len && *&v[right].priority &gt; *&v[max].priority) {
        max = right;
    };
    // If the parent node (node `i`) doesn't have the highest priority, we swap the parent with the
    // max priority node.
    <b>if</b> (max != i) {
        v.swap(max, i);
        // After the swap, we have restored the property at node `i` but now the max heap property
        // may be violated at node `max` since this node now <b>has</b> a <a href="../sui/priority_queue.md#sui_priority_queue_new">new</a> value. So we need to now
        // max heapify the subtree rooted at node `max`.
        <a href="../sui/priority_queue.md#sui_priority_queue_max_heapify_recursive">max_heapify_recursive</a>(v, len, max);
    }
}
</code></pre>



</details>

<a name="sui_priority_queue_priorities"></a>

## Function `priorities`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/priority_queue.md#sui_priority_queue_priorities">priorities</a>&lt;T: drop&gt;(pq: &<a href="../sui/priority_queue.md#sui_priority_queue_PriorityQueue">sui::priority_queue::PriorityQueue</a>&lt;T&gt;): vector&lt;u64&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/priority_queue.md#sui_priority_queue_priorities">priorities</a>&lt;T: drop&gt;(pq: &<a href="../sui/priority_queue.md#sui_priority_queue_PriorityQueue">PriorityQueue</a>&lt;T&gt;): vector&lt;u64&gt; {
    <b>let</b> <b>mut</b> res = vector[];
    <b>let</b> <b>mut</b> i = 0;
    <b>while</b> (i &lt; pq.entries.length()) {
        res.push_back(pq.entries[i].priority);
        i = i +1;
    };
    res
}
</code></pre>



</details>
