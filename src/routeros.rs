use crate::config::Router;
use anyhow::{bail, Context, Result};
use reqwest::blocking::Client;
use serde::Deserialize;
use std::time::Duration;

/// Bail DHCP tel que renvoyé par GET /rest/ip/dhcp-server/lease
/// (RouterOS renvoie toutes les valeurs sous forme de chaînes).
#[derive(Debug, Deserialize, Clone)]
pub struct Lease {
    pub address: Option<String>,
    #[serde(rename = "mac-address")]
    pub mac_address: Option<String>,
    #[serde(rename = "active-address")]
    pub active_address: Option<String>,
    #[serde(rename = "host-name")]
    pub host_name: Option<String>,
    pub server: Option<String>,
    pub status: Option<String>,
    #[serde(rename = "last-seen")]
    pub last_seen: Option<String>,
    pub dynamic: Option<String>,
    pub comment: Option<String>,
}

/// Entrée ARP telle que renvoyée par GET /rest/ip/arp
#[derive(Debug, Deserialize, Clone)]
pub struct ArpEntry {
    pub address: Option<String>,
    #[serde(rename = "mac-address")]
    pub mac_address: Option<String>,
    pub interface: Option<String>,
    pub complete: Option<String>,
    pub comment: Option<String>,
}

pub struct RouterClient {
    router: Router,
    user: String,
    password: String,
    http: Client,
}

impl RouterClient {
    pub fn new(router: Router, user: String, password: String) -> Result<Self> {
        let http = Client::builder()
            .timeout(Duration::from_secs(router.timeout_secs))
            .danger_accept_invalid_certs(router.insecure)
            .build()?;
        Ok(Self {
            router,
            user,
            password,
            http,
        })
    }

    pub fn name(&self) -> &str {
        &self.router.name
    }

    pub fn user(&self) -> &str {
        &self.user
    }

    pub fn password(&self) -> &str {
        &self.password
    }

    pub fn router(&self) -> &Router {
        &self.router
    }

    fn get<T: for<'de> Deserialize<'de>>(&self, path: &str) -> Result<T> {
        let url = format!("{}/rest{}", self.router.url.trim_end_matches('/'), path);
        let resp = self
            .http
            .get(&url)
            .basic_auth(&self.user, Some(&self.password))
            .send()
            .with_context(|| format!("[{}] connexion impossible à {}", self.router.name, url))?;

        let status = resp.status();
        if status.as_u16() == 401 {
            bail!(
                "[{}] authentification refusée (401) — réessaie avec `--ask`, ou `macfind forget {}` pour effacer les identifiants enregistrés",
                self.router.name,
                self.router.name
            );
        }
        if !status.is_success() {
            let body = resp.text().unwrap_or_default();
            bail!(
                "[{}] HTTP {} sur {} : {}",
                self.router.name,
                status,
                path,
                body
            );
        }
        resp.json::<T>().with_context(|| {
            format!(
                "[{}] réponse JSON inattendue sur {}",
                self.router.name, path
            )
        })
    }

    pub fn dhcp_leases(&self) -> Result<Vec<Lease>> {
        self.get("/ip/dhcp-server/lease")
    }

    pub fn arp(&self) -> Result<Vec<ArpEntry>> {
        self.get("/ip/arp")
    }
}
