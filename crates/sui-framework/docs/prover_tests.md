
<a name="0x2_prover_tests"></a>

# Module `0x2::prover_tests`



-  [Resource `Obj`](#0x2_prover_tests_Obj)
-  [Function `simple_transfer`](#0x2_prover_tests_simple_transfer)
-  [Function `simple_share`](#0x2_prover_tests_simple_share)
-  [Function `simple_freeze`](#0x2_prover_tests_simple_freeze)
-  [Function `simple_delete`](#0x2_prover_tests_simple_delete)
-  [Function `simple_field_add`](#0x2_prover_tests_simple_field_add)
-  [Function `simple_field_remove`](#0x2_prover_tests_simple_field_remove)


<pre><code><b>use</b> <a href="dynamic_field.md#0x2_dynamic_field">0x2::dynamic_field</a>;
<b>use</b> <a href="object.md#0x2_object">0x2::object</a>;
<b>use</b> <a href="transfer.md#0x2_transfer">0x2::transfer</a>;
</code></pre>



<a name="0x2_prover_tests_Obj"></a>

## Resource `Obj`



<pre><code><b>struct</b> <a href="prover_tests.md#0x2_prover_tests_Obj">Obj</a> <b>has</b> store, key
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>id: <a href="object.md#0x2_object_UID">object::UID</a></code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x2_prover_tests_simple_transfer"></a>

## Function `simple_transfer`



<pre><code><b>public</b> <b>fun</b> <a href="prover_tests.md#0x2_prover_tests_simple_transfer">simple_transfer</a>(o: <a href="prover_tests.md#0x2_prover_tests_Obj">prover_tests::Obj</a>, recipient: <b>address</b>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="prover_tests.md#0x2_prover_tests_simple_transfer">simple_transfer</a>(o: <a href="prover_tests.md#0x2_prover_tests_Obj">Obj</a>, recipient: <b>address</b>) {
    sui::transfer::transfer(o, recipient);
}
</code></pre>



</details>

<details>
<summary>Specification</summary>



<pre><code><b>ensures</b> sui::prover::owned_by(o, recipient);
<b>aborts_if</b> <b>false</b>;
</code></pre>



</details>

<a name="0x2_prover_tests_simple_share"></a>

## Function `simple_share`



<pre><code><b>public</b> <b>fun</b> <a href="prover_tests.md#0x2_prover_tests_simple_share">simple_share</a>(o: <a href="prover_tests.md#0x2_prover_tests_Obj">prover_tests::Obj</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="prover_tests.md#0x2_prover_tests_simple_share">simple_share</a>(o: <a href="prover_tests.md#0x2_prover_tests_Obj">Obj</a>) {
    sui::transfer::share_object(o)
}
</code></pre>



</details>

<details>
<summary>Specification</summary>



<pre><code><b>ensures</b> sui::prover::shared(o);
<b>aborts_if</b> sui::prover::owned(o);
</code></pre>



</details>

<a name="0x2_prover_tests_simple_freeze"></a>

## Function `simple_freeze`



<pre><code><b>public</b> <b>fun</b> <a href="prover_tests.md#0x2_prover_tests_simple_freeze">simple_freeze</a>(o: <a href="prover_tests.md#0x2_prover_tests_Obj">prover_tests::Obj</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="prover_tests.md#0x2_prover_tests_simple_freeze">simple_freeze</a>(o: <a href="prover_tests.md#0x2_prover_tests_Obj">Obj</a>) {
    sui::transfer::freeze_object(o)
}
</code></pre>



</details>

<details>
<summary>Specification</summary>



<pre><code><b>ensures</b> sui::prover::immutable(o);
<b>aborts_if</b> <b>false</b>;
</code></pre>



</details>

<a name="0x2_prover_tests_simple_delete"></a>

## Function `simple_delete`



<pre><code><b>public</b> <b>fun</b> <a href="prover_tests.md#0x2_prover_tests_simple_delete">simple_delete</a>(o: <a href="prover_tests.md#0x2_prover_tests_Obj">prover_tests::Obj</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="prover_tests.md#0x2_prover_tests_simple_delete">simple_delete</a>(o: <a href="prover_tests.md#0x2_prover_tests_Obj">Obj</a>) {
    <b>let</b> <a href="prover_tests.md#0x2_prover_tests_Obj">Obj</a> { id } = o;
    sui::object::delete(id);
}
</code></pre>



</details>

<details>
<summary>Specification</summary>



<pre><code><b>aborts_if</b> <b>false</b>;
<b>ensures</b> !sui::prover::owned(o) && !sui::prover::shared(o) && !sui::prover::immutable(o);
</code></pre>



</details>

<a name="0x2_prover_tests_simple_field_add"></a>

## Function `simple_field_add`



<pre><code><b>public</b> <b>fun</b> <a href="prover_tests.md#0x2_prover_tests_simple_field_add">simple_field_add</a>(o: &<b>mut</b> <a href="prover_tests.md#0x2_prover_tests_Obj">prover_tests::Obj</a>, n1: u64, v1: u8, n2: u8, v2: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="prover_tests.md#0x2_prover_tests_simple_field_add">simple_field_add</a>(o: &<b>mut</b> <a href="prover_tests.md#0x2_prover_tests_Obj">Obj</a>, n1: u64, v1: u8, n2: u8, v2: u64) {
    sui::dynamic_field::add(&<b>mut</b> o.id, n1, v1);
    sui::dynamic_field::add(&<b>mut</b> o.id, n2, v2);
}
</code></pre>



</details>

<details>
<summary>Specification</summary>



<pre><code><b>aborts_if</b> sui::prover::has_field(o, n1);
<b>aborts_if</b> sui::prover::has_field(o, n2);
<b>ensures</b> sui::prover::has_field(o, n1);
<b>ensures</b> sui::prover::has_field(o, n2);
<b>ensures</b> sui::prover::num_fields&lt;<a href="prover_tests.md#0x2_prover_tests_Obj">Obj</a>,u64&gt;(o) == <b>old</b>(sui::prover::num_fields&lt;<a href="prover_tests.md#0x2_prover_tests_Obj">Obj</a>,u64&gt;(o)) + 1;
<b>ensures</b> sui::prover::num_fields&lt;<a href="prover_tests.md#0x2_prover_tests_Obj">Obj</a>,u8&gt;(o) == <b>old</b>(sui::prover::num_fields&lt;<a href="prover_tests.md#0x2_prover_tests_Obj">Obj</a>,u8&gt;(o)) + 1;
</code></pre>



</details>

<a name="0x2_prover_tests_simple_field_remove"></a>

## Function `simple_field_remove`



<pre><code><b>public</b> <b>fun</b> <a href="prover_tests.md#0x2_prover_tests_simple_field_remove">simple_field_remove</a>(o: &<b>mut</b> <a href="prover_tests.md#0x2_prover_tests_Obj">prover_tests::Obj</a>, n1: u64, n2: u8)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="prover_tests.md#0x2_prover_tests_simple_field_remove">simple_field_remove</a>(o: &<b>mut</b> <a href="prover_tests.md#0x2_prover_tests_Obj">Obj</a>, n1: u64, n2: u8) {
    sui::dynamic_field::remove&lt;u64,u8&gt;(&<b>mut</b> o.id, n1);
    sui::dynamic_field::remove&lt;u8,u64&gt;(&<b>mut</b> o.id, n2);
}
</code></pre>



</details>

<details>
<summary>Specification</summary>



<pre><code><b>aborts_if</b> !sui::prover::has_field(o, n1);
<b>aborts_if</b> !sui::prover::has_field(o, n2);
<b>ensures</b> !sui::prover::has_field(o, n1);
<b>ensures</b> !sui::prover::has_field(o, n2);
<b>ensures</b> sui::prover::num_fields&lt;<a href="prover_tests.md#0x2_prover_tests_Obj">Obj</a>,u64&gt;(o) == <b>old</b>(sui::prover::num_fields&lt;<a href="prover_tests.md#0x2_prover_tests_Obj">Obj</a>,u64&gt;(o)) - 1;
<b>ensures</b> sui::prover::num_fields&lt;<a href="prover_tests.md#0x2_prover_tests_Obj">Obj</a>,u8&gt;(o) == <b>old</b>(sui::prover::num_fields&lt;<a href="prover_tests.md#0x2_prover_tests_Obj">Obj</a>,u8&gt;(o)) - 1;
</code></pre>



</details>
