fn vvc_luma_mpm_list(
    left: Option<VvcIntraPredictionMode>,
    above: Option<VvcIntraPredictionMode>,
) -> [u8; VVC_NUM_MOST_PROBABLE_LUMA_MODES] {
    let left = left
        .unwrap_or(VvcIntraPredictionMode::Planar)
        .luma_mode_index();
    let above = above
        .unwrap_or(VvcIntraPredictionMode::Planar)
        .luma_mode_index();
    let min = left.min(above);
    let max = left.max(above);
    let mut mpm = [0; VVC_NUM_MOST_PROBABLE_LUMA_MODES];
    mpm[0] = VvcIntraPredictionMode::Planar.luma_mode_index();
    if max < VVC_LUMA_ANGULAR_BASE as u8 {
        mpm[1] = VvcIntraPredictionMode::Dc.luma_mode_index();
        mpm[2] = VvcIntraPredictionMode::Vertical.luma_mode_index();
        mpm[3] = VvcIntraPredictionMode::Horizontal.luma_mode_index();
        mpm[4] = vvc_wrap_luma_angular_mode(
            i16::from(VvcIntraPredictionMode::Vertical.luma_mode_index()) - 4,
        );
        mpm[5] = vvc_wrap_luma_angular_mode(
            i16::from(VvcIntraPredictionMode::Vertical.luma_mode_index()) + 4,
        );
        return mpm;
    }
    if left == above || min < VVC_LUMA_ANGULAR_BASE as u8 {
        mpm[1] = max;
        mpm[2] = vvc_wrap_luma_angular_mode(i16::from(max) - 1);
        mpm[3] = vvc_wrap_luma_angular_mode(i16::from(max) + 1);
        mpm[4] = vvc_wrap_luma_angular_mode(i16::from(max) - 2);
        mpm[5] = vvc_wrap_luma_angular_mode(i16::from(max) + 2);
        return mpm;
    }

    mpm[1] = left;
    mpm[2] = above;
    let diff = max - min;
    if diff == 1 {
        mpm[3] = vvc_wrap_luma_angular_mode(i16::from(min) - 1);
        mpm[4] = vvc_wrap_luma_angular_mode(i16::from(max) + 1);
        mpm[5] = vvc_wrap_luma_angular_mode(i16::from(min) - 2);
    } else if diff >= VVC_NUM_INTRA_ANGULAR_MODES as u8 - 3 {
        mpm[3] = vvc_wrap_luma_angular_mode(i16::from(min) + 1);
        mpm[4] = vvc_wrap_luma_angular_mode(i16::from(max) - 1);
        mpm[5] = vvc_wrap_luma_angular_mode(i16::from(min) + 2);
    } else if diff == 2 {
        mpm[3] = vvc_wrap_luma_angular_mode(i16::from(min) + 1);
        mpm[4] = vvc_wrap_luma_angular_mode(i16::from(min) - 1);
        mpm[5] = vvc_wrap_luma_angular_mode(i16::from(max) + 1);
    } else {
        mpm[3] = vvc_wrap_luma_angular_mode(i16::from(min) - 1);
        mpm[4] = vvc_wrap_luma_angular_mode(i16::from(min) + 1);
        mpm[5] = vvc_wrap_luma_angular_mode(i16::from(max) - 1);
    }
    mpm
}

#[cfg(test)]
pub(in crate::vvc) fn vvc_luma_mpm_list_for_test(
    left: Option<VvcIntraPredictionMode>,
    above: Option<VvcIntraPredictionMode>,
) -> [u8; VVC_NUM_MOST_PROBABLE_LUMA_MODES] {
    vvc_luma_mpm_list(left, above)
}

