use async_trait::async_trait;
use pingora::{
    listeners::TlsAccept,
    tls::{self, ssl},
};
use tracing::debug;

pub struct TLSCertCallback {
    cert: tls::x509::X509,
    key: tls::pkey::PKey<tls::pkey::Private>,
    hostname: String,
}

impl TLSCertCallback {
    pub fn new(cert_path: String, key_path: String, hostname: String) -> Self {
        // TODO: error handling
        debug!("cert path: {}", cert_path);
        debug!("key path: {}", key_path);

        let cert_bytes = std::fs::read(cert_path).unwrap();
        let key_bytes = std::fs::read(key_path).unwrap();
        Self {
            cert: tls::x509::X509::from_pem(&cert_bytes).unwrap(),
            key: tls::pkey::PKey::private_key_from_pem(&key_bytes).unwrap(),
            hostname,
        }
    }
}

#[async_trait]
impl TlsAccept for TLSCertCallback {
    async fn certificate_callback(&self, ssl: &mut ssl::SslRef) -> () {
        debug!(?ssl);

        // TODO proper error logging & check what a robust check would be
        let server_name = ssl.servername(ssl::NameType::HOST_NAME).unwrap();
        debug!("server name: {:?}", server_name);
        debug!("hostname: {}", self.hostname);

        if server_name == self.hostname {
            debug!("setting certificate");
            debug!("subject name: {:?}", self.cert.subject_name());
            tls::ext::ssl_use_certificate(ssl, &self.cert).unwrap();
            tls::ext::ssl_use_private_key(ssl, &self.key).unwrap();
        }
    }
}
