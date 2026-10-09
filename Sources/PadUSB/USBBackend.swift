import Foundation
import IOKit
import IOUSBHost
import PadCore

/// Finds Xbox 360 protocol pads on USB, with hotplug.
///
/// A pad is claimed if its VID/PID is in `KnownDevices`, or if macOS has
/// already configured it and one of its interfaces carries the Xbox 360 class
/// triple. Unknown vendor devices are never opened, so we cannot steal e.g. an
/// audio interface that also uses a vendor-specific class.
public final class USBBackend: ControllerBackend {
    public let name = "usb"

    private let queue = DispatchQueue(label: "padbridge.usb.backend")
    private weak var delegate: ControllerBackendDelegate?
    private var notifyPort: IONotificationPortRef?
    private var iterators: [io_iterator_t] = []
    private var devices: [UInt64: Xbox360Device] = [:]

    public init() {}

    public func start(delegate: ControllerBackendDelegate) {
        queue.async { self.startOnQueue(delegate: delegate) }
    }

    public func stop() {
        queue.sync {
            iterators.forEach { IOObjectRelease($0) }
            iterators.removeAll()
            if let notifyPort { IONotificationPortDestroy(notifyPort) }
            notifyPort = nil
            devices.values.forEach { $0.close() }
            devices.removeAll()
        }
    }

    private func startOnQueue(delegate: ControllerBackendDelegate) {
        guard notifyPort == nil else { return }
        self.delegate = delegate
        guard let port = IONotificationPortCreate(kIOMainPortDefault) else {
            delegate.backend(self, didFail: "IONotificationPortCreate failed")
            return
        }
        IONotificationPortSetDispatchQueue(port, queue)
        notifyPort = port

        let refcon = Unmanaged.passUnretained(self).toOpaque()
        for (type, handler) in [(kIOFirstMatchNotification, Self.matched), (kIOTerminatedNotification, Self.terminated)] {
            var iterator: io_iterator_t = 0
            let kr = IOServiceAddMatchingNotification(port, type, IOServiceMatching("IOUSBHostDevice"),
                                                      handler, refcon, &iterator)
            guard kr == KERN_SUCCESS else {
                delegate.backend(self, didFail: "IOServiceAddMatchingNotification failed: \(kr)")
                continue
            }
            iterators.append(iterator)
            // Draining handles already-present devices and arms the notification.
            handler(refcon, iterator)
        }
    }

    private static let matched: IOServiceMatchingCallback = { refcon, iterator in
        let backend = Unmanaged<USBBackend>.fromOpaque(refcon!).takeUnretainedValue()
        for service in Registry.drain(iterator) {
            backend.deviceAppeared(service)
            IOObjectRelease(service)
        }
    }

    private static let terminated: IOServiceMatchingCallback = { refcon, iterator in
        let backend = Unmanaged<USBBackend>.fromOpaque(refcon!).takeUnretainedValue()
        for service in Registry.drain(iterator) {
            backend.deviceDisappeared(Registry.entryID(service))
            IOObjectRelease(service)
        }
    }

    private func deviceAppeared(_ service: io_service_t) {
        guard let vendorID = Registry.int(service, "idVendor"),
              let productID = Registry.int(service, "idProduct") else { return }
        let known = KnownDevices.xbox360Entry(vendorID: vendorID, productID: productID)
        guard known != nil || hasConfiguredGamepadInterface(service) else { return }

        let location = Registry.int(service, "locationID") ?? Int(truncatingIfNeeded: Registry.entryID(service))
        let productString: String? = Registry.property(service, "kUSBProductString") ?? Registry.property(service, "USB Product Name")
        let info = ControllerInfo(
            id: String(format: "usb-%08x", location),
            name: known?.name ?? productString ?? "Xbox 360 compatible pad",
            vendorID: vendorID,
            productID: productID,
            backend: name,
            capabilities: [.rumble, .playerIndicator]
        )

        IOObjectRetain(service)
        DispatchQueue.global(qos: .userInitiated).async {
            defer { IOObjectRelease(service) }
            do {
                let device = try Xbox360Device.open(service: service, info: info)
                self.queue.async {
                    self.devices[device.registryID] = device
                    device.setPlayerIndicator(.player(self.devices.count))
                    self.delegate?.backend(self, didConnect: device)
                }
            } catch {
                self.queue.async {
                    self.delegate?.backend(self, didFail: "\(info.name) (\(String(format: "%04x:%04x", vendorID, productID))): \(error)")
                }
            }
        }
    }

    private func deviceDisappeared(_ registryID: UInt64) {
        guard let device = devices.removeValue(forKey: registryID) else { return }
        device.invalidate()
        delegate?.backend(self, didDisconnect: device)
    }

    private func hasConfiguredGamepadInterface(_ service: io_service_t) -> Bool {
        guard let interface = Xbox360Device.gamepadInterface(of: service) else { return false }
        IOObjectRelease(interface)
        return true
    }
}
