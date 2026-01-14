import Foundation
import Combine

class ConnectionManager: ObservableObject {
    @Published var isConnected = false
    @Published var connectionStatus = "Disconnected"
    @Published var errorMessage: String?

    private var endpoint: Endpoint?
    private var session: Session?
    private var fileBrowser: FileBrowser?

    func connect(connectionString: String) {
        connectionStatus = "Connecting..."
        errorMessage = nil

        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            do {
                // Create endpoint
                let endpoint = try createEndpoint()
                self?.endpoint = endpoint

                // Connect to remote
                let session = try endpoint.connect(connectionString: connectionString)
                self?.session = session

                DispatchQueue.main.async {
                    self?.isConnected = true
                    self?.connectionStatus = "Connected"
                }
            } catch {
                DispatchQueue.main.async {
                    self?.errorMessage = "Connection failed: \(error.localizedDescription)"
                    self?.connectionStatus = "Failed"
                    self?.isConnected = false
                }
            }
        }
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
