import SwiftUI

// MARK: - Saved connection model

struct SavedConnection: Codable, Identifiable {
    var id: UUID = UUID()
    var alias: String
    var hostName: String
    var connectionString: String
    var registeredAt: Date

    static let storageKey = "kerr_saved_connections"

    static func load() -> [SavedConnection] {
        guard let data = UserDefaults.standard.data(forKey: storageKey),
              let list = try? JSONDecoder().decode([SavedConnection].self, from: data)
        else { return [] }
        return list
    }

    static func save(_ list: [SavedConnection]) {
        if let data = try? JSONEncoder().encode(list) {
            UserDefaults.standard.set(data, forKey: storageKey)
        }
    }
}

// MARK: - QR payload (matches Vue output: {"alias":"...","host_name":"...","cs":"..."})

private struct QRConnectionPayload: Decodable {
    let alias: String
    let host_name: String
    let cs: String
}

// MARK: - Connection list view

struct ConnectionView: View {
    @ObservedObject var connectionManager: ConnectionManager
    @State private var connections: [SavedConnection] = SavedConnection.load()
    @State private var connectingId: UUID? = nil
    @State private var showAddSheet = false
    @State private var showScanner = false
    @State private var errorMessage: String? = nil
    @State private var successMessage: String? = nil

    var body: some View {
        VStack(spacing: 0) {
            headerView

            if let msg = successMessage {
                Text(msg)
                    .font(.caption)
                    .foregroundColor(.green)
                    .padding()
                    .frame(maxWidth: .infinity)
                    .background(Color.green.opacity(0.1))
            }

            if let err = errorMessage {
                Text(err)
                    .font(.caption)
                    .foregroundColor(.red)
                    .padding()
                    .frame(maxWidth: .infinity)
                    .background(Color.red.opacity(0.1))
            }

            if connections.isEmpty {
                emptyStateView
            } else {
                listView
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Color(uiColor: .systemGroupedBackground))
        .navigationTitle("Kerr")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .navigationBarTrailing) {
                HStack(spacing: 4) {
                    Button(action: { showScanner = true }) {
                        Image(systemName: "qrcode.viewfinder")
                    }
                    Button(action: { showAddSheet = true }) {
                        Image(systemName: "square.and.pencil")
                    }
                }
            }
        }
        .fullScreenCover(isPresented: $showAddSheet) {
            AddConnectionSheet { saved in
                connections.append(saved)
                SavedConnection.save(connections)
            }
        }
        .sheet(isPresented: $showScanner) {
            QRScannerSheet { payload in
                handleScannedQR(payload)
            }
        }
    }

    private var headerView: some View {
        VStack(spacing: 6) {
            Image(systemName: "network")
                .font(.system(size: 48))
                .foregroundColor(.accentColor)
            Text("Select a Connection")
                .font(.title2).fontWeight(.semibold)
            Text("Choose a saved connection to connect to")
                .font(.subheadline).foregroundColor(.secondary)
        }
        .padding(.vertical, 28)
    }

    private var emptyStateView: some View {
        VStack(spacing: 16) {
            Spacer()
            Image(systemName: "antenna.radiowaves.left.and.right")
                .font(.system(size: 56))
                .foregroundColor(.secondary)
            Text("No saved connections")
                .font(.headline)
            Text("Scan a QR code from the web UI, or tap the pencil icon to add manually.")
                .font(.subheadline)
                .foregroundColor(.secondary)
                .multilineTextAlignment(.center)
                .padding(.horizontal, 40)
            HStack(spacing: 12) {
                Button(action: { showScanner = true }) {
                    Label("Scan QR", systemImage: "qrcode.viewfinder")
                        .padding(.horizontal, 20)
                        .padding(.vertical, 12)
                }
                .buttonStyle(.borderedProminent)
                Button(action: { showAddSheet = true }) {
                    Label("Add Manually", systemImage: "square.and.pencil")
                        .padding(.horizontal, 20)
                        .padding(.vertical, 12)
                }
                .buttonStyle(.bordered)
            }
            Spacer()
        }
    }

    private var listView: some View {
        List {
            ForEach(connections) { conn in
                connectionRow(conn)
            }
            .onDelete { indexSet in
                connections.remove(atOffsets: indexSet)
                SavedConnection.save(connections)
            }
        }
        .listStyle(.insetGrouped)
    }

    private func connectionRow(_ conn: SavedConnection) -> some View {
        Button(action: { connect(conn) }) {
            HStack {
                VStack(alignment: .leading, spacing: 4) {
                    Text(conn.alias.isEmpty ? "Unnamed" : conn.alias)
                        .font(.headline)
                        .foregroundColor(.primary)
                    Text(conn.hostName.isEmpty ? "Unknown host" : conn.hostName)
                        .font(.subheadline)
                        .foregroundColor(.accentColor)
                    Text("Registered: \(conn.registeredAt.formatted(date: .abbreviated, time: .shortened))")
                        .font(.caption)
                        .foregroundColor(.secondary)
                }
                Spacer()
                if connectingId == conn.id {
                    ProgressView()
                } else {
                    Image(systemName: "chevron.right")
                        .foregroundColor(.secondary)
                }
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(connectingId != nil)
    }

    private func connect(_ conn: SavedConnection) {
        errorMessage = nil
        successMessage = nil
        let cs = conn.connectionString.trimmingCharacters(in: .whitespacesAndNewlines)
        if cs.isEmpty {
            errorMessage = "Connection string is empty."
            return
        }
        do {
            _ = try decodeConnectionString(connStr: cs)
        } catch let e as KerrError {
            errorMessage = e.localizedDescription
            return
        } catch {
            errorMessage = "Invalid connection string: \(error)"
            return
        }
        connectingId = conn.id
        connectionManager.connect(connectionString: cs) { err in
            connectingId = nil
            if let err = err {
                errorMessage = err
            }
        }
    }

    private func handleScannedQR(_ payload: String) {
        errorMessage = nil
        successMessage = nil
        guard let data = payload.data(using: .utf8),
              let qr = try? JSONDecoder().decode(QRConnectionPayload.self, from: data)
        else {
            errorMessage = "Invalid QR code — expected Kerr connection format."
            return
        }
        let cs = qr.cs.trimmingCharacters(in: .whitespacesAndNewlines)
        if connections.contains(where: { $0.connectionString == cs }) {
            errorMessage = "\"\(qr.alias)\" is already saved."
            return
        }
        let conn = SavedConnection(
            alias: qr.alias,
            hostName: qr.host_name,
            connectionString: cs,
            registeredAt: Date()
        )
        connections.append(conn)
        SavedConnection.save(connections)
        successMessage = "Added \"\(qr.alias.isEmpty ? "connection" : qr.alias)\" from QR code."
        // Auto-clear success message
        DispatchQueue.main.asyncAfter(deadline: .now() + 3) {
            successMessage = nil
        }
    }
}

// MARK: - Add connection sheet (manual entry)

struct AddConnectionSheet: View {
    let onSave: (SavedConnection) -> Void

    @Environment(\.dismiss) private var dismiss
    @State private var alias = ""
    @State private var hostName = ""
    @State private var connectionString = ""

    var body: some View {
        NavigationStack {
            Form {
                Section("Details") {
                    TextField("Alias (e.g. my-mac)", text: $alias)
                    TextField("Host name (optional)", text: $hostName)
                }
                Section("Connection String") {
                    TextEditor(text: $connectionString)
                        .font(.system(.caption, design: .monospaced))
                        .frame(minHeight: 80)
                        .autocapitalization(.none)
                        .disableAutocorrection(true)
                    Button(action: paste) {
                        Label("Paste from Clipboard", systemImage: "doc.on.clipboard")
                    }
                }
            }
            .navigationTitle("Add Connection")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Save") { save() }
                        .disabled(connectionString.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                }
            }
        }
    }

    private func paste() {
        if let s = UIPasteboard.general.string {
            connectionString = s.trimmingCharacters(in: .whitespacesAndNewlines)
        }
    }

    private func save() {
        let conn = SavedConnection(
            alias: alias.trimmingCharacters(in: .whitespacesAndNewlines),
            hostName: hostName.trimmingCharacters(in: .whitespacesAndNewlines),
            connectionString: connectionString.trimmingCharacters(in: .whitespacesAndNewlines),
            registeredAt: Date()
        )
        onSave(conn)
        dismiss()
    }
}
