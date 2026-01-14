import SwiftUI
// import SwiftTerm  // You'll add this after adding SwiftTerm via Swift Package Manager

struct TerminalView: View {
    @ObservedObject var connectionManager: ConnectionManager
    @StateObject private var terminalController = TerminalController()

    var body: some View {
        VStack {
            if terminalController.isActive {
                // TODO: Integrate SwiftTerm here
                // TerminalViewRepresentable(controller: terminalController)
                //     .edgesIgnoringSafeArea(.all)

                // Placeholder until SwiftTerm is integrated
                VStack {
                    ScrollView {
                        Text(terminalController.outputBuffer)
                            .font(.system(.body, design: .monospaced))
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .padding()
                    }
                    .background(Color.black)
                    .foregroundColor(.green)

                    HStack {
                        TextField("Enter command", text: $terminalController.inputBuffer)
                            .textFieldStyle(RoundedBorderTextFieldStyle())
                            .autocapitalization(.none)
                            .disableAutocorrection(true)

                        Button("Send") {
                            terminalController.sendInput()
                        }
                    }
                    .padding()
                }
            } else {
                VStack(spacing: 20) {
                    Image(systemName: "terminal")
                        .font(.system(size: 60))
                        .foregroundColor(.green)

                    Text("Terminal")
                        .font(.title)

                    if let error = terminalController.errorMessage {
                        Text(error)
                            .foregroundColor(.red)
                            .padding()
                    }

                    Button(action: {
                        startTerminal()
                    }) {
                        Label("Start Shell", systemImage: "play.circle")
                            .frame(maxWidth: .infinity)
                            .padding()
                            .background(Color.green)
                            .foregroundColor(.white)
                            .cornerRadius(10)
                    }
                    .padding()
                }
            }
        }
        .navigationTitle("Terminal")
        .navigationBarTitleDisplayMode(.inline)
    }

    private func startTerminal() {
        guard let session = connectionManager.getSession() else {
            terminalController.errorMessage = "Not connected"
            return
        }

        do {
            try terminalController.start(session: session)
        } catch {
            terminalController.errorMessage = "Failed to start terminal: \(error.localizedDescription)"
        }
    }
}

class TerminalController: ObservableObject, ShellCallback {
    @Published var isActive = false
    @Published var outputBuffer = ""
    @Published var inputBuffer = ""
    @Published var errorMessage: String?

    private var shellSession: ShellSession?

    func start(session: Session) throws {
        // Start shell with self as callback
        shellSession = try session.startShell(callback: self)
        isActive = true
        errorMessage = nil
        outputBuffer = "Shell started...\n"
    }

    func sendInput() {
        guard let shell = shellSession, !inputBuffer.isEmpty else {
            return
        }

        do {
            try shell.sendInput(data: inputBuffer + "\n")
            inputBuffer = ""
        } catch {
            errorMessage = "Failed to send input: \(error.localizedDescription)"
        }
    }

    func resize(cols: UInt16, rows: UInt16) {
        guard let shell = shellSession else {
            return
        }

        do {
            try shell.resize(cols: cols, rows: rows)
        } catch {
            errorMessage = "Failed to resize: \(error.localizedDescription)"
        }
    }

    func stop() {
        shellSession?.close()
        shellSession = nil
        isActive = false
    }

    // MARK: - ShellCallback

    func onOutput(data: String) {
        DispatchQueue.main.async {
            self.outputBuffer += data
        }
    }

    func onError(message: String) {
        DispatchQueue.main.async {
            self.errorMessage = message
            self.outputBuffer += "\nError: \(message)\n"
        }
    }

    func onClose() {
        DispatchQueue.main.async {
            self.isActive = false
            self.outputBuffer += "\nShell closed.\n"
        }
    }
}

// TODO: When SwiftTerm is integrated, create this representable
/*
struct TerminalViewRepresentable: UIViewRepresentable {
    let controller: TerminalController

    func makeUIView(context: Context) -> TerminalView {
        let terminalView = TerminalView(frame: .zero)
        terminalView.terminalDelegate = context.coordinator
        return terminalView
    }

    func updateUIView(_ uiView: TerminalView, context: Context) {
        // Update as needed
    }

    func makeCoordinator() -> Coordinator {
        Coordinator(controller: controller)
    }

    class Coordinator: NSObject, TerminalViewDelegate {
        let controller: TerminalController

        init(controller: TerminalController) {
            self.controller = controller
        }

        func send(source: TerminalView, data: ArraySlice<UInt8>) {
            let string = String(bytes: data, encoding: .utf8) ?? ""
            controller.inputBuffer = string
            controller.sendInput()
        }

        func sizeChanged(source: TerminalView, newCols: Int, newRows: Int) {
            controller.resize(cols: UInt16(newCols), rows: UInt16(newRows))
        }
    }
}
*/
