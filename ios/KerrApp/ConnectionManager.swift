import Foundation
import Combine

class ConnectionManager: ObservableObject {
    @Published var isConnected = false
    @Published var connectionStatus = "Disconnected"
    @Published var errorMessage: String?

    private var endpoint: Endpoint?
    private var session: Session?
    private var fileBrowser: FileBrowser?

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
        // The Rust Display impl already contains the full message; just return it.
        return error.localizedDescription
    }

    func disconnect() {
        session?.disconnect()
        session = nil
        fileBrowser = nil
        endpoint = nil

        isConnected = false
        connectionStatus = "Disconnected"
    }

    func getFileBrowser() throws -> FileBrowser {
        guard let session = session else {
            throw KerrError.ConnectionFailed(message: "Not connected")
        }

        if let browser = fileBrowser {
            return browser
        }

        let browser = try session.fileBrowser()
        fileBrowser = browser
        return browser
    }

    func getSession() -> Session? {
        return session
    }
}
