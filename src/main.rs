mod anomaly;
mod config;
mod mac;
mod oui;
mod routeros;
mod scan;

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

    /// MAC (complète ou partielle, tout format), IP, nom d'hôte ou fabricant
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

    /// Ne jamais poser de question : échouer si des identifiants manquent
    #[arg(long, global = true, conflicts_with = "ask")]
    no_prompt: bool,
}

#[derive(Subcommand)]
enum Command {
    /// Crée ~/.config/macfind/config.toml avec un exemple
    Init,
    /// Ajoute un routeur : teste la connexion, puis l'écrit dans la config
    /// et enregistre le mot de passe dans le Trousseau
    AddRouter {
        /// Nom court du routeur (ex : principal)
        name: String,
        /// Adresse du routeur (ex : https://192.168.88.1)
        #[arg(long)]
        url: String,
        /// Compte RouterOS (lecture seule conseillé)
        #[arg(long)]
        user: String,
        /// Accepter un certificat auto-signé
        #[arg(long)]
        insecure: bool,
        /// Lire le mot de passe sur l'entrée standard au lieu de le demander
        #[arg(long)]
        password_stdin: bool,
    },
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
    /// Liste les anomalies : plusieurs baux actifs pour une même MAC, une IP
    /// utilisée par deux MAC, un équipement qui répond hors de ses baux
    Anomalies,
    /// Affiche les routeurs configurés
    Routers,
    /// Balaie le sous-réseau local du Mac : IP, MAC, fabricant et nom
    Scan {
        /// Sous-réseau à balayer : 192.168.1.  ou  192.168.1.0/24
        /// (par défaut, celui du Mac)
        target: Option<String>,
        /// Ne garder que les lignes correspondantes (MAC, IP, nom, fabricant)
        #[arg(short, long, value_name = "TEXTE")]
        filter: Option<String>,
    },
}

/// Une ligne de résultat, unifiée entre DHCP et ARP.
#[derive(Debug, serde::Serialize)]
struct Hit {
    router: String,
    source: &'static str,
    ip: String,
    mac: String,
    /// Fabricant déduit de l'OUI ; vide s'il est inconnu.
    vendor: String,
    host: String,
    detail: String,
    status: String,
    /// Anomalie relevée sur cette ligne ; vide s'il n'y en a pas.
    anomaly: String,
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
            vendor: oui::vendor(l.mac_address.as_deref().unwrap_or_default()).to_string(),
            host: l.host_name.clone().unwrap_or_default(),
            detail,
            status,
            anomaly: String::new(),
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
            vendor: oui::vendor(a.mac_address.as_deref().unwrap_or_default()).to_string(),
            host: String::new(),
            detail,
            status: status.to_string(),
            anomaly: String::new(),
        }
    }
}

/// Règle de correspondance : MAC partielle (tout format), IP exacte/préfixe,
/// nom d'hôte, commentaire ou fabricant.
fn matches(query: &str, ip: &str, mac_addr: &str, host: &str, comment: &str) -> bool {
    let q = query.trim();
    if mac::looks_like_mac(q) && mac::normalize(mac_addr).contains(&mac::normalize(q)) {
        return true;
    }
    if !ip.is_empty() && (ip == q || (q.ends_with('.') && ip.starts_with(q))) {
        return true;
    }
    let ql = q.to_lowercase();
    !ql.is_empty()
        && [host, comment, oui::vendor(mac_addr)]
            .iter()
            .any(|field| field.to_lowercase().contains(&ql))
}

fn find_router<'a>(cfg: &'a config::Config, name: &str) -> Result<&'a config::Router> {
    cfg.routers
        .iter()
        .find(|r| r.name == name)
        .ok_or_else(|| anyhow::anyhow!("Routeur « {name} » absent de la config"))
}