pub(in crate::vvc) fn vvc_luma_intra_mode_syntax_bin_count(
    mode: VvcIntraPredictionMode,
    left: Option<VvcIntraPredictionMode>,
    above: Option<VvcIntraPredictionMode>,
) -> u8 {
    let mode_index = mode.luma_mode_index();
    let mpm = vvc_luma_mpm_list(left, above);
    if let Some(mpm_idx) = vvc_luma_mpm_index_for_mode_index(mode_index, mpm) {
        let bypass_bins = if mpm_idx == 0 { 0 } else { mpm_idx.min(4) };
        return 2 + bypass_bins as u8;
    }

    1 + vvc_trunc_bin_code_ep_bin_count(
        vvc_luma_remaining_mode_index(mode_index, mpm),
        VVC_REMAINING_LUMA_MODE_COUNT,
    )
}

pub(in crate::vvc) fn vvc_luma_intra_mode_is_mpm(
    mode: VvcIntraPredictionMode,
    left: Option<VvcIntraPredictionMode>,
    above: Option<VvcIntraPredictionMode>,
) -> bool {
    vvc_luma_mpm_index_for_mode_index(mode.luma_mode_index(), vvc_luma_mpm_list(left, above))
        .is_some()
}

fn vvc_luma_mpm_index_for_mode_index(
    mode_index: u8,
    mpm: [u8; VVC_NUM_MOST_PROBABLE_LUMA_MODES],
) -> Option<usize> {
    mpm.iter().position(|candidate| *candidate == mode_index)
}

pub(in crate::vvc) fn vvc_chroma_intra_mode_syntax_bin_count(
    mode: VvcChromaIntraPredictionMode,
    cclm_enabled: bool,
) -> u8 {
    let cclm_flag_bins = u8::from(cclm_enabled);
    match mode {
        VvcChromaIntraPredictionMode::Cclm(cclm_mode) => {
            debug_assert!(cclm_enabled);
            cclm_flag_bins
                + 1
                + match cclm_mode {
                    VvcChromaCclmMode::Linear => 0,
                    VvcChromaCclmMode::MdlmLeft | VvcChromaCclmMode::MdlmTop => 1,
                }
        }
        VvcChromaIntraPredictionMode::Derived => cclm_flag_bins + 1,
        VvcChromaIntraPredictionMode::Explicit(_) => cclm_flag_bins + 3,
    }
}

fn vvc_wrap_luma_angular_mode(mode: i16) -> u8 {
    ((mode - VVC_LUMA_ANGULAR_BASE).rem_euclid(VVC_NUM_INTRA_ANGULAR_MODE_WRAP)
        + VVC_LUMA_ANGULAR_BASE) as u8
}

fn vvc_luma_remaining_mode_index(
    mode_index: u8,
    mut mpm: [u8; VVC_NUM_MOST_PROBABLE_LUMA_MODES],
) -> u32 {
    let mut remaining = u32::from(mode_index);
    mpm.sort_unstable();
    for candidate in mpm.into_iter().rev() {
        if remaining > u32::from(candidate) {
            remaining -= 1;
        }
    }
    debug_assert!(remaining < VVC_REMAINING_LUMA_MODE_COUNT);
    remaining
}

fn vvc_trunc_bin_code_ep_bin_count(symbol: u32, num_symbols: u32) -> u8 {
    debug_assert!(symbol < num_symbols);
    let thresh = 31 - num_symbols.leading_zeros();
    let val = 1 << thresh;
    let b = num_symbols - val;
    if symbol < val - b {
        thresh as u8
    } else {
        (thresh + 1) as u8
    }
}

fn encode_vvc_trunc_bin_code_ep(cabac: &mut VvcCabacEncoder, symbol: u32, num_symbols: u32) {
    debug_assert!(symbol < num_symbols);
    let thresh = 31 - num_symbols.leading_zeros();
    let val = 1 << thresh;
    let b = num_symbols - val;
    if symbol < val - b {
        cabac.encode_bins_ep(symbol, thresh);
    } else {
        cabac.encode_bins_ep(symbol + val - b, thresh + 1);
    }
}
