// NOTE: This is a proposal for adding OWS keystore support.
// The Keystore enum would gain an Ows variant:
//
//   pub enum Keystore {
//       File(FileBasedKeystore),
//       InMem(InMemKeystore),
//       Ows(OWSKeystore),  // <-- new
//   }
//
// OWSKeystore would implement AccountKeystore by decrypting keys
// from the OWS vault via ows-lib crate.
