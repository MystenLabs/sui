// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use maplit::btreemap;
use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::BTreeMap;

/// The word size (in bytes) of the EVM.
pub const WORD_SIZE: usize = 32;

/// A lazy constant which defines placeholders which can be referenced as `${NAME}`
/// in emitted code. All emitted strings have those placeholders substituted.
static PLACEHOLDERS: Lazy<BTreeMap<&'static str, &'static str>> = Lazy::new(|| {
    btreemap! {
        // ---------------------------------
        // Numerical constants
        "MAX_U8" => "0xff",
        "MAX_U16" => "0xffff",
        "MAX_U24" => "0xffffff",
        "MAX_U32" => "0xffffffff",
        "MAX_U40" => "0xffffffffff",
        "MAX_U48" => "0xffffffffffff",
        "MAX_U56" => "0xffffffffffffff",
        "MAX_U64" => "0xffffffffffffffff",
        "MAX_U72" => "0xffffffffffffffffff",
        "MAX_U80" => "0xffffffffffffffffffff",
        "MAX_U88" => "0xffffffffffffffffffffff",
        "MAX_U96" => "0xffffffffffffffffffffffff",
        "MAX_U104" => "0xffffffffffffffffffffffffff",
        "MAX_U112" => "0xffffffffffffffffffffffffffff",
        "MAX_U120" => "0xffffffffffffffffffffffffffffff",
        "MAX_U128" => "0xffffffffffffffffffffffffffffffff",
        "MAX_U136" => "0xffffffffffffffffffffffffffffffffff",
        "MAX_U144" => "0xffffffffffffffffffffffffffffffffffff",
        "MAX_U152" => "0xffffffffffffffffffffffffffffffffffffff",
        "MAX_U160" => "0xffffffffffffffffffffffffffffffffffffffff",
        "MAX_U168" => "0xffffffffffffffffffffffffffffffffffffffffff",
        "MAX_U176" => "0xffffffffffffffffffffffffffffffffffffffffffff",
        "MAX_U184" => "0xffffffffffffffffffffffffffffffffffffffffffffff",
        "MAX_U192" => "0xffffffffffffffffffffffffffffffffffffffffffffffff",
        "MAX_U200" => "0xffffffffffffffffffffffffffffffffffffffffffffffffff",
        "MAX_U208" => "0xffffffffffffffffffffffffffffffffffffffffffffffffffff",
        "MAX_U216" => "0xffffffffffffffffffffffffffffffffffffffffffffffffffffff",
        "MAX_U224" => "0xffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
        "MAX_U232" => "0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
        "MAX_U240" => "0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
        "MAX_U248" => "0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
        "MAX_U256" =>
        "0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",

        // ---------------------------------
        // Memory
        // The size of the memory used by the compilation scheme. This must be the
        // sum of the sizes required by the locations defined below.
        "USED_MEM" => "160",

        // Location where the current size of the used memory is stored. New memory will
        // be allocated from there.
        "MEM_SIZE_LOC" => "0",

        // Locations in memory we use for scratch computations
        "SCRATCH1_LOC" => "32",
        "SCRATCH2_LOC" => "64",

        // Storage groups. Those are used to augment words by creating a keccak256 value from
        // word and group to create a unique storage key. This basically allows -- by the
        // magic if keccak -- to multiplex the 256 bit address space into multiple ones, and
        // to implement tables with 256 bit keys. The LINEAR_STORAGE_GROUP is reserved
        // for Move memory. Other groups are created as tables are dynamically allocated.
        // STORAGE_GROUP_COUNTER_LOC contains the largest storage group allocated so far.
        // A storage group identifier is 4 bytes long.
        "LINEAR_STORAGE_GROUP" => "0",
        "ADMIN_STORAGE_GROUP" => "1",
        "WORD_AND_STORAGE_GROUP_LENGTH" => "36",

        // Number of storage groups already allocated. Right now there are two: linear storage
        // group and admin storage group.
        "NUM_STATIC_STORAGE_GROUP" => "2",

        // Counters in the ADMIN_STORAGE_GROUP for persistent storage of group and linked storage
        // counters.
        "STORAGE_GROUP_COUNTER_LOC" => "0",
        "LINKED_STORAGE_COUNTER_LOC" => "1",

        // Categories to distinguish different types of pointers into the LINEAR_STORAGE_GROUP.
        // See discussion of YulFunction::MakeTypeStorageBase.
        "RESOURCE_STORAGE_CATEGORY" => "0",
        "LINKED_STORAGE_CATEGORY" => "1",

        // Size (in bytes) of the resource exists flag which proceeds any data in storage for
        // a resource.
        "RESOURCE_EXISTS_FLAG_SIZE" => "32",

        // Size (in bytes) of the vector metadata.
        "VECTOR_METADATA_SIZE" => "32",
    }
});

