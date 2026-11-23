/// Sets above this many reps estimate the 1RM as if they had this many.
const MAX_ESTIMATION_REPS: u16 = 20;

/// Load step, in tenths of a kg, used to round when the exercise has no weight increment.
const DEFAULT_INCREMENT: u16 = 5;

/// Estimated 1RM of a set, truncated to whole kg: Brzycki up to 10 reps, a linear fit beyond.
/// Unilateral sets count both sides, so half their reps. Mirrors `ActivityUtils.estimate1rm`.
pub fn estimate_1rm(weight: f32, reps: u16, unilateral: bool) -> f32 {
    let reps = if unilateral { reps / 2 } else { reps };
    if reps <= 1 {
        return weight.trunc();
    }

    let r = reps.min(MAX_ESTIMATION_REPS) as f64;
    let weight = weight as f64;
    let estimate = if r <= 10.0 {
        weight * 36.0 / (37.0 - r)
    } else {
        100.0 * weight / (101.3 - 2.67123 * r)
    };
    estimate.trunc() as f32
}

/// `pct` percent of `one_rm` (kg), in tenths of a kg, rounded to the nearest multiple of
/// `increment` (tenths of a kg; 0.5 kg without one).
pub fn percent_load(one_rm: f32, pct: u8, increment: u16) -> u16 {
    let increment = if increment > 0 {
        increment
    } else {
        DEFAULT_INCREMENT
    };
    // one_rm * pct / 100 kg, times 10 for tenths
    let load = one_rm * f32::from(pct) / 10.0;
    ((load / f32::from(increment)).round() as u16).saturating_mul(increment)
}
