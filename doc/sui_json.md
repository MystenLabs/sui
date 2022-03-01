
# SuiJSON

## **Intro:**

**SuiJSON** is a JSON based format with restrictions which allow us to align inputs more closely with Move Call arguments.

The table below shows the restrictions placed on JSON types to make them SuiJSON compatible.

<table>
  <tr>
   <td><strong>JSON</strong>
   </td>
   <td><strong>SuiJSON Restrictions</strong>
   </td>
   <td><strong>Move Type Mapping</strong>
   </td>
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


## **Type Coercion**
Due to the loosely typed nature of JSON/SuiJSON and the strongly typed nature of Move Types, we sometimes need to overload SuiJSON types to represent multiple Move Types. \
For example _SuiJSON::Number_ can represent both _U8_ and _U64_. This means we have to coerce and sometimes convert types.

Which type we coerce to depends on the expected Move Type. For example if the Move function expects a U8, we must have received a _SuiJSON::Number_with a value less than 256. More importantly, we have no way to easily express Move Addresses in JSON, so we encode them as Hex Strings prefixed by “0x”.

**Summary of Type Coercion Rules**

<table>
  <tr>
   <td><strong>Move Type </strong>
   </td>
   <td><strong>SuiJSON</strong>
   </td>
   <td><strong>Valid Examples</strong>
   </td>
   <td><strong>Invalid Examples</strong>
   </td>
  </tr>
  <tr>
   <td>Bool
   </td>
   <td>Bool
   </td>
   <td>true, false
   </td>
   <td>
   </td>
  </tr>
  <tr>
   <td>U8
   </td>
   <td>Unsigned Number &lt; 256
   </td>
   <td>7
   </td>
   <td><strong>-5</strong>: negative not allowed

<strong>3.9</strong>: float now allowed

<strong>NaN</strong>: not allowed

<strong>300</strong>: U8 must be less than 255

   </td>
  </tr>
  <tr>
   <td>U64
   </td>
   <td>Unsigned Number &lt; U64::MAX
   </td>
   <td>12345
   </td>
   <td><strong>184467440737095516159</strong>: must be less than U64::MAX
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
   <td>20 byte hex string prefixed with 0x
   </td>
   <td>"0x2B1A39A1514E1D8A7CE45919CFEB4FEE70B4E011"
   </td>
   <td><strong>"0x2B1A39"</strong>: string too short

<strong>"2B1A39A1514E1D8A7CE45919CFEB4FEE70B4E011"</strong>: missing <strong>“0x”</strong> prefix

<strong>"0xG2B1A39A1514E1D8A7CE45919CFEB4FEE70B4E01"</strong>: invalid hex char <strong>“G”</strong>
   </td>
  </tr>
  <tr>
   <td>ObjectID
   </td>
   <td>16 byte hex string prefixed with 0x
   </td>
   <td>“0x2B1A39A1514E1D8A7CE45919CFEB4FEE”
   </td>
   <td>Similar to above
   </td>
  </tr>
  <tr>
   <td>Vector&lt;Move Type>
   </td>
   <td>Homogeneous vector of aforementioned types including nested vectors
   </td>
   <td><strong>[1,2,3,4]</strong>: simple U8 vector

<strong>[[3,600],[],[0,7,4]]</strong>: nested U64 vector

   </td>
   <td><strong>[1,2,3,false]</strong>: not homogenous

<strong>[1,2,null,4]</strong>: invalid elements
   </td>
  </tr>
  <tr>
   <td>Vector&lt;U8>
   </td>
   <td><em>For convenience, we allow:</em>

U8 vectors represented as UTF-8 strings.

   </td>
   <td>“√®ˆbo72 √∂†∆˚–œ∑π2ie”
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
   <td>
   </td>
  </tr>
</table>

For practical examples, see *Anatomy Of A Move Call From REST & CLI*