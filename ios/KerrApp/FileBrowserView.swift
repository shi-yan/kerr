import SwiftUI

struct FileBrowserView: View {
    @ObservedObject var connectionManager: ConnectionManager
    @State private var currentPath = "/"
    @State private var files: [FileEntry] = []
    @State private var isLoading = false
    @State private var errorMessage: String?
    @State private var selectedFile: FileEntry?
    @State private var showFileActions = false

    var body: some View {
        VStack {
            // Path navigation bar
            HStack {
                Button(action: goBack) {
                    Image(systemName: "chevron.left")
                        .font(.headline)
                }
                .disabled(currentPath == "/")

                ScrollView(.horizontal, showsIndicators: false) {
                    Text(currentPath)
                        .font(.system(.body, design: .monospaced))
                        .lineLimit(1)
                }

                Spacer()

                Button(action: refresh) {
                    Image(systemName: "arrow.clockwise")
                }
            }
            .padding()
            .background(Color(.systemGray6))

            if isLoading {
                ProgressView("Loading...")
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else if let error = errorMessage {
                VStack {
                    Image(systemName: "exclamationmark.triangle")
                        .font(.system(size: 50))
                        .foregroundColor(.orange)
                    Text(error)
                        .foregroundColor(.secondary)
                        .padding()
                    Button("Retry") {
                        loadFiles()
                    }
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                List(files, id: \.path) { file in
                    FileRowView(file: file)
                        .contentShape(Rectangle())
                        .onTapGesture {
                            if file.isDir {
                                navigateToDirectory(file.path)
                            } else {
                                selectedFile = file
                                showFileActions = true
                            }
                        }
                }
                .listStyle(PlainListStyle())
            }
        }
        .navigationTitle("Files")
        .navigationBarTitleDisplayMode(.inline)
        .onAppear {
            loadFiles()
        }
        .sheet(isPresented: $showFileActions) {
            if let file = selectedFile {
                FileActionsView(
                    file: file,
                    connectionManager: connectionManager,
                    onDismiss: { showFileActions = false }
                )
            }
        }
    }

    private func loadFiles() {
        isLoading = true
        errorMessage = nil

        DispatchQueue.global(qos: .userInitiated).async {
            do {
                let browser = try connectionManager.getFileBrowser()
                let entries = try browser.listDir(path: currentPath)

                DispatchQueue.main.async {
                    self.files = entries.sorted { f1, f2 in
                        // Directories first, then alphabetically
                        if f1.isDir != f2.isDir {
                            return f1.isDir
                        }
                        return f1.name.localizedCaseInsensitiveCompare(f2.name) == .orderedAscending
                    }
                    self.isLoading = false
                }
            } catch {
                DispatchQueue.main.async {
                    self.errorMessage = "Failed to load files: \(error.localizedDescription)"
                    self.isLoading = false
                }
            }
        }
    }

    private func navigateToDirectory(_ path: String) {
        currentPath = path
        loadFiles()
    }

    private func goBack() {
        if currentPath == "/" {
            return
        }

        // Remove trailing slash
        var path = currentPath
        if path.hasSuffix("/") {
            path.removeLast()
        }

        // Get parent directory
        if let lastSlash = path.lastIndex(of: "/") {
            currentPath = String(path[...lastSlash])
            if currentPath.isEmpty {
                currentPath = "/"
            }
        } else {
            currentPath = "/"
        }

        loadFiles()
    }

    private func refresh() {
        loadFiles()
    }
}

struct FileRowView: View {
    let file: FileEntry

    var body: some View {
        HStack {
            Image(systemName: file.isDir ? "folder.fill" : "doc.fill")
                .foregroundColor(file.isDir ? .blue : .gray)
                .frame(width: 30)

            VStack(alignment: .leading, spacing: 4) {
                Text(file.name)
                    .font(.body)
                    .foregroundColor(file.isHidden ? .secondary : .primary)

                if let metadata = file.metadata {
                    Text(formatFileSize(metadata.size))
                        .font(.caption)
                        .foregroundColor(.secondary)
                }
            }

            Spacer()

            if !file.isDir {
                Image(systemName: "chevron.right")
                    .font(.caption)
                    .foregroundColor(.secondary)
            }
        }
        .padding(.vertical, 4)
    }

    private func formatFileSize(_ bytes: UInt64) -> String {
        let formatter = ByteCountFormatter()
        formatter.countStyle = .file
        return formatter.string(fromByteCount: Int64(bytes))
    }
}

struct FileActionsView: View {
    let file: FileEntry
    @ObservedObject var connectionManager: ConnectionManager
    let onDismiss: () -> Void

    @State private var isDownloading = false
    @State private var downloadProgress: Double = 0
    @State private var downloadError: String?

    var body: some View {
        NavigationView {
            VStack(spacing: 20) {
                Image(systemName: "doc.fill")
                    .font(.system(size: 60))
                    .foregroundColor(.blue)

                Text(file.name)
                    .font(.headline)

                if let metadata = file.metadata {
                    Text(formatFileSize(metadata.size))
                        .font(.subheadline)
                        .foregroundColor(.secondary)
                }

                if isDownloading {
                    ProgressView(value: downloadProgress)
                        .padding()
                } else if let error = downloadError {
                    Text(error)
                        .foregroundColor(.red)
                        .padding()
                }

                Button(action: downloadFile) {
                    Label("Download", systemImage: "arrow.down.circle")
                        .frame(maxWidth: .infinity)
                        .padding()
                        .background(Color.blue)
                        .foregroundColor(.white)
                        .cornerRadius(10)
                }
                .disabled(isDownloading)
                .padding()

                Spacer()
            }
            .padding()
            .navigationTitle("File Actions")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Close") {
                        onDismiss()
                    }
                }
            }
        }
    }

    private func downloadFile() {
        isDownloading = true
        downloadError = nil

        DispatchQueue.global(qos: .userInitiated).async {
            do {
                let browser = try connectionManager.getFileBrowser()
                let data = try browser.downloadFile(path: file.path)

                // Save to Files app
                let tempURL = FileManager.default.temporaryDirectory.appendingPathComponent(file.name)
                try data.write(to: tempURL)

                DispatchQueue.main.async {
                    isDownloading = false
                    // TODO: Show share sheet to save file
                    onDismiss()
                }
            } catch {
                DispatchQueue.main.async {
                    isDownloading = false
                    downloadError = "Download failed: \(error.localizedDescription)"
                }
            }
        }
    }

    private func formatFileSize(_ bytes: UInt64) -> String {
        let formatter = ByteCountFormatter()
        formatter.countStyle = .file
        return formatter.string(fromByteCount: Int64(bytes))
    }
}
