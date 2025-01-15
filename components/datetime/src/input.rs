// This file is part of ICU4X. For terms of use, please see the file
// called LICENSE at the top level of the ICU4X source tree
// (online at: https://github.com/unicode-org/icu4x/blob/main/LICENSE ).

//! A collection of utilities for representing and working with dates as an input to
//! formatting operations.

use crate::options::FractionalSecondDigits;
use crate::scaffold::{DateInputMarkers, GetField, TimeMarkers, ZoneMarkers};
use fixed_decimal::UnsignedFixedDecimal;
use icu_calendar::types::DayOfYearInfo;
use icu_calendar::{Date, Iso};
use icu_timezone::scaffold::IntoOption;
use icu_timezone::{
    types::{IsoHour, IsoMinute, IsoSecond, NanoSecond},
    Time, TimeZoneBcp47Id, UtcOffset, ZoneVariant,
};

// TODO(#2630) fix up imports to directly import from icu_calendar
pub(crate) use icu_calendar::types::{DayOfMonth, IsoWeekday, MonthInfo, YearInfo};
use writeable::Writeable;

/// A nanosecond pre-converted to digits form
#[derive(Debug, Copy, Clone)]
pub(crate) struct ExtractedNanosecond {
    /// A string of the form: "0.#########"
    digits: [u8; 11],
    /// The original nanosecond
    nanosecond: NanoSecond,
}

impl ExtractedNanosecond {
    pub(crate) fn from_nanosecond(nanosecond: NanoSecond) -> Self {
        let fd = UnsignedFixedDecimal::from(nanosecond.number()).multiplied_pow10(-9);
        struct FixedBuf {
            buf: [u8; 11],
            offset: usize,
        }
        impl core::fmt::Write for FixedBuf {
            fn write_str(&mut self, s: &str) -> core::fmt::Result {
                let new_offset = self.offset + s.len();
                self.buf.get_mut(self.offset..new_offset).ok_or(core::fmt::Error)?.copy_from_slice(s.as_bytes());
                self.offset = new_offset;
                Ok(())
            }
        }
        let mut fixed_buf = FixedBuf {
            buf: [0; 11],
            offset: 0,
        };
        let Ok(()) = fd.write_to(&mut fixed_buf) else {
            debug_assert!(false, "Failed writing to fixed buf: {nanosecond:?}");
            return ExtractedNanosecond {
                digits: *b"0.000000000",
                nanosecond,
            };
        };
        debug_assert_eq!(fixed_buf.offset, 11, "Should have written 11 chars: {nanosecond:?}");
        ExtractedNanosecond {
            digits: fixed_buf.buf,
            nanosecond,
        }
    }

    pub(crate) fn digits(self) -> Option<FractionalSecondDigits> {
        match self.digits.iter().rev().position(|c| *c != b'0') {
            Some(0) => Some(FractionalSecondDigits::F9),
            Some(1) => Some(FractionalSecondDigits::F8),
            Some(2) => Some(FractionalSecondDigits::F7),
            Some(3) => Some(FractionalSecondDigits::F6),
            Some(4) => Some(FractionalSecondDigits::F5),
            Some(5) => Some(FractionalSecondDigits::F4),
            Some(6) => Some(FractionalSecondDigits::F3),
            Some(7) => Some(FractionalSecondDigits::F2),
            Some(8) => Some(FractionalSecondDigits::F1),
            _ => None,
        }
    }

    pub(crate) fn millis(self) -> u32 {
        self.nanosecond.number() / 1_000_000
    }

    pub(crate) fn number(self) -> u32 {
        self.nanosecond.number()
    }

    pub(crate) fn to_unsigned_fixed_decimal(self) -> UnsignedFixedDecimal {
        let Ok(result) = UnsignedFixedDecimal::try_from_utf8(&self.digits) else {
            debug_assert!(false, "Unable to restore the fixed decimal: {self:?}");
            return UnsignedFixedDecimal::default()
        };
        result
    }
}

#[derive(Debug, Copy, Clone)]
pub(crate) struct ExtractedInput {
    pub(crate) year: Option<YearInfo>,
    pub(crate) month: Option<MonthInfo>,
    pub(crate) day_of_month: Option<DayOfMonth>,
    pub(crate) iso_weekday: Option<IsoWeekday>,
    pub(crate) day_of_year: Option<DayOfYearInfo>,
    pub(crate) hour: Option<IsoHour>,
    pub(crate) minute: Option<IsoMinute>,
    pub(crate) second: Option<IsoSecond>,
    pub(crate) nanosecond: Option<ExtractedNanosecond>,
    pub(crate) time_zone_id: Option<TimeZoneBcp47Id>,
    pub(crate) offset: Option<UtcOffset>,
    pub(crate) zone_variant: Option<ZoneVariant>,
    pub(crate) local_time: Option<(Date<Iso>, Time)>,
}

impl ExtractedInput {
    /// Construct given neo date input instances.
    pub(crate) fn extract_from_neo_input<D, T, Z, I>(input: &I) -> Self
    where
        D: DateInputMarkers,
        T: TimeMarkers,
        Z: ZoneMarkers,
        I: ?Sized
            + GetField<D::YearInput>
            + GetField<D::MonthInput>
            + GetField<D::DayOfMonthInput>
            + GetField<D::DayOfWeekInput>
            + GetField<D::DayOfYearInput>
            + GetField<T::HourInput>
            + GetField<T::MinuteInput>
            + GetField<T::SecondInput>
            + GetField<T::NanoSecondInput>
            + GetField<Z::TimeZoneIdInput>
            + GetField<Z::TimeZoneOffsetInput>
            + GetField<Z::TimeZoneVariantInput>
            + GetField<Z::TimeZoneLocalTimeInput>,
    {
        let nanosecond = GetField::<T::NanoSecondInput>::get_field(input).into_option()
            .map(ExtractedNanosecond::from_nanosecond);
        Self {
            year: GetField::<D::YearInput>::get_field(input).into_option(),
            month: GetField::<D::MonthInput>::get_field(input).into_option(),
            day_of_month: GetField::<D::DayOfMonthInput>::get_field(input).into_option(),
            iso_weekday: GetField::<D::DayOfWeekInput>::get_field(input).into_option(),
            day_of_year: GetField::<D::DayOfYearInput>::get_field(input).into_option(),
            hour: GetField::<T::HourInput>::get_field(input).into_option(),
            minute: GetField::<T::MinuteInput>::get_field(input).into_option(),
            second: GetField::<T::SecondInput>::get_field(input).into_option(),
            nanosecond,
            time_zone_id: GetField::<Z::TimeZoneIdInput>::get_field(input).into_option(),
            offset: GetField::<Z::TimeZoneOffsetInput>::get_field(input).into_option(),
            zone_variant: GetField::<Z::TimeZoneVariantInput>::get_field(input).into_option(),
            local_time: GetField::<Z::TimeZoneLocalTimeInput>::get_field(input).into_option(),
        }
    }
}