/// Substitutes placeholders in the given string.
pub fn substitute_placeholders(s: &str) -> Option<String> {
    static REX: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?m)(\$\{(?P<var>[A-Z0-9_]+)\})").unwrap());
    let mut at = 0;
    let mut changes = false;
    let mut res = "".to_string();
    while let Some(cap) = (*REX).captures(&s[at..]) {
        let m = cap.get(0).unwrap();
        let v = cap.name("var").unwrap();
        res.push_str(&s[at..at + m.start()]);
        if let Some(repl) = PLACEHOLDERS.get(v.as_str()) {
            changes = true;
            res.push_str(repl)
        } else {
            res.push_str(m.as_str())
        }
        at += m.end();
    }
    if changes {
        res.push_str(&s[at..]);
        Some(res)
    } else {
        None
    }
}

/// A macro which allows to define Yul functions together with their definitions.
/// This generates an enum `YulFunction` and functions `yule_name`, `yul_def`,
/// and `yul_deps` for values of this type.
macro_rules! functions {
    ($($name:ident: $def:literal $(dep $dep:ident)*),* $(, )?) => {
        #[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Debug, Hash)]
        #[allow(dead_code)]
        pub enum YulFunction {
            $($name,)*
        }
        impl YulFunction {
            #[allow(dead_code)]
            pub fn yule_name(self) -> String {
                match self {
                $(
                    YulFunction::$name => make_yule_name(stringify!($name)),
                )*
                }
            }
            #[allow(dead_code)]
            pub fn yule_def(self) -> String {
                match self {
                $(
                    YulFunction::$name => make_yule_def(stringify!($name), $def),
                )*
                }
            }
            #[allow(dead_code)]
            pub fn yule_deps(self) -> Vec<YulFunction> {
                match self {
                $(
                    YulFunction::$name => vec![$(YulFunction::$dep,)*],
                )*
                }

            }
        }
    }
}

/// Helper to create name of Yul function.
fn make_yule_name(name: &str) -> String {
    format!("${}", name)
}

/// Helper to create definition of a Yule function.
fn make_yule_def(name: &str, body: &str) -> String {
    format!("function ${}{}", name, body)
}

// The Yul functions supporting the compilation scheme.
functions! {
// -------------------------------------------------------------------------------------------
// Abort
Abort: "(code) {
    mstore(0, code)
    revert(24, 8) // TODO: store code as a string?
}",
AbortBuiltin: "() {
    $Abort(sub(0, 1))
}" dep Abort,
NotImplemented: "() {
    $AbortBuiltin()
}" dep AbortBuiltin,
RevertForward: "() {
  let pos := mload(${MEM_SIZE_LOC})
  returndatacopy(pos, 0, returndatasize())
  revert(pos, returndatasize())
}",

// -------------------------------------------------------------------------------------------
// Memory

// TODO: many of the memory operations which take a `size` parameter can be specialized
//   for the given size (8, 64, 128, or 256 bytes). The Yul optimizer does some of this,
//   but it is not transparent how far this goes. We should better generate those
//   functions algorithmically and specialize them ourselves. Doing the specialization
//   manual is too error prone.

// Allocates memory of size.
// TODO: add some memory recovery (e.g. over free lists), and benchmark against the current
//   arena style.
Malloc: "(size) -> offs {
    offs := mload(${MEM_SIZE_LOC})
    // pad to word size
    mstore(${MEM_SIZE_LOC}, add(offs, shl(5, shr(5, add(size, 31)))))
}",

MallocAt: "(offs, size) {
  let new_free_ptr := add(offs, $RoundUp(size))
  // protect against overflow
  if or(gt(new_free_ptr, 0xffffffffffffffff), lt(new_free_ptr, offs)) { $AbortBuiltin() }
  mstore(${MEM_SIZE_LOC}, new_free_ptr)
}" dep RoundUp dep AbortBuiltin,

// Frees memory of size
Free: "(offs, size) {
}",