/// Ajoute un routeur. Rien n'est écrit tant que la connexion n'a pas réussi.
fn add_router(router: config::Router, password_stdin: bool) -> Result<()> {
    config::check_new_router(&config::existing_routers()?, &router)?;
    let password = if password_stdin {
        let mut line = String::new();
        std::io::stdin().read_line(&mut line)?;
        line.trim_end_matches(['\r', '\n']).to_string()
    } else {
        rpassword::prompt_password("Mot de passe : ")?
    };
    if password.is_empty() {
        bail!("Le mot de passe est vide");
    }
    let user = router.user.clone().unwrap_or_default();
    let client = RouterClient::new(router.clone(), user.clone(), password.clone())?;
    let leases = client.dhcp_leases().map_err(|e| {
        // Le message 401 habituel renvoie à des options sans objet ici.
        if format!("{e:#}").contains("(401)") {
            anyhow::anyhow!("Authentification refusée : vérifie le compte et le mot de passe")
        } else {
            e
        }
    })?;
    let path = config::append_router(&router)?;
    config::save(&router, &user, &password)?;
    println!(
        "✓ Routeur « {} » ajouté ({} baux lus) : {}",
        router.name,
        leases.len(),
        path.display()
    );
    Ok(())
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
            let creds = if cli.no_prompt {
                config::stored_credentials(&r).ok_or_else(|| {
                    anyhow::anyhow!(
                        "Identifiants manquants pour « {0} » : lance `macfind set-password {0}`",
                        r.name
                    )
                })?
            } else {
                config::credentials(&r, cli.ask)?
            };
            let prompted = creds.prompted;
            Ok((RouterClient::new(r, creds.user, creds.password)?, prompted))
        })
        .collect()
}

/// Ce qu'un routeur a renvoyé.
struct RouterData {
    router: String,
    leases: Vec<Lease>,
    arp: Vec<ArpEntry>,
}

