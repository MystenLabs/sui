
<a name="0x2_prover"></a>

# Module `0x2::prover`



-  [Constants](#@Constants_0)
-  [Module Specification](#@Module_Specification_1)


<pre><code></code></pre>



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
   <b>exists</b>&lt;<a href="object.md#0x2_object_Ownership">object::Ownership</a>&gt;(addr) &&
   <b>global</b>&lt;<a href="object.md#0x2_object_Ownership">object::Ownership</a>&gt;(addr).status == <a href="prover.md#0x2_prover_OWNED">OWNED</a>
}
</code></pre>


Verifies if a given object is owned.


<a name="0x2_prover_owned_by"></a>


<pre><code><b>fun</b> <a href="prover.md#0x2_prover_owned_by">owned_by</a>&lt;T: key&gt;(obj: T, owner: <b>address</b>): bool {
   <b>let</b> addr = <a href="object.md#0x2_object_id">object::id</a>(obj).bytes;
   <b>exists</b>&lt;<a href="object.md#0x2_object_Ownership">object::Ownership</a>&gt;(addr) &&
   <b>global</b>&lt;<a href="object.md#0x2_object_Ownership">object::Ownership</a>&gt;(addr).status == <a href="prover.md#0x2_prover_OWNED">OWNED</a> &&
   <b>global</b>&lt;<a href="object.md#0x2_object_Ownership">object::Ownership</a>&gt;(addr).owner == owner
}
</code></pre>


Verifies if a given object is shared.


<a name="0x2_prover_shared"></a>


<pre><code><b>fun</b> <a href="prover.md#0x2_prover_shared">shared</a>&lt;T: key&gt;(obj: T): bool {
   <b>let</b> addr = <a href="object.md#0x2_object_id">object::id</a>(obj).bytes;
   <b>exists</b>&lt;<a href="object.md#0x2_object_Ownership">object::Ownership</a>&gt;(addr) &&
   <b>global</b>&lt;<a href="object.md#0x2_object_Ownership">object::Ownership</a>&gt;(addr).status == <a href="prover.md#0x2_prover_SHARED">SHARED</a>
}
</code></pre>


Verifies if a given object is immutable.


<a name="0x2_prover_immutable"></a>


<pre><code><b>fun</b> <a href="prover.md#0x2_prover_immutable">immutable</a>&lt;T: key&gt;(obj: T): bool {
   <b>let</b> addr = <a href="object.md#0x2_object_id">object::id</a>(obj).bytes;
   <b>exists</b>&lt;<a href="object.md#0x2_object_Ownership">object::Ownership</a>&gt;(addr) &&
   <b>global</b>&lt;<a href="object.md#0x2_object_Ownership">object::Ownership</a>&gt;(addr).status == <a href="prover.md#0x2_prover_IMMUTABLE">IMMUTABLE</a>
}
</code></pre>


Verifies if a given object has field with a given name.


<a name="0x2_prover_has_field"></a>


<pre><code><b>fun</b> <a href="prover.md#0x2_prover_has_field">has_field</a>&lt;T: key, K: <b>copy</b> + drop + store&gt;(obj: T, name: K): bool {
   <b>let</b> uid = <a href="object.md#0x2_object_borrow_uid">object::borrow_uid</a>(obj);
   <a href="prover.md#0x2_prover_uid_has_field">uid_has_field</a>&lt;K&gt;(uid, name)
}
</code></pre>


Returns number of K-type fields of a given object.


<a name="0x2_prover_num_fields"></a>


<pre><code><b>fun</b> <a href="prover.md#0x2_prover_num_fields">num_fields</a>&lt;T: key, K: <b>copy</b> + drop + store&gt;(obj: T): u64 {
   <b>let</b> uid = <a href="object.md#0x2_object_borrow_uid">object::borrow_uid</a>(obj);
   <a href="prover.md#0x2_prover_uid_num_fields">uid_num_fields</a>&lt;K&gt;(uid)
}
</code></pre>




<a name="0x2_prover_uid_has_field"></a>


<pre><code><b>fun</b> <a href="prover.md#0x2_prover_uid_has_field">uid_has_field</a>&lt;K: <b>copy</b> + drop + store&gt;(uid: sui::object::UID, name: K): bool {
   <b>let</b> addr = <a href="object.md#0x2_object_uid_to_address">object::uid_to_address</a>(uid);
   <b>exists</b>&lt;<a href="object.md#0x2_object_DynamicFields">object::DynamicFields</a>&lt;K&gt;&gt;(addr) && contains(<b>global</b>&lt;<a href="object.md#0x2_object_DynamicFields">object::DynamicFields</a>&lt;K&gt;&gt;(addr).names, name)
}
</code></pre>




<a name="0x2_prover_uid_num_fields"></a>


<pre><code><b>fun</b> <a href="prover.md#0x2_prover_uid_num_fields">uid_num_fields</a>&lt;K: <b>copy</b> + drop + store&gt;(uid: sui::object::UID): u64 {
   <b>let</b> addr = <a href="object.md#0x2_object_uid_to_address">object::uid_to_address</a>(uid);
   <b>if</b> (!<b>exists</b>&lt;<a href="object.md#0x2_object_DynamicFields">object::DynamicFields</a>&lt;K&gt;&gt;(addr)) {
       0
   } <b>else</b> {
       len(<b>global</b>&lt;<a href="object.md#0x2_object_DynamicFields">object::DynamicFields</a>&lt;K&gt;&gt;(addr).names)
   }
}
</code></pre>




<a name="0x2_prover_vec_remove"></a>


<pre><code><b>native</b> <b>fun</b> <a href="prover.md#0x2_prover_vec_remove">vec_remove</a>&lt;T&gt;(v: <a href="">vector</a>&lt;T&gt;, elem_idx: u64): <a href="">vector</a>&lt;T&gt;;
</code></pre>
