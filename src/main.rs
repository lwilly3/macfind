mod anomaly;
mod config;
mod history;
mod mac;
mod names;
mod oui;
mod routeros;
mod scan;

use anyhow::{bail, Result};
use clap::{Parser, Subcommand};
use comfy_table::{presets::UTF8_FULL_CONDENSED, Table};
use routeros::{ArpEntry, BridgeHost, Lease, RouterClient};

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

    /// Sortie CSV pour Excel (séparateur « ; », UTF-8)
    #[arg(long, global = true, conflicts_with = "json")]
    csv: bool,

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
    /// Retire un routeur de la config et efface ses identifiants du Trousseau
    RemoveRouter {
        /// Nom du routeur tel que défini dans la config
        name: String,
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
    Leases {
        /// Regrouper par sous-réseau (subnet) ou par serveur DHCP (server),
        /// trié par adresse dans chaque groupe
        #[arg(long, value_enum, value_name = "CRITÈRE")]
        group: Option<Group>,
    },
    /// Liste les anomalies : plusieurs baux actifs pour une même MAC, une IP
    /// utilisée par deux MAC, un équipement qui répond hors de ses baux
    Anomalies,
    /// Affiche les routeurs configurés
    Routers,
    /// Donne un nom à un équipement, d'après son adresse MAC (le routeur
    /// n'est pas modifié)
    Name {
        /// Adresse MAC complète, dans n'importe quel format
        mac: String,
        /// Nom à donner
        #[arg(required_unless_present = "clear")]
        name: Option<String>,
        /// Effacer le nom donné à cette MAC
        #[arg(long, conflicts_with = "name")]
        clear: bool,
    },
    /// Liste les noms donnés aux équipements
    Names,
    /// Liste les équipements apparus récemment (d'après l'historique local)
    New {
        /// Ancienneté maximale, en jours
        #[arg(long, default_value_t = NEW_DAYS, value_name = "JOURS")]
        days: u64,
    },
    /// Affiche l'historique d'un équipement : adresses IP successives,
    /// première et dernière vue
    History {
        /// MAC (complète ou partielle), IP, nom donné ou nom d'hôte
        query: String,
    },
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

/// Un équipement reste « nouveau » pendant ce nombre de jours.
const NEW_DAYS: u64 = 7;

/// Forme de la sortie.
#[derive(Clone, Copy, PartialEq)]
enum Format {
    Table,
    Json,
    Csv,
}

impl Cli {
    fn format(&self) -> Format {
        if self.json {
            Format::Json
        } else if self.csv {
            Format::Csv
        } else {
            Format::Table
        }
    }
}

/// Un champ CSV. Les noms d'hôte viennent des équipements du réseau : un
/// champ qui commence par `=`, `+`, `-` ou `@` serait exécuté comme formule
/// par Excel, il est donc neutralisé par une apostrophe.
fn csv_field(value: &str) -> String {
    let mut v = value.to_string();
    if v.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        v.insert(0, '\'');
    }
    if v.contains([';', '"', '\n', '\r']) {
        v = format!("\"{}\"", v.replace('"', "\"\""));
    }
    v
}

/// Les lignes au format CSV lu par Excel : séparateur « ; », fins de ligne
/// CRLF, et marque d'ordre d'octets pour que les accents soient reconnus.
fn to_csv(hits: &[Hit]) -> String {
    let mut out = String::from(
        "\u{feff}Routeur;Source;IP;MAC;Fabricant;Nom;Hôte;Port;Serveur / Interface;Statut;Anomalie\r\n",
    );
    for h in hits {
        let fields = [
            h.router.as_str(),
            h.source,
            &h.ip,
            &h.mac,
            &h.vendor,
            &h.name,
            &h.host,
            &h.port,
            &h.detail,
            &h.status,
            &h.anomaly,
        ];
        let line: Vec<String> = fields.iter().map(|f| csv_field(f)).collect();
        out.push_str(&line.join(";"));
        out.push_str("\r\n");
    }
    out
}

/// Critère de regroupement de `leases --group`.
#[derive(Clone, Copy, PartialEq, Debug, clap::ValueEnum)]
enum Group {
    /// Par sous-réseau /24
    Subnet,
    /// Par serveur DHCP
    Server,
}

