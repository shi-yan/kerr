import SwiftUI
import SwiftTerm

// MARK: - Terminal View (SwiftUI)

struct TerminalView: View {
    @ObservedObject var connectionManager: ConnectionManager
    @StateObject private var controller = TerminalController()

    var body: some View {
        if controller.isActive {
            ZStack(alignment: .topTrailing) {
                SwiftTermRepresentable(controller: controller)

                Button(action: { controller.stop() }) {
                    Text("Stop")
                        .font(.caption.weight(.semibold))
                        .foregroundColor(.white)
                        .padding(.horizontal, 12)
                        .padding(.vertical, 6)
                        .background(.black.opacity(0.7))
                        .clipShape(Capsule())
                }
                .padding(.top, 8)
                .padding(.trailing, 12)
            }
            .toolbar(.hidden, for: .navigationBar)
        } else {
            idleView
                .navigationTitle("Terminal")
                .navigationBarTitleDisplayMode(.inline)
        }
    }

    private var idleView: some View {
        VStack(spacing: 24) {
            Spacer()

            Image(systemName: "terminal.fill")
                .font(.system(size: 64))
                .foregroundColor(.green)

            Text("Remote Shell")
                .font(.title2)
                .fontWeight(.semibold)

            if let err = controller.errorMessage {
                Text(err)
                    .font(.callout)
                    .foregroundColor(.red)
                    .multilineTextAlignment(.center)
                    .padding()
                    .background(Color.red.opacity(0.1))
                    .cornerRadius(10)
                    .padding(.horizontal)
            }

            Button(action: startShell) {
                Label("Start Shell", systemImage: "play.circle.fill")
                    .font(.headline)
                    .frame(maxWidth: .infinity)
                    .padding()
                    .background(Color.green)
                    .foregroundColor(.white)
                    .cornerRadius(12)
            }
            .padding(.horizontal)

            Spacer()
        }
    }

    private func startShell() {
        guard let session = connectionManager.getSession() else {
            controller.errorMessage = "Not connected"
            return
        }
        do {
            try controller.start(session: session)
        } catch {
            controller.errorMessage = error.localizedDescription
        }
    }
}

// MARK: - UIViewRepresentable

struct SwiftTermRepresentable: UIViewRepresentable {
    let controller: TerminalController

    func makeUIView(context: Context) -> SwiftTerm.TerminalView {
        let tv = SwiftTerm.TerminalView(frame: .zero)
        tv.terminalDelegate = context.coordinator
        tv.nativeBackgroundColor = .black
        controller.attachTerminalView(tv)
        // Become first responder so the software keyboard appears
        DispatchQueue.main.async {
            tv.becomeFirstResponder()
        }
        return tv
    }

    func updateUIView(_ uiView: SwiftTerm.TerminalView, context: Context) {}

    func makeCoordinator() -> Coordinator {
        Coordinator(controller: controller)
    }

    final class Coordinator: NSObject, SwiftTerm.TerminalViewDelegate {
        let controller: TerminalController

        init(controller: TerminalController) {
            self.controller = controller
        }

        // User typed on the keyboard — forward raw bytes to the remote shell.
        func send(source: SwiftTerm.TerminalView, data: ArraySlice<UInt8>) {
            guard let shell = controller.shellSession else { return }
            if let str = String(bytes: data, encoding: .utf8) {
                try? shell.sendInput(data: str)
            }
        }

        // Terminal was resized — tell the remote PTY.
        func sizeChanged(source: SwiftTerm.TerminalView, newCols: Int, newRows: Int) {
            try? controller.shellSession?.resize(cols: UInt16(newCols), rows: UInt16(newRows))
        }

        func setTerminalTitle(source: SwiftTerm.TerminalView, title: String) {}
        func hostCurrentDirectoryUpdate(source: SwiftTerm.TerminalView, directory: String?) {}
        func requestOpenLink(source: SwiftTerm.TerminalView, link: String, params: [String: String]) {}

        func bell(source: SwiftTerm.TerminalView) {
            UIImpactFeedbackGenerator(style: .medium).impactOccurred()
        }

        func clipboardCopy(source: SwiftTerm.TerminalView, content: Data) {
            UIPasteboard.general.string = String(data: content, encoding: .utf8)
        }

        func scrolled(source: SwiftTerm.TerminalView, position: Double) {}
        func rangeChanged(source: SwiftTerm.TerminalView, startY: Int, endY: Int) {}
    }
}

// MARK: - Terminal Controller

class TerminalController: ObservableObject, ShellCallback, @unchecked Sendable {
    @Published var isActive = false
    @Published var errorMessage: String?

    private(set) var shellSession: ShellSession?
    private weak var terminalView: SwiftTerm.TerminalView?

    func attachTerminalView(_ tv: SwiftTerm.TerminalView) {
        terminalView = tv
    }

    func start(session: Session) throws {
        let shell = try session.startShell(callback: self)
        shellSession = shell
        // Re-assign shellSession inside the dispatch too: startShell() closes
        // the previous shell internally, which calls onClose() → dispatches
        // shellSession = nil to the main queue. That dispatch fires after this
        // function returns, wiping out the new session. Re-assigning here
        // (inside a later dispatch) ensures we always win.
        DispatchQueue.main.async { [weak self] in
            self?.shellSession = shell
            self?.isActive = true
            self?.errorMessage = nil
        }
    }

    func stop() {
        // Clear Swift state immediately on the main thread.
        // Do NOT call shellSession?.close() here — that calls Rust block_on
        // from the main thread, which panics if another block_on is already
        // active or follows immediately. The session.disconnect() call in
        // ConnectionManager handles Rust cleanup on a background thread.
        shellSession = nil
        terminalView = nil
        isActive = false
    }

    // MARK: - ShellCallback

    // Raw terminal output — feed bytes directly so ANSI escape codes are handled by SwiftTerm.
    func onOutput(data: String) {
        let bytes = ArraySlice(data.utf8)
        DispatchQueue.main.async { [weak self] in
            self?.terminalView?.feed(byteArray: bytes)
        }
    }

    func onError(message: String) {
        let text = "\r\n\u{1B}[31mError: \(message)\u{1B}[0m\r\n"
        DispatchQueue.main.async { [weak self] in
            self?.terminalView?.feed(text: text)
            self?.errorMessage = message
        }
    }

    func onClose() {
        DispatchQueue.main.async { [weak self] in
            self?.terminalView?.feed(text: "\r\n[Shell closed]\r\n")
            self?.isActive = false
            // Do NOT nil shellSession here. startShell() closes the previous
            // shell internally and this dispatch fires after the new session
            // is already assigned — clearing it would break reconnection.
            // stop() handles the explicit nil when the user exits.
        }
    }
}
