#![doc = include_str!("../README.md")]
#![cfg_attr(not(test), no_std)]
#![allow(async_fn_in_trait)]

use device_driver::FieldsetMetadata;
use embedded_sensors_hal_async::sensor;

#[allow(clippy::all)]
#[allow(clippy::pedantic)]
#[allow(unsafe_code)]
#[allow(missing_docs)]
// The ALERT_LIMIT and ALERT_CONFIG accessors are generated but not yet wired
// into the public API. See "Not yet implemented" in the README.
#[allow(dead_code)]
mod device;

pub mod convert;
pub mod units;

pub use crate::units::{
    AdcRange, AddrPinState, Address, AddressPins, BusVoltage, Calibration, CalibrationError, Channel, Current,
    CurrentLsb, Energy, Power, ShuntCal, ShuntResistance, ShuntVoltage,
};

/// Maximum register data size in bytes (energy registers are 32-bit = 4 bytes).
const LARGEST_REG_SIZE_BYTES: usize = 4;

// ── Error type ────────────────────────────────────────────────────────────────

/// INA4230 driver error.
///
/// Overflow conditions are deliberately *not* errors. They live in [`Flags`],
/// which reports every condition at once; collapsing them into a single error
/// variant would discard the others, and reading the register clears them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Ina4230Error<I2cError> {
    /// An error occurred on the I²C bus.
    Bus(I2cError),
    /// A current, power, or energy reading was requested on a channel that has
    /// not been calibrated.
    ///
    /// Detected before any bus traffic is generated.
    NotCalibrated(Channel),
}

impl<E: embedded_hal_async::i2c::Error> sensor::Error for Ina4230Error<E> {
    fn kind(&self) -> sensor::ErrorKind {
        match self {
            Self::Bus(_) => sensor::ErrorKind::Peripheral,
            Self::NotCalibrated(_) => sensor::ErrorKind::NotReady,
        }
    }
}

// ── DeviceInterface ───────────────────────────────────────────────────────────

/// Async I²C interface adapter for the INA4230.
struct DeviceInterface<I2c: embedded_hal_async::i2c::I2c> {
    i2c: I2c,
    address: u8,
}

impl<I2c: embedded_hal_async::i2c::I2c> device_driver::RegisterInterfaceBase for DeviceInterface<I2c> {
    type Error = Ina4230Error<I2c::Error>;
    type AddressType = u8;
}

impl<I2c: embedded_hal_async::i2c::I2c> device_driver::AsyncRegisterInterface for DeviceInterface<I2c> {
    async fn write_register(
        &mut self,
        address: Self::AddressType,
        data: &mut [u8],
        _meta_data: &FieldsetMetadata,
    ) -> Result<(), Self::Error> {
        debug_assert!(data.len() <= LARGEST_REG_SIZE_BYTES, "Register data too large");
        let mut buf = [0u8; 1 + LARGEST_REG_SIZE_BYTES];
        buf[0] = address;
        buf[1..=data.len()].copy_from_slice(data);
        self.i2c
            .write(self.address, &buf[..=data.len()])
            .await
            .map_err(Ina4230Error::Bus)
    }

    async fn read_register(
        &mut self,
        address: Self::AddressType,
        data: &mut [u8],
        _meta_data: &FieldsetMetadata,
    ) -> Result<(), Self::Error> {
        self.i2c
            .write_read(self.address, &[address], data)
            .await
            .map_err(Ina4230Error::Bus)
    }
}

// ── Flags ─────────────────────────────────────────────────────────────────────

/// A snapshot of the `FLAGS` register.
///
/// Every condition the register can report is preserved here. That matters
/// because reading `FLAGS` clears the conversion-ready and latched alert bits
/// (datasheet Table 7-20), so a discarded flag is lost for good.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Flags {
    conversion_ready: bool,
    math_overflow: bool,
    energy_overflow: [bool; 4],
    limit_alerts: [bool; 4],
}

impl Flags {
    /// All conversions and averaging are complete.
    #[must_use]
    pub const fn conversion_ready(self) -> bool {
        self.conversion_ready
    }

    /// An arithmetic operation overflowed; current and power data may be
    /// invalid.
    #[must_use]
    pub const fn math_overflow(self) -> bool {
        self.math_overflow
    }

    /// The energy accumulator for `channel` has overflowed.
    #[must_use]
    pub const fn energy_overflow(self, channel: Channel) -> bool {
        self.energy_overflow[channel.index()]
    }

    /// Any channel's energy accumulator has overflowed.
    #[must_use]
    pub fn any_energy_overflow(self) -> bool {
        self.energy_overflow.iter().any(|&v| v)
    }

