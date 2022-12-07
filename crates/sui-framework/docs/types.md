
<a name="0x2_types"></a>

# Module `0x2::types`

Sui types helpers and utilities


-  [Function `is_one_time_witness`](#0x2_types_is_one_time_witness)
-  [Function `type_tag_bytes`](#0x2_types_type_tag_bytes)


<pre><code></code></pre>



<a name="0x2_types_is_one_time_witness"></a>

## Function `is_one_time_witness`

Tests if the argument type is a one-time witness, that is a type with only one instantiation
across the entire code base.


<pre><code><b>public</b> <b>fun</b> <a href="types.md#0x2_types_is_one_time_witness">is_one_time_witness</a>&lt;T: drop&gt;(_: &T): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>native</b> <b>fun</b> <a href="types.md#0x2_types_is_one_time_witness">is_one_time_witness</a>&lt;T: drop&gt;(_: &T): bool;
</code></pre>



</details>

<a name="0x2_types_type_tag_bytes"></a>

## Function `type_tag_bytes`



<pre><code><b>public</b> <b>fun</b> <a href="types.md#0x2_types_type_tag_bytes">type_tag_bytes</a>&lt;T&gt;(): <a href="">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>native</b> <b>fun</b> <a href="types.md#0x2_types_type_tag_bytes">type_tag_bytes</a>&lt;T&gt;(): <a href="">vector</a>&lt;u8&gt;;
</code></pre>



</details>
