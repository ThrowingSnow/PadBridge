/// Vendor/product IDs of wired pads that speak the Xbox 360 protocol.
///
/// Matching by ID lets us claim a pad before macOS has configured it. Pads that
/// are not listed still work if their interface advertises the Xbox 360 class
/// triple (see `Xbox360Protocol`), so this list only needs the common ones.
/// Subset of the table in Linux `xpad.c`.
enum KnownDevices {
    struct Entry {
        let vendorID: Int
        let productID: Int
        let name: String
    }

    static let xbox360: [Entry] = [
        Entry(vendorID: 0x045E, productID: 0x028E, name: "Xbox 360 Controller"),
        Entry(vendorID: 0x045E, productID: 0x028F, name: "Xbox 360 Controller (Play & Charge)"),
        Entry(vendorID: 0x046D, productID: 0xC21D, name: "Logitech Gamepad F310"),
        Entry(vendorID: 0x046D, productID: 0xC21E, name: "Logitech Gamepad F510"),
        Entry(vendorID: 0x046D, productID: 0xC21F, name: "Logitech Gamepad F710"),
        Entry(vendorID: 0x0738, productID: 0x4716, name: "Mad Catz Wired Xbox 360 Controller"),
        Entry(vendorID: 0x0738, productID: 0x4726, name: "Mad Catz Xbox 360 Controller"),
        Entry(vendorID: 0x1BAD, productID: 0xF016, name: "Mad Catz Xbox 360 Controller"),
        Entry(vendorID: 0x0E6F, productID: 0x0105, name: "HSM3 Xbox 360 Dancepad"),
        Entry(vendorID: 0x0E6F, productID: 0x0113, name: "Afterglow AX.1 Gamepad"),
        Entry(vendorID: 0x0E6F, productID: 0x0201, name: "Pelican PL-3601 'TSZ' Wired Xbox 360 Controller"),
        Entry(vendorID: 0x0E6F, productID: 0x0213, name: "Afterglow Gamepad for Xbox 360"),
        Entry(vendorID: 0x0F0D, productID: 0x000A, name: "Hori Co. DOA4 FightStick"),
        Entry(vendorID: 0x0F0D, productID: 0x000C, name: "Hori PadEX Turbo"),
        Entry(vendorID: 0x1532, productID: 0x0037, name: "Razer Sabertooth"),
        Entry(vendorID: 0x24C6, productID: 0x5300, name: "PowerA MINI PROEX Controller"),
        Entry(vendorID: 0x24C6, productID: 0x5303, name: "Xbox Airflo wired controller"),
    ]

    static func xbox360Entry(vendorID: Int, productID: Int) -> Entry? {
        xbox360.first { $0.vendorID == vendorID && $0.productID == productID }
    }
}
