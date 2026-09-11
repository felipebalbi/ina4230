//! Discover INA4230 devices on the bus and identify their address strapping.
//!
//! Walks all sixteen A0/A1 combinations, reads the manufacturer ID at each,
//! and reports which strappings answered. Run this first: it proves the wiring
//! and the address before any measurement example can work.
//!
//! # Wiring
//!
//! Pico de Gallo **v1.1** 2×12 box header. Viewed from above with the USB
//! connector pointing up, pin 1 is top-right; odd pins are the top row.
//!
//! ```text
//! Pico de Gallo v1.1 header        INA4230 (YBJ DSBGA)
//! -------------------------        -------------------
//! Pin  1  VREF (+3V3)  ----------  VS   (A4)   supply, 1.7 V to 5.5 V
//! Pin  1  VREF (+3V3)  ----------  EN   (B3)   logic high enables the device
//! Pin  2  GND          ----------  GND  (B4)
//! Pin  3  SDA (GPIO 2) ----------  SDA  (D4)   4.7 kOhm pull-up on the board
//! Pin  4  SCL (GPIO 3) ----------  SCL  (C4)   4.7 kOhm pull-up on the board
//! Pin  2  GND          ----------  A0   (C2)   \  GND/GND selects 0x40
//! Pin  2  GND          ----------  A1   (C3)   /
//! ```
//!
//! `EN` is easy to forget: the device is disabled and will not acknowledge
//! until it is driven high.
//!
//! To exercise the scan properly, strap A0 and A1 to something other than
//! GND — SDA and SCL are valid strapping levels as well as VS and GND, so all
//! sixteen addresses are reachable with no extra parts.
//!
//! # Safety
//!
//! The sense inputs tolerate a common-mode voltage up to 50 V, but Pico de
//! Gallo is a 3.3 V board. Only VS, GND, SDA, SCL, EN, A0 and A1 may be shared
//! with the bridge. Never connect a monitored high-voltage rail, or either
//! side of a shunt sitting on one, to the header.
//!
//! # Running
//!
//! ```sh
//! cargo run --example scan
//! ```

use ina4230::{AddrPinState, Address, AddressPins, Ina4230};
use pico_de_gallo_hal::{Hal, I2cFrequency};

#[tokio::main]
async fn main() {
    let mut hal = Hal::new_validated().expect("no Pico de Gallo found, or firmware too old");
    hal.i2c_set_config(I2cFrequency::Fast)
        .expect("failed to set I2C to 400 kHz");

    // A raw bus scan first: this sees every device, not just INA4230s, and
    // distinguishes "nothing is wired up" from "something is there but is not
    // an INA4230".
    let found = hal.i2c_scan(false).expect("bus scan failed");
    println!("devices acknowledging on the bus: {found:02X?}");
    if found.is_empty() {
        println!("nothing responded. check VS, GND, EN and the pull-ups.");
        return;
    }

    println!();
    println!("probing all 16 INA4230 address strappings:");

    let mut i2c = hal.i2c();
    let mut hits = 0;

    // The datasheet lists Table 6-1 as (A1, A0), and the address encoding is
    // 0x40 | (A1 << 2) | A0. Iterating A1 in the outer loop walks 0x40..=0x4F
    // in order.
    for a1 in AddrPinState::ALL {
        for a0 in AddrPinState::ALL {
            let pins = AddressPins { a0, a1 };
            let address = Address::from_pins(pins);

            let mut sensor = Ina4230::new(i2c, pins);
            let result = sensor.is_present().await;
            let id = sensor.manufacturer_id().await;
            i2c = sensor.release();

            match (result, id) {
                (Ok(true), Ok(id)) => {
                    hits += 1;
                    println!(
                        "  {:#04X}  A1={:?}, A0={:?}  -> INA4230 (ID {id:#06X})",
                        address.as_u8(),
                        a1,
                        a0
                    );
                }
                (Ok(false), Ok(id)) => {
                    println!(
                        "  {:#04X}  A1={:?}, A0={:?}  -> responded, but ID is {id:#06X}, not TI",
                        address.as_u8(),
                        a1,
                        a0
                    );
                }
                // A NACK is the normal answer for an address nothing is
                // strapped to, so it is not worth reporting.
                _ => {}
            }
        }
    }

    println!();
    match hits {
        0 => println!("no INA4230 found. is EN high? are A0/A1 strapped as expected?"),
        1 => println!("1 INA4230 found."),
        n => println!("{n} INA4230 devices found."),
    }
}
