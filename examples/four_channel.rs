//! Monitor all four channels with independent shunts and current ranges.
//!
//! Shows why the four channels are calibrated separately: each carries its own
//! shunt value and its own current resolution, so a 10 A rail and a 500 mA rail
//! can be watched by the same part without either losing resolution.
//!
//! Uses [`Ina4230::calibrate_all`], which writes `CONFIG2` once for all four
//! range bits rather than doing a read-modify-write per channel.
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
//! Each channel needs its own shunt, wired high-side:
//!
//! ```text
//!   rail n ---+--[ shunt n ]--+--- load n
//!             |               |
//!          IN+n            IN-n
//! ```
//!
//! | Channel | Bump `IN+` | Bump `IN-` | Shunt in this example | Full scale |
//! | ------- | ---------- | ---------- | --------------------- | ---------- |
//! | 1       | A3         | A2         | 10 mOhm               | 5 A        |
//! | 2       | A1         | B1         | 50 mOhm               | 1 A        |
//! | 3       | D1         | C1         | 100 mOhm              | 500 mA     |
//! | 4       | D3         | D2         | 5 mOhm                | 10 A       |
//!
//! Every shunt develops 50 mV at its full-scale current, which sits inside the
//! +/-81.92 mV range of [`AdcRange::Range0`] with margin.
//!
//! Unused channels can be left unconnected, but disable them with
//! [`Ina4230::set_channel_active`] so they do not lengthen the round-robin
//! conversion cycle.
//!
//! # Safety
//!
//! The sense inputs tolerate a common-mode voltage up to 50 V, but Pico de
//! Gallo is a 3.3 V board. Only VS, GND, SDA, SCL, EN, A0 and A1 may be shared
//! with the bridge. Never connect a monitored rail, or either side of a shunt,
//! to the header. All four monitored rails must share a ground with the
//! INA4230.
//!
//! # Running
//!
//! ```sh
//! cargo run --example four_channel
//! ```

use ina4230::{
    AdcRange, AddrPinState, AddressPins, Calibration, Channel, CurrentLsb, CurrentSensor, Ina4230, PowerSensor,
    ShuntResistance, VoltageSensor,
};
use pico_de_gallo_hal::{Hal, I2cFrequency};

/// Per channel: shunt in microohms, current LSB in nanoamps per LSB.
///
/// The current LSB is the datasheet minimum for the intended full scale,
/// rounded up to a round number as datasheet section 8.2.2.3 recommends.
const CHANNELS: [(u32, u32); 4] = [
    (10_000, 200_000), // Ch1: 10 mOhm,  5 A    -> 200 uA/LSB
    (50_000, 50_000),  // Ch2: 50 mOhm,  1 A    ->  50 uA/LSB
    (100_000, 20_000), // Ch3: 100 mOhm, 500 mA ->  20 uA/LSB
    (5_000, 500_000),  // Ch4: 5 mOhm,   10 A   -> 500 uA/LSB
];

#[tokio::main]
async fn main() {
    let mut hal = Hal::new_validated().expect("no Pico de Gallo found, or firmware too old");
    hal.i2c_set_config(I2cFrequency::Fast)
        .expect("failed to set I2C to 400 kHz");

    // Build all four calibrations up front. Any mistake in the design values
    // is reported here, by channel, before the device is touched.
    let calibrations: [Calibration; 4] = core::array::from_fn(|i| {
        let (shunt_uohm, lsb_na) = CHANNELS[i];
        Calibration::new(
            CurrentLsb::from_nanoamps(lsb_na).expect("current LSB out of range"),
            ShuntResistance::from_microohms(shunt_uohm).expect("shunt must be non-zero"),
            AdcRange::Range0,
        )
        .unwrap_or_else(|e| panic!("channel {} has no usable SHUNT_CAL: {e:?}", i + 1))
    });

    for (ch, cal) in Channel::ALL.iter().zip(calibrations.iter()) {
        let (shunt_uohm, lsb_na) = CHANNELS[ch.index()];
        println!(
            "{:?}: {:>6} uOhm, {:>6} nA/LSB, SHUNT_CAL {:>5}",
            ch,
            shunt_uohm,
            lsb_na,
            cal.shunt_cal().as_u16()
        );
    }
    println!();

    let mut sensor = Ina4230::new(
        hal.i2c(),
        AddressPins {
            a0: AddrPinState::Gnd,
            a1: AddrPinState::Gnd,
        },
    );

    assert!(
        sensor.is_present().await.expect("I2C read failed"),
        "device at {:#04X} is not an INA4230",
        sensor.address().as_u8()
    );

    sensor.reset().await.expect("reset failed");
    sensor
        .calibrate_all(calibrations)
        .await
        .expect("calibration write failed");

    println!(
        "{:>4} {:>10} {:>12} {:>12} {:>12}",
        "ch", "bus", "shunt", "current", "power"
    );

    for _ in 0..10 {
        // One conversion cycle covers every enabled channel, so wait once and
        // then read all four.
        while !sensor.read_flags().await.expect("flags read failed").conversion_ready() {}

        for ch in Channel::ALL {
            let bus = sensor.bus_voltage(ch).await.expect("bus voltage read failed");
            let shunt = sensor.shunt_voltage(ch).await.expect("shunt voltage read failed");
            let current = sensor.current(ch).await.expect("current read failed");
            let power = sensor.power(ch).await.expect("power read failed");

            println!(
                "{:>4?} {:>8.3} V {:>10.3} mV {:>10.3} mA {:>10.3} mW",
                ch,
                bus.to_volts(),
                shunt.to_millivolts(),
                current.to_milliamps(),
                power.to_milliwatts(),
            );
        }
        println!();
    }
}
