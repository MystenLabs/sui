
# SuiJSON

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

(U128 not supported yet)
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
   <td>Must be homogenous and of SuiJSON type
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
For example `SuiJSON::Number` can represent both _U8_ and _U64_. This means we have to coerce and sometimes convert types.

Which type we coerce depends on the expected Move type. For example, if the Move function expects a U8, we must have received a `SuiJSON::Number` with a value less than 256. More importantly, we have no way to easily express Move addresses in JSON, so we encode them as hex strings prefixed by `0x`.

## Type coercion rules

<table>
  <tr>
   <th>Move Type
   </th>
   <th>SuiJSON
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
   <td>Unsigned number &lt; 256
   </td>
   <td>7
   </td>
   <td><code>-5</code>: negative not allowed

<code>3.9</code>: float now allowed

<code>NaN</code>: not allowed

<code>300</code>: U8 must be less than 256

   </td>
  </tr>
  <tr>
   <td>U64
   </td>
   <td>Unsigned number &lt; U64::MAX
   </td>
   <td><code>12345</code>
   </td>
   <td><code>184467440737095516159</code>: must be less than U64::MAX
   </td>
  </tr>
  <tr>
   <td>U128
   </td>
   <td>Not supported yet
   </td>
   <td>N/A
   </td>
   <td>
   </td>
  </tr>
  <tr>
   <td>Address
   </td>
    <td>20 byte hex string prefixed with <code>0x</code>
   </td>
   <td><code>0x2B1A39A1514E1D8A7CE45919CFEB4FEE70B4E011</code>
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
   <td><code>0x2B1A39A1514E1D8A7CE45919CFEB4FEE</code>
   </td>
   <td>Similar to above
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
   <td><code>[1,2,3,false]</code>: not homogenous

<code>[1,2,null,4]</code>: invalid elements
   </td>
  </tr>
  <tr>
   <td>Vector&lt;U8>
   </td>
   <td><em>For convenience, we allow:</em>

U8 vectors represented as UTF-8 (and ASCII) strings.

   </td>
   <td><code>√®ˆbo72 √∂†∆˚–œ∑π2ie</code>: UTF-8

   <code>abcdE738-2 _=?</code>: ASCII

   </td>
   <td>TODO: Complete invalid example.
   </td>
  </tr>
</table>

For practical examples, see _Anatomy Of A Move Call From REST & CLI_

TODO: Fix Anatomy link above.
