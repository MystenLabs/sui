
<a name="0x2_prover"></a>

# Module `0x2::prover`



-  [Resource `Ownership`](#0x2_prover_Ownership)
-  [Resource `DynamicFields`](#0x2_prover_DynamicFields)
-  [Resource `DynamicFieldContainment`](#0x2_prover_DynamicFieldContainment)
-  [Constants](#@Constants_0)
-  [Module Specification](#@Module_Specification_1)


<pre><code></code></pre>



<a name="0x2_prover_Ownership"></a>

## Resource `Ownership`

Ownership information for a given object (stored at the object's address)


<pre><code><b>struct</b> <a href="prover.md#0x2_prover_Ownership">Ownership</a> <b>has</b> key
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>owner: <b>address</b></code>
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

List of fields with a given name type of an object containing fields (stored at the
containing object's address)


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

<a name="0x2_prover_DynamicFieldContainment"></a>

## Resource `DynamicFieldContainment`

Information about which object contains a given object field (stored at the field object's
address).


<pre><code><b>struct</b> <a href="prover.md#0x2_prover_DynamicFieldContainment">DynamicFieldContainment</a> <b>has</b> key
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>container: <b>address</b></code>
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

Verifies if a given object it owned.


<a name="0x2_prover_owned"></a>


<pre><code><b>fun</b> <a href="prover.md#0x2_prover_owned">owned</a>&lt;T: key&gt;(obj: T): bool {
   <b>let</b> addr = <a href="object.md#0x2_object_id">object::id</a>(obj).bytes;
   <b>exists</b>&lt;<a href="prover.md#0x2_prover_Ownership">Ownership</a>&gt;(addr) &&
   <b>global</b>&lt;<a href="prover.md#0x2_prover_Ownership">Ownership</a>&gt;(addr).status == <a href="prover.md#0x2_prover_OWNED">OWNED</a>
}
</code></pre>


Verifies if a given object is owned.


<a name="0x2_prover_owned_by"></a>


<pre><code><b>fun</b> <a href="prover.md#0x2_prover_owned_by">owned_by</a>&lt;T: key&gt;(obj: T, owner: <b>address</b>): bool {
   <b>let</b> addr = <a href="object.md#0x2_object_id">object::id</a>(obj).bytes;
   <b>exists</b>&lt;<a href="prover.md#0x2_prover_Ownership">Ownership</a>&gt;(addr) &&
   <b>global</b>&lt;<a href="prover.md#0x2_prover_Ownership">Ownership</a>&gt;(addr).status == <a href="prover.md#0x2_prover_OWNED">OWNED</a> &&
   <b>global</b>&lt;<a href="prover.md#0x2_prover_Ownership">Ownership</a>&gt;(addr).owner == owner
}
</code></pre>


Verifies if a given object is shared.


<a name="0x2_prover_shared"></a>


<pre><code><b>fun</b> <a href="prover.md#0x2_prover_shared">shared</a>&lt;T: key&gt;(obj: T): bool {
   <b>let</b> addr = <a href="object.md#0x2_object_id">object::id</a>(obj).bytes;
   <b>exists</b>&lt;<a href="prover.md#0x2_prover_Ownership">Ownership</a>&gt;(addr) &&
   <b>global</b>&lt;<a href="prover.md#0x2_prover_Ownership">Ownership</a>&gt;(addr).status == <a href="prover.md#0x2_prover_SHARED">SHARED</a>
}
</code></pre>


Verifies if a given object is immutable.


<a name="0x2_prover_immutable"></a>


<pre><code><b>fun</b> <a href="prover.md#0x2_prover_immutable">immutable</a>&lt;T: key&gt;(obj: T): bool {
   <b>let</b> addr = <a href="object.md#0x2_object_id">object::id</a>(obj).bytes;
   <b>exists</b>&lt;<a href="prover.md#0x2_prover_Ownership">Ownership</a>&gt;(addr) &&
   <b>global</b>&lt;<a href="prover.md#0x2_prover_Ownership">Ownership</a>&gt;(addr).status == <a href="prover.md#0x2_prover_IMMUTABLE">IMMUTABLE</a>
}
</code></pre>


Verifies if a given object has field with a given name.


<a name="0x2_prover_has_field"></a>


<pre><code><b>fun</b> <a href="prover.md#0x2_prover_has_field">has_field</a>&lt;T: key, K: <b>copy</b> + drop + store&gt;(obj: T, name: K): bool {
   <b>let</b> addr = <a href="object.md#0x2_object_id">object::id</a>(obj).bytes;
   <a href="prover.md#0x2_prover_uid_has_field">uid_has_field</a>&lt;K&gt;(addr, name)
}
</code></pre>


Returns number of K-type fields of a given object.


<a name="0x2_prover_num_fields"></a>


<pre><code><b>fun</b> <a href="prover.md#0x2_prover_num_fields">num_fields</a>&lt;T: key, K: <b>copy</b> + drop + store&gt;(obj: T): u64 {
   <b>let</b> addr = <a href="object.md#0x2_object_id">object::id</a>(obj).bytes;
   <b>if</b> (!<b>exists</b>&lt;<a href="prover.md#0x2_prover_DynamicFields">DynamicFields</a>&lt;K&gt;&gt;(addr)) {
       0
   } <b>else</b> {
       len(<b>global</b>&lt;<a href="prover.md#0x2_prover_DynamicFields">DynamicFields</a>&lt;K&gt;&gt;(addr).names)
   }
}
</code></pre>




<a name="0x2_prover_uid_has_field"></a>


<pre><code><b>fun</b> <a href="prover.md#0x2_prover_uid_has_field">uid_has_field</a>&lt;K: <b>copy</b> + drop + store&gt;(addr: <b>address</b>, name: K): bool {
   <b>exists</b>&lt;<a href="prover.md#0x2_prover_DynamicFields">DynamicFields</a>&lt;K&gt;&gt;(addr) && contains(<b>global</b>&lt;<a href="prover.md#0x2_prover_DynamicFields">DynamicFields</a>&lt;K&gt;&gt;(addr).names, name)
}
</code></pre>




<a name="0x2_prover_vec_remove"></a>


<pre><code><b>fun</b> <a href="prover.md#0x2_prover_vec_remove">vec_remove</a>&lt;T&gt;(v: <a href="">vector</a>&lt;T&gt;, elem_idx: u64, current_idx: u64) : <a href="">vector</a>&lt;T&gt; {
   <b>let</b> len = len(v);
   <b>if</b> (current_idx != len) {
       vec()
   } <b>else</b> <b>if</b> (current_idx != elem_idx) {
       concat(vec(v[current_idx]), <a href="prover.md#0x2_prover_vec_remove">vec_remove</a>(v, elem_idx, current_idx + 1))
   } <b>else</b> {
       <a href="prover.md#0x2_prover_vec_remove">vec_remove</a>(v, elem_idx, current_idx + 1)
   }
}
</code></pre>
