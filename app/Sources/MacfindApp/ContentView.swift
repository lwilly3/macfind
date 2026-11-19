import AppKit
import SwiftUI

struct ContentView: View {
    @State private var inventaire = Inventaire()
    @State private var tri = [KeyPathComparator(\Equipement.ipKey)]
    @State private var selection = Set<Equipement.ID>()
    @State private var ajoutRouteur = false
    @AppStorage("regroupement") private var regroupement = Regroupement.aucun

    private var lignes: [Equipement] {
        inventaire.visibles.sorted(using: tri)
    }

    var body: some View {
        VStack(spacing: 0) {
            ForEach(inventaire.avertissements, id: \.self) { texte in
                Label(texte, systemImage: "exclamationmark.triangle.fill")
                    .foregroundStyle(.orange)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 12)
                    .padding(.vertical, 6)
                    .background(.orange.opacity(0.12))
            }
            contenu
            Divider()
            piedDePage
        }
        .searchable(text: $inventaire.recherche, prompt: "MAC, IP, nom, fabricant")
        .toolbar {
            ToolbarItem(placement: .principal) {
                Picker("Source", selection: $inventaire.source) {
                    ForEach(Source.allCases) { Text($0.rawValue).tag($0) }
                }
                .pickerStyle(.segmented)
            }
            ToolbarItem {
                Button {
                    ajoutRouteur = true
                } label: {
                    Label("Ajouter un routeur", systemImage: "plus")
                }
                .help("Ajouter un routeur")
            }
            ToolbarItem {
                Button {
                    Task { await inventaire.charger() }
                } label: {
                    Label("Actualiser", systemImage: "arrow.clockwise")
                }
                .keyboardShortcut("r")
                .disabled(inventaire.enCours)
                .help("Actualiser (⌘R)")
            }
        }
        .sheet(isPresented: $ajoutRouteur) {
            AjoutRouteurView {
                // Affiche les baux du routeur qui vient d'être ajouté.
                if inventaire.source == .balayage {
                    inventaire.source = .baux
                } else {
                    Task { await inventaire.charger() }
                }
            }
        }
        // Charge au lancement, puis à chaque changement de source.
        .task(id: inventaire.source) { await inventaire.charger() }
    }

    @ViewBuilder
    private var contenu: some View {
        if inventaire.configurationManquante {
            ContentUnavailableView {
                Label("Aucun routeur configuré", systemImage: "network.slash")
            } description: {
                Text(
                    "Ajoute ton routeur MikroTik pour lire ses baux DHCP.\nLe balayage local fonctionne sans routeur."
                )
            } actions: {
                Button("Ajouter un routeur…") { ajoutRouteur = true }
                    .buttonStyle(.borderedProminent)
                Button("Balayer le réseau local") { inventaire.source = .balayage }
            }
        } else if let erreur = inventaire.erreur {
            ContentUnavailableView {
                Label("Lecture impossible", systemImage: "exclamationmark.triangle")
            } description: {
                Text(erreur).textSelection(.enabled)
            } actions: {
                Button("Réessayer") { Task { await inventaire.charger() } }
            }
        } else if inventaire.enCours && inventaire.equipements.isEmpty {
            ProgressView(
                inventaire.source == .balayage
                    ? "Balayage du réseau local…" : "Lecture des routeurs…"
            )
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        } else if lignes.isEmpty {
            if !inventaire.recherche.isEmpty {
                ContentUnavailableView.search(text: inventaire.recherche)
            } else if inventaire.source == .anomalies {
                ContentUnavailableView(
                    "Aucune anomalie", systemImage: "checkmark.circle",
                    description: Text(
                        "Aucune MAC avec plusieurs baux actifs, aucune IP partagée, aucun équipement hors de ses baux."
                    ))
            } else {
                ContentUnavailableView("Aucun équipement", systemImage: "network")
            }
        } else if inventaire.source == .anomalies {
            tableauAnomalies
        } else {
            tableau
        }
    }

    /// Vue « Anomalies » : le motif d'abord, le reste en appui.
    private var tableauAnomalies: some View {
        Table(lignes, selection: $selection, sortOrder: $tri) {
            TableColumn("IP", value: \.ipKey) { Text($0.ip).monospaced() }
                .width(min: 125, ideal: 135)
            TableColumn("MAC", value: \.mac) { Text($0.mac).monospaced() }
                .width(min: 140, ideal: 150)
            TableColumn("Hôte", value: \.host) { Text($0.host).help($0.host) }
                .width(min: 80, ideal: 160)
            TableColumn("Anomalie", value: \.anomaly) { e in
                Label(e.anomaly, systemImage: "exclamationmark.triangle.fill")
                    .foregroundStyle(.orange, .primary)
                    .help(e.anomaly)
            }
            .width(min: 200, ideal: 460)
            TableColumn("Source", value: \.source)
                .width(min: 50, ideal: 60)
            TableColumn("Routeur", value: \.router)
                .width(min: 60, ideal: 85)
        }
        .contextMenu(forSelectionType: Equipement.ID.self) { ids in menu(ids) }
    }

    @ViewBuilder
    private func menu(_ ids: Set<Equipement.ID>) -> some View {
        if let e = lignes.first(where: { ids.contains($0.id) }), ids.count == 1 {
            Button("Copier l'IP") { copier(e.ip) }
            Button("Copier la MAC") { copier(e.mac) }
            if !e.host.isEmpty {
                Button("Copier le nom") { copier(e.host) }
            }
        }
    }

    @ViewBuilder
    private var tableau: some View {
        if regroupement == .aucun {
            Table(lignes, selection: $selection, sortOrder: $tri) { colonnes }
                .contextMenu(forSelectionType: Equipement.ID.self) { ids in menu(ids) }
        } else {
            // Le tri choisi s'applique à l'intérieur de chaque groupe.
            Table(of: Equipement.self, selection: $selection, sortOrder: $tri) {
                colonnes
            } rows: {
                ForEach(Groupe.repartir(lignes, par: regroupement)) { groupe in
                    Section(groupe.titre) {
                        ForEach(groupe.lignes) { TableRow($0) }
                    }
                }
            }
            .contextMenu(forSelectionType: Equipement.ID.self) { ids in menu(ids) }
        }
    }

    @TableColumnBuilder<Equipement, KeyPathComparator<Equipement>>
    private var colonnes: some TableColumnContent<Equipement, KeyPathComparator<Equipement>> {
        // Repère d'anomalie : le détail est dans l'infobulle et dans la
        // vue « Anomalies ». Trier cette colonne regroupe les lignes signalées.
        TableColumn("⚠︎", value: \.anomaly) { e in
            if !e.anomaly.isEmpty {
                Image(systemName: "exclamationmark.triangle.fill")
                    .foregroundStyle(.orange)
                    .help(e.anomaly)
                    .accessibilityLabel("Anomalie : \(e.anomaly)")
            }
        }
        .width(24)
        // Largeur idéale = contenu le plus long attendu (une IP ou une MAC
        // ne doit jamais être tronquée). Pas de borne haute : chaque
        // colonne reste élargissable ; une fenêtre étroite défile.
        TableColumn("IP", value: \.ipKey) { Text($0.ip).monospaced() }
            .width(min: 125, ideal: 135)
        TableColumn("MAC", value: \.mac) { Text($0.mac).monospaced() }
            .width(min: 140, ideal: 150)
        TableColumn("Hôte", value: \.host) { Text($0.host).help($0.host) }
            .width(min: 80, ideal: 170)
        TableColumn("Fabricant", value: \.vendor) { Text($0.vendor).help($0.vendor) }
            .width(min: 80, ideal: 170)
        TableColumn("Serveur / Interface", value: \.detail) {
            Text($0.detail).help($0.detail)
        }
        .width(min: 80, ideal: 220)
        TableColumn("Statut", value: \.status) { Text($0.status).help($0.status) }
            .width(min: 80, ideal: 190)
        TableColumn("Routeur", value: \.router)
            .width(min: 60, ideal: 85)
    }

    private var piedDePage: some View {
        HStack(spacing: 8) {
            if inventaire.enCours {
                ProgressView().controlSize(.small)
            }
            Text(compte)
            Spacer()
            if inventaire.source != .anomalies {
                Picker("Regrouper par", selection: $regroupement) {
                    ForEach(Regroupement.allCases) { Text($0.rawValue).tag($0) }
                }
                .pickerStyle(.menu)
                .fixedSize()
                .controlSize(.small)
            }
            if let date = inventaire.derniereMaj {
                Text("Mis à jour à \(date.formatted(date: .omitted, time: .standard))")
            }
        }
        .font(.callout)
        .foregroundStyle(.secondary)
        .padding(.horizontal, 12)
        .padding(.vertical, 6)
    }

    private var compte: String {
        let total = inventaire.equipements.count
        let visibles = lignes.count
        let nom = total > 1 ? "équipements" : "équipement"
        return visibles == total ? "\(total) \(nom)" : "\(visibles) sur \(total) \(nom)"
    }

    private func copier(_ texte: String) {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(texte, forType: .string)
    }
}