// Makes a pointer, using the lowest bit to indicate whether it is for storage or memory.
MakePtr: "(is_storage, offs) -> ptr {
  ptr := or(is_storage, shl(1, offs))
}",

// Returns true if this is a storage  pointer.
IsStoragePtr: "(ptr) -> b {
  b := and(ptr, 0x1)
}",

// Returns the offset of this pointer.
OffsetPtr: "(ptr) -> offs {
  offs := shr(1, ptr)
}",

// Constructs a bit mask for a value of size bytes. E.g. if size == 1, returns 0xff.
// Note that we expect the Yul optimizer to specialize this for constant parameters.
MaskForSize: "(size) -> mask {
  mask := sub(shl(shl(3, size), 1), 1)
}",

// Extracts size bytes from word, starting at byte index start. The most significant byte
// is at index 0 (big endian).
ExtractBytes: "(word, start, size) -> bytes {
   switch size
   case 1 {
      // use the faster byte primitive
      bytes := byte(start, word)
   }
   default {
      // As we have big endian, we need to right shift the value from
      // where the highest byte starts in the word (32 - start), minus
      // the size.
      let shift_bits := shl(3, sub(sub(32, start), size))
      bytes := and(shr(shift_bits, word), $MaskForSize(size))
   }
}" dep MaskForSize,

// Inject size bytes into word, starting a byte index start.
InjectBytes: "(word, start, size, bytes) -> new_word {
   let shift_bits := shl(3, sub(sub(32, start), size))
   // Blend out the bits which we inject
   let neg_mask := not(shl(shift_bits, $MaskForSize(size)))
   word := and(word, neg_mask)
   // Overlay the bits we inject
   new_word := or(word, shl(shift_bits, bytes))
}" dep MaskForSize,

// For a byte offset, compute word offset and byte offset within this word.
ToWordOffs: "(offs) -> word_offs, byte_offset {
  word_offs := shr(5, offs)
  byte_offset := and(offs, 0x1F)
}",

// For a byte offset within a word (< 32), compute the number of bytes which
// overflow the word for a value of size.
OverflowBytes: "(byte_offset, size) -> overflow_bytes {
  let available_bytes := sub(32, byte_offset)
  switch gt(size, available_bytes)
  case 0 {
    overflow_bytes := 0
  }
  default {
    overflow_bytes := sub(size, available_bytes)
  }
}",

// Loads bytes from memory offset.
MemoryLoadBytes: "(offs, size) -> val {
  // Lower bit where the value in the higher bytes ends
  let bit_end := shl(3, sub(32, size))
  val := shr(bit_end, mload(offs))
}" dep MaskForSize,

// Stores bytes to memory offset.
MemoryStoreBytes: "(offs, size, val) {
  let bit_end := shl(3, sub(32, size))
  let mask := shl(bit_end, $MaskForSize(size))
  mstore(offs, or(and(mload(offs), not(mask)), shl(bit_end, val)))
}" dep MaskForSize,

// Loads bytes from storage offset.
StorageLoadBytes: "(offs, size) -> val {
  let word_offs, byte_offs := $ToWordOffs(offs)
  let key := $StorageKey(${LINEAR_STORAGE_GROUP}, word_offs)
  val := $ExtractBytes(sload(key), byte_offs, size)
  let overflow_bytes := $OverflowBytes(byte_offs, size)
  if $LogicalNot(iszero(overflow_bytes)) {
    key := $StorageKey(${LINEAR_STORAGE_GROUP}, add(word_offs, 1))
    let extra_bytes := $ExtractBytes(sload(key), 0, overflow_bytes)
    val := or(shl(shl(3, overflow_bytes), val), extra_bytes)
  }
}" dep ToWordOffs dep StorageKey dep ExtractBytes dep OverflowBytes dep LogicalNot,

// Store bytes to storage offset.
StorageStoreBytes: "(offs, size, bytes) {
  let word_offs, byte_offs := $ToWordOffs(offs)
  let key := $StorageKey(${LINEAR_STORAGE_GROUP}, word_offs)
  let overflow_bytes := $OverflowBytes(byte_offs, size)
  switch overflow_bytes
  case 0 {
    sstore(key, $InjectBytes(sload(key), byte_offs, size, bytes))
  }
  default {
    // Shift the higher bytes to the right
    let used_bytes := sub(size, overflow_bytes)
    let higher_bytes := shr(used_bytes, bytes)
    let lower_bytes := and(bytes, $MaskForSize(overflow_bytes))
    sstore(key, $InjectBytes(sload(key), byte_offs, used_bytes, higher_bytes))
    key := $StorageKey(${LINEAR_STORAGE_GROUP}, add(word_offs, 1))
    sstore(key, $InjectBytes(sload(key), 0, overflow_bytes, lower_bytes))
  }
}" dep ToWordOffs dep StorageKey dep InjectBytes dep OverflowBytes,

