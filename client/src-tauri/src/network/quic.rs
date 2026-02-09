use std::net::SocketAddr;
use std::sync::Arc;

use quinn::{Connection, Endpoint, ServerConfig};
use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer};
use sha2::{Digest, Sha256};
use tracing::info;

use crate::error::{AppError, AppResult};

/// A QUIC endpoint that can both listen (accept) and connect.
/// Uses a self-signed certificate; authentication is via SPAKE2-derived key,
/// not the TLS certificate chain.
pub struct QuicEndpoint {
    endpoint: Endpoint,
    cert_fingerprint: [u8; 32],
}

impl QuicEndpoint {
    /// Create a new QUIC endpoint bound to `0.0.0.0:{port}`.
    /// Use port 0 for OS-assigned.
    pub async fn new(port: u16) -> AppResult<Self> {
        // Generate self-signed cert
        let subject_alt_names = vec!["relay.local".to_string()];
        let cert_params = rcgen::CertificateParams::new(subject_alt_names)
            .map_err(|e| AppError::Crypto(format!("cert params: {e}")))?;
        let key_pair = rcgen::KeyPair::generate()
            .map_err(|e| AppError::Crypto(format!("keygen: {e}")))?;
        let cert = cert_params
            .self_signed(&key_pair)
            .map_err(|e| AppError::Crypto(format!("self-sign: {e}")))?;

        let cert_der = CertificateDer::from(cert.der().to_vec());
        let key_der = PrivatePkcs8KeyDer::from(key_pair.serialize_der());

        // Compute fingerprint (SHA-256 of DER cert)
        let mut hasher = Sha256::new();
        hasher.update(cert_der.as_ref());
        let fingerprint: [u8; 32] = hasher.finalize().into();

        // Build server config (for accepting connections)
        let server_crypto = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![cert_der.clone()], key_der.into())
            .map_err(|e| AppError::Crypto(format!("server TLS config: {e}")))?;

        let server_config = ServerConfig::with_crypto(Arc::new(
            quinn::crypto::rustls::QuicServerConfig::try_from(server_crypto)
                .map_err(|e| AppError::Crypto(format!("QUIC server config: {e}")))?,
        ));

        let addr: SocketAddr = format!("0.0.0.0:{port}").parse().unwrap();
        let endpoint = Endpoint::server(server_config, addr)
            .map_err(|e| AppError::Network(format!("failed to bind QUIC endpoint: {e}")))?;

        info!(
            "QUIC endpoint listening on {}",
            endpoint.local_addr().map_err(|e| AppError::Network(e.to_string()))?
        );

        Ok(Self {
            endpoint,
            cert_fingerprint: fingerprint,
        })
    }

    /// Accept one incoming connection. Accepts any peer (Phase 1: LAN, no fingerprint check).
    pub async fn accept_any(&self) -> AppResult<Connection> {
        let incoming = self
            .endpoint
            .accept()
            .await
            .ok_or_else(|| AppError::Network("endpoint closed".into()))?;

        let conn = incoming
            .await
            .map_err(|e| AppError::Network(format!("failed to accept connection: {e}")))?;

        info!("accepted QUIC connection from {}", conn.remote_address());
        Ok(conn)
    }

    /// Connect to a peer at the given address.
    /// Uses the existing endpoint with a client config so the connection
    /// lifetime is tied to the endpoint (not dropped prematurely).
    pub async fn connect(&self, addr: SocketAddr) -> AppResult<Connection> {
        // Client config that accepts any cert (we rely on SPAKE2 for auth)
        let client_crypto = rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(SkipServerVerification))
            .with_no_client_auth();

        let client_config = quinn::ClientConfig::new(Arc::new(
            quinn::crypto::rustls::QuicClientConfig::try_from(client_crypto)
                .map_err(|e| AppError::Crypto(format!("QUIC client config: {e}")))?,
        ));

        let conn = self
            .endpoint
            .connect_with(client_config, addr, "relay.local")
            .map_err(|e| AppError::Network(format!("connect: {e}")))?
            .await
            .map_err(|e| AppError::Network(format!("connection failed: {e}")))?;

        info!("connected to peer at {addr}");
        Ok(conn)
    }

    /// SHA-256 fingerprint of our certificate.
    pub fn cert_fingerprint(&self) -> [u8; 32] {
        self.cert_fingerprint
    }

    /// Local address the endpoint is bound to.
    pub fn local_addr(&self) -> AppResult<SocketAddr> {
        self.endpoint
            .local_addr()
            .map_err(|e| AppError::Network(e.to_string()))
    }
}

impl Drop for QuicEndpoint {
    fn drop(&mut self) {
        // Use wait_idle=false to avoid blocking in drop.
        // The connection should drain naturally via QUIC's protocol.
        self.endpoint.close(0u32.into(), b"done");
    }
}

/// Accepts any server certificate.
/// Real authentication comes from SPAKE2 key agreement — if the peer
/// can decrypt our file chunks, they know the transfer code.
#[derive(Debug)]
struct SkipServerVerification;

impl rustls::client::danger::ServerCertVerifier for SkipServerVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        rustls::crypto::ring::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}
