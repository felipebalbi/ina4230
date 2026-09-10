//! Continuously measure one channel: bus voltage, shunt voltage, current and
//! power.
//!
//! This is the everyday case. It builds a [`Calibration`] from the shunt value
//! and the largest current you expect, programs channel 1, then polls the
//! conversion-ready flag and prints a reading each cycle.
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
//! Channel 1 sense side, high-side configuration:
//!
//! ```text
//!   supply ---+--[ 10 mOhm shunt ]--+--- load
//!             |                     |
//!          IN+1 (A3)             IN-1 (A2)
//! ```
//!
//! `IN+1` goes to the supply side of the shunt and `IN-1` to the load side.
//! Bus voltage is measured at `IN-1` with respect to `GND`, so the load rail
//! and the INA4230 must share a ground.
//!
//! # Safety
//!
//! The sense inputs tolerate a common-mode voltage up to 50 V, but Pico de
//! Gallo is a 3.3 V board. Only VS, GND, SDA, SCL, EN, A0 and A1 may be shared
//! with the bridge. Never connect the monitored rail, or either side of the
//! shunt, to the header.
//!
//! # Running
//!
//! ```sh
//! cargo run --example single_channel
//! ```

use ina4230::{
    AdcRange, AddrPinState, AddressPins, Calibration, Channel, CurrentLsb, CurrentSensor, Ina4230, PowerSensor,
    ShuntResistance, VoltageSensor,
};
use pico_de_gallo_hal::{Hal, I2cFrequency};

/// Shunt fitted on channel 1, in microohms. 10 mOhm.
const SHUNT_UOHM: u32 = 10_000;

/// Largest current expected through that shunt, in nanoamps. 5 A.
///
/// At 10 mOhm this develops 50 mV, comfortably inside the +/-81.92 mV
/// full-scale range of [`AdcRange::Range0`].
const MAX_CURRENT_NA: u64 = 5_000_000_000;

/// Current resolution, in nanoamps per LSB.
///
/// Datasheet Equation 2 puts the minimum at `MAX_CURRENT / 2^15`, about
/// 152.6 uA here. The datasheet expects that rounded up to a convenient value
/// and requires staying strictly below eight times the minimum, so 200 uA is a
/// good choice.
const CURRENT_LSB_NA: u32 = 200_000;

#[tokio::main]
async fn main() {
    let mut hal = Hal::new_validated().expect("no Pico de Gallo found, or firmware too old");
    hal.i2c_set_config(I2cFrequency::Fast)
        .expect("failed to set I2C to 400 kHz");

    // Everything that can be wrong about the calibration is caught here, on
    // the host, before a single byte reaches the bus.
    let calibration = Calibration::new(
        CurrentLsb::from_nanoamps(CURRENT_LSB_NA).expect("current LSB out of range"),
        ShuntResistance::from_microohms(SHUNT_UOHM).expect("shunt resistance must be non-zero"),
        AdcRange::Range0,
    )
    .expect("no usable SHUNT_CAL for this shunt and current LSB");

    // Sanity-check the choice against the datasheet's guidance before using it.
    let minimum = CurrentLsb::min_for_max_current(MAX_CURRENT_NA).expect("max current out of range");
    println!("shunt          : {SHUNT_UOHM} uOhm");
    println!("max current    : {} mA", MAX_CURRENT_NA / 1_000_000);
    println!(
        "CURRENT_LSB    : {} nA/LSB (datasheet minimum {} nA/LSB)",
        CURRENT_LSB_NA,
        minimum.as_nanoamps()
    );
    println!("SHUNT_CAL      : {}", calibration.shunt_cal().as_u16());
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

    // Start from a known state, then calibrate. reset() clears the driver's
    // cached calibration as well as the device's, so this order matters: the
    // reverse would leave the channel uncalibrated.
    sensor.reset().await.expect("reset failed");
    sensor
        .calibrate(Channel::Ch1, calibration)
        .await
        .expect("calibration write failed");

    println!("{:>10} {:>12} {:>12} {:>12}", "bus", "shunt", "current", "power");

    for _ in 0..20 {
        // Reading FLAGS clears the conversion-ready bit, so each poll consumes
        // the flag it is waiting for. That is what makes this a poll rather
        // than a level check.
        while !sensor.read_flags().await.expect("flags read failed").conversion_ready() {}

        let bus = sensor.bus_voltage(Channel::Ch1).await.expect("bus voltage read failed");
        let shunt = sensor
            .shunt_voltage(Channel::Ch1)
            .await
            .expect("shunt voltage read failed");
        let current = sensor.current(Channel::Ch1).await.expect("current read failed");
        let power = sensor.power(Channel::Ch1).await.expect("power read failed");

        println!(
            "{:>8.3} V {:>10.3} mV {:>10.3} mA {:>10.3} mW",
            bus.to_volts(),
            shunt.to_millivolts(),
            current.to_milliamps(),
            power.to_milliwatts(),
        );
    }
}
