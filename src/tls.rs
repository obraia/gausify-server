//! Self-signed certificate handling. On first run a cert covering `localhost`,
//! `127.0.0.1` and every detected LAN IP is generated and cached under
//! `<library>/.gausify/`; later runs reuse it. A LAN cert is inherently
//! untrusted — the browser must accept it once (see the server's README).

use std::error::Error;
use std::net::{IpAddr, Ipv4Addr};
use std::path::{Path, PathBuf};

use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair, SanType};

/// Where the cert/key live for a given library.
pub struct CertPaths {
    pub cert: PathBuf,
    pub key: PathBuf,
}

/// Ensure a usable cert/key pair exists for `ips`, generating one if needed.
pub fn ensure_cert(library: &Path, ips: &[Ipv4Addr]) -> Result<CertPaths, Box<dyn Error>> {
    let dir = library.join(".gausify");
    std::fs::create_dir_all(&dir)?;
    let cert = dir.join("cert.pem");
    let key = dir.join("key.pem");

    if cert.exists() && key.exists() {
        return Ok(CertPaths { cert, key });
    }

    let mut sans: Vec<SanType> = vec![
        SanType::DnsName("localhost".try_into()?),
        SanType::IpAddress(IpAddr::V4(Ipv4Addr::LOCALHOST)),
    ];
    for ip in ips {
        sans.push(SanType::IpAddress(IpAddr::V4(*ip)));
    }

    let mut params = CertificateParams::new(Vec::<String>::new())?;
    params.subject_alt_names = sans;
    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, "Gausify Local Server");
    params.distinguished_name = dn;

    let key_pair = KeyPair::generate()?;
    let certificate = params.self_signed(&key_pair)?;

    std::fs::write(&cert, certificate.pem())?;
    std::fs::write(&key, key_pair.serialize_pem())?;

    Ok(CertPaths { cert, key })
}
