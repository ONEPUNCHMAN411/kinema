//! Time for Kinema.
//!
//! Time is never a float on the timeline. Frames are integers, seconds are floats only
//! at the JSON boundary, and everything inside runs on a 90 kHz tick clock so that
//! 30000/1001 fps stays exact over hours of material.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Ticks per second. 90 kHz divides every common rate exactly, including 30000/1001.
pub const TICKS_PER_SECOND: i64 = 90_000;

fn gcd(a: i64, b: i64) -> i64 {
    if b == 0 {
        a.abs().max(1)
    } else {
        gcd(b, a % b)
    }
}

/// A rational number, used for frame rates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rational {
    pub num: i64,
    pub den: i64,
}

impl Rational {
    pub fn new(num: i64, den: i64) -> Self {
        assert!(den != 0, "denominator must be non-zero");
        let sign = if den < 0 { -1 } else { 1 };
        let g = gcd(num, den);
        Rational {
            num: sign * num / g,
            den: sign * den / g,
        }
    }

    pub fn as_f64(self) -> f64 {
        self.num as f64 / self.den as f64
    }

    /// Seconds as ticks, rounded to the nearest tick.
    pub fn seconds_to_ticks(self) -> i64 {
        ((self.num as i128 * TICKS_PER_SECOND as i128 * 2 + self.den as i128)
            / (self.den as i128 * 2)) as i64
    }
}

impl fmt::Display for Rational {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.num, self.den)
    }
}

impl Serialize for Rational {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        [self.num, self.den].serialize(s)
    }
}

impl<'de> Deserialize<'de> for Rational {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let [num, den] = <[i64; 2]>::deserialize(d)?;
        if den == 0 {
            return Err(serde::de::Error::custom(
                "frame rate denominator must be non-zero",
            ));
        }
        Ok(Rational::new(num, den))
    }
}

/// A frame rate. `Fps::new(30000, 1001)` is 29.97.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Fps(pub Rational);

impl Fps {
    pub fn new(num: i64, den: i64) -> Self {
        Fps(Rational::new(num, den))
    }
    pub fn as_f64(self) -> f64 {
        self.0.as_f64()
    }
    /// Exact start tick of `frame`.
    pub fn frame_to_ticks(self, frame: u32) -> i64 {
        let f = frame as i128;
        ((f * TICKS_PER_SECOND as i128 * self.0.den as i128) / self.0.num as i128) as i64
    }
    /// The frame containing `ticks`.
    pub fn ticks_to_frame(self, ticks: i64) -> u32 {
        if ticks <= 0 {
            return 0;
        }
        let t = ticks as i128;
        ((t * self.0.num as i128) / (TICKS_PER_SECOND as i128 * self.0.den as i128)) as u32
    }
    /// Number of whole frames in a duration, rounded up so a 5.0 s comp is never short.
    pub fn duration_frames(self, ticks: i64) -> u32 {
        if ticks <= 0 {
            return 0;
        }
        let t = ticks as i128;
        let num = t * self.0.num as i128;
        let den = TICKS_PER_SECOND as i128 * self.0.den as i128;
        ((num + den - 1) / den) as u32
    }
}

/// Seconds to ticks. This is the only place a float enters the timeline.
pub fn seconds_to_ticks(seconds: f64) -> i64 {
    (seconds * TICKS_PER_SECOND as f64).round() as i64
}

/// Ticks back to seconds, for display and for the JSON boundary.
pub fn ticks_to_seconds(ticks: i64) -> f64 {
    ticks as f64 / TICKS_PER_SECOND as f64
}

/// SMPTE-style timecode, `HH:MM:SS:FF`, non-drop.
pub fn timecode(ticks: i64, fps: Fps) -> String {
    let fps_round = fps.as_f64().round() as i64;
    let total_frames = if ticks <= 0 {
        0
    } else {
        fps.ticks_to_frame(ticks) as i64
    };
    let f = total_frames % fps_round.max(1);
    let total_seconds = total_frames / fps_round.max(1);
    let s = total_seconds % 60;
    let m = (total_seconds / 60) % 60;
    let h = total_seconds / 3600;
    format!("{h:02}:{m:02}:{s:02}:{f:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rational_reduces_and_normalises_sign() {
        assert_eq!(Rational::new(60000, 2002), Rational::new(30000, 1001));
        assert_eq!(Rational::new(24, -1), Rational::new(-24, 1));
    }

    #[test]
    fn ntsc_frames_are_exact() {
        let fps = Fps::new(30000, 1001);
        // Frame 30000 is (30000 * 1001 / 30000) seconds = 1001 s, exactly.
        assert_eq!(fps.frame_to_ticks(30000), 1001 * TICKS_PER_SECOND);
        assert_eq!(fps.ticks_to_frame(1001 * TICKS_PER_SECOND), 30000);
    }

    #[test]
    fn frame_time_round_trips_at_common_rates() {
        for (num, den) in [(24, 1), (25, 1), (30, 1), (30000, 1001), (60, 1)] {
            let fps = Fps::new(num, den);
            for frame in [0u32, 1, 7, 48, 149, 1000] {
                let ticks = fps.frame_to_ticks(frame);
                assert_eq!(
                    fps.ticks_to_frame(ticks),
                    frame,
                    "rate {num}/{den} frame {frame}"
                );
            }
        }
    }

    #[test]
    fn duration_rounds_up() {
        let fps = Fps::new(30, 1);
        let five_seconds = seconds_to_ticks(5.0);
        assert_eq!(fps.duration_frames(five_seconds), 150);
        // A hair over 5 s still yields 151 frames rather than silently truncating.
        assert_eq!(fps.duration_frames(five_seconds + 1), 151);
    }

    #[test]
    fn timecode_formats() {
        let fps = Fps::new(30, 1);
        assert_eq!(timecode(0, fps), "00:00:00:00");
        assert_eq!(timecode(seconds_to_ticks(61.5), fps), "00:01:01:15");
    }

    #[test]
    fn seconds_and_ticks_are_stable() {
        assert_eq!(seconds_to_ticks(5.0), 450_000);
        assert!((ticks_to_seconds(450_000) - 5.0).abs() < 1e-12);
    }
}