// Make a unique key into storage, where word can have full 32 byte size, and type
// indicates the kind of the key given as a byte. This uses keccak256 to fold
// value and type into a unique storage key.
StorageKey: "(group, word) -> key {
  mstore(${SCRATCH1_LOC}, word)
  mstore(${SCRATCH2_LOC}, shl(224, group))
  key := keccak256(${SCRATCH1_LOC}, ${WORD_AND_STORAGE_GROUP_LENGTH})
}",

// Make a base storage offset for a given type. The result has 255 bits width and can be passed into
// $MakePtr(true, result) to create a pointer. This pointer can be used to linearly address
// exclusive memory, owned by the pointer, with an address space of 60 bits.
//
//  254                                                    0
//  cccccc..cccccctttttt..tttttiiiii..iiiiiioooooo..oooooooo
//   category       type_hash     id           offset
//      3              32         160           60
//
// The category indicates what kind of type storage this is, and determines how id
// is interpreted. RESOURCE_STORAGE_CATEGORY indicates that id is a resource
// address. LINKED_STORAGE_CATEGORY indicates that id is a handle for data linked
// to from some other storage (for instance, a vector aggregated by a resource).
// The type_hash identifies the type of the stored value. The id is any 20 byte
// number which identifies an instance of this type (e.g. an address if this is a resource).
MakeTypeStorageBase: "(category, type_hash, id) -> offs {
  offs := or(shl(252, category), or(shl(220, type_hash), shl(60, id)))
}",

// Make a new base storage offset for linked storage. This creates a new handle
// and then calls MakeTypeStorageBase.
NewLinkedStorageBase: "(type_hash) -> offs {
  let key := $StorageKey(${ADMIN_STORAGE_GROUP}, ${LINKED_STORAGE_COUNTER_LOC})
  let handle := sload(key)
  sstore(key, add(handle, 1))
  offs := $MakeTypeStorageBase(${LINKED_STORAGE_CATEGORY}, type_hash, handle)
}" dep MakeTypeStorageBase,

// Indexes pointer by offset.
IndexPtr: "(ptr, offs) -> new_ptr {
  new_ptr := $MakePtr($IsStoragePtr(ptr), add($OffsetPtr(ptr), offs))
}" dep MakePtr dep IsStoragePtr dep OffsetPtr,

NewTableHandle: "() -> handle {
  let key := $StorageKey(${ADMIN_STORAGE_GROUP}, ${STORAGE_GROUP_COUNTER_LOC})
  handle := sload(key)
  if iszero(handle) {
     // no tables have been allocated in this contract, need to initialize the counter
     // to the number of storage groups already statically allocated
     handle := ${NUM_STATIC_STORAGE_GROUP}
  }
  sstore(key, add(handle, 1))
}
" dep StorageKey,
// ------------

// Loads u8 from pointer.
LoadU8: "(ptr) -> val {
  let offs := $OffsetPtr(ptr)
  switch $IsStoragePtr(ptr)
  case 0 {
    val := $MemoryLoadU8(offs)
  }
  default {
    val := $StorageLoadU8(offs)
  }
}" dep OffsetPtr dep IsStoragePtr dep MemoryLoadU8 dep StorageLoadU8,

// Loads u8 from memory offset.
MemoryLoadU8: "(offs) -> val {
  val := $MemoryLoadBytes(offs, 1)
}" dep MemoryLoadBytes,

// Loads u8 from storage offset.
StorageLoadU8: "(offs) -> val {
  val := $StorageLoadBytes(offs, 1)
}" dep StorageLoadBytes,

// Stores u8 to pointer.
StoreU8: "(ptr, val) {
  let offs := $OffsetPtr(ptr)
  switch $IsStoragePtr(ptr)
  case 0 {
    $MemoryStoreU8(offs, val)
  }
  default {
    $StorageStoreU8(offs, val)
  }
}" dep OffsetPtr dep IsStoragePtr dep MemoryStoreU8 dep StorageStoreU8,

// Stores u8 to memory offset.
MemoryStoreU8: "(offs, val) {
  // Shortcut via special instruction
  mstore8(offs, val)
}",

