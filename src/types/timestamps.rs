use std::cmp::Ordering;
use std::convert::TryInto;
use std::fmt::{Display, Formatter};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::errors::{QCompressError, QCompressResult};
use crate::types::NumberLike;

const BILLION_U32: u32 = 1_000_000_000;

// an instant - does not store time zone
// always relative to Unix Epoch
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub struct TimestampNanos(i128);

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub struct TimestampMicros(i128);

macro_rules! impl_timestamp {
  ($t: ty, $parts_per_sec: expr, $header_byte: expr) => {
    impl $t {
      const MAX: i128 = $parts_per_sec as i128 * (i64::MAX as i128 + 1) - 1;
      const MIN: i128 = $parts_per_sec as i128 * (i64::MIN as i128);
      const NS_PER_PART: u32 = 1_000_000_000 / $parts_per_sec;

      pub fn new(parts: i128) -> QCompressResult<Self> {
        if parts > Self::MAX || parts < Self::MIN {
          Err(QCompressError::invalid_argument(format!(
            "invalid timestamp with {}/{} of a second",
            parts,
            $parts_per_sec,
          )))
        } else {
          Ok(Self(parts))
        }
      }

      pub fn from_secs_and_nanos(seconds: i64, subsec_nanos: u32) -> Self {
        Self(seconds as i128 * $parts_per_sec as i128 + (subsec_nanos / Self::NS_PER_PART) as i128)
      }

      pub fn to_secs_and_nanos(self) -> (i64, u32) {
        let parts = self.0;
        let seconds = parts.div_euclid($parts_per_sec as i128) as i64;
        let subsec_nanos = parts.rem_euclid($parts_per_sec as i128) as u32 * Self::NS_PER_PART;
        (seconds, subsec_nanos)
      }

      pub fn to_total_parts(self) -> i128 {
        self.0
      }
    }

    impl From<SystemTime> for $t {
      fn from(system_time: SystemTime) -> Self {
        let (seconds, subsec_nanos) = match system_time.duration_since(UNIX_EPOCH) {
          Ok(dur) => (dur.as_secs() as i64, dur.subsec_nanos()),
          Err(e) => {
            let dur = e.duration();
            let complement_nanos = dur.subsec_nanos();
            let ceil_secs = -(dur.as_secs() as i64);
            if complement_nanos == 0 {
              (ceil_secs, 0)
            } else {
              (ceil_secs - 1, BILLION_U32 - complement_nanos)
            }
          }
        };

        Self::from_secs_and_nanos(seconds, subsec_nanos)
      }
    }

    impl From<$t> for SystemTime {
      fn from(value: $t) -> SystemTime {
        let (seconds, subsec_nanos) = value.to_secs_and_nanos();
        if seconds >= 0 {
          let dur = Duration::new(seconds as u64, subsec_nanos);
          UNIX_EPOCH + dur
        } else {
          let dur = if subsec_nanos == 0 {
            Duration::new((-seconds) as u64, 0)
          } else {
            Duration::new((-seconds - 1) as u64, BILLION_U32 - subsec_nanos)
          };
          UNIX_EPOCH - dur
        }
      }
    }

    impl Display for $t {
      fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
          f,
          "Timestamp({}/{})",
          self.0,
          $parts_per_sec,
        )
      }
    }

    impl NumberLike for $t {
      const HEADER_BYTE: u8 = $header_byte;
      const PHYSICAL_BITS: usize = 96;

      type Signed = i128;
      type Unsigned = u128;

      fn to_unsigned(self) -> u128 {
        self.0.wrapping_sub(i128::MIN) as u128
      }

      fn from_unsigned(off: u128) -> Self {
        Self(i128::MIN.wrapping_add(off as i128))
      }

      fn to_signed(self) -> i128 {
        self.0
      }

      fn from_signed(signed: i128) -> Self {
        // TODO configure some check at the end of decompression to make sure
        // all timestamps are within bounds
        Self(signed)
      }

      fn num_eq(&self, other: &Self) -> bool {
        self.0.eq(&other.0)
      }

      fn num_cmp(&self, other: &Self) -> Ordering {
        self.0.cmp(&other.0)
      }

      fn to_bytes(self) -> Vec<u8> {
        ((self.0 - Self::MIN) as u128).to_be_bytes()[4..].to_vec()
      }

      fn from_bytes(bytes: Vec<u8>) -> QCompressResult<Self> {
        let mut full_bytes = vec![0; 4];
        full_bytes.extend(bytes);
        let parts = (u128::from_be_bytes(full_bytes.try_into().unwrap()) as i128) + Self::MIN;
        Self::new(parts)
      }
    }
  }
}

impl_timestamp!(TimestampNanos, 1_000_000_000_u32, 8);
impl_timestamp!(TimestampMicros, 1_000_000_u32, 9);

#[cfg(test)]
mod tests {
  use std::time::SystemTime;
  use crate::{TimestampMicros, TimestampNanos};

  #[test]
  fn test_system_time_conversion() {
    let t = SystemTime::now();
    let micro_t = TimestampMicros::from(t);
    let nano_t = TimestampNanos::from(t);
    let (micro_t_s, micro_t_ns) = micro_t.to_secs_and_nanos();
    let (nano_t_s, nano_t_ns) = nano_t.to_secs_and_nanos();
    assert!(micro_t_s > 1500000000); // would be better if we mocked time
    assert_eq!(micro_t_s, nano_t_s);
    assert!(micro_t_ns <= nano_t_ns);
    assert!(micro_t_ns + 1000 > nano_t_ns);
    assert_eq!(SystemTime::from(micro_t), t);
    assert_eq!(SystemTime::from(nano_t), t);
  }
}
