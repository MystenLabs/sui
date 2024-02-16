
<a name="0x2_hex"></a>

# Module `0x2::hex`



-  [Constants](#@Constants_0)
-  [Function `encode`](#0x2_hex_encode)
-  [Function `decode`](#0x2_hex_decode)
-  [Function `decode_byte`](#0x2_hex_decode_byte)


<pre><code><b>use</b> <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">0x1::vector</a>;
</code></pre>



<a name="@Constants_0"></a>

## Constants


<a name="0x2_hex_EInvalidHexLength"></a>



<pre><code><b>const</b> <a href="../../dependencies/sui-framework/hex.md#0x2_hex_EInvalidHexLength">EInvalidHexLength</a>: u64 = 0;
</code></pre>



<a name="0x2_hex_ENotValidHexCharacter"></a>



<pre><code><b>const</b> <a href="../../dependencies/sui-framework/hex.md#0x2_hex_ENotValidHexCharacter">ENotValidHexCharacter</a>: u64 = 1;
</code></pre>



<a name="0x2_hex_HEX"></a>



<pre><code><b>const</b> <a href="../../dependencies/sui-framework/hex.md#0x2_hex_HEX">HEX</a>: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;&gt; = [ByteArray([48, 48]), ByteArray([48, 49]), ByteArray([48, 50]), ByteArray([48, 51]), ByteArray([48, 52]), ByteArray([48, 53]), ByteArray([48, 54]), ByteArray([48, 55]), ByteArray([48, 56]), ByteArray([48, 57]), ByteArray([48, 97]), ByteArray([48, 98]), ByteArray([48, 99]), ByteArray([48, 100]), ByteArray([48, 101]), ByteArray([48, 102]), ByteArray([49, 48]), ByteArray([49, 49]), ByteArray([49, 50]), ByteArray([49, 51]), ByteArray([49, 52]), ByteArray([49, 53]), ByteArray([49, 54]), ByteArray([49, 55]), ByteArray([49, 56]), ByteArray([49, 57]), ByteArray([49, 97]), ByteArray([49, 98]), ByteArray([49, 99]), ByteArray([49, 100]), ByteArray([49, 101]), ByteArray([49, 102]), ByteArray([50, 48]), ByteArray([50, 49]), ByteArray([50, 50]), ByteArray([50, 51]), ByteArray([50, 52]), ByteArray([50, 53]), ByteArray([50, 54]), ByteArray([50, 55]), ByteArray([50, 56]), ByteArray([50, 57]), ByteArray([50, 97]), ByteArray([50, 98]), ByteArray([50, 99]), ByteArray([50, 100]), ByteArray([50, 101]), ByteArray([50, 102]), ByteArray([51, 48]), ByteArray([51, 49]), ByteArray([51, 50]), ByteArray([51, 51]), ByteArray([51, 52]), ByteArray([51, 53]), ByteArray([51, 54]), ByteArray([51, 55]), ByteArray([51, 56]), ByteArray([51, 57]), ByteArray([51, 97]), ByteArray([51, 98]), ByteArray([51, 99]), ByteArray([51, 100]), ByteArray([51, 101]), ByteArray([51, 102]), ByteArray([52, 48]), ByteArray([52, 49]), ByteArray([52, 50]), ByteArray([52, 51]), ByteArray([52, 52]), ByteArray([52, 53]), ByteArray([52, 54]), ByteArray([52, 55]), ByteArray([52, 56]), ByteArray([52, 57]), ByteArray([52, 97]), ByteArray([52, 98]), ByteArray([52, 99]), ByteArray([52, 100]), ByteArray([52, 101]), ByteArray([52, 102]), ByteArray([53, 48]), ByteArray([53, 49]), ByteArray([53, 50]), ByteArray([53, 51]), ByteArray([53, 52]), ByteArray([53, 53]), ByteArray([53, 54]), ByteArray([53, 55]), ByteArray([53, 56]), ByteArray([53, 57]), ByteArray([53, 97]), ByteArray([53, 98]), ByteArray([53, 99]), ByteArray([53, 100]), ByteArray([53, 101]), ByteArray([53, 102]), ByteArray([54, 48]), ByteArray([54, 49]), ByteArray([54, 50]), ByteArray([54, 51]), ByteArray([54, 52]), ByteArray([54, 53]), ByteArray([54, 54]), ByteArray([54, 55]), ByteArray([54, 56]), ByteArray([54, 57]), ByteArray([54, 97]), ByteArray([54, 98]), ByteArray([54, 99]), ByteArray([54, 100]), ByteArray([54, 101]), ByteArray([54, 102]), ByteArray([55, 48]), ByteArray([55, 49]), ByteArray([55, 50]), ByteArray([55, 51]), ByteArray([55, 52]), ByteArray([55, 53]), ByteArray([55, 54]), ByteArray([55, 55]), ByteArray([55, 56]), ByteArray([55, 57]), ByteArray([55, 97]), ByteArray([55, 98]), ByteArray([55, 99]), ByteArray([55, 100]), ByteArray([55, 101]), ByteArray([55, 102]), ByteArray([56, 48]), ByteArray([56, 49]), ByteArray([56, 50]), ByteArray([56, 51]), ByteArray([56, 52]), ByteArray([56, 53]), ByteArray([56, 54]), ByteArray([56, 55]), ByteArray([56, 56]), ByteArray([56, 57]), ByteArray([56, 97]), ByteArray([56, 98]), ByteArray([56, 99]), ByteArray([56, 100]), ByteArray([56, 101]), ByteArray([56, 102]), ByteArray([57, 48]), ByteArray([57, 49]), ByteArray([57, 50]), ByteArray([57, 51]), ByteArray([57, 52]), ByteArray([57, 53]), ByteArray([57, 54]), ByteArray([57, 55]), ByteArray([57, 56]), ByteArray([57, 57]), ByteArray([57, 97]), ByteArray([57, 98]), ByteArray([57, 99]), ByteArray([57, 100]), ByteArray([57, 101]), ByteArray([57, 102]), ByteArray([97, 48]), ByteArray([97, 49]), ByteArray([97, 50]), ByteArray([97, 51]), ByteArray([97, 52]), ByteArray([97, 53]), ByteArray([97, 54]), ByteArray([97, 55]), ByteArray([97, 56]), ByteArray([97, 57]), ByteArray([97, 97]), ByteArray([97, 98]), ByteArray([97, 99]), ByteArray([97, 100]), ByteArray([97, 101]), ByteArray([97, 102]), ByteArray([98, 48]), ByteArray([98, 49]), ByteArray([98, 50]), ByteArray([98, 51]), ByteArray([98, 52]), ByteArray([98, 53]), ByteArray([98, 54]), ByteArray([98, 55]), ByteArray([98, 56]), ByteArray([98, 57]), ByteArray([98, 97]), ByteArray([98, 98]), ByteArray([98, 99]), ByteArray([98, 100]), ByteArray([98, 101]), ByteArray([98, 102]), ByteArray([99, 48]), ByteArray([99, 49]), ByteArray([99, 50]), ByteArray([99, 51]), ByteArray([99, 52]), ByteArray([99, 53]), ByteArray([99, 54]), ByteArray([99, 55]), ByteArray([99, 56]), ByteArray([99, 57]), ByteArray([99, 97]), ByteArray([99, 98]), ByteArray([99, 99]), ByteArray([99, 100]), ByteArray([99, 101]), ByteArray([99, 102]), ByteArray([100, 48]), ByteArray([100, 49]), ByteArray([100, 50]), ByteArray([100, 51]), ByteArray([100, 52]), ByteArray([100, 53]), ByteArray([100, 54]), ByteArray([100, 55]), ByteArray([100, 56]), ByteArray([100, 57]), ByteArray([100, 97]), ByteArray([100, 98]), ByteArray([100, 99]), ByteArray([100, 100]), ByteArray([100, 101]), ByteArray([100, 102]), ByteArray([101, 48]), ByteArray([101, 49]), ByteArray([101, 50]), ByteArray([101, 51]), ByteArray([101, 52]), ByteArray([101, 53]), ByteArray([101, 54]), ByteArray([101, 55]), ByteArray([101, 56]), ByteArray([101, 57]), ByteArray([101, 97]), ByteArray([101, 98]), ByteArray([101, 99]), ByteArray([101, 100]), ByteArray([101, 101]), ByteArray([101, 102]), ByteArray([102, 48]), ByteArray([102, 49]), ByteArray([102, 50]), ByteArray([102, 51]), ByteArray([102, 52]), ByteArray([102, 53]), ByteArray([102, 54]), ByteArray([102, 55]), ByteArray([102, 56]), ByteArray([102, 57]), ByteArray([102, 97]), ByteArray([102, 98]), ByteArray([102, 99]), ByteArray([102, 100]), ByteArray([102, 101]), ByteArray([102, 102])];
</code></pre>



<a name="0x2_hex_encode"></a>

## Function `encode`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/hex.md#0x2_hex_encode">encode</a>(bytes: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;): <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/hex.md#0x2_hex_encode">encode</a>(bytes: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;): <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt; {
    <b>let</b> (i, r, l) = (0, <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>[], <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_length">vector::length</a>(&bytes));
    <b>let</b> hex_vector = <a href="../../dependencies/sui-framework/hex.md#0x2_hex_HEX">HEX</a>;
    <b>while</b> (i &lt; l) {
        <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_append">vector::append</a>(
            &<b>mut</b> r,
            *<a href="../../dependencies/move-stdlib/vector.md#0x1_vector_borrow">vector::borrow</a>(&hex_vector, (*<a href="../../dependencies/move-stdlib/vector.md#0x1_vector_borrow">vector::borrow</a>(&bytes, i) <b>as</b> u64))
        );
        i = i + 1;
    };
    r
}
</code></pre>



</details>

<a name="0x2_hex_decode"></a>

## Function `decode`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/hex.md#0x2_hex_decode">decode</a>(<a href="../../dependencies/sui-framework/hex.md#0x2_hex">hex</a>: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;): <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/hex.md#0x2_hex_decode">decode</a>(<a href="../../dependencies/sui-framework/hex.md#0x2_hex">hex</a>: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;): <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt; {
    <b>let</b> (i, r, l) = (0, <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>[], <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_length">vector::length</a>(&<a href="../../dependencies/sui-framework/hex.md#0x2_hex">hex</a>));
    <b>assert</b>!(l % 2 == 0, <a href="../../dependencies/sui-framework/hex.md#0x2_hex_EInvalidHexLength">EInvalidHexLength</a>);
    <b>while</b> (i &lt; l) {
        <b>let</b> decimal = (<a href="../../dependencies/sui-framework/hex.md#0x2_hex_decode_byte">decode_byte</a>(*<a href="../../dependencies/move-stdlib/vector.md#0x1_vector_borrow">vector::borrow</a>(&<a href="../../dependencies/sui-framework/hex.md#0x2_hex">hex</a>, i)) * 16) +
                      <a href="../../dependencies/sui-framework/hex.md#0x2_hex_decode_byte">decode_byte</a>(*<a href="../../dependencies/move-stdlib/vector.md#0x1_vector_borrow">vector::borrow</a>(&<a href="../../dependencies/sui-framework/hex.md#0x2_hex">hex</a>, i + 1));
        <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_push_back">vector::push_back</a>(&<b>mut</b> r, decimal);
        i = i + 2;
    };
    r
}
</code></pre>



</details>

<a name="0x2_hex_decode_byte"></a>

## Function `decode_byte`



<pre><code><b>fun</b> <a href="../../dependencies/sui-framework/hex.md#0x2_hex_decode_byte">decode_byte</a>(<a href="../../dependencies/sui-framework/hex.md#0x2_hex">hex</a>: u8): u8
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../../dependencies/sui-framework/hex.md#0x2_hex_decode_byte">decode_byte</a>(<a href="../../dependencies/sui-framework/hex.md#0x2_hex">hex</a>: u8): u8 {
    <b>if</b> (/* 0 .. 9 */ 48 &lt;= <a href="../../dependencies/sui-framework/hex.md#0x2_hex">hex</a> && <a href="../../dependencies/sui-framework/hex.md#0x2_hex">hex</a> &lt; 58) {
        <a href="../../dependencies/sui-framework/hex.md#0x2_hex">hex</a> - 48
    } <b>else</b> <b>if</b> (/* A .. F */ 65 &lt;= <a href="../../dependencies/sui-framework/hex.md#0x2_hex">hex</a> && <a href="../../dependencies/sui-framework/hex.md#0x2_hex">hex</a> &lt; 71) {
        10 + <a href="../../dependencies/sui-framework/hex.md#0x2_hex">hex</a> - 65
    } <b>else</b> <b>if</b> (/* a .. f */ 97 &lt;= <a href="../../dependencies/sui-framework/hex.md#0x2_hex">hex</a> && <a href="../../dependencies/sui-framework/hex.md#0x2_hex">hex</a> &lt; 103) {
        10 + <a href="../../dependencies/sui-framework/hex.md#0x2_hex">hex</a> - 97
    } <b>else</b> {
        <b>abort</b> <a href="../../dependencies/sui-framework/hex.md#0x2_hex_ENotValidHexCharacter">ENotValidHexCharacter</a>
    }
}
</code></pre>



</details>
