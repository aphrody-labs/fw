//! TLS material for the server: PEM loading and, for development and tests, a self-signed certificate.

use std::io;
use std::sync::Arc;

use rustls::pki_types::{CertificateDer, PrivateKeyDer};

/// A certificate chain and its private key.
pub struct Tls {
    pub(crate) chain: Vec<CertificateDer<'static>>,
    pub(crate) key: PrivateKeyDer<'static>,
}

impl Tls {
    /// From DER material.
    #[must_use]
    pub fn from_der(chain: Vec<CertificateDer<'static>>, key: PrivateKeyDer<'static>) -> Self {
        Self { chain, key }
    }

    /// From PEM text: one or more certificates, then a PKCS#8, PKCS#1 or SEC1 private key.
    ///
    /// # Errors
    /// When the PEM has no certificate or no key.
    pub fn from_pem(cert_pem: &[u8], key_pem: &[u8]) -> io::Result<Self> {
        use rustls::pki_types::pem::PemObject;
        let chain = CertificateDer::pem_slice_iter(cert_pem).collect::<Result<Vec<_>, _>>().map_err(io::Error::other)?;
        if chain.is_empty() {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "no certificate in the PEM"));
        }
        let key = PrivateKeyDer::from_pem_slice(key_pem).map_err(io::Error::other)?;
        Ok(Self { chain, key })
    }

    /// A self-signed certificate for `names` (development and tests); returns the material and the certificate PEM clients trust.
    ///
    /// # Errors
    /// When key or certificate generation fails.
    #[cfg(feature = "selfsigned")]
    pub fn self_signed(names: &[&str]) -> io::Result<(Self, String)> {
        let names: Vec<String> = names.iter().map(|n| (*n).to_owned()).collect();
        let rcgen::CertifiedKey { cert, signing_key } = rcgen::generate_simple_self_signed(names).map_err(io::Error::other)?;
        let pem = cert.pem();
        let key = PrivateKeyDer::try_from(signing_key.serialize_der()).map_err(io::Error::other)?;
        Ok((Self { chain: vec![cert.der().clone()], key }, pem))
    }

    /// The rustls server config for `alpn`, built on the aws-lc-rs provider.
    pub(crate) fn server_config(&self, alpn: &[&[u8]], early_data: bool) -> io::Result<Arc<rustls::ServerConfig>> {
        let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
        let mut config = rustls::ServerConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .map_err(io::Error::other)?
            .with_no_client_auth()
            .with_single_cert(self.chain.clone(), self.key.clone_key())
            .map_err(io::Error::other)?;
        config.alpn_protocols = alpn.iter().map(|p| p.to_vec()).collect();
        if early_data {
            config.max_early_data_size = u32::MAX;
        }
        Ok(Arc::new(config))
    }
}
