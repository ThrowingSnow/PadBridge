import Foundation
import IOKit

/// Thin helpers over the IORegistry C API.
enum Registry {
    static func property<T>(_ entry: io_registry_entry_t, _ key: String) -> T? {
        IORegistryEntryCreateCFProperty(entry, key as CFString, kCFAllocatorDefault, 0)?
            .takeRetainedValue() as? T
    }

    static func int(_ entry: io_registry_entry_t, _ key: String) -> Int? {
        let number: NSNumber? = property(entry, key)
        return number?.intValue
    }

    static func entryID(_ entry: io_registry_entry_t) -> UInt64 {
        var id: UInt64 = 0
        IORegistryEntryGetRegistryEntryID(entry, &id)
        return id
    }

    /// Children of `entry` in the service plane. Caller releases the returned objects.
    static func children(_ entry: io_registry_entry_t) -> [io_registry_entry_t] {
        var iterator: io_iterator_t = 0
        guard IORegistryEntryGetChildIterator(entry, kIOServicePlane, &iterator) == KERN_SUCCESS else { return [] }
        defer { IOObjectRelease(iterator) }
        return drain(iterator)
    }

    /// Takes every object out of an iterator (which also re-arms IOKit notifications).
    static func drain(_ iterator: io_iterator_t) -> [io_object_t] {
        var out: [io_object_t] = []
        while case let object = IOIteratorNext(iterator), object != 0 { out.append(object) }
        return out
    }

    static func conforms(_ entry: io_registry_entry_t, to className: String) -> Bool {
        IOObjectConformsTo(entry, className) != 0
    }
}

