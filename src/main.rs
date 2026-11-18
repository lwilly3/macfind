mod config;
mod mac;
mod routeros;

use anyhow::{bail, Result};
use clap::{Parser, Subcommand};
use comfy_table::{presets::UTF8_FULL_CONDENSED, Table};
use routeros::{ArpEntry, Lease, RouterClient};

#[derive(Parser)]
#[command(
    name = "macfind",
    version,
    about = "Retrouve l'IP d'un équipement via les baux DHCP et l'ARP de tes MikroTik"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// MAC (complète ou partielle, tout format), IP ou nom d'hôte
    query: Option<String>,

    /// Ne chercher que sur ce routeur
    #[arg(short, long, global = true)]
    router: Option<String>,

    /// Ignorer la table ARP (baux DHCP uniquement)
    #[arg(long, global = true)]
    no_arp: bool,

    /// Sortie JSON (pour scripts)
    #[arg(long, global = true)]
    json: bool,

    /// Demander les identifiants même s'ils sont enregistrés
    #[arg(long, global = true)]
    ask: bool,
}

#[derive(Subcommand)]
enum Command {
    /// Crée ~/.config/macfind/config.toml avec un exemple
    Init,
    /// Enregistre les identifiants d'un routeur dans le Trousseau macOS
    SetPassword {
        /// Nom du routeur tel que défini dans la config
        name: String,
    },
    /// Efface les identifiants enregistrés d'un routeur
    Forget {
        /// Nom du routeur tel que défini dans la config
        name: String,
    },
    /// Liste tous les baux DHCP
    Leases,
    /// Affiche les routeurs configurés
    Routers,
}

/// Une ligne de résultat, unifiée entre DHCP et ARP.
#[derive(Debug, serde::Serialize)]
struct Hit {
    router: String,
    source: &'static str,
    ip: String,
    mac: String,
    host: String,
    detail: String,
    status: String,
}

impl Hit {
    fn from_lease(router: &str, l: &Lease) -> Self {
        let ip = l
            .active_address
            .clone()
            .filter(|s| !s.is_empty())
            .or_else(|| l.address.clone())
            .unwrap_or_default();
        let kind = if l.dynamic.as_deref() == Some("true") {
            "dynamique"
        } else {
            "statique"
        };
        let mut detail = format!("{} ({kind})", l.server.clone().unwrap_or_default());
        if let Some(c) = l.comment.as_ref().filter(|c| !c.is_empty()) {
            detail.push_str(&format!(" — {c}"));
        }
        let mut status = l.status.clone().unwrap_or_default();
        if let Some(ls) = l.last_seen.as_ref().filter(|s| !s.is_empty()) {
            status.push_str(&format!(" (vu il y a {ls})"));
        }
        Self {
            router: router.to_string(),
            source: "DHCP",
            ip,
            mac: mac::pretty(l.mac_address.as_deref().unwrap_or_default()),
            host: l.host_name.clone().unwrap_or_default(),
            detail,
            status,
        }
    }

    fn from_arp(router: &str, a: &ArpEntry) -> Self {
        let mut detail = a.interface.clone().unwrap_or_default();
        if let Some(c) = a.comment.as_ref().filter(|c| !c.is_empty()) {
            detail.push_str(&format!(" — {c}"));
        }
        let status = match a.complete.as_deref() {
            Some("true") => "complete",
            Some("false") => "incomplete",
            _ => "",
        };
        Self {
            router: router.to_string(),
            source: "ARP",
            ip: a.address.clone().unwrap_or_default(),
            mac: mac::pretty(a.mac_address.as_deref().unwrap_or_default()),
            host: String::new(),
            detail,
            status: status.to_string(),
        }
    }
}

/// Règle de correspondance : MAC partielle (tout format), IP exacte/préfixe, ou nom d'hôte.
fn matches(query: &str, ip: &str, mac_addr: &str, host: &str, comment: &str) -> bool {
    let q = query.trim();
    if mac::looks_like_mac(q) && mac::normalize(mac_addr).contains(&mac::normalize(q)) {
        return true;
    }
    if !ip.is_empty() && (ip == q || (q.ends_with('.') && ip.starts_with(q))) {
        return true;
    }
    let ql = q.to_lowercase();
    (!host.is_empty() && host.to_lowercase().contains(&ql))
        || (!comment.is_empty() && comment.to_lowercase().contains(&ql))
}

fn find_router<'a>(cfg: &'a config::Config, name: &str) -> Result<&'a config::Router> {
    cfg.routers
        .iter()
        .find(|r| r.name == name)
        .ok_or_else(|| anyhow::anyhow!("Routeur « {name} » absent de la config"))
}

/// Prépare un client par routeur. Les identifiants manquants sont demandés
/// ici, un routeur après l'autre, avant le lancement des requêtes parallèles.
fn clients(cli: &Cli, cfg: &config::Config) -> Result<Vec<(RouterClient, bool)>> {
    let only = cli.router.as_deref();
    let routers: Vec<_> = cfg
        .routers
        .iter()
        .filter(|r| only.is_none_or(|n| r.name == n))
        .cloned()
        .collect();
    if routers.is_empty() {
        bail!(
            "Aucun routeur nommé « {} » dans la config",
            only.unwrap_or("")
        );
    }
    routers
        .into_iter()
        .map(|r| {
            let creds = config::credentials(&r, cli.ask)?;
            let prompted = creds.prompted;
            Ok((RouterClient::new(r, creds.user, creds.password)?, prompted))
        })
        .collect()
}

type RouterData = Result<(Vec<Lease>, Vec<ArpEntry>)>;