    /// The four alert limit flags, ordered `[LIMIT1, LIMIT2, LIMIT3, LIMIT4]`.
    #[must_use]
    pub const fn limit_alerts(self) -> [bool; 4] {
        self.limit_alerts
    }
}

// ── Sensor traits ─────────────────────────────────────────────────────────────

/// Async voltage sensor — reads bus or shunt voltage per channel.
pub trait VoltageSensor: sensor::ErrorType {
    /// Read the bus voltage for the given channel.
    async fn bus_voltage(&mut self, channel: Channel) -> Result<BusVoltage, Self::Error>;
    /// Read the shunt voltage for the given channel.
    ///
    /// The scale depends on the channel's configured [`AdcRange`], so this
    /// requires the channel to have been calibrated.
    async fn shunt_voltage(&mut self, channel: Channel) -> Result<ShuntVoltage, Self::Error>;
}

impl<T: VoltageSensor + ?Sized> VoltageSensor for &mut T {
    async fn bus_voltage(&mut self, channel: Channel) -> Result<BusVoltage, Self::Error> {
        T::bus_voltage(self, channel).await
    }
    async fn shunt_voltage(&mut self, channel: Channel) -> Result<ShuntVoltage, Self::Error> {
        T::shunt_voltage(self, channel).await
    }
}

/// Async current sensor — reads calculated current per channel.
pub trait CurrentSensor: sensor::ErrorType {
    /// Read the calculated current for the given channel.
    /// Requires [`Ina4230::calibrate`] to have been called first.
    async fn current(&mut self, channel: Channel) -> Result<Current, Self::Error>;
}

impl<T: CurrentSensor + ?Sized> CurrentSensor for &mut T {
    async fn current(&mut self, channel: Channel) -> Result<Current, Self::Error> {
        T::current(self, channel).await
    }
}

/// Async power sensor — reads calculated power per channel.
pub trait PowerSensor: sensor::ErrorType {
    /// Read the calculated power for the given channel.
    /// Requires [`Ina4230::calibrate`] to have been called first.
    async fn power(&mut self, channel: Channel) -> Result<Power, Self::Error>;
}

impl<T: PowerSensor + ?Sized> PowerSensor for &mut T {
    async fn power(&mut self, channel: Channel) -> Result<Power, Self::Error> {
        T::power(self, channel).await
    }
}

/// Async energy sensor — reads accumulated energy per channel.
pub trait EnergySensor: sensor::ErrorType {
    /// Read the accumulated energy for the given channel.
    /// Requires [`Ina4230::calibrate`] to have been called first.
    async fn energy(&mut self, channel: Channel) -> Result<Energy, Self::Error>;
}

impl<T: EnergySensor + ?Sized> EnergySensor for &mut T {
    async fn energy(&mut self, channel: Channel) -> Result<Energy, Self::Error> {
        T::energy(self, channel).await
    }
}

// ── Ina4230 driver struct ─────────────────────────────────────────────────────

/// High-level driver for the INA4230 quad-channel power and energy monitor.
///
/// The driver holds no logic of its own: it moves bytes to and from the device
/// and hands them to the pure functions in [`convert`]. Everything that can be
/// reasoned about lives there.
pub struct Ina4230<I2c: embedded_hal_async::i2c::I2c> {
    device: device::Device<DeviceInterface<I2c>>,
    /// The address this instance talks to, kept for introspection.
    address: Address,
    /// Per-channel calibration. `None` means the channel is not calibrated,
    /// and is the single source of truth for whether a reading can be scaled.
    calibration: [Option<Calibration>; 4],
}

impl<I2c: embedded_hal_async::i2c::I2c> Ina4230<I2c> {
    /// Create a new driver instance.
    ///
    /// `pins` describes how A0 and A1 are strapped on the board; the fields are
    /// named so the two cannot be transposed.
    pub fn new(i2c: I2c, pins: AddressPins) -> Self {
        let address = Address::from_pins(pins);
        Self {
            device: device::Device::new(DeviceInterface {
                i2c,
                address: address.as_u8(),
            }),
            address,
            calibration: [None; 4],
        }
    }

    /// The I²C address this instance talks to.
    #[must_use]
    pub const fn address(&self) -> Address {
        self.address
    }

    /// Release the underlying I²C bus.
    pub fn release(self) -> I2c {
        self.device.free().i2c
    }

    // ── Device management ─────────────────────────────────────────────────────

