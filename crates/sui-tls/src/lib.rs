// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod certgen;
mod verifier;

pub const TLS_CERTIFICATE_SERVER_NAME: &str = "sui";

pub use certgen::generate_self_signed_tls_certificate;
pub use verifier::ValidatorCertVerifier;

#[cfg(test)]
mod tests {
    use super::*;
    use fastcrypto::ed25519::Ed25519KeyPair;
    use fastcrypto::traits::KeyPair;
    use rustls::server::ClientCertVerifier;

    #[test]
    fn verify() {
        let mut rng = rand::thread_rng();
        let allowed = Ed25519KeyPair::generate(&mut rng);
        let disallowed = Ed25519KeyPair::generate(&mut rng);

        let allowed_public_key = allowed.public().to_owned();
        let allowed_cert = generate_self_signed_tls_certificate(allowed.private()).0;

        let disallowed_cert = generate_self_signed_tls_certificate(disallowed.private()).0;

        let (verifier, allowlist) = ValidatorCertVerifier::new();

        allowlist.write().unwrap().insert(allowed_public_key);

        // The allowed cert passes validation
        verifier
            .verify_client_cert(&allowed_cert, &[], std::time::SystemTime::now())
            .unwrap();

        // The disallowed cert fails validation
        verifier
            .verify_client_cert(&disallowed_cert, &[], std::time::SystemTime::now())
            .unwrap_err();

        // After removing the allowed public key from the set it now fails validation
        allowlist.write().unwrap().clear();
        verifier
            .verify_client_cert(&allowed_cert, &[], std::time::SystemTime::now())
            .unwrap_err();
    }
}
