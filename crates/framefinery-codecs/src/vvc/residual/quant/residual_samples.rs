pub(in crate::vvc) fn reconstructed_residual_sse(
    source_residuals: &[i16],
    reconstructed_residuals: &[i16],
) -> u64 {
    source_residuals
        .iter()
        .zip(reconstructed_residuals)
        .map(|(&source, &reconstructed)| {
            let diff = i64::from(source) - i64::from(reconstructed);
            (diff * diff) as u64
        })
        .sum()
}

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
    let _ = residual_chroma_tu_at_into_impl::<false>(
        residuals, samples, geometry, format, origin_x, origin_y, width, height, predicted,
    );
}

pub(in crate::vvc) fn residual_chroma_pair_tu_at_into(
    cb_residuals: &mut Vec<i16>,
    cr_residuals: &mut Vec<i16>,
    cb_samples: &[VvcSample],
    cr_samples: &[VvcSample],
    geometry: VvcVideoGeometry,
    format: VvcPictureFormat,
    origin_x: usize,
    origin_y: usize,
    width: usize,
    height: usize,
    predicted_cb: &[VvcSample],
    predicted_cr: &[VvcSample],
) {
    let _ = residual_chroma_pair_tu_at_into_impl::<false>(
        cb_residuals,
        cr_residuals,
        cb_samples,
        cr_samples,
        geometry,
        format,
        origin_x,
        origin_y,
        width,
        height,
        predicted_cb,
        predicted_cr,
    );
}

pub(in crate::vvc) fn residual_chroma_pair_tu_at_into_and_detect_zero(
    cb_residuals: &mut Vec<i16>,
    cr_residuals: &mut Vec<i16>,
    cb_samples: &[VvcSample],
    cr_samples: &[VvcSample],
    geometry: VvcVideoGeometry,
    format: VvcPictureFormat,
    origin_x: usize,
    origin_y: usize,
    width: usize,
    height: usize,
    predicted_cb: &[VvcSample],
    predicted_cr: &[VvcSample],
) -> (bool, bool) {
    residual_chroma_pair_tu_at_into_impl::<true>(
        cb_residuals,
        cr_residuals,
        cb_samples,
        cr_samples,
        geometry,
        format,
        origin_x,
        origin_y,
        width,
        height,
        predicted_cb,
        predicted_cr,
    )
}

fn residual_chroma_pair_tu_at_into_impl<const TRACK_ZERO: bool>(
    cb_residuals: &mut Vec<i16>,
    cr_residuals: &mut Vec<i16>,
    cb_samples: &[VvcSample],
    cr_samples: &[VvcSample],
    geometry: VvcVideoGeometry,
    format: VvcPictureFormat,
    origin_x: usize,
    origin_y: usize,
    width: usize,
    height: usize,
    predicted_cb: &[VvcSample],
    predicted_cr: &[VvcSample],
) -> (bool, bool) {
    debug_assert_eq!(predicted_cb.len(), width * height);
    debug_assert_eq!(predicted_cr.len(), width * height);
    let chroma_width = geometry.width / chroma_subsample_x(format.chroma_sampling);
    let chroma_height = geometry.height / chroma_subsample_y(format.chroma_sampling);
    let copy_width = width.min(chroma_width.saturating_sub(origin_x));
    let copy_height = height.min(chroma_height.saturating_sub(origin_y));
    cb_residuals.clear();
    cr_residuals.clear();
    cb_residuals.reserve(width * height);
    cr_residuals.reserve(width * height);
    let mut cb_all_zero = true;
    let mut cr_all_zero = true;
    if copy_width == width && copy_height == height {
        for y in 0..height {
            let src = (origin_y + y) * chroma_width + origin_x;
            let dst = y * width;
            for x in 0..width {
                let cb = vvc_sample_delta_i16(cb_samples[src + x], predicted_cb[dst + x]);
                let cr = vvc_sample_delta_i16(cr_samples[src + x], predicted_cr[dst + x]);
                if TRACK_ZERO {
                    cb_all_zero &= cb == 0;
                    cr_all_zero &= cr == 0;
                }
                cb_residuals.push(cb);
                cr_residuals.push(cr);
            }
        }
        return (cb_all_zero, cr_all_zero);
    }
    let max_x = chroma_width - 1;
    let max_y = chroma_height - 1;
    for y in 0..height {
        let src_y = (origin_y + y).min(max_y);
        let src_row = src_y * chroma_width;
        let dst = y * width;
        for x in 0..width {
            let src_x = (origin_x + x).min(max_x);
            let cb = vvc_sample_delta_i16(cb_samples[src_row + src_x], predicted_cb[dst + x]);
            let cr = vvc_sample_delta_i16(cr_samples[src_row + src_x], predicted_cr[dst + x]);
            if TRACK_ZERO {
                cb_all_zero &= cb == 0;
                cr_all_zero &= cr == 0;
            }
            cb_residuals.push(cb);
            cr_residuals.push(cr);
        }
    }
    (cb_all_zero, cr_all_zero)
}

