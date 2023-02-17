
<a name="0x2_elliptic_curve"></a>

# Module `0x2::elliptic_curve`

Library for Elliptic Curve operations for Pedersen Commitment on a prime order group.
We specifically support the Ristretto-255 sub-group.


-  [Struct `RistrettoPoint`](#0x2_elliptic_curve_RistrettoPoint)
-  [Struct `Scalar`](#0x2_elliptic_curve_Scalar)
-  [Function `native_create_pedersen_commitment`](#0x2_elliptic_curve_native_create_pedersen_commitment)
-  [Function `native_add_ristretto_point`](#0x2_elliptic_curve_native_add_ristretto_point)
-  [Function `native_subtract_ristretto_point`](#0x2_elliptic_curve_native_subtract_ristretto_point)
-  [Function `native_scalar_from_u64`](#0x2_elliptic_curve_native_scalar_from_u64)
-  [Function `native_scalar_from_bytes`](#0x2_elliptic_curve_native_scalar_from_bytes)
-  [Function `new_scalar_from_u64`](#0x2_elliptic_curve_new_scalar_from_u64)
-  [Function `create_pedersen_commitment`](#0x2_elliptic_curve_create_pedersen_commitment)
-  [Function `new_scalar_from_bytes`](#0x2_elliptic_curve_new_scalar_from_bytes)
-  [Function `scalar_bytes`](#0x2_elliptic_curve_scalar_bytes)
-  [Function `bytes`](#0x2_elliptic_curve_bytes)
-  [Function `add`](#0x2_elliptic_curve_add)
-  [Function `subtract`](#0x2_elliptic_curve_subtract)
-  [Function `new_from_bytes`](#0x2_elliptic_curve_new_from_bytes)


<pre><code></code></pre>



<a name="0x2_elliptic_curve_RistrettoPoint"></a>

## Struct `RistrettoPoint`

Elliptic Curve structs
Represents a point on the Ristretto-255 subgroup.


<pre><code><b>struct</b> <a href="elliptic_curve.md#0x2_elliptic_curve_RistrettoPoint">RistrettoPoint</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>value: <a href="">vector</a>&lt;u8&gt;</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x2_elliptic_curve_Scalar"></a>

## Struct `Scalar`

Represents a scalar within the Curve25519 prime-order group.


<pre><code><b>struct</b> <a href="elliptic_curve.md#0x2_elliptic_curve_Scalar">Scalar</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>value: <a href="">vector</a>&lt;u8&gt;</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x2_elliptic_curve_native_create_pedersen_commitment"></a>

## Function `native_create_pedersen_commitment`

Private
@param value: The value to commit to
@param blinding_factor: A random number used to ensure that the commitment is hiding.


<pre><code><b>fun</b> <a href="elliptic_curve.md#0x2_elliptic_curve_native_create_pedersen_commitment">native_create_pedersen_commitment</a>(value: <a href="">vector</a>&lt;u8&gt;, blinding_factor: <a href="">vector</a>&lt;u8&gt;): <a href="">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="elliptic_curve.md#0x2_elliptic_curve_native_create_pedersen_commitment">native_create_pedersen_commitment</a>(value: <a href="">vector</a>&lt;u8&gt;, blinding_factor: <a href="">vector</a>&lt;u8&gt;): <a href="">vector</a>&lt;u8&gt;;
</code></pre>



</details>

<details>
<summary>Specification</summary>



<pre><code><b>pragma</b> opaque;
<b>aborts_if</b> [abstract] <b>true</b>;
</code></pre>



</details>

<a name="0x2_elliptic_curve_native_add_ristretto_point"></a>

## Function `native_add_ristretto_point`

@param self: bytes representation of an EC point on the Ristretto-255 subgroup
@param other: bytes representation of an EC point on the Ristretto-255 subgroup
A native move wrapper around the addition of Ristretto points. Returns self + other.


<pre><code><b>fun</b> <a href="elliptic_curve.md#0x2_elliptic_curve_native_add_ristretto_point">native_add_ristretto_point</a>(point1: <a href="">vector</a>&lt;u8&gt;, point2: <a href="">vector</a>&lt;u8&gt;): <a href="">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="elliptic_curve.md#0x2_elliptic_curve_native_add_ristretto_point">native_add_ristretto_point</a>(point1: <a href="">vector</a>&lt;u8&gt;, point2: <a href="">vector</a>&lt;u8&gt;): <a href="">vector</a>&lt;u8&gt;;
</code></pre>



</details>

<details>
<summary>Specification</summary>



<pre><code><b>pragma</b> opaque;
<b>aborts_if</b> [abstract] <b>true</b>;
</code></pre>



</details>

<a name="0x2_elliptic_curve_native_subtract_ristretto_point"></a>

## Function `native_subtract_ristretto_point`

@param self: bytes representation of an EC point on the Ristretto-255 subgroup
@param other: bytes representation of an EC point on the Ristretto-255 subgroup
A native move wrapper around the subtraction of Ristretto points. Returns self - other.


<pre><code><b>fun</b> <a href="elliptic_curve.md#0x2_elliptic_curve_native_subtract_ristretto_point">native_subtract_ristretto_point</a>(point1: <a href="">vector</a>&lt;u8&gt;, point2: <a href="">vector</a>&lt;u8&gt;): <a href="">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="elliptic_curve.md#0x2_elliptic_curve_native_subtract_ristretto_point">native_subtract_ristretto_point</a>(point1: <a href="">vector</a>&lt;u8&gt;, point2: <a href="">vector</a>&lt;u8&gt;): <a href="">vector</a>&lt;u8&gt;;
</code></pre>



</details>

<details>
<summary>Specification</summary>



<pre><code><b>pragma</b> opaque;
<b>aborts_if</b> [abstract] <b>true</b>;
</code></pre>



</details>

<a name="0x2_elliptic_curve_native_scalar_from_u64"></a>

## Function `native_scalar_from_u64`

@param value: the value of the to-be-created scalar
TODO: Transfer this into a Move function some time in the future.
A native move wrapper for the creation of Scalars on Curve25519.


<pre><code><b>fun</b> <a href="elliptic_curve.md#0x2_elliptic_curve_native_scalar_from_u64">native_scalar_from_u64</a>(value: u64): <a href="">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="elliptic_curve.md#0x2_elliptic_curve_native_scalar_from_u64">native_scalar_from_u64</a>(value: u64): <a href="">vector</a>&lt;u8&gt;;
</code></pre>



</details>

<details>
<summary>Specification</summary>



<pre><code><b>pragma</b> opaque;
<b>aborts_if</b> [abstract] <b>true</b>;
</code></pre>



</details>

<a name="0x2_elliptic_curve_native_scalar_from_bytes"></a>

## Function `native_scalar_from_bytes`

@param value: the bytes representation of the scalar.
TODO: Transfer this into a Move function some time in the future.
A native move wrapper for the creation of Scalars on Curve25519.


<pre><code><b>fun</b> <a href="elliptic_curve.md#0x2_elliptic_curve_native_scalar_from_bytes">native_scalar_from_bytes</a>(bytes: <a href="">vector</a>&lt;u8&gt;): <a href="">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="elliptic_curve.md#0x2_elliptic_curve_native_scalar_from_bytes">native_scalar_from_bytes</a>(bytes: <a href="">vector</a>&lt;u8&gt;): <a href="">vector</a>&lt;u8&gt;;
</code></pre>



</details>

<details>
<summary>Specification</summary>



<pre><code><b>pragma</b> opaque;
<b>aborts_if</b> [abstract] <b>true</b>;
</code></pre>



</details>

<a name="0x2_elliptic_curve_new_scalar_from_u64"></a>

## Function `new_scalar_from_u64`

Public
Create a field element from u64


<pre><code><b>public</b> <b>fun</b> <a href="elliptic_curve.md#0x2_elliptic_curve_new_scalar_from_u64">new_scalar_from_u64</a>(value: u64): <a href="elliptic_curve.md#0x2_elliptic_curve_Scalar">elliptic_curve::Scalar</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="elliptic_curve.md#0x2_elliptic_curve_new_scalar_from_u64">new_scalar_from_u64</a>(value: u64): <a href="elliptic_curve.md#0x2_elliptic_curve_Scalar">Scalar</a> {
    <a href="elliptic_curve.md#0x2_elliptic_curve_Scalar">Scalar</a> {
        value: <a href="elliptic_curve.md#0x2_elliptic_curve_native_scalar_from_u64">native_scalar_from_u64</a>(value)
    }
}
</code></pre>



</details>

<a name="0x2_elliptic_curve_create_pedersen_commitment"></a>

## Function `create_pedersen_commitment`

Create a pedersen commitment from two field elements


<pre><code><b>public</b> <b>fun</b> <a href="elliptic_curve.md#0x2_elliptic_curve_create_pedersen_commitment">create_pedersen_commitment</a>(value: <a href="elliptic_curve.md#0x2_elliptic_curve_Scalar">elliptic_curve::Scalar</a>, blinding_factor: <a href="elliptic_curve.md#0x2_elliptic_curve_Scalar">elliptic_curve::Scalar</a>): <a href="elliptic_curve.md#0x2_elliptic_curve_RistrettoPoint">elliptic_curve::RistrettoPoint</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="elliptic_curve.md#0x2_elliptic_curve_create_pedersen_commitment">create_pedersen_commitment</a>(value: <a href="elliptic_curve.md#0x2_elliptic_curve_Scalar">Scalar</a>, blinding_factor: <a href="elliptic_curve.md#0x2_elliptic_curve_Scalar">Scalar</a>): <a href="elliptic_curve.md#0x2_elliptic_curve_RistrettoPoint">RistrettoPoint</a> {
    <b>return</b> <a href="elliptic_curve.md#0x2_elliptic_curve_RistrettoPoint">RistrettoPoint</a> {
        value: <a href="elliptic_curve.md#0x2_elliptic_curve_native_create_pedersen_commitment">native_create_pedersen_commitment</a>(value.value, blinding_factor.value)
    }
}
</code></pre>



</details>

<a name="0x2_elliptic_curve_new_scalar_from_bytes"></a>

## Function `new_scalar_from_bytes`

Creates a new field element from byte representation. Note that
<code>value</code> must be 32-bytes


<pre><code><b>public</b> <b>fun</b> <a href="elliptic_curve.md#0x2_elliptic_curve_new_scalar_from_bytes">new_scalar_from_bytes</a>(value: <a href="">vector</a>&lt;u8&gt;): <a href="elliptic_curve.md#0x2_elliptic_curve_Scalar">elliptic_curve::Scalar</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="elliptic_curve.md#0x2_elliptic_curve_new_scalar_from_bytes">new_scalar_from_bytes</a>(value: <a href="">vector</a>&lt;u8&gt;): <a href="elliptic_curve.md#0x2_elliptic_curve_Scalar">Scalar</a> {
    <a href="elliptic_curve.md#0x2_elliptic_curve_Scalar">Scalar</a> {
        value: <a href="elliptic_curve.md#0x2_elliptic_curve_native_scalar_from_bytes">native_scalar_from_bytes</a>(value)
    }
}
</code></pre>



</details>

<a name="0x2_elliptic_curve_scalar_bytes"></a>

## Function `scalar_bytes`

Get the byte representation of the field element


<pre><code><b>public</b> <b>fun</b> <a href="elliptic_curve.md#0x2_elliptic_curve_scalar_bytes">scalar_bytes</a>(self: &<a href="elliptic_curve.md#0x2_elliptic_curve_Scalar">elliptic_curve::Scalar</a>): <a href="">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="elliptic_curve.md#0x2_elliptic_curve_scalar_bytes">scalar_bytes</a>(self: &<a href="elliptic_curve.md#0x2_elliptic_curve_Scalar">Scalar</a>): <a href="">vector</a>&lt;u8&gt; {
    self.value
}
</code></pre>



</details>

<a name="0x2_elliptic_curve_bytes"></a>

## Function `bytes`

Get the underlying compressed byte representation of the group element


<pre><code><b>public</b> <b>fun</b> <a href="elliptic_curve.md#0x2_elliptic_curve_bytes">bytes</a>(self: &<a href="elliptic_curve.md#0x2_elliptic_curve_RistrettoPoint">elliptic_curve::RistrettoPoint</a>): <a href="">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="elliptic_curve.md#0x2_elliptic_curve_bytes">bytes</a>(self: &<a href="elliptic_curve.md#0x2_elliptic_curve_RistrettoPoint">RistrettoPoint</a>): <a href="">vector</a>&lt;u8&gt; {
    self.value
}
</code></pre>



</details>

<a name="0x2_elliptic_curve_add"></a>

## Function `add`

Perform addition on two group elements


<pre><code><b>public</b> <b>fun</b> <a href="elliptic_curve.md#0x2_elliptic_curve_add">add</a>(self: &<a href="elliptic_curve.md#0x2_elliptic_curve_RistrettoPoint">elliptic_curve::RistrettoPoint</a>, other: &<a href="elliptic_curve.md#0x2_elliptic_curve_RistrettoPoint">elliptic_curve::RistrettoPoint</a>): <a href="elliptic_curve.md#0x2_elliptic_curve_RistrettoPoint">elliptic_curve::RistrettoPoint</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="elliptic_curve.md#0x2_elliptic_curve_add">add</a>(self: &<a href="elliptic_curve.md#0x2_elliptic_curve_RistrettoPoint">RistrettoPoint</a>, other: &<a href="elliptic_curve.md#0x2_elliptic_curve_RistrettoPoint">RistrettoPoint</a>): <a href="elliptic_curve.md#0x2_elliptic_curve_RistrettoPoint">RistrettoPoint</a> {
    <a href="elliptic_curve.md#0x2_elliptic_curve_RistrettoPoint">RistrettoPoint</a> {
        value: <a href="elliptic_curve.md#0x2_elliptic_curve_native_add_ristretto_point">native_add_ristretto_point</a>(self.value, other.value)
    }
}
</code></pre>



</details>

<a name="0x2_elliptic_curve_subtract"></a>

## Function `subtract`

Perform subtraction on two group elements


<pre><code><b>public</b> <b>fun</b> <a href="elliptic_curve.md#0x2_elliptic_curve_subtract">subtract</a>(self: &<a href="elliptic_curve.md#0x2_elliptic_curve_RistrettoPoint">elliptic_curve::RistrettoPoint</a>, other: &<a href="elliptic_curve.md#0x2_elliptic_curve_RistrettoPoint">elliptic_curve::RistrettoPoint</a>): <a href="elliptic_curve.md#0x2_elliptic_curve_RistrettoPoint">elliptic_curve::RistrettoPoint</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="elliptic_curve.md#0x2_elliptic_curve_subtract">subtract</a>(self: &<a href="elliptic_curve.md#0x2_elliptic_curve_RistrettoPoint">RistrettoPoint</a>, other: &<a href="elliptic_curve.md#0x2_elliptic_curve_RistrettoPoint">RistrettoPoint</a>): <a href="elliptic_curve.md#0x2_elliptic_curve_RistrettoPoint">RistrettoPoint</a> {
    <a href="elliptic_curve.md#0x2_elliptic_curve_RistrettoPoint">RistrettoPoint</a> {
        value: <a href="elliptic_curve.md#0x2_elliptic_curve_native_subtract_ristretto_point">native_subtract_ristretto_point</a>(self.value, other.value)
    }
}
</code></pre>



</details>

<a name="0x2_elliptic_curve_new_from_bytes"></a>

## Function `new_from_bytes`

Attempt to create a new group element from compressed bytes representation


<pre><code><b>public</b> <b>fun</b> <a href="elliptic_curve.md#0x2_elliptic_curve_new_from_bytes">new_from_bytes</a>(bytes: <a href="">vector</a>&lt;u8&gt;): <a href="elliptic_curve.md#0x2_elliptic_curve_RistrettoPoint">elliptic_curve::RistrettoPoint</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="elliptic_curve.md#0x2_elliptic_curve_new_from_bytes">new_from_bytes</a>(bytes: <a href="">vector</a>&lt;u8&gt;): <a href="elliptic_curve.md#0x2_elliptic_curve_RistrettoPoint">RistrettoPoint</a> {
    <b>assert</b>!(<a href="_length">vector::length</a>(&bytes) == 32, 1);
    <a href="elliptic_curve.md#0x2_elliptic_curve_RistrettoPoint">RistrettoPoint</a> {
        value: bytes
    }
}
</code></pre>



</details>