/// Sous-réseau /24 d'une adresse (`192.168.1.0/24`) ; vide si elle est
/// illisible. Le masque réel n'est pas connu : /24 couvre le cas courant.
fn subnet_of(ip: &str) -> String {
    match ip.parse::<std::net::Ipv4Addr>() {
        Ok(a) => {
            let [x, y, z, _] = a.octets();
            format!("{x}.{y}.{z}.0/24")
        }
        Err(_) => String::new(),
    }
}

/// Clé de tri numérique d'une adresse ; les adresses illisibles vont à la fin.
fn ip_key(ip: &str) -> u32 {
    ip.parse::<std::net::Ipv4Addr>()
        .map(u32::from)
        .unwrap_or(u32::MAX)
}

/// Répartit les lignes par groupe. Les groupes sont triés (sous-réseaux dans
/// l'ordre numérique, serveurs par nom), les lignes par adresse.
fn group_hits(mut hits: Vec<Hit>, group: Group) -> Vec<(String, Vec<Hit>)> {
    let key = |h: &Hit| match group {
        Group::Subnet => h.subnet.clone(),
        Group::Server => h.server.clone(),
    };
    match group {
        // Trier par adresse range du même coup les sous-réseaux dans l'ordre.
        Group::Subnet => hits.sort_by_key(|h| ip_key(&h.ip)),
        Group::Server => hits.sort_by(|a, b| {
            (a.server.as_str(), ip_key(&a.ip)).cmp(&(b.server.as_str(), ip_key(&b.ip)))
        }),
    }
    let mut groups: Vec<(String, Vec<Hit>)> = Vec::new();
    for h in hits {
        let k = key(&h);
        match groups.last_mut() {
            Some((last, members)) if *last == k => members.push(h),
            _ => groups.push((k, vec![h])),
        }
    }
    groups
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
    /// Serveur DHCP du bail ; vide hors DHCP.
    server: String,
    /// Sous-réseau /24 de l'adresse ; vide si elle est inconnue.
    subnet: String,
    /// Port du routeur où l'équipement est branché ; vide s'il est inconnu.
    port: String,
    /// Nom donné par l'utilisateur (`macfind name`) ; vide s'il n'y en a pas.
    name: String,
    /// Première et dernière vue d'après l'historique local, en secondes
    /// Unix ; 0 si l'équipement n'y figure pas.
    first_seen: u64,
    last_seen: u64,
    /// Apparu après le premier relevé, depuis moins de sept jours.
    new: bool,
}

impl Hit {
    /// Texte de la colonne « Hôte » : le nom donné, suivi du nom d'hôte.
    fn label(&self) -> String {
        match (self.name.is_empty(), self.host.is_empty()) {
            (true, _) => self.host.clone(),
            (false, true) => self.name.clone(),
            (false, false) => format!("{} ({})", self.name, self.host),
        }
    }
}

/// Noms personnalisés ; un fichier illisible est signalé puis ignoré.
fn load_names() -> names::Names {
    names::load().unwrap_or_else(|e| {
        eprintln!("⚠️  {e:#}");
        names::Names::default()
    })
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
            server: l.server.clone().unwrap_or_default(),
            subnet: subnet_of(&ip),
            port: String::new(),
            name: String::new(),
            first_seen: 0,
            last_seen: 0,
            new: false,
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
            server: String::new(),
            subnet: subnet_of(a.address.as_deref().unwrap_or_default()),
            port: String::new(),
            name: String::new(),
            first_seen: 0,
            last_seen: 0,
            new: false,
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
    /// Hôtes des bridges ; vide si le routeur n'en a pas ou refuse la lecture.
    hosts: Vec<BridgeHost>,
}

/// Port du routeur où une MAC est branchée ; vide s'il est inconnu. Derrière
/// un switch ou une borne, c'est le port qui y mène.
fn port_of(hosts: &[BridgeHost], mac_addr: &str) -> String {
    let wanted = mac::normalize(mac_addr);
    if wanted.is_empty() {
        return String::new();
    }
    hosts
        .iter()
        .filter(|h| h.local.as_deref() != Some("true"))
        .filter(|h| mac::normalize(h.mac_address.as_deref().unwrap_or("")) == wanted)
        .find_map(|h| {
            h.on_interface
                .clone()
                .or_else(|| h.interface.clone())
                .filter(|p| !p.is_empty())
        })
        .unwrap_or_default()
}

