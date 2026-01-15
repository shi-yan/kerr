import SwiftUI

struct ConnectionView: View {
    @ObservedObject var connectionManager: ConnectionManager
    @State private var connectionString = ""
    @State private var showScanner = false

    var body: some View {
        VStack(spacing: 20) {
            Image(systemName: "network")
                .font(.system(size: 80))
                .foregroundColor(.blue)
                .padding(.bottom, 20)

            Text("Connect to Kerr Server")
                .font(.title)
                .fontWeight(.bold)

            Text(connectionManager.connectionStatus)
                .font(.subheadline)
                .foregroundColor(.secondary)

            if let error = connectionManager.errorMessage {
                Text(error)
                    .font(.caption)
                    .foregroundColor(.red)
                    .padding()
                    .background(Color.red.opacity(0.1))
                    .cornerRadius(8)
            }

            VStack(alignment: .leading, spacing: 8) {
                Text("Connection String")
                    .font(.caption)
                    .foregroundColor(.secondary)

                TextField("Paste connection string here", text: $connectionString)
                    .textFieldStyle(RoundedBorderTextFieldStyle())
                    .autocapitalization(.none)
                    .disableAutocorrection(true)
            }
            .padding(.horizontal)

            HStack(spacing: 15) {
                Button(action: {
                    showScanner = true
                }) {
                    Label("Scan QR", systemImage: "qrcode.viewfinder")
                        .frame(maxWidth: .infinity)
                }
                .buttonStyle(.bordered)

                Button(action: {
                    // Try to paste from clipboard
                    if let string = UIPasteboard.general.string {
                        connectionString = string
                    }
                }) {
                    Label("Paste", systemImage: "doc.on.clipboard")
                        .frame(maxWidth: .infinity)
                }
                .buttonStyle(.bordered)
            }
            .padding(.horizontal)

            Button(action: {
                connectionManager.connect(connectionString: connectionString)
            }) {
                Text("Connect")
                    .fontWeight(.semibold)
                    .frame(maxWidth: .infinity)
                    .padding()
                    .background(connectionString.isEmpty ? Color.gray : Color.blue)
                    .foregroundColor(.white)
                    .cornerRadius(10)
            }
            .disabled(connectionString.isEmpty)
            .padding(.horizontal)

            Spacer()
        }
        .padding()
        .navigationTitle("Kerr")
        .sheet(isPresented: $showScanner) {
            QRScannerView { scannedString in
                connectionString = scannedString
                showScanner = false
            }
        }
    }
}

struct QRScannerView: View {
    let onScanned: (String) -> Void

    var body: some View {
        VStack {
            Text("QR Scanner")
                .font(.headline)
                .padding()

            // TODO: Implement QR code scanner using AVFoundation
            Text("QR Scanner coming soon...")
                .foregroundColor(.secondary)

            Button("Cancel") {
                // This will be dismissed by the parent
            }
            .padding()
        }
    }
}

struct ConnectionView_Previews: PreviewProvider {
    static var previews: some View {
        ConnectionView(connectionManager: ConnectionManager())
    }
}