pub(in crate::vvc) fn residual_chroma_tu_at_into_and_detect_zero(
    residuals: &mut Vec<i16>,
    samples: &[VvcSample],
    geometry: VvcVideoGeometry,
    format: VvcPictureFormat,
    origin_x: usize,
    origin_y: usize,
    width: usize,
    height: usize,
    predicted: &[VvcSample],
) -> bool {
    residual_chroma_tu_at_into_impl::<true>(
        residuals, samples, geometry, format, origin_x, origin_y, width, height, predicted,
    )
}

fn residual_chroma_tu_at_into_impl<const TRACK_ZERO: bool>(
    residuals: &mut Vec<i16>,
    samples: &[VvcSample],
    geometry: VvcVideoGeometry,
    format: VvcPictureFormat,
    origin_x: usize,
    origin_y: usize,
    width: usize,
    height: usize,
    predicted: &[VvcSample],
) -> bool {
    debug_assert_eq!(predicted.len(), width * height);
    let chroma_width = geometry.width / chroma_subsample_x(format.chroma_sampling);
    let chroma_height = geometry.height / chroma_subsample_y(format.chroma_sampling);
    let copy_width = width.min(chroma_width.saturating_sub(origin_x));
    let copy_height = height.min(chroma_height.saturating_sub(origin_y));
    residuals.clear();
    if copy_width == width && copy_height == height {
        residuals.reserve(predicted.len());
        let mut all_zero = true;
        for y in 0..height {
            let src = (origin_y + y) * chroma_width + origin_x;
            let dst = y * width;
            for (sample, predicted) in samples[src..src + width]
                .iter()
                .zip(&predicted[dst..dst + width])
            {
                let residual = vvc_sample_delta_i16(*sample, *predicted);
                if TRACK_ZERO {
                    all_zero &= residual == 0;
                }
                residuals.push(residual);
            }
        }
        debug_assert_eq!(residuals.len(), predicted.len());
        return all_zero;
    }
    residuals.reserve(predicted.len());
    let max_x = chroma_width - 1;
    let max_y = chroma_height - 1;
    let mut all_zero = true;
    for y in 0..height {
        let src_y = (origin_y + y).min(max_y);
        let src_row = src_y * chroma_width;
        let dst = y * width;
        for x in 0..width {
            let src_x = (origin_x + x).min(max_x);
            let residual = vvc_sample_delta_i16(samples[src_row + src_x], predicted[dst + x]);
            if TRACK_ZERO {
                all_zero &= residual == 0;
            }
            residuals.push(residual);
        }
    }
    debug_assert_eq!(residuals.len(), predicted.len());
    all_zero
}

fn vvc_sample_delta_i16(sample: VvcSample, predicted: VvcSample) -> i16 {
    (i32::from(sample) - i32::from(predicted)).clamp(i32::from(i16::MIN), i32::from(i16::MAX))
        as i16
}