    /// Issue a full device reset (`CONFIG2.RST = 1`).
    ///
    /// All registers return to power-on defaults and the bit self-clears. That
    /// includes the calibration registers and `CONFIG2.RANGE`, so the cached
    /// per-channel calibration is discarded here too: leaving it in place would
    /// make the driver confidently scale readings with settings the device no
    /// longer has.
    ///
    /// Call [`Ina4230::calibrate`] again before reading current, power, or
    /// energy.
    ///
    /// # Errors
    ///
    /// Returns [`Ina4230Error::Bus`] if an I²C bus error occurs.
    pub async fn reset(&mut self) -> Result<(), Ina4230Error<I2c::Error>> {
        self.device.config_2().write_async(|w| w.set_rst(true)).await?;
        self.calibration = [None; 4];
        Ok(())
    }

    /// Read the manufacturer ID register.
    ///
    /// # Errors
    ///
    /// Returns [`Ina4230Error::Bus`] if an I²C bus error occurs.
    pub async fn manufacturer_id(&mut self) -> Result<u16, Ina4230Error<I2c::Error>> {
        Ok(self.device.manufacturer_id().read_async().await?.id())
    }

    /// Check that the manufacturer ID reads back the expected value.
    ///
    /// # Errors
    ///
    /// Returns [`Ina4230Error::Bus`] if an I²C bus error occurs.
    pub async fn is_present(&mut self) -> Result<bool, Ina4230Error<I2c::Error>> {
        Ok(self.manufacturer_id().await? == Self::MANUFACTURER_ID)
    }

    /// Value the manufacturer ID register reads back on a healthy device:
    /// `"TI"` in ASCII.
    pub const MANUFACTURER_ID: u16 = 0x5449;

    /// Read and clear the `FLAGS` register.
    ///
    /// # This read is destructive
    ///
    /// Reading `FLAGS` clears the conversion-ready flag and the latched alert
    /// flags (datasheet Table 7-20). There is no way to poll one bit without
    /// consuming the others, which is why this returns the whole register
    /// rather than offering per-bit accessors that would quietly discard the
    /// rest.
    ///
    /// To poll for conversion completion:
    ///
    /// ```rust,ignore
    /// while !sensor.read_flags().await?.conversion_ready() {}
    /// ```
    ///
    /// Note that this discards any overflow flags raised in the meantime; keep
    /// the [`Flags`] value if you care about them.
    ///
    /// # Errors
    ///
    /// Returns [`Ina4230Error::Bus`] if an I²C bus error occurs.
    pub async fn read_flags(&mut self) -> Result<Flags, Ina4230Error<I2c::Error>> {
        let f = self.device.flags().read_async().await?;
        Ok(Flags {
            conversion_ready: f.cvrf(),
            math_overflow: f.ovf(),
            energy_overflow: [
                f.energyof_ch1(),
                f.energyof_ch2(),
                f.energyof_ch3(),
                f.energyof_ch4(),
            ],
            limit_alerts: [
                f.limit1_alert(),
                f.limit2_alert(),
                f.limit3_alert(),
                f.limit4_alert(),
            ],
        })
    }

    /// Enable or disable a channel in `CONFIG1.ACTIVE_CHANNEL`.
    ///
    /// # Errors
    ///
    /// Returns [`Ina4230Error::Bus`] if an I²C bus error occurs.
    pub async fn set_channel_active(
        &mut self,
        channel: Channel,
        active: bool,
    ) -> Result<(), Ina4230Error<I2c::Error>> {
        self.device
            .config_1()
            .modify_async(|w| {
                w.set_active_channel(convert::set_channel_bit(w.active_channel(), channel, active));
            })
            .await
    }

    // ── Calibration ───────────────────────────────────────────────────────────

    /// Program the calibration for a single channel.
    ///
    /// Writes `CONFIG2.RANGE` for the channel and the channel's `SHUNT_CAL`
    /// register, then caches the calibration for scaling subsequent readings.
    ///
    /// Build the [`Calibration`] with [`Calibration::new`]; all validation
    /// happens there, so this cannot fail for anything but a bus error.
    ///
    /// # Errors
    ///
    /// Returns [`Ina4230Error::Bus`] if an I²C bus error occurs.
    pub async fn calibrate(
        &mut self,
        channel: Channel,
        calibration: Calibration,
    ) -> Result<(), Ina4230Error<I2c::Error>> {
        self.device
            .config_2()
            .modify_async(|w| {
                let set = matches!(calibration.adc_range(), AdcRange::Range1);
                w.set_range(convert::set_channel_bit(w.range(), channel, set));
            })
            .await?;

        self.device
            .channel_regs(channel.into())
            .calibration()
            .write_async(|w| w.set_shunt_cal(calibration.shunt_cal().as_u16()))
            .await?;

        self.calibration[channel.index()] = Some(calibration);
        Ok(())
    }

