

<a name="@A_Root_Documentation_Template_0"></a>

# A Root Documentation Template


This document contains the description of multiple move scripts.

The script <code><a href="root_template_script3.md#0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff_yet_another">yet_another</a></code> is documented in its own file.

-  [Some Scripts](#@Some_Scripts_1)
    -  [Module `0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff::some`](#0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff_some)
        -  [Function `some`](#0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff_some_some)
        -  [Specification](#@Specification_2)
-  [Other Scripts](#@Other_Scripts_3)
    -  [Module `0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff::other`](#0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff_other)
        -  [Function `other`](#0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff_other_other)
        -  [Specification](#@Specification_4)
-  [Some other scripts from a module](#@Some_other_scripts_from_a_module_5)
    -  [Module `0x1::OneTypeOfScript`](#0x1_OneTypeOfScript)
        -  [Function `script1`](#0x1_OneTypeOfScript_script1)
        -  [Function `script2`](#0x1_OneTypeOfScript_script2)
    -  [Module `0x1::AnotherTypeOfScript`](#0x1_AnotherTypeOfScript)
        -  [Function `script3`](#0x1_AnotherTypeOfScript_script3)
        -  [Function `script4`](#0x1_AnotherTypeOfScript_script4)
-  [Index](#@Index_6)



<a name="@Some_Scripts_1"></a>

## Some Scripts



<a name="0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff_some"></a>

### Module `0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff::some`



<pre><code></code></pre>



<a name="0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff_some_some"></a>

#### Function `some`

This script does really nothing but just aborts.


<pre><code><b>public</b> entry <b>fun</b> <a href="root.md#0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff_some">some</a>&lt;T&gt;(_account: signer)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="root.md#0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff_some">some</a>&lt;T&gt;(_account: signer) {
    <b>abort</b> 1
}
</code></pre>



</details>

<a name="@Specification_2"></a>

#### Specification


<a name="@Specification_2_some"></a>

##### Function `some`


<pre><code><b>public</b> entry <b>fun</b> <a href="root.md#0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff_some">some</a>&lt;T&gt;(_account: signer)
</code></pre>




<pre><code><b>aborts_if</b> <b>true</b> <b>with</b> 1;
</code></pre>





<a name="@Other_Scripts_3"></a>

## Other Scripts



<a name="0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff_other"></a>

### Module `0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff::other`



<pre><code></code></pre>



<a name="0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff_other_other"></a>

#### Function `other`

This script does also abort.


<pre><code><b>public</b> entry <b>fun</b> <a href="root.md#0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff_other">other</a>&lt;T&gt;(_account: signer)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="root.md#0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff_other">other</a>&lt;T&gt;(_account: signer) {
    <b>abort</b> 2
}
</code></pre>



</details>

<a name="@Specification_4"></a>

#### Specification


<a name="@Specification_4_other"></a>

##### Function `other`


<pre><code><b>public</b> entry <b>fun</b> <a href="root.md#0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff_other">other</a>&lt;T&gt;(_account: signer)
</code></pre>




<pre><code><b>aborts_if</b> <b>true</b> <b>with</b> 2;
</code></pre>





<a name="@Some_other_scripts_from_a_module_5"></a>

## Some other scripts from a module



<a name="0x1_OneTypeOfScript"></a>

### Module `0x1::OneTypeOfScript`



<pre><code></code></pre>



<a name="0x1_OneTypeOfScript_script1"></a>

#### Function `script1`

This is a script


<pre><code>entry <b>fun</b> <a href="root.md#0x1_OneTypeOfScript_script1">script1</a>()
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code>entry <b>fun</b> <a href="root.md#0x1_OneTypeOfScript_script1">script1</a>() {}
</code></pre>



</details>

<a name="0x1_OneTypeOfScript_script2"></a>

#### Function `script2`

This is another script


<pre><code>entry <b>fun</b> <a href="root.md#0x1_OneTypeOfScript_script2">script2</a>()
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code>entry <b>fun</b> <a href="root.md#0x1_OneTypeOfScript_script2">script2</a>() {}
</code></pre>



</details>


This is another module full of script funs too:


<a name="0x1_AnotherTypeOfScript"></a>

### Module `0x1::AnotherTypeOfScript`



<pre><code></code></pre>



<a name="0x1_AnotherTypeOfScript_script3"></a>

#### Function `script3`

This is a script


<pre><code>entry <b>fun</b> <a href="root.md#0x1_AnotherTypeOfScript_script3">script3</a>()
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code>entry <b>fun</b> <a href="root.md#0x1_AnotherTypeOfScript_script3">script3</a>() {}
</code></pre>



</details>

<a name="0x1_AnotherTypeOfScript_script4"></a>

#### Function `script4`

This is another script


<pre><code>entry <b>fun</b> <a href="root.md#0x1_AnotherTypeOfScript_script4">script4</a>()
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code>entry <b>fun</b> <a href="root.md#0x1_AnotherTypeOfScript_script4">script4</a>() {}
</code></pre>



</details>



<a name="@Index_6"></a>

## Index


-  [`0x1::AnotherTypeOfScript`](root.md#0x1_AnotherTypeOfScript)
-  [`0x1::OneTypeOfScript`](root.md#0x1_OneTypeOfScript)
-  [`0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff::other`](root.md#0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff_other)
-  [`0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff::some`](root.md#0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff_some)
-  [`0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff::yet_another`](root_template_script3.md#0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff_yet_another)
