use std::path::Path;
use std::sync::Arc;

use ureq::tls::{Certificate, PemItem, RootCerts, TlsConfig, parse_pem};
use ureq::{Agent, Proxy};

pub struct AgentOptions {
    pub proxy_from_env: bool,
    pub proxy_url: Option<String>,
    pub ssl_cert_file: Option<String>,
}

impl AgentOptions {
    pub fn from_env(read_var: &dyn Fn(&str) -> Option<String>) -> Self {
        Self {
            proxy_from_env: true,
            proxy_url: None,
            ssl_cert_file: read_var("SSL_CERT_FILE").filter(|value| !value.trim().is_empty()),
        }
    }

    pub fn direct() -> Self {
        Self {
            proxy_from_env: false,
            proxy_url: None,
            ssl_cert_file: None,
        }
    }
}

pub fn extra_root_certs_from_pem(pem: &[u8]) -> Vec<Certificate<'static>> {
    parse_pem(pem)
        .filter_map(Result::ok)
        .filter_map(|item| match item {
            PemItem::Certificate(cert) => Some(cert.to_owned()),
            _ => None,
        })
        .collect()
}

pub fn extra_root_certs_from_env(
    read_var: &dyn Fn(&str) -> Option<String>,
) -> Vec<Certificate<'static>> {
    read_var("SSL_CERT_FILE")
        .and_then(|path| std::fs::read(Path::new(path.trim())).ok())
        .map(|pem| extra_root_certs_from_pem(&pem))
        .unwrap_or_default()
}

fn root_certs(extra: Vec<Certificate<'static>>) -> RootCerts {
    let mut all: Vec<Certificate<'static>> = webpki_root_certs::TLS_SERVER_ROOT_CERTS
        .iter()
        .map(|cert| Certificate::from_der(cert.as_ref()).to_owned())
        .collect();
    all.extend(extra);
    RootCerts::Specific(Arc::new(all))
}

pub fn build_agent(options: &AgentOptions) -> Result<Agent, ureq::Error> {
    let extra = options
        .ssl_cert_file
        .as_ref()
        .and_then(|path| std::fs::read(Path::new(path.trim())).ok())
        .map(|pem| extra_root_certs_from_pem(&pem))
        .unwrap_or_default();
    let tls = TlsConfig::builder().root_certs(root_certs(extra)).build();
    let builder = Agent::config_builder()
        .tls_config(tls)
        .http_status_as_error(false);
    let config = if let Some(url) = &options.proxy_url {
        builder.proxy(Some(Proxy::new(url)?)).build()
    } else if options.proxy_from_env {
        builder.build()
    } else {
        builder.proxy(None).build()
    };
    Ok(Agent::new_with_config(config))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PEM: &str = "-----BEGIN CERTIFICATE-----\nMIIBkjCCATegAwIBAgIUBqwZZd9AvV23qX3D4QIgOtZcOa4wCgYIKoZIzj0EAwIw\nHTEbMBkGA1UEAwwSZGVzYWZpbmFkby10ZXN0LWNhMCAXDTI2MTExNDE2MjcyM1oY\nDzIxMjYxMDIxMTYyNzIzWjAdMRswGQYDVQQDDBJkZXNhZmluYWRvLXRlc3QtY2Ew\nWTATBgcqhkjOPQIBBggqhkjOPQMBBwNCAASn/nXPRaSEwwGVLPwNLn22pEweKQCa\n5dEMPqdqYauqQRo+lVEF/2UBgirFsAzyGwjDCx+Pz+Kl1hWJ5VXU4vkio1MwUTAd\nBgNVHQ4EFgQURLQgm9/dbDIUUQ/cQn+HbamdO80wHwYDVR0jBBgwFoAURLQgm9/d\nbDIUUQ/cQn+HbamdO80wDwYDVR0TAQH/BAUwAwEB/zAKBggqhkjOPQQDAgNJADBG\nAiEAmHSZIQihMc76u5p09ZncppZqe8BO+5NqnDEyqEMi0ycCIQDCNSajJhpKx8UM\nxRzLCwVLiHDesdwPVNFoJghCvgNMGw==\n-----END CERTIFICATE-----\n";

    #[test]
    fn reads_every_certificate_of_a_pem_bundle_and_skips_other_items() {
        let bundle =
            format!("{PEM}\n-----BEGIN PRIVATE KEY-----\nAAAA\n-----END PRIVATE KEY-----\n{PEM}");
        assert_eq!(extra_root_certs_from_pem(bundle.as_bytes()).len(), 2);
        assert!(extra_root_certs_from_pem(b"no certs here").is_empty());
    }

    #[test]
    fn loads_the_bundle_named_by_ssl_cert_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("ca.pem");
        std::fs::write(&file, PEM).unwrap();
        let path = file.to_string_lossy().into_owned();
        assert_eq!(extra_root_certs_from_env(&|_| Some(path.clone())).len(), 1);
        assert!(extra_root_certs_from_env(&|_| None).is_empty());
        assert!(extra_root_certs_from_env(&|_| Some("/missing.pem".into())).is_empty());
    }

    #[test]
    fn builds_an_agent_with_the_webpki_roots_plus_the_extra_certificates() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("ca.pem");
        std::fs::write(&file, PEM).unwrap();
        let options = AgentOptions {
            proxy_from_env: false,
            proxy_url: None,
            ssl_cert_file: Some(file.to_string_lossy().into_owned()),
        };
        let agent = build_agent(&options).unwrap();
        let RootCerts::Specific(roots) = agent.config().tls_config().root_certs().clone() else {
            panic!("expected specific roots")
        };
        assert_eq!(
            roots.len(),
            webpki_root_certs::TLS_SERVER_ROOT_CERTS.len() + 1
        );
        assert!(agent.config().proxy().is_none());
    }

    #[test]
    fn reads_the_proxy_from_the_environment_unless_told_not_to() {
        let agent = build_agent(&AgentOptions {
            proxy_from_env: false,
            proxy_url: Some("http://127.0.0.1:1".into()),
            ssl_cert_file: None,
        })
        .unwrap();
        assert!(agent.config().proxy().is_some());
    }
}
