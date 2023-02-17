// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod certgen;
mod verifier;

pub const SUI_VALIDATOR_SERVER_NAME: &str = "sui";

pub use certgen::SelfSignedCertificate;
pub use verifier::ValidatorCertVerifier;

pub use rustls;

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
        let allowed_cert = SelfSignedCertificate::new(allowed.private(), SUI_VALIDATOR_SERVER_NAME);

        let disallowed_cert =
            SelfSignedCertificate::new(disallowed.private(), SUI_VALIDATOR_SERVER_NAME);

        let (verifier, allowlist) = ValidatorCertVerifier::new();

        allowlist.write().unwrap().insert(allowed_public_key);

        // The allowed cert passes validation
        verifier
            .verify_client_cert(
                &allowed_cert.rustls_certificate(),
                &[],
                std::time::SystemTime::now(),
            )
            .unwrap();

        // The disallowed cert fails validation
        verifier
            .verify_client_cert(
                &disallowed_cert.rustls_certificate(),
                &[],
                std::time::SystemTime::now(),
            )
            .unwrap_err();

        // After removing the allowed public key from the set it now fails validation
        allowlist.write().unwrap().clear();
        verifier
            .verify_client_cert(
                &allowed_cert.rustls_certificate(),
                &[],
                std::time::SystemTime::now(),
            )
            .unwrap_err();
    }
}