/// Interroge chaque routeur en parallèle (un thread par routeur). Un routeur
/// en erreur est signalé puis ignoré. Renvoie aussi les clients dont les
/// identifiants saisis ont fonctionné, pour proposer de les enregistrer.
fn fetch(cli: &Cli, want_arp: bool) -> Result<(Vec<RouterData>, Vec<RouterClient>)> {
    let cfg = config::load()?;
    let clients = clients(cli, &cfg)?;

    let results: Vec<Result<(Vec<Lease>, Vec<ArpEntry>)>> = std::thread::scope(|s| {
        let handles: Vec<_> = clients
            .iter()
            .map(|(c, _)| {
                s.spawn(move || {
                    let leases = c.dhcp_leases()?;
                    let arp = if want_arp { c.arp()? } else { vec![] };
                    Ok((leases, arp))
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });

    let mut data = Vec::new();
    let mut to_offer = Vec::new();
    for (res, (client, prompted)) in results.into_iter().zip(clients) {
        match res {
            Ok((leases, arp)) => {
                data.push(RouterData {
                    router: client.name().to_string(),
                    leases,
                    arp,
                });
                if prompted {
                    to_offer.push(client);
                }
            }
            Err(e) => eprintln!("⚠️  {e:#}"),
        }
    }
    Ok((data, to_offer))
}

/// Propose d'enregistrer les identifiants saisis qui ont fonctionné.
fn offer_saves(clients: &[RouterClient]) {
    for c in clients {
        let creds = config::Credentials {
            user: c.user().to_string(),
            password: c.password().to_string(),
            prompted: true,
        };
        if let Err(e) = config::offer_to_save(c.router(), &creds) {
            eprintln!("⚠️  enregistrement impossible : {e:#}");
        }
    }
}

/// Lignes d'un routeur. `show_arp` ajoute les équipements connus de l'ARP
/// seul ; `only_anomalies` ne garde que les lignes signalées, y compris une
/// entrée ARP dont la MAC a déjà des baux (machine hors de ses baux).
fn hits_for(d: &RouterData, query: Option<&str>, show_arp: bool, only_anomalies: bool) -> Vec<Hit> {
    let report = anomaly::Report::new(&d.leases, &d.arp);
    let mut hits = Vec::new();
    let mut seen_macs = Vec::new();
    for l in &d.leases {
        let ip = l
            .active_address
            .as_deref()
            .or(l.address.as_deref())
            .unwrap_or("");
        let m = l.mac_address.as_deref().unwrap_or("");
        let host = l.host_name.as_deref().unwrap_or("");
        let comment = l.comment.as_deref().unwrap_or("");
        if !query.is_none_or(|q| matches(q, ip, m, host, comment)) {
            continue;
        }
        seen_macs.push(mac::normalize(m));
        let mut hit = Hit::from_lease(&d.router, l);
        hit.anomaly = report.for_lease(l);
        if !only_anomalies || !hit.anomaly.is_empty() {
            hits.push(hit);
        }
    }
    if !show_arp && !only_anomalies {
        return hits;
    }
    for a in &d.arp {
        let m = a.mac_address.as_deref().unwrap_or("");
        let ip = a.address.as_deref().unwrap_or("");
        let comment = a.comment.as_deref().unwrap_or("");
        let mut hit = Hit::from_arp(&d.router, a);
        hit.anomaly = report.for_arp(a);
        // L'ARP ne connaît pas les noms : reprendre celui d'un bail de la même MAC.
        if let Some(name) = d
            .leases
            .iter()
            .filter(|l| mac::normalize(l.mac_address.as_deref().unwrap_or("")) == mac::normalize(m))
            .find_map(|l| l.host_name.clone().filter(|h| !h.is_empty()))
        {
            hit.host = name;
        }
        let keep = if only_anomalies {
            !hit.anomaly.is_empty()
        } else {
            // L'ARP complète le DHCP (équipements en IP fixe), sans doublons.
            !seen_macs.contains(&mac::normalize(m))
                && query.is_none_or(|q| matches(q, ip, m, "", comment))
        };
        if keep {
            hits.push(hit);
        }
    }
    hits
}

fn collect(cli: &Cli, query: Option<&str>, only_anomalies: bool) -> Result<Vec<Hit>> {
    // L'ARP est lue même pour la simple liste des baux : elle sert à relever
    // les anomalies. `--no-arp` limite la détection aux baux.
    let (data, to_offer) = fetch(cli, !cli.no_arp)?;
    let show_arp = !cli.no_arp && query.is_some();
    let hits: Vec<Hit> = data
        .iter()
        .flat_map(|d| hits_for(d, query, show_arp, only_anomalies))
        .collect();
    // Affiche d'abord les résultats, puis propose l'enregistrement.
    let empty = if only_anomalies {
        "Aucune anomalie."
    } else {
        "Aucun résultat."
    };
    print(&hits, cli.json, empty)?;
    offer_saves(&to_offer);
    Ok(hits)
}

/// Bail du routeur correspondant à un équipement trouvé par le balayage :
/// de préférence celui qui porte la même IP, sinon le premier de cette MAC.
fn lease_for<'a>(data: &'a [RouterData], found: &scan::Found) -> Option<(&'a str, &'a Lease)> {
    let wanted = mac::normalize(&found.mac);
    let ip = found.ip.to_string();
    let mut same_mac = data
        .iter()
        .flat_map(|d| d.leases.iter().map(move |l| (d.router.as_str(), l)))
        .filter(|(_, l)| mac::normalize(l.mac_address.as_deref().unwrap_or("")) == wanted)
        .peekable();
    let first = same_mac.peek().copied();
    same_mac
        .find(|(_, l)| {
            l.active_address.as_deref() == Some(ip.as_str())
                || l.address.as_deref() == Some(ip.as_str())
        })
        .or(first)
}

/// Construit la ligne d'un équipement balayé. `router_known` indique que les
/// baux d'au moins un routeur ont pu être lus.
fn scan_hit(found: &scan::Found, lease: Option<(&str, &Lease)>, router_known: bool) -> Hit {
    let mut host = lease
        .and_then(|(_, l)| l.host_name.clone())
        .unwrap_or_default();
    if found.own && host.is_empty() {
        host = "(ce Mac)".to_string();
    }
    let mut detail = found.iface.clone();
    if let Some(c) = lease
        .and_then(|(_, l)| l.comment.as_ref())
        .filter(|c| !c.is_empty())
    {
        detail.push_str(&format!(" — {c}"));
    }
    let status = match lease {
        Some((_, l)) if l.dynamic.as_deref() == Some("true") => "bail dynamique",
        Some(_) => "bail statique",
        None if router_known => "sans bail",
        None => "",
    };
    Hit {
        router: lease.map(|(r, _)| r.to_string()).unwrap_or_default(),
        source: "SCAN",
        ip: found.ip.to_string(),
        mac: found.mac.clone(),
        vendor: oui::vendor(&found.mac).to_string(),
        host,
        detail,
        status: status.to_string(),
        anomaly: String::new(),
    }
}

fn scan_command(cli: &Cli, target: Option<&str>, filter: Option<&str>) -> Result<Vec<Hit>> {
    let (iface, hosts) = scan::plan(target, &scan::local_ifaces()?)?;
    eprintln!(
        "🔎 Balayage de {} adresse(s) sur {} (environ 2 s)…",
        hosts.len(),
        iface.name
    );
    let found = scan::run(&iface, &hosts)?;

    // Les noms d'hôte viennent des baux du routeur, s'il est configuré et joignable.
    let (data, to_offer) = if config::config_path().exists() {
        fetch(cli, false).unwrap_or_else(|e| {
            eprintln!("⚠️  noms d'hôte indisponibles : {e:#}");
            (vec![], vec![])
        })
    } else {
        (vec![], vec![])
    };

    let hits: Vec<Hit> = found
        .iter()
        .map(|f| scan_hit(f, lease_for(&data, f), !data.is_empty()))
        .filter(|h| filter.is_none_or(|q| matches(q, &h.ip, &h.mac, &h.host, &h.detail)))
        .collect();
    print(&hits, cli.json, "Aucun résultat.")?;
    offer_saves(&to_offer);
    Ok(hits)
}

/// Largeur maximale de la colonne « Fabricant » (le JSON garde le nom entier).
const VENDOR_WIDTH: usize = 24;

fn print(hits: &[Hit], json: bool, empty: &str) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(hits)?);
        return Ok(());
    }
    if hits.is_empty() {
        println!("{empty}");
        return Ok(());
    }
    // La colonne « Anomalie » n'apparaît que s'il y a quelque chose à signaler.
    let flagged = hits.iter().filter(|h| !h.anomaly.is_empty()).count();
    let mut header = vec![
        "Routeur",
        "Source",
        "IP",
        "MAC",
        "Fabricant",
        "Hôte",
        "Serveur / Interface",
        "Statut",
    ];
    if flagged > 0 {
        header.push("Anomalie");
    }
    let mut t = Table::new();
    t.load_preset(UTF8_FULL_CONDENSED).set_header(header);
    for h in hits {
        let vendor = oui::short(&h.vendor, VENDOR_WIDTH);
        let mut row = vec![
            h.router.as_str(),
            h.source,
            &h.ip,
            &h.mac,
            &vendor,
            &h.host,
            &h.detail,
            &h.status,
        ];
        if flagged > 0 {
            row.push(&h.anomaly);
        }
        t.add_row(row);
    }
    println!("{t}");
    if flagged > 0 && flagged < hits.len() {
        println!("{} résultat(s), dont {flagged} avec anomalie", hits.len());
    } else {
        println!("{} résultat(s)", hits.len());
    }
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
        Some(Command::AddRouter {
            name,
            url,
            user,
            insecure,
            password_stdin,
        }) => {
            let router = config::Router {
                name: name.trim().to_string(),
                url: url.trim().trim_end_matches('/').to_string(),
                user: Some(user.trim().to_string()),
                insecure: *insecure,
                timeout_secs: config::default_timeout(),
            };
            add_router(router, *password_stdin)?;
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
            collect(&cli, None, false)?;
        }
        Some(Command::Anomalies) => {
            collect(&cli, None, true)?;
        }
        Some(Command::Scan { target, filter }) => {
            let hits = scan_command(&cli, target.as_deref(), filter.as_deref())?;
            if hits.is_empty() {
                std::process::exit(1);
            }
        }
        None => match cli.query.as_deref() {
            Some(q) => {
                let hits = collect(&cli, Some(q), false)?;
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

    #[test]
    fn matches_vendor() {
        // 00:15:5D appartient à Microsoft (cartes virtuelles Hyper-V).
        let (ip, m) = ("192.168.1.50", "00:15:5D:00:00:01");
        assert!(matches("microsoft", ip, m, "", ""));
        assert!(matches("Micro", ip, m, "serveur", ""));
        assert!(!matches("apple", ip, m, "serveur", ""));
        // Les adresses aléatoires se retrouvent par leur libellé.
        assert!(matches("privée", ip, "DA:A1:19:00:00:01", "", ""));
    }
}
