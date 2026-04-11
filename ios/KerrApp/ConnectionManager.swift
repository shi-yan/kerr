import Foundation
import Combine
import NetworkExtension

// MARK: - Constants

private let kExtensionBundleID = "com.kerr.app.NetworkExtension"
private let kAppGroupID        = "group.com.kerr.app"
private let kVpnConnStringKey  = "vpn_connection_string"
private let kVpnHandleUDPKey   = "vpn_handle_udp"

// MARK: - ConnectionManager

class ConnectionManager: ObservableObject {
    @Published var isConnected     = false
    @Published var connectionStatus = "Disconnected"
    @Published var errorMessage: String?

    private var endpoint:    Endpoint?
    private var session:     Session?
    private var fileBrowser: FileBrowser?

    /// The raw connection string of the active session — shared with the VPN extension.
    private var activeConnectionString: String?

    // MARK: - Connection lifecycle

    func connect(connectionString: String, completion: ((String?) -> Void)? = nil) {
        connectionStatus = "Connecting..."
        errorMessage = nil

        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            do {
                print("[Kerr] Creating endpoint...")
                let endpoint = try createEndpoint()
                self?.endpoint = endpoint
                print("[Kerr] Endpoint created. Connecting...")

                let session = try endpoint.connect(connectionString: connectionString)
                self?.session = session
                self?.activeConnectionString = connectionString
                print("[Kerr] Connected successfully.")

                DispatchQueue.main.async {
                    self?.isConnected = true
                    self?.connectionStatus = "Connected"
                    completion?(nil)
                }
            } catch let e as KerrError {
                let msg = Self.describe(e)
                print("[Kerr] Connection error: \(msg)")
                DispatchQueue.main.async {
                    self?.errorMessage = msg
                    self?.connectionStatus = "Failed"
                    self?.isConnected = false
                    completion?(msg)
                }
            } catch {
                let msg = "Unexpected error: \(error)"
                print("[Kerr] \(msg)")
                DispatchQueue.main.async {
                    self?.errorMessage = msg
                    self?.connectionStatus = "Failed"
                    self?.isConnected = false
                    completion?(msg)
                }
            }
        }
    }

    private static func describe(_ error: KerrError) -> String {
        return error.localizedDescription
    }

    func disconnect() {
        // Stop VPN if it happens to be running when the user exits.
        stopVPN()

        session = nil
        fileBrowser = nil
        endpoint = nil
        activeConnectionString = nil

        isConnected = false
        connectionStatus = "Disconnected"
    }

    func getFileBrowser() throws -> FileBrowser {
        guard let session = session else {
            throw KerrError.ConnectionFailed(message: "Not connected")
        }
        if let browser = fileBrowser { return browser }
        let browser = try session.fileBrowser()
        fileBrowser = browser
        return browser
    }

    func getSession() -> Session? { session }

    // MARK: - VPN management

    /// Loads the saved NETunnelProviderManager for the Kerr VPN, if any.
    /// Calls `completion` on the main queue.
    func loadVPNManager(completion: @escaping (NETunnelProviderManager?) -> Void) {
        NETunnelProviderManager.loadAllFromPreferences { managers, error in
            if let error = error {
                print("[Kerr VPN] loadAll error: \(error)")
                DispatchQueue.main.async { completion(nil) }
                return
            }
            let kerr = managers?.first {
                ($0.protocolConfiguration as? NETunnelProviderProtocol)?
                    .providerBundleIdentifier == kExtensionBundleID
            }
            DispatchQueue.main.async { completion(kerr) }
        }
    }

    /// Installs (or updates) the VPN profile and starts the tunnel.
    /// The current `activeConnectionString` is written to the shared App Group
    /// so the Network Extension can read it when it wakes.
    func startVPN(handleUDP: Bool, completion: @escaping (String?) -> Void) {
        guard let cs = activeConnectionString, !cs.isEmpty else {
            completion("No active Kerr session. Connect via Files or Terminal first.")
            return
        }

        // Persist configuration for the extension process.
        if let defaults = UserDefaults(suiteName: kAppGroupID) {
            defaults.set(cs,        forKey: kVpnConnStringKey)
            defaults.set(handleUDP, forKey: kVpnHandleUDPKey)
        }

        // Load existing profiles (so we don't create duplicates) then save/start.
        NETunnelProviderManager.loadAllFromPreferences { [weak self] managers, _ in
            guard let self = self else { return }

            let manager: NETunnelProviderManager
            if let existing = managers?.first(where: {
                ($0.protocolConfiguration as? NETunnelProviderProtocol)?
                    .providerBundleIdentifier == kExtensionBundleID
            }) {
                manager = existing
            } else {
                manager = NETunnelProviderManager()
            }

            let proto = NETunnelProviderProtocol()
            proto.providerBundleIdentifier = kExtensionBundleID
            proto.serverAddress = "Kerr P2P VPN"
            manager.protocolConfiguration = proto
            manager.localizedDescription = "Kerr VPN"
            manager.isEnabled = true

            manager.saveToPreferences { error in
                if let error = error {
                    DispatchQueue.main.async {
                        completion("Failed to save VPN profile: \(error.localizedDescription)")
                    }
                    return
                }
                manager.loadFromPreferences { error in
                    if let error = error {
                        DispatchQueue.main.async {
                            completion("Failed to reload VPN profile: \(error.localizedDescription)")
                        }
                        return
                    }
                    do {
                        try manager.connection.startVPNTunnel()
                        DispatchQueue.main.async { completion(nil) }
                    } catch {
                        DispatchQueue.main.async {
                            completion("Failed to start VPN tunnel: \(error.localizedDescription)")
                        }
                    }
                }
            }
        }
    }

    /// Stops the running VPN tunnel (if any).
    func stopVPN() {
        loadVPNManager { manager in
            manager?.connection.stopVPNTunnel()
        }
    }
}