// Stores u8 to storage offset.
StorageStoreU8: "(offs, val) {
  $StorageStoreBytes(offs, 1, val)
}" dep StorageStoreBytes,

// ------------

// Loads u64 from pointer.
LoadU64: "(ptr) -> val {
  let offs := $OffsetPtr(ptr)
  switch $IsStoragePtr(ptr)
  case 0 {
    val := $MemoryLoadU64(offs)
  }
  default {
    val := $StorageLoadU64(offs)
  }
}" dep OffsetPtr dep IsStoragePtr dep MemoryLoadU64 dep StorageLoadU64,

// Loads u64 from memory offset.
MemoryLoadU64: "(offs) -> val {
  val := $MemoryLoadBytes(offs, 8)
}" dep MemoryLoadBytes,

// Loads u64 from storage offset.
StorageLoadU64: "(offs) -> val {
  val := $StorageLoadBytes(offs, 8)
}" dep StorageLoadBytes,

// Stores u64 to pointer.
StoreU64: "(ptr, val) {
  let offs := $OffsetPtr(ptr)
  switch $IsStoragePtr(ptr)
  case 0 {
    $MemoryStoreU64(offs, val)
  }
  default {
    $StorageStoreU64(offs, val)
  }
}" dep OffsetPtr dep IsStoragePtr dep MemoryStoreU64 dep StorageStoreU64,

// Stores u64 to memory offset.
MemoryStoreU64: "(offs, val) {
  $MemoryStoreBytes(offs, 8, val)
}" dep MemoryStoreBytes,

// Stores u64 to storage offset.
StorageStoreU64: "(offs, val) {
  $StorageStoreBytes(offs, 8, val)
}" dep StorageStoreBytes,

// ------------

// Loads u128 from pointer.
LoadU128: "(ptr) -> val {
  let offs := $OffsetPtr(ptr)
  switch $IsStoragePtr(ptr)
  case 0 {
    val := $MemoryLoadU128(offs)
  }
  default {
    val := $StorageLoadU128(offs)
  }
}" dep OffsetPtr dep IsStoragePtr dep MemoryLoadU128 dep StorageLoadU128,

// Loads u128 from memory offset.
MemoryLoadU128: "(offs) -> val {
  val := $MemoryLoadBytes(offs, 16)
}" dep MemoryLoadBytes,

// Loads u128 from storage offset.
StorageLoadU128: "(offs) -> val {
  val := $StorageLoadBytes(offs, 16)
}" dep StorageLoadBytes,

// Stores u128 to pointer.
StoreU128: "(ptr, val) {
  let offs := $OffsetPtr(ptr)
  switch $IsStoragePtr(ptr)
  case 0 {
    $MemoryStoreU128(offs, val)
  }
  default {
    $StorageStoreU128(offs, val)
  }
}" dep OffsetPtr dep IsStoragePtr dep MemoryStoreU128 dep StorageStoreU128,

// Stores u128 to memory offset.
MemoryStoreU128: "(offs, val) {
  $MemoryStoreBytes(offs, 16, val)
}" dep MemoryStoreBytes,

// Stores u128 to storage offset.
StorageStoreU128: "(offs, val) {
  $StorageStoreBytes(offs, 16, val)
}" dep StorageStoreBytes,

// ------------

// Loads u256 from pointer.
LoadU256: "(ptr) -> val {
  let offs := $OffsetPtr(ptr)
  switch $IsStoragePtr(ptr)
  case 0 {
    val := $MemoryLoadU256(offs)
  }
  default {
    val := $StorageLoadU256(offs)
  }
}" dep OffsetPtr dep IsStoragePtr dep MemoryLoadU256 dep StorageLoadU256,

// Loads u256 from memory offset.
MemoryLoadU256: "(offs) -> val {
  val := $MemoryLoadBytes(offs, 32)
}" dep MemoryLoadBytes,

// Loads u256 from storage offset.
StorageLoadU256: "(offs) -> val {
  val := $StorageLoadBytes(offs, 32)
}" dep StorageLoadBytes,

// Stores u256 to pointer.
StoreU256: "(ptr, val) {
  let offs := $OffsetPtr(ptr)
  switch $IsStoragePtr(ptr)
  case 0 {
    $MemoryStoreU256(offs, val)
  }
  default {
    $StorageStoreU256(offs, val)
  }
}" dep OffsetPtr dep IsStoragePtr dep MemoryStoreU256 dep StorageStoreU256,

