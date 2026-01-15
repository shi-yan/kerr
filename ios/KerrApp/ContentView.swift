import SwiftUI

struct ContentView: View {
    @StateObject private var connectionManager = ConnectionManager()

    var body: some View {
        NavigationView {
            if connectionManager.isConnected {
                MainTabView(connectionManager: connectionManager)
            } else {
                ConnectionView(connectionManager: connectionManager)
            }
        }
    }
}

struct MainTabView: View {
    @ObservedObject var connectionManager: ConnectionManager

    var body: some View {
        TabView {
            FileBrowserView(connectionManager: connectionManager)
                .tabItem {
                    Label("Files", systemImage: "folder")
                }

            TerminalView(connectionManager: connectionManager)
                .tabItem {
                    Label("Terminal", systemImage: "terminal")
                }
        }
    }
}

struct ContentView_Previews: PreviewProvider {
    static var previews: some View {
        ContentView()
    }
}
