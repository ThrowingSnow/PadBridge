//! Lists what hidapi sees and tries to open and read Sony pads.
//! Run: hid_probe.exe (Windows) / cargo run --example hid_probe

fn main() {
    let api = match hidapi::HidApi::new() {
        Ok(a) => a,
        Err(e) => return println!("HidApi::new: {e}"),
    };
    for info in api.device_list() {
        println!(
            "{:04x}:{:04x} usage {:04x}/{:04x} bus {:?} if {} {:?} {:?}",
            info.vendor_id(),
            info.product_id(),
            info.usage_page(),
            info.usage(),
            info.bus_type(),
            info.interface_number(),
            info.product_string(),
            info.path()
        );
        if info.vendor_id() != 0x054c {
            continue;
        }
        match info.open_device(&api) {
            Err(e) => println!("  open failed: {e}"),
            Ok(dev) => {
                let mut buf = [0u8; 128];
                for _ in 0..3 {
                    match dev.read_timeout(&mut buf, 500) {
                        Ok(n) => println!("  read {n}: {:02x?}", &buf[..n.min(16)]),
                        Err(e) => println!("  read failed: {e}"),
                    }
                }
            }
        }
    }
}
