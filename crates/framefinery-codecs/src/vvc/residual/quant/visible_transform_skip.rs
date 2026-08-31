#[allow(clippy::too_many_arguments)]
fn fill_visible_transform_skip_samples(
    plane: &mut [VvcSample],
    plane_stride: usize,
    start_x: usize,
    start_y: usize,
    visible_width: usize,
    visible_height: usize,
    node_width: usize,
    active_width: usize,
    active_height: usize,
    predicted: &[VvcSample],
    residual_dc: i16,
    residual_ac: &[i16],
    bdpcm_mode: VvcBdpcmMode,
    bit_depth: SampleBitDepth,
    quant_table: &VvcTransformSkipQuantTable,
) {
    let max_sample = i32::from(bit_depth.max_sample());
    if bdpcm_mode.is_enabled() {
        let active_rows = visible_height.min(active_height);
        let mut level_state = VvcBdpcmLevelState::default();
        for local_y in 0..active_rows {
            let row = (start_y + local_y) * plane_stride + start_x;
            let predicted_row = local_y * node_width;
            level_state.begin_row();
            for local_x in 0..active_width {
                let delta = if local_x == 0 && local_y == 0 {
                    residual_dc
                } else {
                    residual_ac[local_y * active_width + local_x - 1]
                };
                let level = level_state.reconstruct(bdpcm_mode, local_x, local_y, delta);
                if local_x < visible_width {
                    let idx = predicted_row + local_x;
                    plane[row + local_x] = (i32::from(predicted[idx])
                        + i32::from(quant_table.reconstructed(level)))
                    .clamp(0, max_sample) as VvcSample;
                }
            }
            for local_x in active_width..visible_width {
                let idx = predicted_row + local_x;
                plane[row + local_x] = predicted[idx];
            }
        }
        for local_y in active_rows..visible_height {
            let row = (start_y + local_y) * plane_stride + start_x;
            let predicted_row = local_y * node_width;
            plane[row..row + visible_width]
                .copy_from_slice(&predicted[predicted_row..predicted_row + visible_width]);
        }
        return;
    }

    for local_y in 0..visible_height {
        let row = (start_y + local_y) * plane_stride + start_x;
        let predicted_row = local_y * node_width;
        for local_x in 0..visible_width {
            let reconstructed_residual = if local_y < active_height && local_x < active_width {
                let level = if local_x == 0 && local_y == 0 {
                    residual_dc
                } else {
                    residual_ac[local_y * active_width + local_x - 1]
                };
                quant_table.reconstructed(level)
            } else {
                0
            };
            let idx = predicted_row + local_x;
            plane[row + local_x] = (i32::from(predicted[idx]) + i32::from(reconstructed_residual))
                .clamp(0, max_sample) as VvcSample;
        }
    }
}