// Stores u256 to memory offset.
MemoryStoreU256: "(offs, val) {
  $MemoryStoreBytes(offs, 32, val)
}" dep MemoryStoreBytes,

// Stores u256 to storage offset.
StorageStoreU256: "(offs, val) {
  $StorageStoreBytes(offs, 32, val)
}" dep StorageStoreBytes,

// ------------

// Loads u256 from a word-aligned storage offset
AlignedStorageLoad: "(offs) -> val {
  let word_offs := shr(5, offs)
  val := sload($StorageKey(${LINEAR_STORAGE_GROUP}, word_offs))
}" dep StorageKey,

// Stores u256 to a word-aligned storage offset
AlignedStorageStore: "(offs, val) {
  let word_offs := shr(5, offs)
  sstore($StorageKey(${LINEAR_STORAGE_GROUP}, word_offs), val)
}" dep StorageKey,

// TODO: this function needs more testing
// Copies size bytes from memory to memory.
CopyMemory: "(src, dst, size) {
  let num_words, overflow_bytes := $ToWordOffs(size)
  let i := 0
  for { } lt(i, mul(num_words, 32)) { i := add(i, 32) } {
    mstore(add(dst, i), mload(add(src, i)))
  }
  if overflow_bytes {
    let mask := $MaskForSize(sub(32, overflow_bytes))
    let overflow_offs := mul(num_words, 32)
    let dst_word := and(mload(add(dst, overflow_offs)), mask)
    let src_word := and(mload(add(src, overflow_offs)), not(mask))
    mstore(add(dst, overflow_offs), or(dst_word, src_word))
  }
}" dep ToWordOffs,

CheckMemorySize: "(len) -> checked_len {
    if gt(len, 0xffffffffffffffff) { $AbortBuiltin() }
    checked_len := len
}" dep AbortBuiltin,

CopyFromCallDataToMemory: "(src, dst, length) {
    calldatacopy(dst, src, length)
    mstore(add(dst, length), 0)
}",

CopyFromMemoryToMemory: "(src, dst, length) {
  let i := 0
  for { } lt(i, length) { i := add(i, 32) }
  {
    mstore(add(dst, i), mload(add(src, i)))
  }
  if gt(i, length)
  {
    // clear end
    mstore(add(dst, length), 0)
  }
}",

ResizeVector: "(v_offs, capacity, type_size) -> new_v_offs {
    let new_capacity := mul(capacity, 2)
    let data_size := add(${VECTOR_METADATA_SIZE}, mul(capacity, type_size))
    let new_data_size := add(${VECTOR_METADATA_SIZE}, mul(new_capacity, type_size))
    new_v_offs := $Malloc(new_data_size)
    $CopyMemory(v_offs, new_v_offs, data_size)
    // update capacity at new location
    $MemoryStoreU64(add(new_v_offs, 8), new_capacity)
    $Free(v_offs, data_size)
}" dep Malloc dep CopyMemory dep MemoryStoreU64 dep Free,

