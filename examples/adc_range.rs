//! Compare the two ADC ranges on the same shunt.
//!
//! The INA4230 can measure shunt voltage over +/-81.92 mV at 2.5 uV/LSB
//! ([`AdcRange::Range0`]) or +/-20.48 mV at 625 nV/LSB ([`AdcRange::Range1`]).
//! Range1 is four times finer and covers a quarter of the span. This example
//! calibrates channel 1 both ways in turn and prints the same physical current
//! measured each way, so the resolution difference is visible directly.
//!
//! Selecting Range1 also divides `SHUNT_CAL` by four; the driver does that for
//! you when the [`Calibration`] is built, and writes `CONFIG2.RANGE` when the
//! channel is calibrated.
//!
//! # Choosing a range
//!
//! With the 100 mOhm shunt used here:
//!
//! | Range    | Full scale       | Shunt LSB | Max current | Current resolution |
//! | -------- | ---------------- | --------- | ----------- | ------------------ |
//! | `Range0` | +/-81.92 mV      | 2.5 uV    | +/-819 mA   | 25 uA/LSB          |
//! | `Range1` | +/-20.48 mV      | 625 nV    | +/-204 mA   | 6.25 uA/LSB        |
//!
//! Exceeding the range saturates the reading, so Range1 is only the better
//! choice when the current genuinely stays inside its narrower span.
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
//!   supply ---+--[ 100 mOhm shunt ]--+--- load
//!             |                      |
//!          IN+1 (A3)              IN-1 (A2)
//! ```
//!
//! Keep the load below 200 mA so both ranges stay in span and the comparison
//! is meaningful. Above that, Range1 saturates and Range0 is the only valid
//! reading.
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
//! cargo run --example adc_range
//! ```

use ina4230::{
    AdcRange, AddrPinState, AddressPins, Calibration, Channel, CurrentLsb, CurrentSensor, Ina4230, ShuntResistance,
    VoltageSensor,
};
use pico_de_gallo_hal::{Hal, I2cFrequency};

/// Shunt fitted on channel 1, in microohms. 100 mOhm.
const SHUNT_UOHM: u32 = 100_000;

/// Current LSB for Range0, in nanoamps per LSB.
///
/// Range0 reaches 81.92 mV / 100 mOhm = 819.2 mA, so the datasheet minimum is
/// `819_200_000 / 2^15` = `25_000` nA. That is already a round number.
const RANGE0_LSB_NA: u32 = 25_000;

/// Current LSB for Range1, in nanoamps per LSB.
///
/// Range1 reaches 20.48 mV / 100 mOhm = 204.8 mA, a quarter of the span, so
/// the minimum LSB is a quarter as large: `204_800_000 / 2^15` = `6_250` nA.
const RANGE1_LSB_NA: u32 = 6_250;

/// Readings to take per range.
const SAMPLES: usize = 8;

#[tokio::main]
async fn main() {
    let mut hal = Hal::new_validated().expect("no Pico de Gallo found, or firmware too old");
    hal.i2c_set_config(I2cFrequency::Fast)
        .expect("failed to set I2C to 400 kHz");

    let shunt = ShuntResistance::from_microohms(SHUNT_UOHM).expect("shunt must be non-zero");

    let range0 = Calibration::new(
        CurrentLsb::from_nanoamps(RANGE0_LSB_NA).expect("current LSB out of range"),
        shunt,
        AdcRange::Range0,
    )
    .expect("no usable SHUNT_CAL for Range0");

    let range1 = Calibration::new(
        CurrentLsb::from_nanoamps(RANGE1_LSB_NA).expect("current LSB out of range"),
        shunt,
        AdcRange::Range1,
    )
    .expect("no usable SHUNT_CAL for Range1");

    println!("shunt: {SHUNT_UOHM} uOhm");
    println!(
        "Range0: {:>6} nA/LSB, shunt LSB {:>4} nV, SHUNT_CAL {}",
        RANGE0_LSB_NA,
        AdcRange::Range0.shunt_lsb_nv(),
        range0.shunt_cal().as_u16()
    );
    println!(
        "Range1: {:>6} nA/LSB, shunt LSB {:>4} nV, SHUNT_CAL {}",
        RANGE1_LSB_NA,
        AdcRange::Range1.shunt_lsb_nv(),
        range1.shunt_cal().as_u16()
    );
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

    for (label, calibration) in [("Range0", range0), ("Range1", range1)] {
        sensor
            .calibrate(Channel::Ch1, calibration)
            .await
            .expect("calibration write failed");

        println!("{label}:");
        println!("{:>14} {:>14} {:>12}", "shunt", "current", "raw LSBs");

        for _ in 0..SAMPLES {
            while !sensor.read_flags().await.expect("flags read failed").conversion_ready() {}

            let shunt_v = sensor
                .shunt_voltage(Channel::Ch1)
                .await
                .expect("shunt voltage read failed");
            let current = sensor.current(Channel::Ch1).await.expect("current read failed");

            // Recovering the raw register value shows the resolution directly:
            // for the same physical current, Range1 uses four times as many
            // LSBs as Range0.
            let raw = shunt_v.as_nanovolts() / calibration.adc_range().shunt_lsb_nv();

            println!(
                "{:>11.4} mV {:>11.4} mA {:>12}",
                shunt_v.to_millivolts(),
                current.to_milliamps(),
                raw,
            );
        }
        println!();
    }

    println!("Range1 reports four LSBs for every one Range0 reports, at a quarter");
    println!("of the span. Both describe the same current.");
}
