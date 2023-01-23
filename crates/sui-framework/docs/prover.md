
<a name="0x2_prover"></a>

# Module `0x2::prover`



-  [Resource `Ownership`](#0x2_prover_Ownership)
-  [Resource `DynamicFields`](#0x2_prover_DynamicFields)
-  [Constants](#@Constants_0)
-  [Module Specification](#@Module_Specification_1)


<pre><code><b>use</b> <a href="">0x1::option</a>;
</code></pre>



<a name="0x2_prover_Ownership"></a>

## Resource `Ownership`



<pre><code><b>struct</b> <a href="prover.md#0x2_prover_Ownership">Ownership</a> <b>has</b> key
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>owner: <a href="_Option">option::Option</a>&lt;<b>address</b>&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>status: u64</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x2_prover_DynamicFields"></a>

## Resource `DynamicFields`



<pre><code><b>struct</b> <a href="prover.md#0x2_prover_DynamicFields">DynamicFields</a>&lt;K: <b>copy</b>, drop, store&gt; <b>has</b> key
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>names: <a href="">vector</a>&lt;K&gt;</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0x2_prover_IMMUTABLE"></a>



<pre><code><b>const</b> <a href="prover.md#0x2_prover_IMMUTABLE">IMMUTABLE</a>: u64 = 3;
</code></pre>



<a name="0x2_prover_OWNED"></a>



<pre><code><b>const</b> <a href="prover.md#0x2_prover_OWNED">OWNED</a>: u64 = 1;
</code></pre>



<a name="0x2_prover_SHARED"></a>



<pre><code><b>const</b> <a href="prover.md#0x2_prover_SHARED">SHARED</a>: u64 = 2;
</code></pre>



<a name="@Module_Specification_1"></a>

## Module Specification



<a name="0x2_prover_owned"></a>


<pre><code><b>fun</b> <a href="prover.md#0x2_prover_owned">owned</a>&lt;T: key&gt;(obj: T): bool {
   <b>let</b> addr = <a href="object.md#0x2_object_id">object::id</a>(obj).bytes;
   <b>exists</b>&lt;<a href="prover.md#0x2_prover_Ownership">Ownership</a>&gt;(addr) &&
   <a href="_is_some">option::is_some</a>(<b>global</b>&lt;<a href="prover.md#0x2_prover_Ownership">Ownership</a>&gt;(addr).owner) &&
   <b>global</b>&lt;<a href="prover.md#0x2_prover_Ownership">Ownership</a>&gt;(addr).status == <a href="prover.md#0x2_prover_OWNED">OWNED</a>
}
</code></pre>




<a name="0x2_prover_owned_by"></a>


<pre><code><b>fun</b> <a href="prover.md#0x2_prover_owned_by">owned_by</a>&lt;T: key&gt;(obj: T, owner: <b>address</b>): bool {
   <b>let</b> addr = <a href="object.md#0x2_object_id">object::id</a>(obj).bytes;
   <b>exists</b>&lt;<a href="prover.md#0x2_prover_Ownership">Ownership</a>&gt;(addr) &&
   <b>global</b>&lt;<a href="prover.md#0x2_prover_Ownership">Ownership</a>&gt;(addr).owner == <a href="_spec_some">option::spec_some</a>(owner) &&
   <b>global</b>&lt;<a href="prover.md#0x2_prover_Ownership">Ownership</a>&gt;(addr).status == <a href="prover.md#0x2_prover_OWNED">OWNED</a>
}
</code></pre>




<a name="0x2_prover_shared"></a>


<pre><code><b>fun</b> <a href="prover.md#0x2_prover_shared">shared</a>&lt;T: key&gt;(obj: T): bool {
   <b>let</b> addr = <a href="object.md#0x2_object_id">object::id</a>(obj).bytes;
   <b>exists</b>&lt;<a href="prover.md#0x2_prover_Ownership">Ownership</a>&gt;(addr) &&
   <a href="_is_none">option::is_none</a>(<b>global</b>&lt;<a href="prover.md#0x2_prover_Ownership">Ownership</a>&gt;(addr).owner) &&
   <b>global</b>&lt;<a href="prover.md#0x2_prover_Ownership">Ownership</a>&gt;(addr).status == <a href="prover.md#0x2_prover_SHARED">SHARED</a>
}
</code></pre>




<a name="0x2_prover_immutable"></a>


<pre><code><b>fun</b> <a href="prover.md#0x2_prover_immutable">immutable</a>&lt;T: key&gt;(obj: T): bool {
   <b>let</b> addr = <a href="object.md#0x2_object_id">object::id</a>(obj).bytes;
   <b>exists</b>&lt;<a href="prover.md#0x2_prover_Ownership">Ownership</a>&gt;(addr) &&
   <a href="_is_none">option::is_none</a>(<b>global</b>&lt;<a href="prover.md#0x2_prover_Ownership">Ownership</a>&gt;(addr).owner) &&
   <b>global</b>&lt;<a href="prover.md#0x2_prover_Ownership">Ownership</a>&gt;(addr).status == <a href="prover.md#0x2_prover_IMMUTABLE">IMMUTABLE</a>
}
</code></pre>




<a name="0x2_prover_uid_has_field"></a>


<pre><code><b>fun</b> <a href="prover.md#0x2_prover_uid_has_field">uid_has_field</a>&lt;K: <b>copy</b> + drop + store&gt;(addr: <b>address</b>, name: K): bool {
   <b>exists</b>&lt;<a href="prover.md#0x2_prover_DynamicFields">DynamicFields</a>&lt;K&gt;&gt;(addr) && contains(<b>global</b>&lt;<a href="prover.md#0x2_prover_DynamicFields">DynamicFields</a>&lt;K&gt;&gt;(addr).names, name)
}
</code></pre>




<a name="0x2_prover_has_field"></a>


<pre><code><b>fun</b> <a href="prover.md#0x2_prover_has_field">has_field</a>&lt;T: key, K: <b>copy</b> + drop + store&gt;(obj: T, name: K): bool {
   <b>let</b> addr = <a href="object.md#0x2_object_id">object::id</a>(obj).bytes;
   <a href="prover.md#0x2_prover_uid_has_field">uid_has_field</a>&lt;K&gt;(addr, name)
}
</code></pre>




<a name="0x2_prover_always_true"></a>


<pre><code><b>fun</b> <a href="prover.md#0x2_prover_always_true">always_true</a>&lt;K: <b>copy</b> + drop + store&gt;(): bool {
   <b>exists</b>&lt;<a href="prover.md#0x2_prover_DynamicFields">DynamicFields</a>&lt;K&gt;&gt;(@0x42) || !<b>exists</b>&lt;<a href="prover.md#0x2_prover_DynamicFields">DynamicFields</a>&lt;K&gt;&gt;(@0x42)
}
</code></pre>
