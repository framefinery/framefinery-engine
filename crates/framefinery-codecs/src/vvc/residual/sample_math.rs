use super::super::VvcSample;

#[inline]
pub(super) fn vvc_sample_delta_i16(sample: VvcSample, predicted: VvcSample) -> i16 {
    (i32::from(sample) - i32::from(predicted)).clamp(i32::from(i16::MIN), i32::from(i16::MAX))
        as i16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_delta_preserves_in_range_residuals() {
        assert_eq!(vvc_sample_delta_i16(100, 40), 60);
        assert_eq!(vvc_sample_delta_i16(40, 100), -60);
        assert_eq!(vvc_sample_delta_i16(73, 73), 0);
    }

    #[test]
    fn sample_delta_clamps_to_residual_storage_range() {
        assert_eq!(vvc_sample_delta_i16(VvcSample::MAX, 0), i16::MAX);
        assert_eq!(vvc_sample_delta_i16(0, VvcSample::MAX), i16::MIN);
    }
}
