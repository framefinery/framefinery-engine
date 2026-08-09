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
    residuals.reserve(predicted.len());
    let max_x = frame.geometry.width - 1;
    let max_y = frame.geometry.height - 1;
    for y in 0..height {
        let src_y = (origin_y + y).min(max_y);
        let src_row = src_y * frame.geometry.width;
        let dst = y * width;
        for x in 0..width {
            let src_x = (origin_x + x).min(max_x);
            residuals.push(vvc_sample_delta_i16(
                frame.luma[src_row + src_x],
                predicted[dst + x],
            ));
        }
    }
    debug_assert_eq!(residuals.len(), predicted.len());
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
    residuals.reserve(predicted.len());
    let max_x = chroma_width - 1;
    let max_y = chroma_height - 1;
    for y in 0..height {
        let src_y = (origin_y + y).min(max_y);
        let src_row = src_y * chroma_width;
        let dst = y * width;
        for x in 0..width {
            let src_x = (origin_x + x).min(max_x);
            residuals.push(vvc_sample_delta_i16(
                samples[src_row + src_x],
                predicted[dst + x],
            ));
        }
    }
    debug_assert_eq!(residuals.len(), predicted.len());
}

fn vvc_sample_delta_i16(sample: VvcSample, predicted: VvcSample) -> i16 {
    (i32::from(sample) - i32::from(predicted)).clamp(i32::from(i16::MIN), i32::from(i16::MAX))
        as i16
}
