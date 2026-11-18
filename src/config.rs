use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::path::PathBuf;

/// Service name used for the macOS Keychain entries.
pub const KEYRING_SERVICE: &str = "macfind";

#[derive(Debug, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub routers: Vec<Router>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Router {
    /// Nom court du routeur (sert aussi de clé dans le Trousseau)
    pub name: String,
    /// URL de base, ex : https://192.168.88.1
    pub url: String,
    /// Utilisateur RouterOS (lecture seule conseillé).
    /// Facultatif : s'il manque, il est demandé au lancement.
    #[serde(default)]
    pub user: Option<String>,
    /// Accepter un certificat auto-signé (cas fréquent sur MikroTik)
    #[serde(default)]
    pub insecure: bool,
    /// Délai max par requête, en secondes
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}

fn default_timeout() -> u64 {
    5
}

pub fn config_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".config")
        .join("macfind")
        .join("config.toml")
}

pub const EXAMPLE: &str = r#"# Configuration de macfind
# Le mot de passe n'est PAS stocké ici : utilise `macfind set-password <nom>`
# pour l'enregistrer dans le Trousseau macOS.

[[routers]]
name = "principal"
url = "https://192.168.88.1"
user = "macfind"     # facultatif : demandé au lancement s'il est absent
insecure = true      # certificat auto-signé du MikroTik
timeout_secs = 5

# [[routers]]
# name = "site2"
# url = "https://10.10.0.1"
# user = "macfind"
# insecure = true
"#;

pub fn load() -> Result<Config> {
    let path = config_path();
    if !path.exists() {
        bail!(
            "Fichier de configuration introuvable : {}\nLance `macfind init` pour en créer un.",
            path.display()
        );
    }
    let raw = std::fs::read_to_string(&path)
        .with_context(|| format!("Lecture impossible de {}", path.display()))?;
    let cfg: Config =
        toml::from_str(&raw).with_context(|| format!("Config invalide : {}", path.display()))?;
    if cfg.routers.is_empty() {
        bail!("Aucun routeur défini dans {}", path.display());
    }
    Ok(cfg)
}

pub fn init() -> Result<PathBuf> {
    let path = config_path();
    if path.exists() {
        bail!("Le fichier existe déjà : {}", path.display());
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(&path, EXAMPLE)?;
    Ok(path)
}

/// Identifiants résolus pour un routeur.
pub struct Credentials {
    pub user: String,
    pub password: String,
    /// Vrai si l'utilisateur a dû les saisir (on proposera de les enregistrer).
    pub prompted: bool,
}

fn user_entry(router: &str) -> Result<keyring::Entry> {
    Ok(keyring::Entry::new(
        KEYRING_SERVICE,
        &format!("{router}.user"),
    )?)
}

fn password_entry(router: &str) -> Result<keyring::Entry> {
    Ok(keyring::Entry::new(KEYRING_SERVICE, router)?)
}

fn stored_user(router: &Router) -> Option<String> {
    router
        .user
        .clone()
        .or_else(|| user_entry(&router.name).ok()?.get_password().ok())
}

fn stored_password(router: &Router) -> Option<String> {
    if let Ok(pw) = std::env::var("MACFIND_PASSWORD") {
        return Some(pw);
    }
    password_entry(&router.name).ok()?.get_password().ok()
}

fn prompt_line(label: &str, default: Option<&str>) -> Result<String> {
    use std::io::Write;
    match default {
        Some(d) => eprint!("{label} [{d}] : "),
        None => eprint!("{label} : "),
    }
    std::io::stderr().flush()?;
    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    let v = line.trim().to_string();
    if v.is_empty() {
        default
            .map(str::to_string)
            .ok_or_else(|| anyhow::anyhow!("valeur requise"))
    } else {
        Ok(v)
    }
}

/// Récupère les identifiants : config/Trousseau d'abord, sinon les demande.
/// `force_prompt` (option --ask) oblige à les saisir sans rien lire du Trousseau.
pub fn credentials(router: &Router, force_prompt: bool) -> Result<Credentials> {
    if !force_prompt {
        if let (Some(user), Some(password)) = (stored_user(router), stored_password(router)) {
            return Ok(Credentials {
                user,
                password,
                prompted: false,
            });
        }
    }
    eprintln!("🔐 Identifiants pour « {} » ({})", router.name, router.url);
    let default_user = stored_user(router);
    let user = prompt_line("   Utilisateur", default_user.as_deref().or(Some("admin")))?;
    let password = rpassword::prompt_password("   Mot de passe : ")?;
    Ok(Credentials {
        user,
        password,
        prompted: true,
    })
}

/// Demande (o/N) puis enregistre les identifiants dans le Trousseau.
pub fn offer_to_save(router: &Router, creds: &Credentials) -> Result<()> {
    let answer = prompt_line(
        &format!(
            "💾 Enregistrer ces identifiants pour « {} » dans le Trousseau ? (o/N)",
            router.name
        ),
        Some("n"),
    )?;
    if matches!(answer.to_lowercase().as_str(), "o" | "oui" | "y" | "yes") {
        save(router, &creds.user, &creds.password)?;
        eprintln!("✓ Enregistré.");
    }
    Ok(())
}

pub fn save(router: &Router, user: &str, password: &str) -> Result<()> {
    password_entry(&router.name)?.set_password(password)?;
    // L'utilisateur n'est rangé dans le Trousseau que s'il n'est pas déjà dans la config.
    if router.user.as_deref() != Some(user) {
        user_entry(&router.name)?.set_password(user)?;
    }
    Ok(())
}

/// Supprime les identifiants enregistrés pour ce routeur.
pub fn forget(router: &Router) -> Result<bool> {
    let mut removed = false;
    for entry in [password_entry(&router.name)?, user_entry(&router.name)?] {
        if entry.delete_credential().is_ok() {
            removed = true;
        }
    }
    Ok(removed)
}
