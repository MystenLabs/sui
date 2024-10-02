module 0x42::m;

public struct Zs {}
public struct As {}

const Zc: u64 = 1;
const Ac: u32 = 0;
const AString: vector<u8> = b"This is some emojis 🤔🤔🤔";
const NotAString: vector<u16> = vector[1,2,3,4];

public fun zf(): u64 { Zc }
public fun af(): u32 { Ac }
public fun sf(): vector<u8> { AString }
public fun nf(): vector<u16> { NotAString }