/// Interroge chaque routeur en parallèle (un thread par routeur). Un routeur
/// en erreur est signalé puis ignoré. Renvoie aussi les clients dont les
/// identifiants saisis ont fonctionné, pour proposer de les enregistrer.
fn fetch(cli: &Cli, want_arp: bool) -> Result<(Vec<RouterData>, Vec<RouterClient>)> {
    let cfg = config::load()?;
    let clients = clients(cli, &cfg)?;

    type Fetched = (Vec<Lease>, Vec<ArpEntry>, Vec<BridgeHost>);
    let results: Vec<Result<Fetched>> = std::thread::scope(|s| {
        let handles: Vec<_> = clients
            .iter()
            .map(|(c, _)| {
                s.spawn(move || {
                    let leases = c.dhcp_leases()?;
                    let arp = if want_arp { c.arp()? } else { vec![] };
                    // Le port est un complément : son absence n'est pas une erreur.
                    let hosts = c.bridge_hosts().unwrap_or_default();
                    Ok((leases, arp, hosts))
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });

    let mut data = Vec::new();
    let mut to_offer = Vec::new();
    for (res, (client, prompted)) in results.into_iter().zip(clients) {
        match res {
            Ok((leases, arp, hosts)) => {
                data.push(RouterData {
                    router: client.name().to_string(),
                    leases,
                    arp,
                    hosts,
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
fn hits_for(
    d: &RouterData,
    names: &names::Names,
    query: Option<&str>,
    show_arp: bool,
    only_anomalies: bool,
) -> Vec<Hit> {
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
        // Le nom donné se cherche comme le commentaire.
        let name = names.get(m);
        if !query.is_none_or(|q| matches(q, ip, m, host, &format!("{comment}\n{name}"))) {
            continue;
        }
        seen_macs.push(mac::normalize(m));
        let mut hit = Hit::from_lease(&d.router, l);
        hit.name = name.to_string();
        hit.anomaly = report.for_lease(l);
        hit.port = port_of(&d.hosts, m);
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
        hit.name = names.get(m).to_string();
        hit.anomaly = report.for_arp(a);
        hit.port = port_of(&d.hosts, m);
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
                && query.is_none_or(|q| matches(q, ip, m, "", &format!("{comment}\n{}", hit.name)))
        };
        if keep {
            hits.push(hit);
        }
    }
    hits
}

/// Enregistre dans l'historique ce qui vient d'être vu (baux actifs, ARP,
/// balayage) et renvoie l'historique à jour. Un historique illisible est
/// signalé et laissé intact : rien n'est alors enregistré.
fn record(data: &[RouterData], found: &[scan::Found]) -> history::History {
    let mut hist = match history::load() {
        Ok(h) => h,
        Err(e) => {
            eprintln!("⚠️  {e:#}");
            return history::History::default();
        }
    };
    let now = history::now();
    for d in data {
        for l in d
            .leases
            .iter()
            .filter(|l| l.status.as_deref() == Some("bound"))
        {
            let ip = l
                .active_address
                .as_deref()
                .filter(|s| !s.is_empty())
                .or(l.address.as_deref())
                .unwrap_or("");
            hist.record(
                l.mac_address.as_deref().unwrap_or(""),
                ip,
                l.host_name.as_deref().unwrap_or(""),
                now,
            );
        }
        for a in d
            .arp
            .iter()
            .filter(|a| a.complete.as_deref() != Some("false"))
        {
            hist.record(
                a.mac_address.as_deref().unwrap_or(""),
                a.address.as_deref().unwrap_or(""),
                "",
                now,
            );
        }
    }
    for f in found {
        hist.record(&f.mac, &f.ip.to_string(), "", now);
    }
    if let Err(e) = history::save(&hist) {
        eprintln!("⚠️  {e:#}");
    }
    hist
}

/// Reporte sur les lignes ce que l'historique sait de chaque équipement.
fn stamp(hits: &mut [Hit], hist: &history::History) {
    let now = history::now();
    for h in hits {
        if let Some(d) = hist.get(&h.mac) {
            h.first_seen = d.first_seen;
            h.last_seen = d.last_seen;
            h.new = hist.is_new(&h.mac, now, NEW_DAYS);
        }
    }
}

/// Ligne d'un équipement de l'historique (commandes `new` et `history`).
fn history_hit(mac_key: &str, d: &history::Device, names: &names::Names, now: u64) -> Hit {
    let ip = d.last_ip().to_string();
    Hit {
        router: String::new(),
        source: "HISTORIQUE",
        subnet: subnet_of(&ip),
        ip,
        mac: mac::pretty(mac_key),
        vendor: oui::vendor(mac_key).to_string(),
        host: d.host.clone(),
        detail: String::new(),
        status: format!(
            "première vue {} ; dernière vue {}",
            history::ago(now, d.first_seen),
            history::ago(now, d.last_seen)
        ),
        anomaly: String::new(),
        server: String::new(),
        port: String::new(),
        name: names.get(mac_key).to_string(),
        first_seen: d.first_seen,
        last_seen: d.last_seen,
        new: false,
    }
}

/// Équipements apparus depuis `days` jours. Les routeurs sont d'abord relus
/// pour que l'historique soit à jour ; s'ils sont injoignables, la liste
/// vient de l'historique tel qu'il est.
fn new_command(cli: &Cli, days: u64) -> Result<()> {
    let hist = if config::config_path().exists() {
        match fetch(cli, !cli.no_arp) {
            Ok((data, _)) if !data.is_empty() => record(&data, &[]),
            Ok(_) => history::load()?,
            Err(e) => {
                eprintln!("⚠️  {e:#}");
                history::load()?
            }
        }
    } else {
        history::load()?
    };
    let names = load_names();
    let now = history::now();
    let mut hits: Vec<Hit> = hist
        .devices
        .iter()
        .filter(|(k, _)| hist.is_new(k, now, days))
        .map(|(k, d)| {
            let mut h = history_hit(k, d, &names, now);
            h.new = true;
            h
        })
        .collect();
    // Les plus récents d'abord.
    hits.sort_by_key(|h| std::cmp::Reverse(h.first_seen));
    let empty = format!("Aucun nouvel équipement depuis {days} jour(s).");
    print(&hits, cli.format(), &empty)
}

/// Fiche d'un équipement pour `history --json`.
#[derive(serde::Serialize)]
struct HistoryEntry {
    mac: String,
    vendor: String,
    name: String,
    host: String,
    first_seen: u64,
    last_seen: u64,
    new: bool,
    ips: Vec<history::Sighting>,
}

/// Historique des équipements qui correspondent à la requête. Ne contacte
/// aucun routeur.
fn history_command(cli: &Cli, query: &str) -> Result<bool> {
    let hist = history::load()?;
    let names = load_names();
    let now = history::now();
    let entries: Vec<HistoryEntry> = hist
        .devices
        .iter()
        .filter(|(k, d)| {
            let name = names.get(k);
            d.ips.iter().any(|s| s.ip == query.trim()) || matches(query, "", k, &d.host, name)
        })
        .map(|(k, d)| {
            let mut ips = d.ips.clone();
            ips.sort_by_key(|s| std::cmp::Reverse(s.last_seen));
            HistoryEntry {
                mac: mac::pretty(k),
                vendor: oui::vendor(k).to_string(),
                name: names.get(k).to_string(),
                host: d.host.clone(),
                first_seen: d.first_seen,
                last_seen: d.last_seen,
                new: hist.is_new(k, now, NEW_DAYS),
                ips,
            }
        })
        .collect();
    if cli.format() == Format::Json {
        println!("{}", serde_json::to_string_pretty(&entries)?);
        return Ok(!entries.is_empty());
    }
    if entries.is_empty() {
        println!("Aucun équipement correspondant dans l'historique.");
        return Ok(false);
    }
    for e in &entries {
        let label = [e.name.as_str(), e.host.as_str(), e.vendor.as_str()]
            .iter()
            .filter(|s| !s.is_empty())
            .copied()
            .collect::<Vec<_>>()
            .join(" · ");
        println!("{}  {label}", e.mac);
        println!(
            "  première vue {} ; dernière vue {}",
            history::ago(now, e.first_seen),
            history::ago(now, e.last_seen)
        );
        for s in &e.ips {
            println!(
                "  {:<15}  première vue {} ; dernière vue {}",
                s.ip,
                history::ago(now, s.first_seen),
                history::ago(now, s.last_seen)
            );
        }
        println!();
    }
    Ok(true)
}

fn collect(
    cli: &Cli,
    query: Option<&str>,
    only_anomalies: bool,
    group: Option<Group>,
) -> Result<Vec<Hit>> {
    // L'ARP est lue même pour la simple liste des baux : elle sert à relever
    // les anomalies. `--no-arp` limite la détection aux baux.
    let (data, to_offer) = fetch(cli, !cli.no_arp)?;
    let show_arp = !cli.no_arp && query.is_some();
    let names = load_names();
    let hist = record(&data, &[]);
    let mut hits: Vec<Hit> = data
        .iter()
        .flat_map(|d| hits_for(d, &names, query, show_arp, only_anomalies))
        .collect();
    stamp(&mut hits, &hist);
    // Affiche d'abord les résultats, puis propose l'enregistrement.
    let empty = if only_anomalies {
        "Aucune anomalie."
    } else {
        "Aucun résultat."
    };
    let hits = match group {
        Some(g) => print_grouped(hits, g, cli.format(), empty)?,
        None => {
            print(&hits, cli.format(), empty)?;
            hits
        }
    };
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
        server: lease
            .and_then(|(_, l)| l.server.clone())
            .unwrap_or_default(),
        subnet: subnet_of(&found.ip.to_string()),
        port: String::new(),
        name: String::new(),
        first_seen: 0,
        last_seen: 0,
        new: false,
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

    let names = load_names();
    let hist = record(&data, &found);
    let mut hits: Vec<Hit> = found
        .iter()
        .map(|f| {
            let mut hit = scan_hit(f, lease_for(&data, f), !data.is_empty());
            hit.port = data
                .iter()
                .map(|d| port_of(&d.hosts, &f.mac))
                .find(|p| !p.is_empty())
                .unwrap_or_default();
            hit.name = names.get(&f.mac).to_string();
            hit
        })
        .filter(|h| {
            filter.is_none_or(|q| {
                matches(
                    q,
                    &h.ip,
                    &h.mac,
                    &h.host,
                    &format!("{}\n{}", h.detail, h.name),
                )
            })
        })
        .collect();
    stamp(&mut hits, &hist);
    print(&hits, cli.format(), "Aucun résultat.")?;
    offer_saves(&to_offer);
    Ok(hits)
}

/// Largeur maximale de la colonne « Fabricant » (le JSON garde le nom entier).
const VENDOR_WIDTH: usize = 24;

fn print(hits: &[Hit], format: Format, empty: &str) -> Result<()> {
    if format == Format::Csv {
        print!("{}", to_csv(hits));
        return Ok(());
    }
    if format == Format::Json {
        println!("{}", serde_json::to_string_pretty(hits)?);
        return Ok(());
    }
    if hits.is_empty() {
        println!("{empty}");
        return Ok(());
    }
    println!("{}", table(hits));
    let flagged = hits.iter().filter(|h| !h.anomaly.is_empty()).count();
    if flagged > 0 && flagged < hits.len() {
        println!("{} résultat(s), dont {flagged} avec anomalie", hits.len());
    } else {
        println!("{} résultat(s)", hits.len());
    }
    Ok(())
}

/// Affiche un tableau par groupe ; en JSON, la liste triée par groupe.
/// Renvoie les lignes dans l'ordre affiché.
fn print_grouped(hits: Vec<Hit>, group: Group, format: Format, empty: &str) -> Result<Vec<Hit>> {
    let groups = group_hits(hits, group);
    if format == Format::Table {
        if groups.is_empty() {
            println!("{empty}");
        }
        for (name, members) in &groups {
            let title = match (name.is_empty(), group) {
                (false, _) => name.as_str(),
                (true, Group::Subnet) => "(adresse inconnue)",
                (true, Group::Server) => "(sans serveur DHCP)",
            };
            println!("{title} — {} équipement(s)", members.len());
            println!("{}\n", table(members));
        }
    }
    let hits: Vec<Hit> = groups
        .into_iter()
        .flat_map(|(_, members)| members)
        .collect();
    if format == Format::Json {
        println!("{}", serde_json::to_string_pretty(&hits)?);
    } else if format == Format::Csv {
        print!("{}", to_csv(&hits));
    } else if !hits.is_empty() {
        println!("{} résultat(s)", hits.len());
    }
    Ok(hits)
}

fn table(hits: &[Hit]) -> Table {
    // La colonne « Anomalie » n'apparaît que s'il y a quelque chose à signaler.
    let flagged = hits.iter().any(|h| !h.anomaly.is_empty());
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
    let ported = hits.iter().any(|h| !h.port.is_empty());
    if ported {
        header.insert(6, "Port");
    }
    if flagged {
        header.push("Anomalie");
    }
    let mut t = Table::new();
    t.load_preset(UTF8_FULL_CONDENSED).set_header(header);
    for h in hits {
        let vendor = oui::short(&h.vendor, VENDOR_WIDTH);
        let label = h.label();
        let mut row = vec![
            h.router.as_str(),
            h.source,
            &h.ip,
            &h.mac,
            &vendor,
            &label,
            &h.detail,
            &h.status,
        ];
        if ported {
            row.insert(6, &h.port);
        }
        if flagged {
            row.push(&h.anomaly);
        }
        t.add_row(row);
    }
    t
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
        Some(Command::RemoveRouter { name }) => {
            let routers = config::existing_routers()?;
            let router = routers
                .iter()
                .find(|r| r.name == *name)
                .ok_or_else(|| anyhow::anyhow!("Routeur « {name} » absent de la config"))?;
            config::remove_router(name)?;
            config::forget(router)?;
            println!("✓ Routeur « {name} » retiré.");
        }
        Some(Command::Routers) if cli.json => {
            // Sans routeur, la liste est vide : ce n'est pas une erreur ici.
            let routers: Vec<_> = config::existing_routers()?
                .iter()
                .map(|r| {
                    serde_json::json!({
                        "name": r.name,
                        "url": r.url,
                        "user": r.user.clone().unwrap_or_default(),
                        "insecure": r.insecure,
                    })
                })
                .collect();
            println!("{}", serde_json::to_string_pretty(&routers)?);
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
        Some(Command::Name { mac, name, clear }) => {
            let mut all = names::load()?;
            if *clear {
                if all.remove(mac)? {
                    names::save(&all)?;
                    println!("✓ Nom effacé pour {}.", mac::pretty(mac));
                } else {
                    println!("Aucun nom enregistré pour {}.", mac::pretty(mac));
                }
            } else {
                let name = name.as_deref().unwrap_or_default();
                all.set(mac, name)?;
                names::save(&all)?;
                println!("✓ {} s'appelle « {} ».", mac::pretty(mac), name.trim());
            }
        }
        Some(Command::New { days }) => new_command(&cli, *days)?,
        Some(Command::History { query }) => {
            if !history_command(&cli, query)? {
                std::process::exit(1);
            }
        }
        Some(Command::Names) => {
            let all = names::load()?;
            let mut count = 0;
            for (m, name) in all.entries() {
                println!("{m}  {name}");
                count += 1;
            }
            if count == 0 {
                println!("Aucun nom enregistré. Pour en donner un : macfind name <MAC> <nom>");
            }
        }
        Some(Command::Leases { group }) => {
            collect(&cli, None, false, *group)?;
        }
        Some(Command::Anomalies) => {
            collect(&cli, None, true, None)?;
        }
        Some(Command::Scan { target, filter }) => {
            let hits = scan_command(&cli, target.as_deref(), filter.as_deref())?;
            if hits.is_empty() {
                std::process::exit(1);
            }
        }
        None => match cli.query.as_deref() {
            Some(q) => {
                let hits = collect(&cli, Some(q), false, None)?;
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
    use super::{
        csv_field, group_hits, ip_key, matches, port_of, subnet_of, to_csv, BridgeHost, Group, Hit,
    };

    fn hit(ip: &str, server: &str) -> Hit {
        Hit {
            router: "principal".to_string(),
            source: "DHCP",
            ip: ip.to_string(),
            mac: "00:15:5D:00:00:01".to_string(),
            vendor: String::new(),
            host: String::new(),
            detail: String::new(),
            status: String::new(),
            anomaly: String::new(),
            server: server.to_string(),
            subnet: subnet_of(ip),
            port: String::new(),
            name: String::new(),
            first_seen: 0,
            last_seen: 0,
            new: false,
        }
    }

    #[test]
    fn csv_for_excel() {
        assert_eq!(csv_field("imprimante"), "imprimante");
        assert_eq!(csv_field("a;b"), "\"a;b\"");
        assert_eq!(csv_field("dit \"x\""), "\"dit \"\"x\"\"\"");
        // Un nom d'hôte ne doit pas devenir une formule.
        assert_eq!(csv_field("=1+1"), "'=1+1");
        assert_eq!(csv_field("@cmd"), "'@cmd");

        let mut h = hit("192.0.2.10", "dhcp1");
        h.host = "poste;1".to_string();
        h.name = "Accueil".to_string();
        assert_eq!(h.label(), "Accueil (poste;1)");
        let csv = to_csv(&[h]);
        let mut lines = csv.split("\r\n");
        assert!(lines
            .next()
            .unwrap()
            .starts_with("\u{feff}Routeur;Source;IP;MAC;"));
        assert_eq!(
            lines.next().unwrap(),
            "principal;DHCP;192.0.2.10;00:15:5D:00:00:01;;Accueil;\"poste;1\";;;;"
        );
    }

    #[test]
    fn port_lookup() {
        let host = |m: &str, on: Option<&str>, local: &str| BridgeHost {
            mac_address: Some(m.to_string()),
            on_interface: on.map(str::to_string),
            interface: Some("bridge".to_string()),
            local: Some(local.to_string()),
        };
        let hosts = [
            host("00:15:5D:00:00:01", Some("ether5"), "false"),
            host("00:15:5D:00:00:02", None, "false"),
            host("00:15:5D:00:00:03", Some("bridge"), "true"),
        ];
        // La MAC est reconnue quel que soit son format.
        assert_eq!(port_of(&hosts, "00-15-5d-00-00-01"), "ether5");
        // Sans port physique connu, l'interface sert de repli.
        assert_eq!(port_of(&hosts, "00:15:5D:00:00:02"), "bridge");
        // Les adresses du routeur lui-même sont ignorées.
        assert_eq!(port_of(&hosts, "00:15:5D:00:00:03"), "");
        assert_eq!(port_of(&hosts, "00:15:5D:00:00:09"), "");
        assert_eq!(port_of(&hosts, ""), "");
    }

    #[test]
    fn subnet_and_ip_key() {
        assert_eq!(subnet_of("192.0.2.57"), "192.0.2.0/24");
        assert_eq!(subnet_of(""), "");
        assert_eq!(subnet_of("pas-une-ip"), "");
        assert!(ip_key("192.0.2.9") < ip_key("192.0.2.10"));
        assert_eq!(ip_key(""), u32::MAX);
    }

    #[test]
    fn groups_by_subnet_in_numeric_order() {
        let hits = vec![
            hit("198.51.100.7", "dhcp2"),
            hit("192.0.2.10", "dhcp1"),
            hit("", "dhcp1"),
            hit("192.0.2.9", "dhcp1"),
        ];
        let groups = group_hits(hits, Group::Subnet);
        let names: Vec<_> = groups.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, ["192.0.2.0/24", "198.51.100.0/24", ""]);
        let first: Vec<_> = groups[0].1.iter().map(|h| h.ip.as_str()).collect();
        assert_eq!(first, ["192.0.2.9", "192.0.2.10"]);
    }

    #[test]
    fn groups_by_server() {
        let hits = vec![
            hit("198.51.100.7", "invites"),
            hit("192.0.2.10", "bureau"),
            hit("198.51.100.3", "invites"),
        ];
        let groups = group_hits(hits, Group::Server);
        let names: Vec<_> = groups.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, ["bureau", "invites"]);
        let second: Vec<_> = groups[1].1.iter().map(|h| h.ip.as_str()).collect();
        assert_eq!(second, ["198.51.100.3", "198.51.100.7"]);
    }

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
