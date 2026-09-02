// A non-internal '#[error]' constant is rejected at the definition, even if never used and
// whatever the written visibility

module 0x42::a {

#[error]
public(package) const ENotFound: vector<u8> = b"not found";

#[error]
public const EOther: vector<u8> = b"other";

}
