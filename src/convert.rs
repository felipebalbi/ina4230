//! Conversions between raw register values and physical quantities.
//!
//! Every function here is pure and, with one deliberate exception, *total*:
//! given a value of the input type it always produces an output, with no
//! failure case and no panic. The exception is [`super::units::Calibration`],
//! which parses caller-supplied calibration inputs and is the single fallible
//! step in the driver.
//!
//! Because nothing here touches a bus, these functions are testable by walking
//! their entire input domain on the host. `decode_bus_voltage` has 65,536
//! inputs; `decode_shunt_voltage` has 131,072. Both are exhausted in the test
//! suite in well under a millisecond.

use crate::units::{AdcRange, BusVoltage, Calibration, Channel, Current, Energy, Power, ShuntVoltage};

/// Bus voltage LSB, in microvolts (1.6 mV, datasheet §7.1.7).
const BUS_LSB_UV: u32 = 1_600;

/// Multiplier relating `POWER`/`ENERGY` LSB to `CURRENT_LSB` (datasheet
/// Equations 4 and 5).
const POWER_LSB_MULTIPLIER: u64 = 32;

/// Decode a `SHUNT_VOLTAGE` register value.
///
/// `Value [V] = shunt_lsb × raw`, where the LSB is 2.5 µV on
/// [`AdcRange::Range0`] and 625 nV on [`AdcRange::Range1`]. Both are whole
/// nanovolts, so the result is exact.
///
/// Total: the widest product, `-32768 × 2500`, is well inside `i32`.
#[must_use]
pub const fn decode_shunt_voltage(raw: i16, range: AdcRange) -> ShuntVoltage {
    ShuntVoltage::from_nanovolts(raw as i32 * range.shunt_lsb_nv())
}

/// Decode a `BUS_VOLTAGE` register value.
///
/// `Value [V] = 1.6 mV × raw`. The register is always positive
/// (datasheet Table 7-13), so the raw value is read unsigned.
///
/// Total: the widest product, `65535 × 1600`, is well inside `u32`.
#[must_use]
pub const fn decode_bus_voltage(raw: u16) -> BusVoltage {
    BusVoltage::from_microvolts(raw as u32 * BUS_LSB_UV)
}

/// Decode a `CURRENT` register value.
///
/// `Value [A] = CURRENT_LSB × raw` (datasheet Equation 3).
///
/// Total: `CurrentLsb` is bounded at construction so the product always fits
/// `i64`.
#[must_use]
pub const fn decode_current(raw: i16, cal: Calibration) -> Current {
    Current::from_nanoamps(raw as i64 * cal.current_lsb().as_nanoamps() as i64)
}

/// Decode a `POWER` register value.
///
/// `Value [W] = 32 × CURRENT_LSB × raw` (datasheet Equation 4). The register
/// is unsigned.
///
/// Total: `CurrentLsb` is bounded at construction so the product always fits
/// `u64`.
#[must_use]
pub const fn decode_power(raw: u16, cal: Calibration) -> Power {
    Power::from_nanowatts(raw as u64 * POWER_LSB_MULTIPLIER * cal.current_lsb().as_nanoamps() as u64)
}

/// Decode an `ENERGY` register value.
///
/// `Value [J] = 32 × CURRENT_LSB × raw` (datasheet Equation 5). The register
/// is an unsigned 32-bit accumulator (datasheet Table 7-18).
///
/// Total: `CurrentLsb` is bounded at construction so the product always fits
/// `u64`.
#[must_use]
pub const fn decode_energy(raw: u32, cal: Calibration) -> Energy {
    Energy::from_nanojoules(raw as u64 * POWER_LSB_MULTIPLIER * cal.current_lsb().as_nanoamps() as u64)
}

/// Set or clear a channel's bit in a four-bit channel mask.
///
/// Used for both `CONFIG1.ACTIVE_CHANNEL` and `CONFIG2.RANGE`, which share a
/// layout in which bit 0 is channel 1.
#[must_use]
pub const fn set_channel_bit(mask: u8, channel: Channel, set: bool) -> u8 {
    if set {
        mask | channel.mask()
    } else {
        mask & !channel.mask()
    }
}

/// Test a channel's bit in a four-bit channel mask.
#[must_use]
pub const fn channel_bit(mask: u8, channel: Channel) -> bool {
    mask & channel.mask() != 0
}
