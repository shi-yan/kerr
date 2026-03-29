import SwiftUI
import VisionKit
import AVFoundation

// MARK: - QR scanner sheet

struct QRScannerSheet: View {
    let onScanned: (String) -> Void
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            Group {
                if DataScannerViewController.isSupported {
                    DataScannerRepresentable { payload in
                        onScanned(payload)
                        dismiss()
                    }
                } else {
                    VStack(spacing: 16) {
                        Spacer()
                        Image(systemName: "camera.fill")
                            .font(.system(size: 48))
                            .foregroundColor(.secondary)
                        Text("QR scanning not available")
                            .font(.headline)
                        Text("This device does not support the camera scanner.")
                            .font(.subheadline)
                            .foregroundColor(.secondary)
                            .multilineTextAlignment(.center)
                            .padding(.horizontal, 40)
                        Spacer()
                    }
                }
            }
            .navigationTitle("Scan QR Code")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
            }
        }
    }
}

// MARK: - UIViewControllerRepresentable wrapper

struct DataScannerRepresentable: UIViewControllerRepresentable {
    let onScanned: (String) -> Void

    func makeUIViewController(context: Context) -> DataScannerViewController {
        let vc = DataScannerViewController(
            recognizedDataTypes: [.barcode(symbologies: [.qr])],
            qualityLevel: .accurate,
            recognizesMultipleItems: false,
            isHighlightingEnabled: true
        )
        vc.delegate = context.coordinator
        try? vc.startScanning()
        return vc
    }

    func updateUIViewController(_ vc: DataScannerViewController, context: Context) {}

    func makeCoordinator() -> Coordinator { Coordinator(self) }

    class Coordinator: NSObject, DataScannerViewControllerDelegate {
        let parent: DataScannerRepresentable
        private var hasScanned = false

        init(_ parent: DataScannerRepresentable) { self.parent = parent }

        func dataScanner(
            _ dataScanner: DataScannerViewController,
            didAdd addedItems: [RecognizedItem],
            allItems: [RecognizedItem]
        ) {
            guard !hasScanned else { return }
            for item in addedItems {
                if case .barcode(let barcode) = item,
                   let payload = barcode.payloadStringValue {
                    hasScanned = true
                    DispatchQueue.main.async {
                        self.parent.onScanned(payload)
                    }
                    return
                }
            }
        }
    }
}
