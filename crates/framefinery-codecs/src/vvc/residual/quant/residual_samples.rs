pub(in crate::vvc) fn residual_luma_tu_at_into(
    residuals: &mut Vec<i16>,
    frame: &VvcSampledFrame,
    origin_x: usize,
    origin_y: usize,
    width: usize,
    height: usize,
    predicted: &[VvcSample],
) {
    debug_assert_eq!(predicted.len(), width * height);
    let copy_width = width.min(frame.geometry.width.saturating_sub(origin_x));
    let copy_height = height.min(frame.geometry.height.saturating_sub(origin_y));
    residuals.clear();
    if copy_width == width && copy_height == height {
        residuals.reserve(predicted.len());
        for y in 0..height {
            let src = (origin_y + y) * frame.geometry.width + origin_x;
            let dst = y * width;
            for (sample, predicted) in frame.luma[src..src + width]
                .iter()
                .zip(&predicted[dst..dst + width])
            {
                residuals.push(vvc_sample_delta_i16(*sample, *predicted));
            }
        }
        debug_assert_eq!(residuals.len(), predicted.len());
        return;
    }
    residuals.extend(
        predicted
            .iter()
            .map(|predicted| vvc_sample_delta_i16(0, *predicted)),
    );
    for y in 0..copy_height {
        let src = (origin_y + y) * frame.geometry.width + origin_x;
        let dst = y * width;
        for ((residual, sample), predicted) in residuals[dst..dst + copy_width]
            .iter_mut()
            .zip(&frame.luma[src..src + copy_width])
            .zip(&predicted[dst..dst + copy_width])
        {
            *residual = vvc_sample_delta_i16(*sample, *predicted);
        }
    }
}

pub(in crate::vvc) fn residual_chroma_tu_at_into(
    residuals: &mut Vec<i16>,
    samples: &[VvcSample],
    geometry: VvcVideoGeometry,
    format: VvcPictureFormat,
    origin_x: usize,
    origin_y: usize,
    width: usize,
    height: usize,
    predicted: &[VvcSample],
) {
    debug_assert_eq!(predicted.len(), width * height);
    let chroma_width = geometry.width / chroma_subsample_x(format.chroma_sampling);
    let chroma_height = geometry.height / chroma_subsample_y(format.chroma_sampling);
    let neutral = vvc_neutral_sample(format.bit_depth);
    let copy_width = width.min(chroma_width.saturating_sub(origin_x));
    let copy_height = height.min(chroma_height.saturating_sub(origin_y));
    residuals.clear();
    if copy_width == width && copy_height == height {
        residuals.reserve(predicted.len());
        for y in 0..height {
            let src = (origin_y + y) * chroma_width + origin_x;
            let dst = y * width;
            for (sample, predicted) in samples[src..src + width]
                .iter()
                .zip(&predicted[dst..dst + width])
            {
                residuals.push(vvc_sample_delta_i16(*sample, *predicted));
            }
        }
        debug_assert_eq!(residuals.len(), predicted.len());
        return;
    }
    residuals.extend(
        predicted
            .iter()
            .map(|predicted| vvc_sample_delta_i16(neutral, *predicted)),
    );
    for y in 0..copy_height {
        let src = (origin_y + y) * chroma_width + origin_x;
        let dst = y * width;
        for ((residual, sample), predicted) in residuals[dst..dst + copy_width]
            .iter_mut()
            .zip(&samples[src..src + copy_width])
            .zip(&predicted[dst..dst + copy_width])
        {
            *residual = vvc_sample_delta_i16(*sample, *predicted);
        }
    }
}

fn vvc_sample_delta_i16(sample: VvcSample, predicted: VvcSample) -> i16 {
    (i32::from(sample) - i32::from(predicted)).clamp(i32::from(i16::MIN), i32::from(i16::MAX))
        as i16
}
