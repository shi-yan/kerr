// PacketTunnelProvider.swift
// KerrNetworkExtension
//
// This NEPacketTunnelProvider starts a local SOCKS5 server (via the Rust
// kerr_ios library) that forwards all TCP connections through an Iroh P2P
// TcpRelay session.  The system's NEProxySettings then direct HTTP/HTTPS
// traffic from every app through that SOCKS5 server while the VPN is active.
//
// Memory budget: keep the Tokio runtime lean; the extension is capped at
// ~15–50 MB on iOS.  The Iroh endpoint is created fresh in the extension
// process (separate from the main app).

import NetworkExtension
import os.log

private let log = OSLog(subsystem: "com.kerr.app.NetworkExtension", category: "tunnel")

// MARK: - Constants (must match ConnectionManager.swift)

private let kAppGroupID       = "group.com.kerr.app"
private let kVpnConnStringKey = "vpn_connection_string"
private let kVpnHandleUDPKey  = "vpn_handle_udp"

// MARK: - PacketTunnelProvider

class PacketTunnelProvider: NEPacketTunnelProvider {

    private var kerrEndpoint: Endpoint?
    private var kerrSession:  Session?
    private var vpnTunnel:    VpnTunnel?

    // MARK: Start

    override func startTunnel(
        options: [String: NSObject]?,
        completionHandler: @escaping (Error?) -> Void
    ) {
        os_log(.info, log: log, "startTunnel called")

        // Read the connection string saved by the main app.
        guard
            let defaults = UserDefaults(suiteName: kAppGroupID),
            let cs = defaults.string(forKey: kVpnConnStringKey),
            !cs.isEmpty
        else {
            completionHandler(vpnError(1, "No Kerr connection string in App Group '\(kAppGroupID)'. Connect from the main app first."))
            return
        }
        let handleUDP = defaults.bool(forKey: kVpnHandleUDPKey)

        // All Iroh / Rust work must happen off the main thread.
        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            guard let self = self else { return }
            do {
                os_log(.info, log: log, "Creating Iroh endpoint in extension…")
                let ep = try createEndpoint()
                self.kerrEndpoint = ep

                os_log(.info, log: log, "Connecting to Kerr server…")
                let sess = try ep.connect(connectionString: cs)
                self.kerrSession = sess

                os_log(.info, log: log, "Starting SOCKS5 VPN tunnel…")
                // Pass port 0 to auto-assign a free port.
                let tunnel = try sess.startVpn(socksPort: 0, handleUdp: handleUDP)
                self.vpnTunnel = tunnel
                let port = tunnel.getSocksPort()
                os_log(.info, log: log, "SOCKS5 server running on 127.0.0.1:%d", port)

                // Apply network settings on the main queue as required by NE.
                DispatchQueue.main.async {
                    self.applyNetworkSettings(
                        socksPort: port,
                        handleUDP: handleUDP,
                        completion: completionHandler
                    )
                }
            } catch let e as KerrError {
                os_log(.error, log: log, "KerrError in startTunnel: %{public}@", e.localizedDescription)
                completionHandler(vpnError(2, e.localizedDescription))
            } catch {
                os_log(.error, log: log, "Error in startTunnel: %{public}@", error.localizedDescription)
                completionHandler(vpnError(3, error.localizedDescription))
            }
        }
    }

    // MARK: Stop

    override func stopTunnel(
        with reason: NEProviderStopReason,
        completionHandler: @escaping () -> Void
    ) {
        os_log(.info, log: log, "stopTunnel reason=%d", reason.rawValue)
        vpnTunnel?.stop()
        vpnTunnel    = nil
        kerrSession  = nil
        kerrEndpoint = nil
        completionHandler()
    }

    // MARK: App → Extension IPC (optional)

    override func handleAppMessage(
        _ messageData: Data,
        completionHandler: ((Data?) -> Void)?
    ) {
        // Reserved for future status queries from the main app.
        completionHandler?(nil)
    }

    // MARK: - Network settings

    /// Installs NEPacketTunnelNetworkSettings that redirect TCP traffic
    /// through our local SOCKS5 server.  A minimal TUN interface is set up
    /// (required by the API) but carries no actual routed traffic — all
    /// forwarding happens via the proxy settings.
    private func applyNetworkSettings(
        socksPort: UInt16,
        handleUDP: Bool,
        completion: @escaping (Error?) -> Void
    ) {
        // tunnelRemoteAddress is a required field; we use a loopback dummy.
        let settings = NEPacketTunnelNetworkSettings(tunnelRemoteAddress: "127.0.0.1")

        // ── Minimal TUN (no actual route hijacking) ──────────────────────
        // We rely entirely on proxy settings for traffic capture.
        let ipv4 = NEIPv4Settings(addresses: ["10.88.0.2"], subnetMasks: ["255.255.255.0"])
        ipv4.includedRoutes = [] // no routes → only proxy settings apply
        settings.ipv4Settings = ipv4
        settings.mtu = NSNumber(value: 1280)

        // ── SOCKS5 proxy (system-wide while VPN is active) ────────────────
        // NEProxySettings.socksServer covers all TCP connections from any app
        // that goes through the system proxy stack (i.e. everything built on
        // URLSession / CFNetwork / Network.framework — the vast majority of
        // iOS apps).
        let proxy = NEProxySettings()
        let socksServer = NEProxyServer(address: "127.0.0.1", port: Int(socksPort))

        proxy.socksServer       = socksServer
        // Also set explicit HTTP/HTTPS proxy so apps using those APIs route
        // through SOCKS5.
        proxy.httpEnabled       = true
        proxy.httpServer        = socksServer
        proxy.httpsEnabled      = true
        proxy.httpsServer       = socksServer

        // Exclude LAN / link-local ranges from the proxy.
        proxy.excludeSimpleHostnames = true
        proxy.exceptionList = [
            "*.local",
            "169.254.0.0/16",  // link-local
            "192.168.0.0/16",  // private LAN
            "10.0.0.0/8",      // private LAN
            "172.16.0.0/12",   // private LAN
        ]
        settings.proxySettings = proxy

        // ── Optional: DNS via tunnel ──────────────────────────────────────
        // When handleUDP is true, we ideally also forward DNS queries via the
        // Iroh DNS relay.  A full implementation requires a NEDNSProxyProvider
        // extension (separate binary) which listens on port 53.  For now we
        // leave DNS as-is; web traffic still works because DNS resolution
        // happens before the TCP connection (which is what SOCKS5 intercepts).
        //
        // Roadmap: add a NEDNSProxyProvider target + DnsQuery Iroh relay.

        setTunnelNetworkSettings(settings) { error in
            if let error = error {
                os_log(.error, log: log, "setTunnelNetworkSettings error: %{public}@",
                       error.localizedDescription)
            }
            completion(error)
        }
    }
}

// MARK: - Helpers

private func vpnError(_ code: Int, _ message: String) -> NSError {
    NSError(
        domain: "com.kerr.app.vpn",
        code: code,
        userInfo: [NSLocalizedDescriptionKey: message]
    )
}
