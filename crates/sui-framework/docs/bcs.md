
<a name="0x2_bcs"></a>

# Module `0x2::bcs`

This module implements BCS (de)serialization in Move.
Full specification can be found here: https://github.com/diem/bcs

Short summary (for Move-supported types):

- address - sequence of X bytes
- bool - byte with 0 or 1
- u8 - a single u8 byte
- u64 / u128 - LE bytes
- vector - ULEB128 length + LEN elements
- option - first byte bool: None (0) or Some (1), then value


-  [Constants](#@Constants_0)
-  [Function `to_bytes`](#0x2_bcs_to_bytes)
-  [Function `peel_address`](#0x2_bcs_peel_address)
-  [Function `peel_bool`](#0x2_bcs_peel_bool)
-  [Function `peel_u8`](#0x2_bcs_peel_u8)
-  [Function `peel_u64`](#0x2_bcs_peel_u64)
-  [Function `peel_u128`](#0x2_bcs_peel_u128)
-  [Function `peel_vec_length`](#0x2_bcs_peel_vec_length)
-  [Function `peel_vec_address`](#0x2_bcs_peel_vec_address)
-  [Function `peel_vec_bool`](#0x2_bcs_peel_vec_bool)
-  [Function `peel_vec_u8`](#0x2_bcs_peel_vec_u8)
-  [Function `peel_vec_u64`](#0x2_bcs_peel_vec_u64)
-  [Function `peel_vec_u128`](#0x2_bcs_peel_vec_u128)
-  [Function `peel_option_address`](#0x2_bcs_peel_option_address)
-  [Function `peel_option_bool`](#0x2_bcs_peel_option_bool)
-  [Function `peel_option_u8`](#0x2_bcs_peel_option_u8)
-  [Function `peel_option_u64`](#0x2_bcs_peel_option_u64)
-  [Function `peel_option_u128`](#0x2_bcs_peel_option_u128)


<pre><code><b>use</b> <a href="">0x1::bcs</a>;
<b>use</b> <a href="">0x1::option</a>;
<b>use</b> <a href="">0x1::vector</a>;
<b>use</b> <a href="object.md#0x2_object">0x2::object</a>;
</code></pre>



<a name="@Constants_0"></a>

## Constants


<a name="0x2_bcs_ELenOutOfRange"></a>

For when ULEB byte is out of range (or not found).


<pre><code><b>const</b> <a href="bcs.md#0x2_bcs_ELenOutOfRange">ELenOutOfRange</a>: u64 = 2;
</code></pre>



<a name="0x2_bcs_ENotBool"></a>

For when the boolean value different than <code>0</code> or <code>1</code>.


<pre><code><b>const</b> <a href="bcs.md#0x2_bcs_ENotBool">ENotBool</a>: u64 = 1;
</code></pre>



<a name="0x2_bcs_EOutOfRange"></a>

For when bytes length is less than required for deserialization.


<pre><code><b>const</b> <a href="bcs.md#0x2_bcs_EOutOfRange">EOutOfRange</a>: u64 = 0;
</code></pre>



<a name="0x2_bcs_SUI_ADDRESS_LENGTH"></a>

Address length in Sui is 20 bytes.


<pre><code><b>const</b> <a href="bcs.md#0x2_bcs_SUI_ADDRESS_LENGTH">SUI_ADDRESS_LENGTH</a>: u64 = 20;
</code></pre>



<a name="0x2_bcs_to_bytes"></a>

## Function `to_bytes`

Get BCS serialized bytes for any value.
Re-exports stdlib <code><a href="_to_bytes">bcs::to_bytes</a></code>.


<pre><code><b>public</b> <b>fun</b> <a href="bcs.md#0x2_bcs_to_bytes">to_bytes</a>&lt;T&gt;(value: &T): <a href="">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="bcs.md#0x2_bcs_to_bytes">to_bytes</a>&lt;T&gt;(value: &T): <a href="">vector</a>&lt;u8&gt; {
    <a href="_to_bytes">bcs::to_bytes</a>(value)
}
</code></pre>



</details>

<a name="0x2_bcs_peel_address"></a>

## Function `peel_address`

Read address from the bcs-serialized bytes.


<pre><code><b>public</b> <b>fun</b> <a href="bcs.md#0x2_bcs_peel_address">peel_address</a>(<a href="">bcs</a>: &<b>mut</b> <a href="">vector</a>&lt;u8&gt;): <b>address</b>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="bcs.md#0x2_bcs_peel_address">peel_address</a>(<a href="">bcs</a>: &<b>mut</b> <a href="">vector</a>&lt;u8&gt;): <b>address</b> {
    <b>assert</b>!(v::length(<a href="">bcs</a>) &gt;= <a href="bcs.md#0x2_bcs_SUI_ADDRESS_LENGTH">SUI_ADDRESS_LENGTH</a>, <a href="bcs.md#0x2_bcs_EOutOfRange">EOutOfRange</a>);
    v::reverse(<a href="">bcs</a>);
    <b>let</b> (addr_bytes, i) = (v::empty(), 0);
    <b>while</b> (i &lt; 20) {
        v::push_back(&<b>mut</b> addr_bytes, v::pop_back(<a href="">bcs</a>));
        i = i + 1;
    };
    v::reverse(<a href="">bcs</a>);
    <a href="object.md#0x2_object_address_from_bytes">object::address_from_bytes</a>(addr_bytes)
}
</code></pre>



</details>

<a name="0x2_bcs_peel_bool"></a>

## Function `peel_bool`

Read a <code>bool</code> value from bcs-serialized bytes.


<pre><code><b>public</b> <b>fun</b> <a href="bcs.md#0x2_bcs_peel_bool">peel_bool</a>(<a href="">bcs</a>: &<b>mut</b> <a href="">vector</a>&lt;u8&gt;): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="bcs.md#0x2_bcs_peel_bool">peel_bool</a>(<a href="">bcs</a>: &<b>mut</b> <a href="">vector</a>&lt;u8&gt;): bool {
    <b>let</b> value = <a href="bcs.md#0x2_bcs_peel_u8">peel_u8</a>(<a href="">bcs</a>);
    <b>if</b> (value == 0) {
        <b>false</b>
    } <b>else</b> <b>if</b> (value == 1) {
        <b>true</b>
    } <b>else</b> {
        <b>abort</b> <a href="bcs.md#0x2_bcs_ENotBool">ENotBool</a>
    }
}
</code></pre>



</details>

<a name="0x2_bcs_peel_u8"></a>

## Function `peel_u8`

Read <code>u8</code> value from bcs-serialized bytes.


<pre><code><b>public</b> <b>fun</b> <a href="bcs.md#0x2_bcs_peel_u8">peel_u8</a>(<a href="">bcs</a>: &<b>mut</b> <a href="">vector</a>&lt;u8&gt;): u8
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="bcs.md#0x2_bcs_peel_u8">peel_u8</a>(<a href="">bcs</a>: &<b>mut</b> <a href="">vector</a>&lt;u8&gt;): u8 {
    <b>assert</b>!(v::length(<a href="">bcs</a>) &gt;= 1, <a href="bcs.md#0x2_bcs_EOutOfRange">EOutOfRange</a>);
    v::remove(<a href="">bcs</a>, 0)
}
</code></pre>



</details>

<a name="0x2_bcs_peel_u64"></a>

## Function `peel_u64`

Read <code>u64</code> value from bcs-serialized bytes.


<pre><code><b>public</b> <b>fun</b> <a href="bcs.md#0x2_bcs_peel_u64">peel_u64</a>(<a href="">bcs</a>: &<b>mut</b> <a href="">vector</a>&lt;u8&gt;): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="bcs.md#0x2_bcs_peel_u64">peel_u64</a>(<a href="">bcs</a>: &<b>mut</b> <a href="">vector</a>&lt;u8&gt;): u64 {
    <b>assert</b>!(v::length(<a href="">bcs</a>) &gt;= 8, <a href="bcs.md#0x2_bcs_EOutOfRange">EOutOfRange</a>);
    <b>let</b> (l_value, r_value, i) = (0u64, 0u64, 0);

    // Read first 4 LE bytes (u32)
    <b>while</b> (i &lt; 4) {
        <b>let</b> l_byte = (v::remove(<a href="">bcs</a>, 0) <b>as</b> u64);
        <b>let</b> r_byte = (v::remove(<a href="">bcs</a>, 3 - i) <b>as</b> u64);

        l_value = l_value + (l_byte &lt;&lt; ((8 * (i)) <b>as</b> u8));
        r_value = r_value + (r_byte &lt;&lt; ((8 * (i)) <b>as</b> u8));

        i = i + 1;
    };

    // Swap LHS and RHS of initial bytes
    (r_value &lt;&lt; 32) | l_value
}
</code></pre>



</details>

<a name="0x2_bcs_peel_u128"></a>

## Function `peel_u128`

Read <code>u128</code> value from bcs-serialized bytes.


<pre><code><b>public</b> <b>fun</b> <a href="bcs.md#0x2_bcs_peel_u128">peel_u128</a>(<a href="">bcs</a>: &<b>mut</b> <a href="">vector</a>&lt;u8&gt;): u128
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="bcs.md#0x2_bcs_peel_u128">peel_u128</a>(<a href="">bcs</a>: &<b>mut</b> <a href="">vector</a>&lt;u8&gt;): u128 {
    <b>assert</b>!(v::length(<a href="">bcs</a>) &gt;= 16, <a href="bcs.md#0x2_bcs_EOutOfRange">EOutOfRange</a>);

    <b>let</b> (l_value, r_value) = (<a href="bcs.md#0x2_bcs_peel_u64">peel_u64</a>(<a href="">bcs</a>), <a href="bcs.md#0x2_bcs_peel_u64">peel_u64</a>(<a href="">bcs</a>));

    ((r_value <b>as</b> u128) &lt;&lt; 64) | (l_value <b>as</b> u128)
}
</code></pre>



</details>

<a name="0x2_bcs_peel_vec_length"></a>

## Function `peel_vec_length`

Read ULEB bytes expecting a vector length. Result should
then be used to perform <code>peel_*</code> operation LEN times.

In BCS <code><a href="">vector</a></code> length is implemented with ULEB128;
See more here: https://en.wikipedia.org/wiki/LEB128


<pre><code><b>public</b> <b>fun</b> <a href="bcs.md#0x2_bcs_peel_vec_length">peel_vec_length</a>(<a href="">bcs</a>: &<b>mut</b> <a href="">vector</a>&lt;u8&gt;): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="bcs.md#0x2_bcs_peel_vec_length">peel_vec_length</a>(<a href="">bcs</a>: &<b>mut</b> <a href="">vector</a>&lt;u8&gt;): u64 {
    v::reverse(<a href="">bcs</a>);
    <b>let</b> (total, shift, len) = (0u64, 0, 0);
    <b>while</b> (<b>true</b>) {
        <b>assert</b>!(len &lt;= 4, <a href="bcs.md#0x2_bcs_ELenOutOfRange">ELenOutOfRange</a>);
        <b>let</b> byte = (v::pop_back(<a href="">bcs</a>) <b>as</b> u64);
        len = len + 1;
        total = total | ((byte & 0x7f) &lt;&lt; shift);
        <b>if</b> ((byte & 0x80) == 0) {
            <b>break</b>
        };
        shift = shift + 7;
    };
    v::reverse(<a href="">bcs</a>);
    total
}
</code></pre>



</details>

<a name="0x2_bcs_peel_vec_address"></a>

## Function `peel_vec_address`

Peel a vector of <code><b>address</b></code> from serialized bytes.


<pre><code><b>public</b> <b>fun</b> <a href="bcs.md#0x2_bcs_peel_vec_address">peel_vec_address</a>(<a href="">bcs</a>: &<b>mut</b> <a href="">vector</a>&lt;u8&gt;): <a href="">vector</a>&lt;<b>address</b>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="bcs.md#0x2_bcs_peel_vec_address">peel_vec_address</a>(<a href="">bcs</a>: &<b>mut</b> <a href="">vector</a>&lt;u8&gt;): <a href="">vector</a>&lt;<b>address</b>&gt; {
    <b>let</b> (len, i, res) = (<a href="bcs.md#0x2_bcs_peel_vec_length">peel_vec_length</a>(<a href="">bcs</a>), 0, <a href="">vector</a>[]);
    <b>while</b> (i &lt; len) {
        v::push_back(&<b>mut</b> res, <a href="bcs.md#0x2_bcs_peel_address">peel_address</a>(<a href="">bcs</a>));
        i = i + 1;
    };
    res
}
</code></pre>



</details>

<a name="0x2_bcs_peel_vec_bool"></a>

## Function `peel_vec_bool`

Peel a vector of <code><b>address</b></code> from serialized bytes.


<pre><code><b>public</b> <b>fun</b> <a href="bcs.md#0x2_bcs_peel_vec_bool">peel_vec_bool</a>(<a href="">bcs</a>: &<b>mut</b> <a href="">vector</a>&lt;u8&gt;): <a href="">vector</a>&lt;bool&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="bcs.md#0x2_bcs_peel_vec_bool">peel_vec_bool</a>(<a href="">bcs</a>: &<b>mut</b> <a href="">vector</a>&lt;u8&gt;): <a href="">vector</a>&lt;bool&gt; {
    <b>let</b> (len, i, res) = (<a href="bcs.md#0x2_bcs_peel_vec_length">peel_vec_length</a>(<a href="">bcs</a>), 0, <a href="">vector</a>[]);
    <b>while</b> (i &lt; len) {
        v::push_back(&<b>mut</b> res, <a href="bcs.md#0x2_bcs_peel_bool">peel_bool</a>(<a href="">bcs</a>));
        i = i + 1;
    };
    res
}
</code></pre>



</details>

<a name="0x2_bcs_peel_vec_u8"></a>

## Function `peel_vec_u8`

Peel a vector of <code>u8</code> (eg string) from serialized bytes.


<pre><code><b>public</b> <b>fun</b> <a href="bcs.md#0x2_bcs_peel_vec_u8">peel_vec_u8</a>(<a href="">bcs</a>: &<b>mut</b> <a href="">vector</a>&lt;u8&gt;): <a href="">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="bcs.md#0x2_bcs_peel_vec_u8">peel_vec_u8</a>(<a href="">bcs</a>: &<b>mut</b> <a href="">vector</a>&lt;u8&gt;): <a href="">vector</a>&lt;u8&gt; {
    <b>let</b> (len, i, res) = (<a href="bcs.md#0x2_bcs_peel_vec_length">peel_vec_length</a>(<a href="">bcs</a>), 0, <a href="">vector</a>[]);
    <b>while</b> (i &lt; len) {
        v::push_back(&<b>mut</b> res, <a href="bcs.md#0x2_bcs_peel_u8">peel_u8</a>(<a href="">bcs</a>));
        i = i + 1;
    };
    res
}
</code></pre>



</details>

<a name="0x2_bcs_peel_vec_u64"></a>

## Function `peel_vec_u64`

Peel a vector of <code>u64</code> from serialized bytes.


<pre><code><b>public</b> <b>fun</b> <a href="bcs.md#0x2_bcs_peel_vec_u64">peel_vec_u64</a>(<a href="">bcs</a>: &<b>mut</b> <a href="">vector</a>&lt;u8&gt;): <a href="">vector</a>&lt;u64&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="bcs.md#0x2_bcs_peel_vec_u64">peel_vec_u64</a>(<a href="">bcs</a>: &<b>mut</b> <a href="">vector</a>&lt;u8&gt;): <a href="">vector</a>&lt;u64&gt; {
    <b>let</b> (len, i, res) = (<a href="bcs.md#0x2_bcs_peel_vec_length">peel_vec_length</a>(<a href="">bcs</a>), 0, <a href="">vector</a>[]);
    <b>while</b> (i &lt; len) {
        v::push_back(&<b>mut</b> res, <a href="bcs.md#0x2_bcs_peel_u64">peel_u64</a>(<a href="">bcs</a>));
        i = i + 1;
    };
    res
}
</code></pre>



</details>

<a name="0x2_bcs_peel_vec_u128"></a>

## Function `peel_vec_u128`

Peel a vector of <code>u128</code> from serialized bytes.


<pre><code><b>public</b> <b>fun</b> <a href="bcs.md#0x2_bcs_peel_vec_u128">peel_vec_u128</a>(<a href="">bcs</a>: &<b>mut</b> <a href="">vector</a>&lt;u8&gt;): <a href="">vector</a>&lt;u128&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="bcs.md#0x2_bcs_peel_vec_u128">peel_vec_u128</a>(<a href="">bcs</a>: &<b>mut</b> <a href="">vector</a>&lt;u8&gt;): <a href="">vector</a>&lt;u128&gt; {
    <b>let</b> (len, i, res) = (<a href="bcs.md#0x2_bcs_peel_vec_length">peel_vec_length</a>(<a href="">bcs</a>), 0, <a href="">vector</a>[]);
    <b>while</b> (i &lt; len) {
        v::push_back(&<b>mut</b> res, <a href="bcs.md#0x2_bcs_peel_u128">peel_u128</a>(<a href="">bcs</a>));
        i = i + 1;
    };
    res
}
</code></pre>



</details>

<a name="0x2_bcs_peel_option_address"></a>

## Function `peel_option_address`

Peel <code>Option&lt;<b>address</b>&gt;</code> from serialized bytes.


<pre><code><b>public</b> <b>fun</b> <a href="bcs.md#0x2_bcs_peel_option_address">peel_option_address</a>(<a href="">bcs</a>: &<b>mut</b> <a href="">vector</a>&lt;u8&gt;): <a href="_Option">option::Option</a>&lt;<b>address</b>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="bcs.md#0x2_bcs_peel_option_address">peel_option_address</a>(<a href="">bcs</a>: &<b>mut</b> <a href="">vector</a>&lt;u8&gt;): Option&lt;<b>address</b>&gt; {
    <b>if</b> (<a href="bcs.md#0x2_bcs_peel_bool">peel_bool</a>(<a href="">bcs</a>)) {
        <a href="_some">option::some</a>(<a href="bcs.md#0x2_bcs_peel_address">peel_address</a>(<a href="">bcs</a>))
    } <b>else</b> {
        <a href="_none">option::none</a>()
    }
}
</code></pre>



</details>

<a name="0x2_bcs_peel_option_bool"></a>

## Function `peel_option_bool`

Peel <code>Option&lt;bool&gt;</code> from serialized bytes.


<pre><code><b>public</b> <b>fun</b> <a href="bcs.md#0x2_bcs_peel_option_bool">peel_option_bool</a>(<a href="">bcs</a>: &<b>mut</b> <a href="">vector</a>&lt;u8&gt;): <a href="_Option">option::Option</a>&lt;bool&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="bcs.md#0x2_bcs_peel_option_bool">peel_option_bool</a>(<a href="">bcs</a>: &<b>mut</b> <a href="">vector</a>&lt;u8&gt;): Option&lt;bool&gt; {
    <b>if</b> (<a href="bcs.md#0x2_bcs_peel_bool">peel_bool</a>(<a href="">bcs</a>)) {
        <a href="_some">option::some</a>(<a href="bcs.md#0x2_bcs_peel_bool">peel_bool</a>(<a href="">bcs</a>))
    } <b>else</b> {
        <a href="_none">option::none</a>()
    }
}
</code></pre>



</details>

<a name="0x2_bcs_peel_option_u8"></a>

## Function `peel_option_u8`

Peel <code>Option&lt;u8&gt;</code> from serialized bytes.


<pre><code><b>public</b> <b>fun</b> <a href="bcs.md#0x2_bcs_peel_option_u8">peel_option_u8</a>(<a href="">bcs</a>: &<b>mut</b> <a href="">vector</a>&lt;u8&gt;): <a href="_Option">option::Option</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="bcs.md#0x2_bcs_peel_option_u8">peel_option_u8</a>(<a href="">bcs</a>: &<b>mut</b> <a href="">vector</a>&lt;u8&gt;): Option&lt;u8&gt; {
    <b>if</b> (<a href="bcs.md#0x2_bcs_peel_bool">peel_bool</a>(<a href="">bcs</a>)) {
        <a href="_some">option::some</a>(<a href="bcs.md#0x2_bcs_peel_u8">peel_u8</a>(<a href="">bcs</a>))
    } <b>else</b> {
        <a href="_none">option::none</a>()
    }
}
</code></pre>



</details>

<a name="0x2_bcs_peel_option_u64"></a>

## Function `peel_option_u64`

Peel <code>Option&lt;u64&gt;</code> from serialized bytes.


<pre><code><b>public</b> <b>fun</b> <a href="bcs.md#0x2_bcs_peel_option_u64">peel_option_u64</a>(<a href="">bcs</a>: &<b>mut</b> <a href="">vector</a>&lt;u8&gt;): <a href="_Option">option::Option</a>&lt;u64&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="bcs.md#0x2_bcs_peel_option_u64">peel_option_u64</a>(<a href="">bcs</a>: &<b>mut</b> <a href="">vector</a>&lt;u8&gt;): Option&lt;u64&gt; {
    <b>if</b> (<a href="bcs.md#0x2_bcs_peel_bool">peel_bool</a>(<a href="">bcs</a>)) {
        <a href="_some">option::some</a>(<a href="bcs.md#0x2_bcs_peel_u64">peel_u64</a>(<a href="">bcs</a>))
    } <b>else</b> {
        <a href="_none">option::none</a>()
    }
}
</code></pre>



</details>

<a name="0x2_bcs_peel_option_u128"></a>

## Function `peel_option_u128`

Peel <code>Option&lt;u128&gt;</code> from serialized bytes.


<pre><code><b>public</b> <b>fun</b> <a href="bcs.md#0x2_bcs_peel_option_u128">peel_option_u128</a>(<a href="">bcs</a>: &<b>mut</b> <a href="">vector</a>&lt;u8&gt;): <a href="_Option">option::Option</a>&lt;u128&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="bcs.md#0x2_bcs_peel_option_u128">peel_option_u128</a>(<a href="">bcs</a>: &<b>mut</b> <a href="">vector</a>&lt;u8&gt;): Option&lt;u128&gt; {
    <b>if</b> (<a href="bcs.md#0x2_bcs_peel_bool">peel_bool</a>(<a href="">bcs</a>)) {
        <a href="_some">option::some</a>(<a href="bcs.md#0x2_bcs_peel_u128">peel_u128</a>(<a href="">bcs</a>))
    } <b>else</b> {
        <a href="_none">option::none</a>()
    }
}
</code></pre>



</details>
