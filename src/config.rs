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

pub fn default_timeout() -> u64 {
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

/// Bloc `[[routers]]` à ajouter au fichier de configuration.
pub fn router_block(router: &Router) -> String {
    // `toml::Value` se charge des guillemets et de l'échappement.
    let quoted = |v: &str| toml::Value::String(v.to_string()).to_string();
    let mut block = format!(
        "[[routers]]\nname = {}\nurl = {}\n",
        quoted(&router.name),
        quoted(&router.url)
    );
    if let Some(user) = &router.user {
        block.push_str(&format!("user = {}\n", quoted(user)));
    }
    block.push_str(&format!(
        "insecure = {}\ntimeout_secs = {}\n",
        router.insecure, router.timeout_secs
    ));
    block
}

/// Vérifie qu'un routeur peut être ajouté à la config (nom libre, URL plausible).
pub fn check_new_router(existing: &[Router], router: &Router) -> Result<()> {
    if router.name.trim().is_empty() {
        bail!("Le nom du routeur est vide");
    }
    if !router.url.starts_with("https://") && !router.url.starts_with("http://") {
        bail!(
            "L'adresse doit commencer par https:// ou http:// : « {} »",
            router.url
        );
    }
    if existing.iter().any(|r| r.name == router.name) {
        bail!("Un routeur nommé « {} » existe déjà", router.name);
    }
    Ok(())
}

/// Routeurs déjà configurés ; aucun si le fichier n'existe pas encore.
pub fn existing_routers() -> Result<Vec<Router>> {
    let path = config_path();
    if !path.exists() {
        return Ok(vec![]);
    }
    let raw = std::fs::read_to_string(&path)
        .with_context(|| format!("Lecture impossible de {}", path.display()))?;
    let cfg: Config =
        toml::from_str(&raw).with_context(|| format!("Config invalide : {}", path.display()))?;
    Ok(cfg.routers)
}

/// Ajoute un routeur à la fin du fichier de configuration, créé au besoin.
/// Le reste du fichier (commentaires compris) n'est pas touché.
pub fn append_router(router: &Router) -> Result<PathBuf> {
    let path = config_path();
    let mut raw = if path.exists() {
        std::fs::read_to_string(&path)
            .with_context(|| format!("Lecture impossible de {}", path.display()))?
    } else {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        "# Configuration de macfind\n# Le mot de passe n'est PAS stocké ici : il est dans le Trousseau macOS.\n"
            .to_string()
    };
    if !raw.ends_with('\n') {
        raw.push('\n');
    }
    raw.push('\n');
    raw.push_str(&router_block(router));
    std::fs::write(&path, raw)
        .with_context(|| format!("Écriture impossible de {}", path.display()))?;
    Ok(path)
}

/// Retire le bloc `[[routers]]` de ce nom du contenu de la config. Les autres
/// routeurs et leurs commentaires sont conservés ; les commentaires placés
/// juste au-dessus du bloc retiré partent avec lui. `None` si aucun routeur
/// ne porte ce nom.
pub fn without_router(raw: &str, name: &str) -> Result<Option<String>> {
    let mut doc: toml_edit::DocumentMut = raw.parse().context("Config invalide")?;
    let Some(routers) = doc
        .get_mut("routers")
        .and_then(|item| item.as_array_of_tables_mut())
    else {
        return Ok(None);
    };
    let Some(index) = routers
        .iter()
        .position(|t| t.get("name").and_then(|v| v.as_str()) == Some(name))
    else {
        return Ok(None);
    };
    routers.remove(index);
    if routers.is_empty() {
        doc.remove("routers");
    }
    Ok(Some(doc.to_string()))
}

/// Retire un routeur du fichier de configuration.
pub fn remove_router(name: &str) -> Result<()> {
    let path = config_path();
    let raw = std::fs::read_to_string(&path)
        .with_context(|| format!("Lecture impossible de {}", path.display()))?;
    match without_router(&raw, name)? {
        Some(updated) => std::fs::write(&path, updated)
            .with_context(|| format!("Écriture impossible de {}", path.display())),
        None => bail!("Routeur « {name} » absent de la config"),
    }
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

/// Identifiants déjà connus (config, Trousseau ou `MACFIND_PASSWORD`), sans
/// rien demander. `None` s'il en manque un.
pub fn stored_credentials(router: &Router) -> Option<Credentials> {
    Some(Credentials {
        user: stored_user(router)?,
        password: stored_password(router)?,
        prompted: false,
    })
}

/// Récupère les identifiants : config/Trousseau d'abord, sinon les demande.
/// `force_prompt` (option --ask) oblige à les saisir sans rien lire du Trousseau.
pub fn credentials(router: &Router, force_prompt: bool) -> Result<Credentials> {
    if !force_prompt {
        if let Some(creds) = stored_credentials(router) {
            return Ok(creds);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn router(name: &str, url: &str) -> Router {
        Router {
            name: name.to_string(),
            url: url.to_string(),
            user: Some("lecture".to_string()),
            insecure: true,
            timeout_secs: default_timeout(),
        }
    }

    #[test]
    fn router_block_is_read_back() {
        // Un nom avec guillemet vérifie l'échappement.
        let r = router("site \"nord\"", "https://192.0.2.1");
        let cfg: Config = toml::from_str(&router_block(&r)).unwrap();
        assert_eq!(cfg.routers.len(), 1);
        assert_eq!(cfg.routers[0].name, r.name);
        assert_eq!(cfg.routers[0].url, r.url);
        assert_eq!(cfg.routers[0].user, r.user);
        assert!(cfg.routers[0].insecure);
    }

    #[test]
    fn block_appends_to_existing_config() {
        let raw = format!(
            "{EXAMPLE}\n{}",
            router_block(&router("site2", "https://192.0.2.2"))
        );
        let cfg: Config = toml::from_str(&raw).unwrap();
        let names: Vec<_> = cfg.routers.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["principal", "site2"]);
    }

    #[test]
    fn removes_one_router_and_keeps_the_rest() {
        let raw = format!(
            "# en-tête\n\n{}\n# second site\n{}",
            router_block(&router("principal", "https://192.0.2.1")),
            router_block(&router("site2", "https://192.0.2.2"))
        );
        let updated = without_router(&raw, "principal").unwrap().unwrap();
        let cfg: Config = toml::from_str(&updated).unwrap();
        let names: Vec<_> = cfg.routers.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["site2"]);
        // Le commentaire d'un autre bloc reste ; celui placé juste au-dessus
        // du bloc retiré part avec lui.
        assert!(updated.contains("# second site"));
        // Nom inconnu : rien à faire.
        assert!(without_router(&raw, "inconnu").unwrap().is_none());
        // Le dernier routeur retiré laisse une config sans routeur, mais valide.
        let empty = without_router(&updated, "site2").unwrap().unwrap();
        let cfg: Config = toml::from_str(&empty).unwrap();
        assert!(cfg.routers.is_empty());
    }

    #[test]
    fn new_router_rules() {
        let existing = [router("principal", "https://192.0.2.1")];
        assert!(check_new_router(&existing, &router("site2", "https://192.0.2.2")).is_ok());
        assert!(check_new_router(&existing, &router("principal", "https://192.0.2.2")).is_err());
        assert!(check_new_router(&existing, &router("site2", "192.0.2.2")).is_err());
        assert!(check_new_router(&existing, &router(" ", "https://192.0.2.2")).is_err());
    }
}
