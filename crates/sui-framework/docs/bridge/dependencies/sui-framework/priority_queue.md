
<a name="0x2_priority_queue"></a>

# Module `0x2::priority_queue`



-  [Struct `PriorityQueue`](#0x2_priority_queue_PriorityQueue)
-  [Struct `Entry`](#0x2_priority_queue_Entry)
-  [Constants](#@Constants_0)
-  [Function `new`](#0x2_priority_queue_new)
-  [Function `pop_max`](#0x2_priority_queue_pop_max)
-  [Function `insert`](#0x2_priority_queue_insert)
-  [Function `new_entry`](#0x2_priority_queue_new_entry)
-  [Function `create_entries`](#0x2_priority_queue_create_entries)
-  [Function `restore_heap_recursive`](#0x2_priority_queue_restore_heap_recursive)
-  [Function `max_heapify_recursive`](#0x2_priority_queue_max_heapify_recursive)
-  [Function `priorities`](#0x2_priority_queue_priorities)


<pre><code><b>use</b> <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">0x1::vector</a>;
</code></pre>



<a name="0x2_priority_queue_PriorityQueue"></a>

## Struct `PriorityQueue`



<pre><code><b>struct</b> <a href="../../dependencies/sui-framework/priority_queue.md#0x2_priority_queue_PriorityQueue">PriorityQueue</a>&lt;T: drop&gt; <b>has</b> drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>entries: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="../../dependencies/sui-framework/priority_queue.md#0x2_priority_queue_Entry">priority_queue::Entry</a>&lt;T&gt;&gt;</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x2_priority_queue_Entry"></a>

## Struct `Entry`



<pre><code><b>struct</b> <a href="../../dependencies/sui-framework/priority_queue.md#0x2_priority_queue_Entry">Entry</a>&lt;T: drop&gt; <b>has</b> drop, store
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


<a name="0x2_priority_queue_EPopFromEmptyHeap"></a>



<pre><code><b>const</b> <a href="../../dependencies/sui-framework/priority_queue.md#0x2_priority_queue_EPopFromEmptyHeap">EPopFromEmptyHeap</a>: u64 = 0;
</code></pre>



<a name="0x2_priority_queue_new"></a>

## Function `new`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/priority_queue.md#0x2_priority_queue_new">new</a>&lt;T: drop&gt;(entries: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="../../dependencies/sui-framework/priority_queue.md#0x2_priority_queue_Entry">priority_queue::Entry</a>&lt;T&gt;&gt;): <a href="../../dependencies/sui-framework/priority_queue.md#0x2_priority_queue_PriorityQueue">priority_queue::PriorityQueue</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/priority_queue.md#0x2_priority_queue_new">new</a>&lt;T: drop&gt;(entries: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="../../dependencies/sui-framework/priority_queue.md#0x2_priority_queue_Entry">Entry</a>&lt;T&gt;&gt;) : <a href="../../dependencies/sui-framework/priority_queue.md#0x2_priority_queue_PriorityQueue">PriorityQueue</a>&lt;T&gt; {
    <b>let</b> len = <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_length">vector::length</a>(&entries);
    <b>let</b> i = len / 2;
    // Max heapify from the first node that is a parent (node at len / 2).
    <b>while</b> (i &gt; 0) {
        i = i - 1;
        <a href="../../dependencies/sui-framework/priority_queue.md#0x2_priority_queue_max_heapify_recursive">max_heapify_recursive</a>(&<b>mut</b> entries, len, i);
    };
    <a href="../../dependencies/sui-framework/priority_queue.md#0x2_priority_queue_PriorityQueue">PriorityQueue</a> { entries }
}
</code></pre>



</details>

<a name="0x2_priority_queue_pop_max"></a>

## Function `pop_max`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/priority_queue.md#0x2_priority_queue_pop_max">pop_max</a>&lt;T: drop&gt;(pq: &<b>mut</b> <a href="../../dependencies/sui-framework/priority_queue.md#0x2_priority_queue_PriorityQueue">priority_queue::PriorityQueue</a>&lt;T&gt;): (u64, T)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/priority_queue.md#0x2_priority_queue_pop_max">pop_max</a>&lt;T: drop&gt;(pq: &<b>mut</b> <a href="../../dependencies/sui-framework/priority_queue.md#0x2_priority_queue_PriorityQueue">PriorityQueue</a>&lt;T&gt;) : (u64, T) {
    <b>let</b> len = <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_length">vector::length</a>(&pq.entries);
    <b>assert</b>!(len &gt; 0, <a href="../../dependencies/sui-framework/priority_queue.md#0x2_priority_queue_EPopFromEmptyHeap">EPopFromEmptyHeap</a>);
    // Swap the max element <b>with</b> the last element in the entries and remove the max element.
    <b>let</b> <a href="../../dependencies/sui-framework/priority_queue.md#0x2_priority_queue_Entry">Entry</a> { priority, value } = <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_swap_remove">vector::swap_remove</a>(&<b>mut</b> pq.entries, 0);
    // Now the max heap property <b>has</b> been violated at the root node, but nowhere <b>else</b>
    // so we call max heapify on the root node.
    <a href="../../dependencies/sui-framework/priority_queue.md#0x2_priority_queue_max_heapify_recursive">max_heapify_recursive</a>(&<b>mut</b> pq.entries, len - 1, 0);
    (priority, value)
}
</code></pre>



</details>

<a name="0x2_priority_queue_insert"></a>

## Function `insert`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/priority_queue.md#0x2_priority_queue_insert">insert</a>&lt;T: drop&gt;(pq: &<b>mut</b> <a href="../../dependencies/sui-framework/priority_queue.md#0x2_priority_queue_PriorityQueue">priority_queue::PriorityQueue</a>&lt;T&gt;, priority: u64, value: T)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/priority_queue.md#0x2_priority_queue_insert">insert</a>&lt;T: drop&gt;(pq: &<b>mut</b> <a href="../../dependencies/sui-framework/priority_queue.md#0x2_priority_queue_PriorityQueue">PriorityQueue</a>&lt;T&gt;, priority: u64, value: T) {
    <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_push_back">vector::push_back</a>(&<b>mut</b> pq.entries, <a href="../../dependencies/sui-framework/priority_queue.md#0x2_priority_queue_Entry">Entry</a> { priority, value});
    <b>let</b> index = <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_length">vector::length</a>(&pq.entries) - 1;
    <a href="../../dependencies/sui-framework/priority_queue.md#0x2_priority_queue_restore_heap_recursive">restore_heap_recursive</a>(&<b>mut</b> pq.entries, index);
}
</code></pre>



</details>

<a name="0x2_priority_queue_new_entry"></a>

## Function `new_entry`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/priority_queue.md#0x2_priority_queue_new_entry">new_entry</a>&lt;T: drop&gt;(priority: u64, value: T): <a href="../../dependencies/sui-framework/priority_queue.md#0x2_priority_queue_Entry">priority_queue::Entry</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/priority_queue.md#0x2_priority_queue_new_entry">new_entry</a>&lt;T: drop&gt;(priority: u64, value: T): <a href="../../dependencies/sui-framework/priority_queue.md#0x2_priority_queue_Entry">Entry</a>&lt;T&gt; {
    <a href="../../dependencies/sui-framework/priority_queue.md#0x2_priority_queue_Entry">Entry</a> { priority, value }
}
</code></pre>



</details>

<a name="0x2_priority_queue_create_entries"></a>

## Function `create_entries`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/priority_queue.md#0x2_priority_queue_create_entries">create_entries</a>&lt;T: drop&gt;(p: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u64&gt;, v: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;T&gt;): <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="../../dependencies/sui-framework/priority_queue.md#0x2_priority_queue_Entry">priority_queue::Entry</a>&lt;T&gt;&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/priority_queue.md#0x2_priority_queue_create_entries">create_entries</a>&lt;T: drop&gt;(p: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u64&gt;, v: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;T&gt;): <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="../../dependencies/sui-framework/priority_queue.md#0x2_priority_queue_Entry">Entry</a>&lt;T&gt;&gt; {
    <b>let</b> len = <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_length">vector::length</a>(&p);
    <b>assert</b>!(<a href="../../dependencies/move-stdlib/vector.md#0x1_vector_length">vector::length</a>(&v) == len, 0);
    <b>let</b> res = <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_empty">vector::empty</a>();
    <b>let</b> i = 0;
    <b>while</b> (i &lt; len) {
        <b>let</b> priority = <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_remove">vector::remove</a>(&<b>mut</b> p, 0);
        <b>let</b> value = <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_remove">vector::remove</a>(&<b>mut</b> v, 0);
        <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_push_back">vector::push_back</a>(&<b>mut</b> res, <a href="../../dependencies/sui-framework/priority_queue.md#0x2_priority_queue_Entry">Entry</a> { priority, value });
        i = i + 1;
    };
    res
}
</code></pre>



</details>

<a name="0x2_priority_queue_restore_heap_recursive"></a>

## Function `restore_heap_recursive`



<pre><code><b>fun</b> <a href="../../dependencies/sui-framework/priority_queue.md#0x2_priority_queue_restore_heap_recursive">restore_heap_recursive</a>&lt;T: drop&gt;(v: &<b>mut</b> <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="../../dependencies/sui-framework/priority_queue.md#0x2_priority_queue_Entry">priority_queue::Entry</a>&lt;T&gt;&gt;, i: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../../dependencies/sui-framework/priority_queue.md#0x2_priority_queue_restore_heap_recursive">restore_heap_recursive</a>&lt;T: drop&gt;(v: &<b>mut</b> <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="../../dependencies/sui-framework/priority_queue.md#0x2_priority_queue_Entry">Entry</a>&lt;T&gt;&gt;, i: u64) {
    <b>if</b> (i == 0) {
        <b>return</b>
    };
    <b>let</b> parent = (i - 1) / 2;

    // If new elem is greater than its parent, swap them and recursively
    // do the restoration upwards.
    <b>if</b> (<a href="../../dependencies/move-stdlib/vector.md#0x1_vector_borrow">vector::borrow</a>(v, i).priority &gt; <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_borrow">vector::borrow</a>(v, parent).priority) {
        <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_swap">vector::swap</a>(v, i, parent);
        <a href="../../dependencies/sui-framework/priority_queue.md#0x2_priority_queue_restore_heap_recursive">restore_heap_recursive</a>(v, parent);
    }
}
</code></pre>



</details>

<a name="0x2_priority_queue_max_heapify_recursive"></a>

## Function `max_heapify_recursive`



<pre><code><b>fun</b> <a href="../../dependencies/sui-framework/priority_queue.md#0x2_priority_queue_max_heapify_recursive">max_heapify_recursive</a>&lt;T: drop&gt;(v: &<b>mut</b> <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="../../dependencies/sui-framework/priority_queue.md#0x2_priority_queue_Entry">priority_queue::Entry</a>&lt;T&gt;&gt;, len: u64, i: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../../dependencies/sui-framework/priority_queue.md#0x2_priority_queue_max_heapify_recursive">max_heapify_recursive</a>&lt;T: drop&gt;(v: &<b>mut</b> <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="../../dependencies/sui-framework/priority_queue.md#0x2_priority_queue_Entry">Entry</a>&lt;T&gt;&gt;, len: u64, i: u64) {
    <b>if</b> (len == 0) {
        <b>return</b>
    };
    <b>assert</b>!(i &lt; len, 1);
    <b>let</b> left = i * 2 + 1;
    <b>let</b> right = left + 1;
    <b>let</b> max = i;
    // Find the node <b>with</b> highest priority among node `i` and its two children.
    <b>if</b> (left &lt; len && <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_borrow">vector::borrow</a>(v, left).priority&gt; <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_borrow">vector::borrow</a>(v, max).priority) {
        max = left;
    };
    <b>if</b> (right &lt; len && <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_borrow">vector::borrow</a>(v, right).priority &gt; <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_borrow">vector::borrow</a>(v, max).priority) {
        max = right;
    };
    // If the parent node (node `i`) doesn't have the highest priority, we swap the parent <b>with</b> the
    // max priority node.
    <b>if</b> (max != i) {
        <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_swap">vector::swap</a>(v, max, i);
        // After the swap, we have restored the property at node `i` but now the max heap property
        // may be violated at node `max` since this node now <b>has</b> a new value. So we need <b>to</b> now
        // max heapify the subtree rooted at node `max`.
        <a href="../../dependencies/sui-framework/priority_queue.md#0x2_priority_queue_max_heapify_recursive">max_heapify_recursive</a>(v, len, max);
    }
}
</code></pre>



</details>

<a name="0x2_priority_queue_priorities"></a>

## Function `priorities`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/priority_queue.md#0x2_priority_queue_priorities">priorities</a>&lt;T: drop&gt;(pq: &<a href="../../dependencies/sui-framework/priority_queue.md#0x2_priority_queue_PriorityQueue">priority_queue::PriorityQueue</a>&lt;T&gt;): <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u64&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/priority_queue.md#0x2_priority_queue_priorities">priorities</a>&lt;T: drop&gt;(pq: &<a href="../../dependencies/sui-framework/priority_queue.md#0x2_priority_queue_PriorityQueue">PriorityQueue</a>&lt;T&gt;): <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u64&gt; {
    <b>let</b> res = <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>[];
    <b>let</b> i = 0;
    <b>while</b> (i &lt; <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_length">vector::length</a>(&pq.entries)) {
        <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_push_back">vector::push_back</a>(&<b>mut</b> res, <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_borrow">vector::borrow</a>(&pq.entries, i).priority);
        i = i +1;
    };
    res
}
</code></pre>



</details>
