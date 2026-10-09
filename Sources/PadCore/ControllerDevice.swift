/// What a connected pad can do beyond reporting input.
/// The UI greys out features a device does not advertise.
public struct Capabilities: OptionSet, Sendable, Hashable {
    public let rawValue: UInt32
    public init(rawValue: UInt32) { self.rawValue = rawValue }

    public static let rumble = Capabilities(rawValue: 1 << 0)
    /// Player indicator such as the Xbox 360 ring of four LEDs.
    public static let playerIndicator = Capabilities(rawValue: 1 << 1)
}

public struct ControllerInfo: Sendable, Hashable, Identifiable {
    /// Stable for as long as the device stays plugged in (e.g. USB location ID).
    public let id: String
    public let name: String
    public let vendorID: Int
    public let productID: Int
    /// Which backend found it, e.g. "usb".
    public let backend: String
    public let capabilities: Capabilities
    public let buttons: [PadButton]
    public let axes: [PadAxis]

    public init(id: String, name: String, vendorID: Int, productID: Int, backend: String,
                capabilities: Capabilities, buttons: [PadButton] = PadButton.allCases, axes: [PadAxis] = PadAxis.allCases) {
        self.id = id
        self.name = name
        self.vendorID = vendorID
        self.productID = productID
        self.backend = backend
        self.capabilities = capabilities
        self.buttons = buttons
        self.axes = axes
    }
}

public enum PlayerIndicator: Equatable, Sendable {
    case off
    /// Player 1...4, briefly flashing then steady.
    case player(Int)
    /// Quadrant 1...4 on immediately, no flash. For animations (clock sync, random).
    case quadrant(Int)
    case blinkAll
    case rotate
}

/// One connected pad. Implementations call `onState` on their own serial queue
/// for every input report; consumers must not block in it.
public protocol ControllerDevice: AnyObject {
    var info: ControllerInfo { get }
    var state: ControllerState { get }
    var onState: ((ControllerState) -> Void)? { get set }

    /// 0...1 per motor. `low` is the heavy left motor, `high` the light right one.
    func setRumble(low: Float, high: Float)
    func setPlayerIndicator(_ indicator: PlayerIndicator)
    func close()
}

public extension ControllerDevice {
    func setRumble(low: Float, high: Float) {}
    func setPlayerIndicator(_ indicator: PlayerIndicator) {}
}

public protocol ControllerBackendDelegate: AnyObject {
    func backend(_ backend: ControllerBackend, didConnect device: ControllerDevice)
    func backend(_ backend: ControllerBackend, didDisconnect device: ControllerDevice)
    func backend(_ backend: ControllerBackend, didFail message: String)
}

/// A source of devices: raw USB, IOHID, GameController.framework, ...
/// Adding support for a new family of pads means adding a backend (or a driver
/// inside an existing one); nothing downstream changes.
public protocol ControllerBackend: AnyObject {
    var name: String { get }
    func start(delegate: ControllerBackendDelegate)
    func stop()
}
