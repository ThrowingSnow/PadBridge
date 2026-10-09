import Foundation
import IOKit
import IOUSBHost
import PadCore

/// A wired Xbox 360 pad driven from user space via IOUSBHost.
/// No kext: we open the USB device ourselves, pick the gamepad interface and
/// read its interrupt endpoint.
final class Xbox360Device: ControllerDevice {
    enum OpenError: Error, CustomStringConvertible {
        case noGamepadInterface
        case missingEndpoint

        var description: String {
            switch self {
            case .noGamepadInterface: return "no Xbox 360 gamepad interface found"
            case .missingEndpoint: return "gamepad interface has no interrupt IN endpoint"
            }
        }
    }

    let info: ControllerInfo
    let registryID: UInt64
    var onState: ((ControllerState) -> Void)?
    private(set) var state = ControllerState()

    private let queue: DispatchQueue
    private let device: IOUSBHostDevice
    private let interface: IOUSBHostInterface
    private let inPipe: IOUSBHostPipe
    private let outPipe: IOUSBHostPipe?
    private var closed = false
    private var lastRumble: [UInt8] = []

    /// Two reads in flight so a report is never missed while one is being parsed.
    private static let readsInFlight = 2

    /// Opens the device and starts reading. Blocks briefly while interfaces
    /// appear after configuration, so call it off the main thread.
    static func open(service: io_service_t, info: ControllerInfo) throws -> Xbox360Device {
        let queue = DispatchQueue(label: "padbridge.usb.\(info.id)", qos: .userInteractive)
        let device = try IOUSBHostDevice(__ioService: service, options: [], queue: queue, interestHandler: nil)
        do {
            if device.configurationDescriptor == nil {
                try device.__configure(withValue: 1, matchInterfaces: true)
            }
            let interfaceService = try waitForGamepadInterface(of: service)
            defer { IOObjectRelease(interfaceService) }
            let interface = try IOUSBHostInterface(__ioService: interfaceService, options: [], queue: queue, interestHandler: nil)
            do {
                let (inAddress, outAddress) = try endpoints(of: interface)
                let inPipe = try interface.copyPipe(withAddress: Int(inAddress))
                let outPipe = try outAddress.map { try interface.copyPipe(withAddress: Int($0)) }
                return Xbox360Device(info: info, registryID: Registry.entryID(service), queue: queue,
                                     device: device, interface: interface, inPipe: inPipe, outPipe: outPipe)
            } catch {
                interface.destroy()
                throw error
            }
        } catch {
            device.destroy()
            throw error
        }
    }

    private init(info: ControllerInfo, registryID: UInt64, queue: DispatchQueue, device: IOUSBHostDevice,
                 interface: IOUSBHostInterface, inPipe: IOUSBHostPipe, outPipe: IOUSBHostPipe?) {
        self.info = info
        self.registryID = registryID
        self.queue = queue
        self.device = device
        self.interface = interface
        self.inPipe = inPipe
        self.outPipe = outPipe
        queue.async {
            for _ in 0..<Self.readsInFlight { self.enqueueRead() }
        }
    }

    // MARK: Input

    private func enqueueRead() {
        guard !closed, let buffer = NSMutableData(length: 32) else { return }
        do {
            try inPipe.enqueueIORequest(with: buffer, completionTimeout: 0) { [weak self] status, length in
                self?.readCompleted(buffer: buffer, status: status, length: length)
            }
        } catch {
            // Device went away between reads; termination notification handles cleanup.
        }
    }

    private func readCompleted(buffer: NSMutableData, status: IOReturn, length: Int) {
        guard !closed else { return }
        if status == kIOReturnSuccess {
            let bytes = UnsafeBufferPointer(start: buffer.bytes.assumingMemoryBound(to: UInt8.self), count: length)
            if let next = Xbox360Protocol.parseInput(Array(bytes)), next != state {
                state = next
                onState?(next)
            }
        } else if status == kIOReturnAborted || status == kIOReturnNoDevice || status == kIOReturnNotResponding {
            return
        }
        enqueueRead()
    }

