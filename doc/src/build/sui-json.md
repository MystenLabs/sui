---
title: SuiJSON
---

## Introduction

*SuiJSON* is a JSON-based format with restrictions that allow Sui to align JSON inputs more closely with Move Call arguments.

This table shows the restrictions placed on JSON types to make them SuiJSON compatible:

<table>
  <tr>
   <th>JSON
   </th>
   <th>SuiJSON Restrictions
   </th>
   <th>Move Type Mapping
   </th>
  </tr>
  <tr>
   <td>Number
   </td>
   <td>Must be unsigned integer
   </td>
   <td>U8

U64

(U128 is encoded as String)
   </td>
  </tr>
  <tr>
   <td>String
   </td>
   <td>No restrictions
   </td>
   <td>Vector&lt;U8>

Address

ObjectID

TypeTag

Identifier

Unsigned Integer (128 bit max)
   </td>
  </tr>
  <tr>
   <td>Boolean
   </td>
   <td>No restrictions
   </td>
   <td>Bool
   </td>
  </tr>
  <tr>
   <td>Array
   </td>
   <td>Must be homogeneous JSON and of SuiJSON type
   </td>
   <td>Vector
   </td>
  </tr>
  <tr>
   <td>Null
   </td>
   <td>Not allowed
   </td>
   <td>
   </td>
  </tr>
  <tr>
   <td>Object
   </td>
   <td>Not allowed
   </td>
   <td>
   </td>
  </tr>
    <tr>
   <td>
   </td>
   <td>
   </td>
   <td>
   </td>
  </tr>
</table>

## Type coercion reasoning

Due to the loosely typed nature of JSON/SuiJSON and the strongly typed nature of Move types, we sometimes need to overload SuiJSON types to represent multiple Move types. \
For example `SuiJSON::Number` can represent both *U8* and *U64*. This means we have to coerce and sometimes convert types.

Which type we coerce depends on the expected Move type. For example, if the Move function expects a U8, we must have received a `SuiJSON::Number` with a value less than 256. More importantly, we have no way to easily express Move addresses in JSON, so we encode them as hex strings prefixed by `0x`.

Additionally, Move supports U128 but JSON doesn't. As a result we allow encoding numbers as strings.

## Type coercion rules

<table>
  <tr>
   <th>Move Type
   </th>
   <th>SuiJSON Representations
   </th>
   <th>Valid Examples
   </th>
   <th>Invalid Examples
   </th>
  </tr>
  <tr>
   <td>Bool
   </td>
   <td>Bool
   </td>
   <td><code>true</code>, <code>false</code>
   </td>
   <td>
   </td>
  </tr>
  <tr>
   <td>U8
   </td>
   <td>
Three formats are supported

* Unsigned number &lt; 256.
* Decimal string with value &lt; 256.
* One byte hex string prefixed with `0x`.
   </td>
   <td>

   <code>7</code>

   <code>"70"</code>

   <code>"0x43"</code>
   </td>
   <td><code>-5</code>: negative not allowed

<code>3.9</code>: float now allowed

<code>NaN</code>: not allowed

<code>300</code>: U8 must be less than 256

<code>" 9"</code>: Spaces not allowed in string

<code>"9A"</code>: Hex num must be prefixed with `0x`

<code>"0x09CD"</code>: Too large for U8

   </td>
  </tr>
  <tr>
   <td>U64
   </td>
   <td>
Similarly to U8, three formats are supported

* Unsigned number &lt; U64::MAX.
* Decimal string with value &lt; U64::MAX.
* Up to 8 byte hex string prefixed with `0x`.

   </td>
   <td>Extrapolate above examples
   </td>
   <td>Extrapolate above examples
   </td>
  </tr>
  <tr>
   <td>U128
   </td>
   <td>

Two formats are supported

* Decimal string with value &lt; U128::MAX.
* Up to 16 byte hex string prefixed with `0x`.

   </td>
   <td>
   <code>"74794734937420002470"</code>

   <code>"0x2B1A39A1514E1D8A7CE"</code>

   </td>
   <td><code>34</code>: Although this is a valid u128 number, it must be encoded as a string
   </td>

  </tr>
  <tr>
   <td>Address
   </td>
    <td>20 byte hex string prefixed with <code>0x</code>
   </td>
   <td><code>"0x2B1A39A1514E1D8A7CE45919CFEB4FEE70B4E011"</code>
   </td>
   <td><code>0x2B1A39</code>: string too short

<code>2B1A39A1514E1D8A7CE45919CFEB4FEE70B4E011</code>: missing <code>0x</code> prefix

<code>0xG2B1A39A1514E1D8A7CE45919CFEB4FEE70B4E01</code>: invalid hex char <code>G</code>
   </td>
  </tr>
  <tr>
   <td>ObjectID
   </td>
   <td>16 byte hex string prefixed with <code>0x</code>
   </td>
   <td><code>"0x2B1A39A1514E1D8A7CE45919CFEB4FEE"</code>
   </td>
   <td>Similar to above
   </td>
  </tr>
  <tr>
   <td>Identifier
   </td>
   <td>Typically used for module and function names. Encoded as one of the following:

   1. A String whose first character is a letter and the remaining characters are letters, digits or underscore.

   2. A String whose first character is an underscore, and there is at least one further letter, digit or underscore

   <td>
   <code>"function"</code>,

   <code>"_function"</code>,

   <code>"some_name"</code>,

   <code>"\___\_some_name"</code>,

   <code>"Another"</code>
   </td>

   <td>

   <code>"_"</code>: missing trailing underscore, digit or letter,

   <code>"8name"</code>: cannot start with digit,

   <code>".function"</code>: cannot start with period,

   <code>" "</code>: cannot be empty space,

   <code>"func name"</code>: cannot have spaces

   </td>
  </tr>

  <tr>
   <td>Vector&lt;Move Type>
   </td>
   <td>Homogeneous vector of aforementioned types including nested vectors
   </td>
   <td><code>[1,2,3,4]</code>: simple U8 vector

<code>[[3,600],[],[0,7,4]]</code>: nested U64 vector

   </td>
   <td><code>[1,2,3,false]</code>: not homogeneous JSON

<code>[1,2,null,4]</code>: invalid elements

<code>[1,2,"7"]</code>: although we allow encoding numbers as strings meaning this array can evaluate to <code>[1,2,7]</code>, the array is still ambiguous so it fails the homogeneity check.

   </td>
  </tr>
  <tr>
   <td>Vector&lt;U8>
   </td>
   <td><em>For convenience, we allow:</em>

U8 vectors represented as UTF-8 (and ASCII) strings.

   </td>
   <td><code>"√®ˆbo72 √∂†∆˚–œ∑π2ie"</code>: UTF-8

   <code>"abcdE738-2 _=?"</code>: ASCII

   </td>
   <td>
   </td>
  </tr>
</table>