fn collect(cli: &Cli, query: Option<&str>) -> Result<Vec<Hit>> {
    let cfg = config::load()?;
    let clients = clients(cli, &cfg)?;

    // Interroge chaque routeur en parallèle (un thread par routeur).
    let results: Vec<(String, RouterData)> = std::thread::scope(|s| {
        let handles: Vec<_> = clients
            .iter()
            .map(|(c, _)| {
                let want_arp = !cli.no_arp && query.is_some();
                s.spawn(move || {
                    let r = (|| {
                        let leases = c.dhcp_leases()?;
                        let arp = if want_arp { c.arp()? } else { vec![] };
                        Ok((leases, arp))
                    })();
                    (c.name().to_string(), r)
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });

    let mut hits = Vec::new();
    let mut to_offer = Vec::new();
    for ((router, res), (client, prompted)) in results.into_iter().zip(&clients) {
        let (leases, arp) = match res {
            Ok(v) => v,
            Err(e) => {
                eprintln!("⚠️  {e:#}");
                continue;
            }
        };
        // Connexion réussie avec des identifiants saisis : on proposera de les garder.
        if *prompted {
            to_offer.push(client);
        }
        let mut seen_macs = Vec::new();
        for l in &leases {
            let ip = l
                .active_address
                .as_deref()
                .or(l.address.as_deref())
                .unwrap_or("");
            let m = l.mac_address.as_deref().unwrap_or("");
            let host = l.host_name.as_deref().unwrap_or("");
            let comment = l.comment.as_deref().unwrap_or("");
            if query.is_none_or(|q| matches(q, ip, m, host, comment)) {
                seen_macs.push(mac::normalize(m));
                hits.push(Hit::from_lease(&router, l));
            }
        }
        // L'ARP complète le DHCP (équipements en IP fixe), sans doublons.
        for a in &arp {
            let m = a.mac_address.as_deref().unwrap_or("");
            if seen_macs.contains(&mac::normalize(m)) {
                continue;
            }
            let ip = a.address.as_deref().unwrap_or("");
            let comment = a.comment.as_deref().unwrap_or("");
            if query.is_none_or(|q| matches(q, ip, m, "", comment)) {
                hits.push(Hit::from_arp(&router, a));
            }
        }
    }
    // Affiche d'abord les résultats, puis propose l'enregistrement.
    print(&hits, cli.json)?;
    for c in to_offer {
        let creds = config::Credentials {
            user: c.user().to_string(),
            password: c.password().to_string(),
            prompted: true,
        };
        if let Err(e) = config::offer_to_save(c.router(), &creds) {
            eprintln!("⚠️  enregistrement impossible : {e:#}");
        }
    }
    Ok(hits)
}

fn print(hits: &[Hit], json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(hits)?);
        return Ok(());
    }
    if hits.is_empty() {
        println!("Aucun résultat.");
        return Ok(());
    }
    let mut t = Table::new();
    t.load_preset(UTF8_FULL_CONDENSED).set_header(vec![
        "Routeur",
        "Source",
        "IP",
        "MAC",
        "Hôte",
        "Serveur / Interface",
        "Statut",
    ]);
    for h in hits {
        t.add_row(vec![
            &h.router, h.source, &h.ip, &h.mac, &h.host, &h.detail, &h.status,
        ]);
    }
    println!("{t}");
    println!("{} résultat(s)", hits.len());
    Ok(())
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Some(Command::Init) => {
            let p = config::init()?;
            println!("Config créée : {}", p.display());
            println!("Édite-la, puis : macfind set-password <nom-du-routeur>");
        }
        Some(Command::SetPassword { name }) => {
            let cfg = config::load()?;
            let router = find_router(&cfg, name)?;
            let creds = config::credentials(router, true)?;
            config::save(router, &creds.user, &creds.password)?;
            println!("✓ Identifiants enregistrés dans le Trousseau.");
        }
        Some(Command::Forget { name }) => {
            let cfg = config::load()?;
            let router = find_router(&cfg, name)?;
            if config::forget(router)? {
                println!("✓ Identifiants de « {name} » effacés du Trousseau.");
            } else {
                println!("Aucun identifiant enregistré pour « {name} ».");
            }
        }
        Some(Command::Routers) => {
            let cfg = config::load()?;
            for r in &cfg.routers {
                println!(
                    "{:<12} {:<28} user={}",
                    r.name,
                    r.url,
                    r.user.as_deref().unwrap_or("(demandé au lancement)")
                );
            }
        }
        Some(Command::Leases) => {
            collect(&cli, None)?;
        }
        None => match cli.query.as_deref() {
            Some(q) => {
                let hits = collect(&cli, Some(q))?;
                if hits.is_empty() {
                    std::process::exit(1);
                }
            }
            None => {
                eprintln!("Usage : macfind <mac|ip|nom>   (macfind --help pour plus)");
                std::process::exit(2);
            }
        },
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::matches;

    #[test]
    fn match_rules() {
        let (ip, m, host) = ("192.168.20.15", "AA:BB:CC:DD:EE:FF", "imprimante-hp");
        assert!(matches("dd-ee-ff", ip, m, host, ""));
        assert!(matches("aabb.ccdd.eeff", ip, m, host, ""));
        assert!(matches("192.168.20.15", ip, m, host, ""));
        assert!(matches("192.168.20.", ip, m, host, ""));
        assert!(!matches("192.168.20.1", ip, m, host, ""));
        assert!(matches("IMPRIMANTE", ip, m, host, ""));
        assert!(!matches("11:22:33", ip, m, host, ""));
    }
}
