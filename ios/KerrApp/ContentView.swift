import SwiftUI

struct ContentView: View {
    @StateObject private var connectionManager = ConnectionManager()

    var body: some View {
        if connectionManager.isConnected {
            MainTabView(connectionManager: connectionManager)
        } else {
            NavigationStack {
                ConnectionView(connectionManager: connectionManager)
            }
        }
    }
}

struct MainTabView: View {
    @ObservedObject var connectionManager: ConnectionManager
    @State private var selectedTab = 0

    var body: some View {
        TabView(selection: $selectedTab) {
            NavigationStack {
                FileBrowserView(connectionManager: connectionManager)
            }
            .tabItem {
                Label("Files", systemImage: "folder")
            }
            .tag(0)

            NavigationStack {
                TerminalView(connectionManager: connectionManager)
            }
            .tabItem {
                Label("Terminal", systemImage: "terminal")
            }
            .tag(1)

            // ── VPN tab ──────────────────────────────────────────────────
            VpnView(connectionManager: connectionManager)
                .tabItem {
                    Label("VPN", systemImage: "shield.fill")
                }
                .tag(2)

            Color.clear
                .tabItem {
                    Label("Exit", systemImage: "xmark.circle")
                }
                .tag(3)
        }
        .onChange(of: selectedTab) { newTab in
            if newTab == 3 {
                connectionManager.disconnect()
            }
        }
    }
}

struct ContentView_Previews: PreviewProvider {
    static var previews: some View {
        ContentView()
    }
}
