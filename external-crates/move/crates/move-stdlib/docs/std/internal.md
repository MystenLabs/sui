
<a name="std_internal"></a>

# Module `std::internal`

Defines the <code><a href="../std/internal.md#std_internal_Permit">Permit</a></code> type, which can be used to constrain the logic of a
generic function to be authorized only by the module that defines the type
parameter.

```move
module example::use_permit;

public struct MyType { /* ... */ }

public fun test_permit() {
let permit = internal::permit<MyType>();
/* external_module::call_with_permit(permit); */
}
```

To write a function that is guarded by a <code><a href="../std/internal.md#std_internal_Permit">Permit</a></code>, require it as an argument.

```move
// Silly mockup of a type registry where a type can be registered only by
// the module that defines the type.
module example::type_registry;

public fun register_type<T>(_: internal::Permit<T> /* ... */) {
/* ... */
}
```


-  [Struct `Permit`](#std_internal_Permit)
-  [Function `permit`](#std_internal_permit)


<pre><code></code></pre>



<a name="std_internal_Permit"></a>

## Struct `Permit`

A privileged witness of the <code>T</code> type.
Instances can only be created by the module that defines the type <code>T</code>.


<pre><code><b>public</b> <b>struct</b> <a href="../std/internal.md#std_internal_Permit">Permit</a>&lt;<b>phantom</b> T&gt; <b>has</b> drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
</dl>


</details>

<a name="std_internal_permit"></a>

## Function `permit`

Construct a new <code><a href="../std/internal.md#std_internal_Permit">Permit</a></code> for the type <code>T</code>.
Can only be called by the module that defines the type <code>T</code>.


<pre><code><b>public</b> <b>fun</b> <a href="../std/internal.md#std_internal_permit">permit</a>&lt;T&gt;(): <a href="../std/internal.md#std_internal_Permit">std::internal::Permit</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../std/internal.md#std_internal_permit">permit</a>&lt;T&gt;(): <a href="../std/internal.md#std_internal_Permit">Permit</a>&lt;T&gt; { <a href="../std/internal.md#std_internal_Permit">Permit</a>() }
</code></pre>



</details>


[//]: # ("File containing references which can be used from documentation")