    /// Program the calibration for all four channels.
    ///
    /// `calibrations` is ordered `[Ch1, Ch2, Ch3, Ch4]`. This performs a single
    /// read-modify-write of `CONFIG2` rather than one per channel, since all
    /// four range bits are known up front.
    ///
    /// # Errors
    ///
    /// Returns [`Ina4230Error::Bus`] if an I²C bus error occurs.
    pub async fn calibrate_all(
        &mut self,
        calibrations: [Calibration; 4],
    ) -> Result<(), Ina4230Error<I2c::Error>> {
        self.device
            .config_2()
            .modify_async(|w| {
                let mut range = w.range();
                for (ch, cal) in Channel::ALL.iter().zip(calibrations.iter()) {
                    let set = matches!(cal.adc_range(), AdcRange::Range1);
                    range = convert::set_channel_bit(range, *ch, set);
                }
                w.set_range(range);
            })
            .await?;

        for (ch, cal) in Channel::ALL.iter().zip(calibrations.iter()) {
            self.device
                .channel_regs((*ch).into())
                .calibration()
                .write_async(|w| w.set_shunt_cal(cal.shunt_cal().as_u16()))
                .await?;
            self.calibration[ch.index()] = Some(*cal);
        }
        Ok(())
    }

    /// The calibration currently programmed for `channel`, if any.
    #[must_use]
    pub fn calibration(&self, channel: Channel) -> Option<Calibration> {
        self.calibration[channel.index()]
    }

    /// Fetch the calibration for a channel, or fail before touching the bus.
    fn require_calibration(&self, channel: Channel) -> Result<Calibration, Ina4230Error<I2c::Error>> {
        self.calibration[channel.index()].ok_or(Ina4230Error::NotCalibrated(channel))
    }
}

impl From<Channel> for device::Channel {
    fn from(channel: Channel) -> Self {
        match channel {
            Channel::Ch1 => Self::Ch1,
            Channel::Ch2 => Self::Ch2,
            Channel::Ch3 => Self::Ch3,
            Channel::Ch4 => Self::Ch4,
        }
    }
}

// ── Trait implementations ─────────────────────────────────────────────────────

impl<I2c: embedded_hal_async::i2c::I2c> sensor::ErrorType for Ina4230<I2c> {
    type Error = Ina4230Error<I2c::Error>;
}

impl<I2c: embedded_hal_async::i2c::I2c> VoltageSensor for Ina4230<I2c> {
    async fn bus_voltage(&mut self, channel: Channel) -> Result<BusVoltage, Self::Error> {
        let raw = self
            .device
            .channel_regs(channel.into())
            .bus_voltage()
            .read_async()
            .await?
            .vbus();
        Ok(convert::decode_bus_voltage(raw))
    }

    async fn shunt_voltage(&mut self, channel: Channel) -> Result<ShuntVoltage, Self::Error> {
        let cal = self.require_calibration(channel)?;
        let raw = self
            .device
            .channel_regs(channel.into())
            .shunt_voltage()
            .read_async()
            .await?
            .vshunt();
        Ok(convert::decode_shunt_voltage(raw, cal.adc_range()))
    }
}

impl<I2c: embedded_hal_async::i2c::I2c> CurrentSensor for Ina4230<I2c> {
    async fn current(&mut self, channel: Channel) -> Result<Current, Self::Error> {
        let cal = self.require_calibration(channel)?;
        let raw = self
            .device
            .channel_regs(channel.into())
            .current()
            .read_async()
            .await?
            .current();
        Ok(convert::decode_current(raw, cal))
    }
}

impl<I2c: embedded_hal_async::i2c::I2c> PowerSensor for Ina4230<I2c> {
    async fn power(&mut self, channel: Channel) -> Result<Power, Self::Error> {
        let cal = self.require_calibration(channel)?;
        let raw = self
            .device
            .channel_regs(channel.into())
            .power()
            .read_async()
            .await?
            .power();
        Ok(convert::decode_power(raw, cal))
    }
}

impl<I2c: embedded_hal_async::i2c::I2c> EnergySensor for Ina4230<I2c> {
    async fn energy(&mut self, channel: Channel) -> Result<Energy, Self::Error> {
        let cal = self.require_calibration(channel)?;
        let raw = self
            .device
            .channel_regs(channel.into())
            .energy()
            .read_async()
            .await?
            .energy();
        Ok(convert::decode_energy(raw, cal))
    }
}