    // MARK: Output

    func setRumble(low: Float, high: Float) {
        queue.async {
            let report = Xbox360Protocol.rumbleReport(low: low, high: high)
            guard report != self.lastRumble else { return }
            self.lastRumble = report
            self.write(report)
        }
    }

    func setPlayerIndicator(_ indicator: PlayerIndicator) {
        queue.async { self.write(Xbox360Protocol.ledReport(indicator)) }
    }

    private func write(_ report: [UInt8]) {
        guard !closed, let outPipe else { return }
        let data = NSMutableData(bytes: report, length: report.count)
        // The handler keeps `data` alive until the transfer is done.
        try? outPipe.enqueueIORequest(with: data, completionTimeout: 0) { _, _ in _ = data }
    }

    // MARK: Lifecycle

    func close() {
        queue.sync {
            guard !closed else { return }
            if outPipe != nil {
                // Leave the pad quiet: motors off, ring back to "searching".
                write(Xbox360Protocol.rumbleReport(low: 0, high: 0))
                write(Xbox360Protocol.ledReport(.blinkAll))
            }
            closed = true
            interface.destroy()
            device.destroy()
        }
    }

    /// Device was unplugged: the handles are already dead, just release them.
    func invalidate() {
        queue.sync {
            guard !closed else { return }
            closed = true
            interface.destroy()
            device.destroy()
        }
    }

    // MARK: Discovery helpers

    /// Interfaces are published asynchronously after SET_CONFIGURATION.
    private static func waitForGamepadInterface(of service: io_service_t) throws -> io_service_t {
        for _ in 0..<20 {
            if let found = gamepadInterface(of: service) { return found }
            Thread.sleep(forTimeInterval: 0.05)
        }
        throw OpenError.noGamepadInterface
    }

    static func gamepadInterface(of service: io_service_t) -> io_service_t? {
        var found: io_service_t?
        for child in Registry.children(service) {
            if found == nil,
               Registry.conforms(child, to: "IOUSBHostInterface"),
               Registry.int(child, "bInterfaceClass") == Int(Xbox360Protocol.interfaceClass),
               Registry.int(child, "bInterfaceSubClass") == Int(Xbox360Protocol.interfaceSubclass),
               Registry.int(child, "bInterfaceProtocol") == Int(Xbox360Protocol.interfaceProtocol) {
                found = child
            } else {
                IOObjectRelease(child)
            }
        }
        return found
    }

    /// First interrupt IN and (optional) interrupt OUT endpoint of the interface.
    private static func endpoints(of interface: IOUSBHostInterface) throws -> (UInt8, UInt8?) {
        let config = interface.configurationDescriptor
        let iface = interface.interfaceDescriptor
        var inAddress: UInt8?
        var outAddress: UInt8?
        var current: UnsafePointer<IOUSBDescriptorHeader>? = UnsafeRawPointer(iface).assumingMemoryBound(to: IOUSBDescriptorHeader.self)
        while let endpoint = IOUSBGetNextEndpointDescriptor(config, iface, current) {
            if IOUSBGetEndpointType(endpoint) == UInt8(kIOUSBEndpointTypeInterrupt.rawValue) {
                let address = IOUSBGetEndpointAddress(endpoint)
                let isIn = IOUSBGetEndpointDirection(endpoint) == UInt8(kIOUSBEndpointDirectionIn.rawValue)
                if isIn, inAddress == nil { inAddress = address | 0x80 }
                if !isIn, outAddress == nil { outAddress = address }
            }
            current = UnsafeRawPointer(endpoint).assumingMemoryBound(to: IOUSBDescriptorHeader.self)
        }
        guard let inAddress else { throw OpenError.missingEndpoint }
        return (inAddress, outAddress)
    }
}
