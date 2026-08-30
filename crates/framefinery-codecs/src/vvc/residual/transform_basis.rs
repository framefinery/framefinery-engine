fn dct2_value(size: u16, k: usize, n: usize) -> i32 {
    if k == 0 {
        return 64;
    }
    match size {
        4 => VVC_DCT2_4[k][n],
        8 => VVC_DCT2_8[k][n],
        16 if k <= 3 => VVC_DCT2_16_AC_ROWS_1_TO_3[k - 1][n],
        32 if k <= 3 => VVC_DCT2_32_AC_ROWS_1_TO_3[k - 1][n],
        16 | 32 => {
            debug_assert!(
                false,
                "DCT-II AC subset for size {size} is not wired for coefficient {k}"
            );
            0
        }
        other => {
            debug_assert!(false, "DCT-II matrix size {other} is not wired yet");
            0
        }
    }
}

fn dct2_row(size: u16, k: usize) -> &'static [i32] {
    match size {
        4 => &VVC_DCT2_4[k],
        8 => &VVC_DCT2_8[k],
        16 if k == 0 => &VVC_DCT2_DC_16,
        16 if k <= 3 => &VVC_DCT2_16_AC_ROWS_1_TO_3[k - 1],
        32 if k == 0 => &VVC_DCT2_DC_32,
        32 if k <= 3 => &VVC_DCT2_32_AC_ROWS_1_TO_3[k - 1],
        16 | 32 => {
            debug_assert!(
                false,
                "DCT-II AC subset for size {size} is not wired for coefficient {k}"
            );
            &[]
        }
        other => {
            debug_assert!(false, "DCT-II matrix size {other} is not wired yet");
            &[]
        }
    }
}

fn vvc_luma_mts_transform_pair(mts_index: u8) -> VvcLumaTransformPair {
    match mts_index {
        0 => VvcLumaTransformPair {
            horizontal: VvcLumaTransformType::Dct2,
            vertical: VvcLumaTransformType::Dct2,
        },
        2 => VvcLumaTransformPair {
            horizontal: VvcLumaTransformType::Dst7,
            vertical: VvcLumaTransformType::Dst7,
        },
        3 => VvcLumaTransformPair {
            horizontal: VvcLumaTransformType::Dct8,
            vertical: VvcLumaTransformType::Dst7,
        },
        4 => VvcLumaTransformPair {
            horizontal: VvcLumaTransformType::Dst7,
            vertical: VvcLumaTransformType::Dct8,
        },
        5 => VvcLumaTransformPair {
            horizontal: VvcLumaTransformType::Dct8,
            vertical: VvcLumaTransformType::Dct8,
        },
        _ => {
            debug_assert!(false, "unsupported VVC luma MTS index {mts_index}");
            VvcLumaTransformPair {
                horizontal: VvcLumaTransformType::Dct2,
                vertical: VvcLumaTransformType::Dct2,
            }
        }
    }
}

fn vvc_luma_mts_index_supported(mts_index: u8) -> bool {
    matches!(mts_index, 0 | 2..=5)
}

fn normalize_vvc_luma_mts_index(mts_index: u8) -> u8 {
    if vvc_luma_mts_index_supported(mts_index) {
        mts_index
    } else {
        debug_assert!(false, "unsupported VVC luma MTS index {mts_index}");
        0
    }
}

fn vvc_luma_transform_value(transform: VvcLumaTransformType, size: u16, k: usize, n: usize) -> i32 {
    match transform {
        VvcLumaTransformType::Dct2 => dct2_value(size, k, n),
        VvcLumaTransformType::Dst7 => dst7_value(size, k, n),
        VvcLumaTransformType::Dct8 => dct8_value(size, k, n),
    }
}

fn dct8_value(size: u16, k: usize, n: usize) -> i32 {
    match size {
        4 => dct8_value_4(k, n),
        8 => dct8_value_8(k, n),
        other => {
            debug_assert!(false, "DCT-VIII matrix size {other} is not wired yet");
            0
        }
    }
}

fn dst7_value(size: u16, k: usize, n: usize) -> i32 {
    match size {
        4 => dst7_value_4(k, n),
        8 => dst7_value_8(k, n),
        other => {
            debug_assert!(false, "DST-VII matrix size {other} is not wired yet");
            0
        }
    }
}

fn signed_basis(base: &[i32], one_based_index: i8) -> i32 {
    match one_based_index.cmp(&0) {
        std::cmp::Ordering::Greater => base[one_based_index as usize - 1],
        std::cmp::Ordering::Less => -base[(-one_based_index) as usize - 1],
        std::cmp::Ordering::Equal => 0,
    }
}

fn dct8_value_4(k: usize, n: usize) -> i32 {
    const ROWS: [[i8; 4]; 4] = [[1, 2, 3, 4], [2, 0, -2, -2], [3, -2, -4, 1], [4, -2, 1, -3]];
    signed_basis(&VVC_DCT8_4_BASE, ROWS[k][n])
}

fn dct8_value_8(k: usize, n: usize) -> i32 {
    const ROWS: [[i8; 8]; 8] = [
        [1, 2, 3, 4, 5, 6, 7, 8],
        [2, 5, 8, -7, -4, -1, -3, -6],
        [3, 8, -5, -1, -6, 7, 2, 4],
        [4, -7, -1, -8, 3, 5, -6, -2],
        [5, -4, -6, 3, 7, -2, -8, 1],
        [6, -1, 7, 5, -2, 8, 4, -3],
        [7, -3, 2, -6, -8, 4, -1, 5],
        [8, -6, 4, -2, 1, -3, 5, -7],
    ];
    signed_basis(&VVC_DCT8_8_BASE, ROWS[k][n])
}

fn dst7_value_4(k: usize, n: usize) -> i32 {
    const ROWS: [[i8; 4]; 4] = [[1, 2, 3, 4], [3, 3, 0, -3], [4, -1, -3, 2], [2, -4, 3, -1]];
    signed_basis(&VVC_DST7_4_BASE, ROWS[k][n])
}

fn dst7_value_8(k: usize, n: usize) -> i32 {
    const ROWS: [[i8; 8]; 8] = [
        [1, 2, 3, 4, 5, 6, 7, 8],
        [3, 6, 8, 5, 2, -1, -4, -7],
        [5, 7, 2, -3, -8, -4, 1, 6],
        [7, 3, -4, -6, 1, 8, 2, -5],
        [8, -1, -7, 2, 6, -3, -5, 4],
        [6, -5, -1, 7, -4, -2, 8, -3],
        [4, -8, 5, -1, -3, 7, -6, 2],
        [2, -4, 6, -8, 7, -5, 3, -1],
    ];
    signed_basis(&VVC_DST7_8_BASE, ROWS[k][n])
}
