import SwiftUI
import NetworkExtension

// MARK: - VPN status helpers

private func statusLabel(_ s: NEVPNStatus) -> String {
    switch s {
    case .connected:      return "Connected"
    case .connecting:     return "Connecting…"
    case .disconnecting:  return "Disconnecting…"
    case .disconnected:   return "Disconnected"
    case .reasserting:    return "Reconnecting…"
    case .invalid:        return "Not configured"
    @unknown default:     return "Unknown"
    }
}

private func statusColor(_ s: NEVPNStatus) -> Color {
    switch s {
    case .connected:            return .green
    case .connecting, .reasserting: return .yellow
    case .disconnecting:        return .orange
    default:                    return .red
    }
}

// MARK: - VpnView

struct VpnView: View {
    @ObservedObject var connectionManager: ConnectionManager

    @State private var handleUDP     = false
    @State private var vpnStatus: NEVPNStatus = .invalid
    @State private var statusMsg     = "Not configured"
    @State private var isConnecting  = false
    @State private var errorMsg: String? = nil

    // Keep a strong reference to the observer so it is not deallocated.
    @State private var statusObserver: NSObjectProtocol? = nil

    var body: some View {
        NavigationStack {
            Form {

                // ── Status ────────────────────────────────────────────────
                Section("VPN Status") {
                    HStack(spacing: 10) {
                        Circle()
                            .fill(statusColor(vpnStatus))
                            .frame(width: 10, height: 10)
                        Text(statusMsg)
                            .font(.subheadline)
                    }
                    .padding(.vertical, 2)
                }

                // ── Options ───────────────────────────────────────────────
                Section {
                    Toggle(isOn: $handleUDP) {
                        VStack(alignment: .leading, spacing: 2) {
                            Text("Route DNS via tunnel")
                            Text("Sends DNS queries through the P2P relay (future release)")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                    }
                    .disabled(vpnStatus == .connected || isConnecting)
                } header: {
                    Text("Options")
                }

                // ── Connect / Disconnect ───────────────────────────────────
                Section {
                    if !connectionManager.isConnected {
                        Label {
                            Text("Connect to a Kerr server first (Files or Terminal tab)")
                                .font(.subheadline)
                                .foregroundStyle(.secondary)
                        } icon: {
                            Image(systemName: "exclamationmark.triangle")
                                .foregroundStyle(.orange)
                        }
                    } else if vpnStatus == .connected {
                        Button(role: .destructive) {
                            stopVPN()
                        } label: {
                            Label("Disconnect VPN", systemImage: "shield.slash.fill")
                                .frame(maxWidth: .infinity)
                        }
                        .buttonStyle(.borderedProminent)
                        .tint(.red)
                    } else {
                        Button {
                            startVPN()
                        } label: {
                            HStack {
                                if isConnecting { ProgressView().padding(.trailing, 4) }
                                Label("Connect as System VPN", systemImage: "shield.fill")
                            }
                            .frame(maxWidth: .infinity)
                        }
                        .buttonStyle(.borderedProminent)
                        .disabled(isConnecting || !connectionManager.isConnected)
                    }
                }

                // ── Error ─────────────────────────────────────────────────
                if let err = errorMsg {
                    Section {
                        Text(err)
                            .font(.caption)
                            .foregroundStyle(.red)
                    }
                }

                // ── How it works ──────────────────────────────────────────
                Section("How it works") {
                    VStack(alignment: .leading, spacing: 6) {
                        BulletRow(icon: "network", text: "Routes TCP traffic from every app through the Kerr P2P relay — no manual Chrome proxy needed.")
                        BulletRow(icon: "lock.shield", text: "Uses a local SOCKS5 proxy backed by your existing Iroh connection.")
                        BulletRow(icon: "exclamationmark.shield", text: "Requires the Network Extension entitlement (Apple Developer Program membership).")
                        BulletRow(icon: "arrow.triangle.2.circlepath", text: "Raw UDP tunneling (gaming, P2P apps) is on the roadmap and will use packet-level forwarding.")
                    }
                    .padding(.vertical, 4)
                }
            }
            .navigationTitle("VPN")
            .onAppear { observeVPNStatus() }
            .onDisappear {
                if let obs = statusObserver {
                    NotificationCenter.default.removeObserver(obs)
                    statusObserver = nil
                }
            }
        }
    }

    // MARK: - Actions

    private func startVPN() {
        errorMsg = nil
        isConnecting = true
        connectionManager.startVPN(handleUDP: handleUDP) { err in
            isConnecting = false
            if let e = err { errorMsg = e }
        }
    }

    private func stopVPN() {
        errorMsg = nil
        connectionManager.stopVPN()
    }

    // MARK: - Status observation

    private func observeVPNStatus() {
        connectionManager.loadVPNManager { manager in
            if let m = manager {
                self.vpnStatus = m.connection.status
                self.statusMsg = statusLabel(self.vpnStatus)
                self.attachObserver(to: m.connection)
            } else {
                self.statusMsg = "Not configured"
            }
        }
    }

    private func attachObserver(to connection: NEVPNConnection) {
        // Remove existing observer first
        if let obs = statusObserver {
            NotificationCenter.default.removeObserver(obs)
        }
        statusObserver = NotificationCenter.default.addObserver(
            forName: .NEVPNStatusDidChange,
            object: connection,
            queue: .main
        ) { _ in
            vpnStatus = connection.status
            statusMsg = statusLabel(vpnStatus)
            if vpnStatus != .connecting { isConnecting = false }
        }
    }
}

// MARK: - Small helper

private struct BulletRow: View {
    let icon: String
    let text: String
    var body: some View {
        HStack(alignment: .top, spacing: 8) {
            Image(systemName: icon)
                .foregroundStyle(.accentColor)
                .frame(width: 20)
            Text(text)
                .font(.caption)
                .foregroundStyle(.secondary)
        }
    }
}