// -------------------------------------------------------------------------------------------
// Arithmetic, Logic, and Relations
AddU64: "(x, y) -> r {
    if lt(sub(${MAX_U64}, x), y) { $AbortBuiltin() }
    r := add(x, y)
}" dep AbortBuiltin,
MulU64: "(x, y) -> r {
    if gt(y, div(${MAX_U64}, x)) { $AbortBuiltin() }
    r := mul(x, y)
}" dep AbortBuiltin,
AddU8: "(x, y) -> r {
    if lt(sub(${MAX_U8}, x), y) { $AbortBuiltin() }
    r := add(x, y)
}" dep AbortBuiltin,
MulU8: "(x, y) -> r {
    if gt(y, div(${MAX_U8}, x)) { $AbortBuiltin() }
    r := mul(x, y)
}" dep AbortBuiltin,
AddU128: "(x, y) -> r {
    if lt(sub(${MAX_U128}, x), y) { $AbortBuiltin() }
    r := add(x, y)
}" dep AbortBuiltin,
MulU128: "(x, y) -> r {
    if gt(y, div(${MAX_U128}, x)) { $AbortBuiltin() }
    r := mul(x, y)
}" dep AbortBuiltin,
AddU256: "(x, y) -> r {
    if lt(sub(${MAX_U256}, x), y) { $AbortBuiltin() }
    r := add(x, y)
}" dep AbortBuiltin,
MulU256: "(x, y) -> r {
    if gt(y, div(${MAX_U256}, x)) { $AbortBuiltin() }
    r := mul(x, y)
}" dep AbortBuiltin,
Sub: "(x, y) -> r {
    if lt(x, y) { $AbortBuiltin() }
    r := sub(x, y)
}" dep AbortBuiltin,
Div: "(x, y) -> r {
    if eq(y, 0) { $AbortBuiltin() }
    r := div(x, y)
}" dep AbortBuiltin,
Mod: "(x, y) -> r {
    if eq(y, 0) { $AbortBuiltin() }
    r := mod(x, y)
}" dep AbortBuiltin,
Shr: "(x, y) -> r {
    r := shr(y, x)
}",
Shl: "(x, y) -> r {
  r := shl(y, x)
}",
ShlU8: "(x, y) -> r {
    r := and(shl(y, x), ${MAX_U8})
}",
ShlU64: "(x, y) -> r {
    r := and(shl(y, x), ${MAX_U64})
}",
ShlU128: "(x, y) -> r {
    r := and(shl(y, x), ${MAX_U128})
}",
ShlU256: "(x, y) -> r {
    r := and(shl(y, x), ${MAX_U256})
}",
Gt: "(x, y) -> r {
    r := gt(x, y)
}",
Lt: "(x, y) -> r {
    r := lt(x, y)
}",
GtEq: "(x, y) -> r {
    r := or(gt(x, y), eq(x, y))
}",
LtEq: "(x, y) -> r {
    r := or(lt(x, y), eq(x, y))
}",
Eq: "(x, y) -> r {
    r := eq(x, y)
}",
EqVector: "(x, y, elem_size) -> r {
    let len_x := $MemoryLoadU64(x)
    let len_y := $MemoryLoadU64(y)
    if $Neq(len_x, len_y) {
        r := false
        leave
    }
    let data_size_bytes := mul(elem_size, len_x)
    let num_words, overflow_bytes := $ToWordOffs(data_size_bytes)
    let i := 0
    for { } lt(i, mul(num_words, 32)) { i := add(i, 32) } {
        if $Neq(mload(add(x, add(i, 32))), mload(add(y, add(i, 32)))) {
            r := false
            leave
        }
    }
    let mask := $MaskForSize(sub(32, overflow_bytes))
    let overflow_offs := mul(num_words, 32)
    let x_overflow := mload(add(x, add(overflow_offs, 32)))
    let y_overflow := mload(add(y, add(overflow_offs, 32)))
    r := eq(or(mask, x_overflow), or(mask, y_overflow))
}" dep Neq dep MemoryLoadU64 dep ToWordOffs dep MaskForSize,
Neq: "(x, y) -> r {
    r := $LogicalNot(eq(x, y))
}" dep LogicalNot,
LogicalAnd: "(x, y) -> r {
    r := and(x, y)
}",
LogicalOr: "(x, y) -> r {
    r := or(x, y)
}",
LogicalNot: "(x) -> r {
    r := iszero(x)
}",
BitAnd: "(x, y) -> r {
    r := and(x, y)
}",
BitOr: "(x, y) -> r {
    r := or(x, y)
}",
BitXor: "(x, y) -> r {
    r := xor(x, y)
}",
BitNot: "(x) -> r {
    r := not(x)
}",
CastU8: "(x) -> r {
    if gt(x, ${MAX_U8}) { $AbortBuiltin() }
    r := x
}" dep AbortBuiltin,
CastU64: "(x) -> r {
    if gt(x, ${MAX_U64}) { $AbortBuiltin() }
    r := x
}" dep AbortBuiltin,
CastU128: "(x) -> r {
    if gt(x, ${MAX_U128}) { $AbortBuiltin() }
    r := x
}" dep AbortBuiltin,
CastU256: "(hi, lo) -> r {
    if gt(hi, ${MAX_U128}) { $AbortBuiltin() }
    if gt(lo, ${MAX_U128}) { $AbortBuiltin() }
    r := add(shl(128, hi), lo)
}" dep AbortBuiltin,
ClosestGreaterPowerOfTwo: "(x) -> r {
    r := or(r, shr(1, x))
    r := or(r, shr(2, r))
    r := or(r, shr(4, r))
    r := or(r, shr(8, r))
    r := or(r, shr(16, r))
    r := or(r, shr(32, r))
    r := add(x, 1)
}",
RoundUp: "(value) -> result {
    result := and(add(value, 31), not(31))
}",
ReturnDataSelector: "() -> sig {
  if gt(returndatasize(), 3) {
    let pos := $Malloc(4)
    returndatacopy(pos, 0, 4)
    sig := shr(224, mload(pos))
  }
}" dep Malloc,
TryDecodePanicData: "() -> success, data {
  if gt(returndatasize(), 0x23) {
    let pos := $Malloc(0x20)
    returndatacopy(pos, 4, 0x20)
    success := 1
    data := mload(pos)
  }
}" dep Malloc,
PackErrData: "() -> data {
  data := $Malloc(add(returndatasize(), 0x20))
  $MemoryStoreU64(data, returndatasize())
  $MemoryStoreU64(add(data, 8), returndatasize())
  returndatacopy(add(data, 0x20), 0, returndatasize())
}
" dep Malloc dep MemoryStoreU64,
TryDecodeErrMsg: "() -> data {
  if lt(returndatasize(), 0x44) { leave }
  data := $Malloc(0x20)
  returndatacopy(data, 4, 0x20)
  let offset := mload(data)
  if or(
      gt(offset, 0xffffffffffffffff),
      gt(add(offset, 0x24), returndatasize())
      ) {
      leave
  }
  data := $Malloc(0x20)
  returndatacopy(data, add(4, offset), 0x20)
  let length := mload(data)
  if or(
    gt(length, 0xffffffffffffffff),
    gt(add(add(offset, 0x24), length), returndatasize())
  ) {
    leave
  }
  data := $Malloc(add(length, 0x20))
  $MemoryStoreU64(data, length)
  $MemoryStoreU64(add(data, 8), length)
  returndatacopy(add(data, 0x20), add(offset, 0x24), length)
}" dep Malloc dep MemoryStoreU64,
NumToString: "(x) -> s {
  if iszero(x) {
    s := $Malloc(add(${VECTOR_METADATA_SIZE}, 2))
    $MemoryStoreU64(s, 1)
    $MemoryStoreU64(add(s, 8), 2)
    $MemoryStoreU8(add(s, ${VECTOR_METADATA_SIZE}), 48) // string \"0\"
    leave
  }
  let temp := x
  let num_digits := 0
  for { } temp { num_digits := add(num_digits, 1) } {
    temp := div(temp, 10)
  }
  let digits_space := $ClosestGreaterPowerOfTwo(num_digits)
  s := $Malloc(add(${VECTOR_METADATA_SIZE}, digits_space))
  $MemoryStoreU64(s, num_digits)
  $MemoryStoreU64(add(s, 8), digits_space)
  let digit
  for { } x { } {
    digit := add(48, mod(x, 10))
    num_digits := sub(num_digits, 1)
    $MemoryStoreU8(add(add(s, ${VECTOR_METADATA_SIZE}), num_digits), digit)
    x := div(x, 10)
  }
}" dep Malloc dep MemoryStoreU64 dep MemoryStoreU8 dep ClosestGreaterPowerOfTwo,
ExtendVector: "(v1, v2, elem_size) -> new_v1 {
  let v1_len := $MemoryLoadU64(v1)
  let v2_len := $MemoryLoadU64(v2)
  let new_len := add(v1_len, v2_len)
  let v1_cap := $MemoryLoadU64(add(v1, 8))
  new_v1 := v1
  if iszero(gt(v1_cap, new_len)){
    let new_cap := $ClosestGreaterPowerOfTwo(new_len)
    new_v1 := $Malloc(add(mul(new_cap, elem_size), ${VECTOR_METADATA_SIZE}))
    $CopyMemory(v1, new_v1, add(mul(v1_len, elem_size), ${VECTOR_METADATA_SIZE}))
    $MemoryStoreU64(add(new_v1, 8), new_cap)
    $Free(v1, add(mul(v1_len, elem_size), ${VECTOR_METADATA_SIZE}))
  }
  let src := add(v2, ${VECTOR_METADATA_SIZE})
  let dst := add(add(new_v1, ${VECTOR_METADATA_SIZE}), mul(elem_size, v1_len))
  $CopyMemory(src, dst, mul(v2_len, elem_size))
  $MemoryStoreU64(new_v1, new_len)
}" dep Malloc dep MemoryLoadU64 dep MemoryStoreU64 dep ClosestGreaterPowerOfTwo dep CopyMemory dep Free,
}
