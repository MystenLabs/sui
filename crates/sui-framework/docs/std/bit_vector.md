---
title: Module `std::bit_vector`
---



-  [Struct `BitVector`](#std_bit_vector_BitVector)
-  [Constants](#@Constants_0)
-  [Function `new`](#std_bit_vector_new)
-  [Function `set`](#std_bit_vector_set)
-  [Function `unset`](#std_bit_vector_unset)
-  [Function `shift_left`](#std_bit_vector_shift_left)
-  [Function `is_index_set`](#std_bit_vector_is_index_set)
-  [Function `length`](#std_bit_vector_length)
-  [Function `longest_set_sequence_starting_at`](#std_bit_vector_longest_set_sequence_starting_at)


<pre><code></code></pre>



<a name="std_bit_vector_BitVector"></a>

## Struct `BitVector`

The <code><a href="../std/bit_vector.md#std_bit_vector_BitVector">BitVector</a></code> structure contains a length parameter and a vector of boolean values.
This is a special vector that uses <code><b>true</b></code> to indicate that the bit is "1",
and <code><b>false</b></code> to indicate that the bit is "0".


<pre><code><b>public</b> <b>struct</b> <a href="../std/bit_vector.md#std_bit_vector_BitVector">BitVector</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code><a href="../std/bit_vector.md#std_bit_vector_length">length</a>: <a href="../std/u64.md#std_u64">u64</a></code>
</dt>
<dd>
</dd>
<dt>
<code>bit_field: <a href="../std/vector.md#std_vector">vector</a>&lt;<a href="../std/bool.md#std_bool">bool</a>&gt;</code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="std_bit_vector_EIndex"></a>

The provided index is out of bounds


<pre><code><b>const</b> <a href="../std/bit_vector.md#std_bit_vector_EIndex">EIndex</a>: <a href="../std/u64.md#std_u64">u64</a> = 131072;
</code></pre>



<a name="std_bit_vector_ELength"></a>

An invalid length of bitvector was given


<pre><code><b>const</b> <a href="../std/bit_vector.md#std_bit_vector_ELength">ELength</a>: <a href="../std/u64.md#std_u64">u64</a> = 131073;
</code></pre>



<a name="std_bit_vector_WORD_SIZE"></a>



<pre><code><b>const</b> <a href="../std/bit_vector.md#std_bit_vector_WORD_SIZE">WORD_SIZE</a>: <a href="../std/u64.md#std_u64">u64</a> = 1;
</code></pre>



<a name="std_bit_vector_MAX_SIZE"></a>

The maximum allowed bitvector size


<pre><code><b>const</b> <a href="../std/bit_vector.md#std_bit_vector_MAX_SIZE">MAX_SIZE</a>: <a href="../std/u64.md#std_u64">u64</a> = 1024;
</code></pre>



<a name="std_bit_vector_new"></a>

## Function `new`

Create a new <code><a href="../std/bit_vector.md#std_bit_vector_BitVector">BitVector</a></code> of the corresponding length with all <code><b>false</b></code> in the vector.


<pre><code><b>public</b> <b>fun</b> <a href="../std/bit_vector.md#std_bit_vector_new">new</a>(<a href="../std/bit_vector.md#std_bit_vector_length">length</a>: <a href="../std/u64.md#std_u64">u64</a>): <a href="../std/bit_vector.md#std_bit_vector_BitVector">std::bit_vector::BitVector</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../std/bit_vector.md#std_bit_vector_new">new</a>(<a href="../std/bit_vector.md#std_bit_vector_length">length</a>: <a href="../std/u64.md#std_u64">u64</a>): <a href="../std/bit_vector.md#std_bit_vector_BitVector">BitVector</a> {
    <b>assert</b>!(<a href="../std/bit_vector.md#std_bit_vector_length">length</a> &gt; 0, <a href="../std/bit_vector.md#std_bit_vector_ELength">ELength</a>);
    <b>assert</b>!(<a href="../std/bit_vector.md#std_bit_vector_length">length</a> &lt; <a href="../std/bit_vector.md#std_bit_vector_MAX_SIZE">MAX_SIZE</a>, <a href="../std/bit_vector.md#std_bit_vector_ELength">ELength</a>);
    <a href="../std/bit_vector.md#std_bit_vector_BitVector">BitVector</a> {
        <a href="../std/bit_vector.md#std_bit_vector_length">length</a>,
        bit_field: <a href="../std/vector.md#std_vector_tabulate">vector::tabulate</a>!(<a href="../std/bit_vector.md#std_bit_vector_length">length</a>, |_| <b>false</b>)
    }
}
</code></pre>



</details>

<a name="std_bit_vector_set"></a>

## Function `set`

Set the bit at <code>bit_index</code> in the <code>bitvector</code> regardless of its previous state.


<pre><code><b>public</b> <b>fun</b> <a href="../std/bit_vector.md#std_bit_vector_set">set</a>(bitvector: &<b>mut</b> <a href="../std/bit_vector.md#std_bit_vector_BitVector">std::bit_vector::BitVector</a>, bit_index: <a href="../std/u64.md#std_u64">u64</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../std/bit_vector.md#std_bit_vector_set">set</a>(bitvector: &<b>mut</b> <a href="../std/bit_vector.md#std_bit_vector_BitVector">BitVector</a>, bit_index: <a href="../std/u64.md#std_u64">u64</a>) {
    <b>assert</b>!(bit_index &lt; bitvector.bit_field.<a href="../std/bit_vector.md#std_bit_vector_length">length</a>(), <a href="../std/bit_vector.md#std_bit_vector_EIndex">EIndex</a>);
    <b>let</b> x = &<b>mut</b> bitvector.bit_field[bit_index];
    *x = <b>true</b>;
}
</code></pre>



</details>

<a name="std_bit_vector_unset"></a>

## Function `unset`

Unset the bit at <code>bit_index</code> in the <code>bitvector</code> regardless of its previous state.


<pre><code><b>public</b> <b>fun</b> <a href="../std/bit_vector.md#std_bit_vector_unset">unset</a>(bitvector: &<b>mut</b> <a href="../std/bit_vector.md#std_bit_vector_BitVector">std::bit_vector::BitVector</a>, bit_index: <a href="../std/u64.md#std_u64">u64</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../std/bit_vector.md#std_bit_vector_unset">unset</a>(bitvector: &<b>mut</b> <a href="../std/bit_vector.md#std_bit_vector_BitVector">BitVector</a>, bit_index: <a href="../std/u64.md#std_u64">u64</a>) {
    <b>assert</b>!(bit_index &lt; bitvector.bit_field.<a href="../std/bit_vector.md#std_bit_vector_length">length</a>(), <a href="../std/bit_vector.md#std_bit_vector_EIndex">EIndex</a>);
    <b>let</b> x = &<b>mut</b> bitvector.bit_field[bit_index];
    *x = <b>false</b>;
}
</code></pre>



</details>

<a name="std_bit_vector_shift_left"></a>

## Function `shift_left`

Shift the <code>bitvector</code> left by <code>amount</code>. If <code>amount</code> is greater than the
bitvector's length the bitvector will be zeroed out.


<pre><code><b>public</b> <b>fun</b> <a href="../std/bit_vector.md#std_bit_vector_shift_left">shift_left</a>(bitvector: &<b>mut</b> <a href="../std/bit_vector.md#std_bit_vector_BitVector">std::bit_vector::BitVector</a>, amount: <a href="../std/u64.md#std_u64">u64</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../std/bit_vector.md#std_bit_vector_shift_left">shift_left</a>(bitvector: &<b>mut</b> <a href="../std/bit_vector.md#std_bit_vector_BitVector">BitVector</a>, amount: <a href="../std/u64.md#std_u64">u64</a>) {
    bitvector.<a href="../std/bit_vector.md#std_bit_vector_length">length</a>.do!(|i| {
        <b>if</b> (i + amount &lt; bitvector.<a href="../std/bit_vector.md#std_bit_vector_length">length</a> && bitvector.<a href="../std/bit_vector.md#std_bit_vector_is_index_set">is_index_set</a>(i + amount)) {
            bitvector.<a href="../std/bit_vector.md#std_bit_vector_set">set</a>(i);
        } <b>else</b> {
            bitvector.<a href="../std/bit_vector.md#std_bit_vector_unset">unset</a>(i);
        };
    });
}
</code></pre>



</details>

<a name="std_bit_vector_is_index_set"></a>

## Function `is_index_set`

Return the value of the bit at <code>bit_index</code> in the <code>bitvector</code>. <code><b>true</b></code>
represents "1" and <code><b>false</b></code> represents "0".


<pre><code><b>public</b> <b>fun</b> <a href="../std/bit_vector.md#std_bit_vector_is_index_set">is_index_set</a>(bitvector: &<a href="../std/bit_vector.md#std_bit_vector_BitVector">std::bit_vector::BitVector</a>, bit_index: <a href="../std/u64.md#std_u64">u64</a>): <a href="../std/bool.md#std_bool">bool</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../std/bit_vector.md#std_bit_vector_is_index_set">is_index_set</a>(bitvector: &<a href="../std/bit_vector.md#std_bit_vector_BitVector">BitVector</a>, bit_index: <a href="../std/u64.md#std_u64">u64</a>): <a href="../std/bool.md#std_bool">bool</a> {
    <b>assert</b>!(bit_index &lt; bitvector.bit_field.<a href="../std/bit_vector.md#std_bit_vector_length">length</a>(), <a href="../std/bit_vector.md#std_bit_vector_EIndex">EIndex</a>);
    bitvector.bit_field[bit_index]
}
</code></pre>



</details>

<a name="std_bit_vector_length"></a>

## Function `length`

Return the length (number of usable bits) of this bitvector


<pre><code><b>public</b> <b>fun</b> <a href="../std/bit_vector.md#std_bit_vector_length">length</a>(bitvector: &<a href="../std/bit_vector.md#std_bit_vector_BitVector">std::bit_vector::BitVector</a>): <a href="../std/u64.md#std_u64">u64</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../std/bit_vector.md#std_bit_vector_length">length</a>(bitvector: &<a href="../std/bit_vector.md#std_bit_vector_BitVector">BitVector</a>): <a href="../std/u64.md#std_u64">u64</a> {
    bitvector.bit_field.<a href="../std/bit_vector.md#std_bit_vector_length">length</a>()
}
</code></pre>



</details>

<a name="std_bit_vector_longest_set_sequence_starting_at"></a>

## Function `longest_set_sequence_starting_at`

Returns the length of the longest sequence of set bits starting at (and
including) <code>start_index</code> in the <code>bitvector</code>. If there is no such
sequence, then <code>0</code> is returned.


<pre><code><b>public</b> <b>fun</b> <a href="../std/bit_vector.md#std_bit_vector_longest_set_sequence_starting_at">longest_set_sequence_starting_at</a>(bitvector: &<a href="../std/bit_vector.md#std_bit_vector_BitVector">std::bit_vector::BitVector</a>, start_index: <a href="../std/u64.md#std_u64">u64</a>): <a href="../std/u64.md#std_u64">u64</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../std/bit_vector.md#std_bit_vector_longest_set_sequence_starting_at">longest_set_sequence_starting_at</a>(bitvector: &<a href="../std/bit_vector.md#std_bit_vector_BitVector">BitVector</a>, start_index: <a href="../std/u64.md#std_u64">u64</a>): <a href="../std/u64.md#std_u64">u64</a> {
    <b>assert</b>!(start_index &lt; bitvector.<a href="../std/bit_vector.md#std_bit_vector_length">length</a>, <a href="../std/bit_vector.md#std_bit_vector_EIndex">EIndex</a>);
    'longest: {
        (bitvector.<a href="../std/bit_vector.md#std_bit_vector_length">length</a> - start_index).do!(|i| <b>if</b> (!bitvector.<a href="../std/bit_vector.md#std_bit_vector_is_index_set">is_index_set</a>(start_index + i)) <b>return</b> 'longest i);
        bitvector.<a href="../std/bit_vector.md#std_bit_vector_length">length</a> - start_index
    }
}
</code></pre>



</details>
