pub const AV2_CODEC_NAME: &str = "av2";
pub const AV2_BITSTREAM_EXTENSION: &str = "av2";
pub const AV2_FIXED_BLACK_444_WIDTH: usize = 64;
pub const AV2_FIXED_BLACK_444_HEIGHT: usize = 64;

pub(crate) type Av2Sample = u16;

const AV2_PROFILE_BITS: u8 = 5;
const AV2_LEVEL_BITS: u8 = 5;
const AV2_SEQUENCE_PROFILE_MAIN_422_10_IP1: u8 = 3;
const AV2_SEQUENCE_PROFILE_MAIN_444_10_IP1: u8 = 4;
const AV2_SEQUENCE_LEVEL_MAX: u8 = 31;
const AV2_CHROMA_FORMAT_420: u32 = 0;
const AV2_CHROMA_FORMAT_444: u32 = 2;
const AV2_CHROMA_FORMAT_422: u32 = 3;
const AV2_BITDEPTH_INDEX_10BIT: u32 = 0;
const AV2_BITDEPTH_INDEX_8BIT: u32 = 1;
const AV2_BITDEPTH_INDEX_12BIT: u32 = 2;
const AV2_DELTA_DCQUANT_MIN: i8 = -23;
const AV2_MAX_MAX_DRL_BITS_MINUS_MIN_PLUS_ONE: u16 = 5;
const AV2_MAX_MAX_IBC_DRL_BITS_MINUS_MIN_PLUS_ONE: u16 = 3;
const AV2_PREDICTIVE_ORDER_HINT_BITS: u8 = 8;
const AV2_MVP_SUPERBLOCK_SIZE: usize = 64;
const AV2_TILE_SIZE_BYTES: usize = 4;
const AV2_MIN_TILE_SIZE_BYTES: usize = 1;
const AV2_MI_SIZE: usize = 4;
const AV2_MIB_SIZE_LOG2_64X64: u8 = 4;
const AV2_SEQ_MIB_SIZE_LOG2_64X64: u8 = 4;
const AV2_MAX_TILE_WIDTH: usize = 4096;
const AV2_MAX_TILE_AREA: usize = 4096 * 2304;
const AV2_MAX_TILE_COLS: usize = 64;
const AV2_MAX_TILE_ROWS: usize = 64;
const AV2_TILE_WIDTH_SCALING_LEVEL_2_0_TIER_0: usize = 4;
const AV2_TILE_AREA_SCALING_LEVEL_2_0_TIER_0: usize = 4;
const AV2_ENABLE_LOSSLESS_SUBSAMPLED_IBC: bool = true;
const AV2_ENABLE_LUMA_PALETTE_INTRABC_444: bool = false;
const AV2_LOSSY_DEFAULT_QP: u8 = 8;
const AV2_COLOR_DESCRIPTION_IDC_SRGB: u32 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Av2ChromaFormat {
    Yuv420,
    Yuv422,
    Yuv444,
}

impl Av2ChromaFormat {
    fn sequence_header_idc(self) -> u32 {
        match self {
            // AV2 v1.0.0 av2/common/blockd.h: CHROMA_FORMAT_420 is coded as
            // zero. This differs from the project-level AXI chroma_format_idc
            // register convention, which follows the older 1/2/3 sampling IDs.
            Self::Yuv420 => AV2_CHROMA_FORMAT_420,
            Self::Yuv422 => AV2_CHROMA_FORMAT_422,
            Self::Yuv444 => AV2_CHROMA_FORMAT_444,
        }
    }

    fn chroma_sampling(self) -> ChromaSampling {
        match self {
            Self::Yuv420 => ChromaSampling::Cs420,
            Self::Yuv422 => ChromaSampling::Cs422,
            Self::Yuv444 => ChromaSampling::Cs444,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Av2StreamFormat {
    chroma_format: Av2ChromaFormat,
    bit_depth: SampleBitDepth,
}

impl Av2StreamFormat {
    fn from_pixel_format(format: PixelFormat) -> Option<Self> {
        if format.is_rgb() {
            return Some(Self {
                chroma_format: Av2ChromaFormat::Yuv444,
                bit_depth: SampleBitDepth::new(8).expect("RGB formats are 8-bit"),
            });
        }
        let bit_depth = format.bit_depth();
        let chroma_format = match (format.chroma_sampling()?, bit_depth.bits()) {
            // AV2 has a 12-bit test-only profile in AVM, but the normal
            // reference-validation profiles support 8/10-bit streams.
            (ChromaSampling::Cs420, 8 | 10) => Av2ChromaFormat::Yuv420,
            (ChromaSampling::Cs422, 8 | 10) => Av2ChromaFormat::Yuv422,
            (ChromaSampling::Cs444, 8 | 10) => Av2ChromaFormat::Yuv444,
            (ChromaSampling::Monochrome, _) => return None,
            (ChromaSampling::Cs420 | ChromaSampling::Cs422 | ChromaSampling::Cs444, _) => {
                return None
            }
        };
        Some(Self {
            chroma_format,
            bit_depth,
        })
    }

    #[cfg(test)]
    fn yuv420_8() -> Self {
        Self {
            chroma_format: Av2ChromaFormat::Yuv420,
            bit_depth: SampleBitDepth::new(8).expect("8-bit depth is supported"),
        }
    }

    #[cfg(test)]
    fn yuv444_8() -> Self {
        Self {
            chroma_format: Av2ChromaFormat::Yuv444,
            bit_depth: SampleBitDepth::new(8).expect("8-bit depth is supported"),
        }
    }

    fn pixel_format(self) -> PixelFormat {
        PixelFormat::planar_yuv(self.chroma_format.chroma_sampling(), self.bit_depth)
    }

    fn sequence_profile_idc(self) -> u8 {
        match self.chroma_format {
            Av2ChromaFormat::Yuv422 => AV2_SEQUENCE_PROFILE_MAIN_422_10_IP1,
            // Profile 4 admits 4:2:0 and 4:4:4 in the AVM reference build.
            Av2ChromaFormat::Yuv420 | Av2ChromaFormat::Yuv444 => {
                AV2_SEQUENCE_PROFILE_MAIN_444_10_IP1
            }
        }
    }

    fn bitdepth_lut_index(self) -> u32 {
        match self.bit_depth.bits() {
            10 => AV2_BITDEPTH_INDEX_10BIT,
            8 => AV2_BITDEPTH_INDEX_8BIT,
            12 => AV2_BITDEPTH_INDEX_12BIT,
            bits => unreachable!("unsupported AV2 bit depth {bits}"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Av2DeltaQParams {
    present: bool,
    resolution_log2: u8,
}

impl Av2DeltaQParams {
    const fn disabled() -> Self {
        Self {
            present: false,
            resolution_log2: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Av2QuantizationParams {
    base_qindex: u16,
    delta_q: Av2DeltaQParams,
    using_qmatrix: bool,
}

impl Av2QuantizationParams {
    const fn lossless() -> Self {
        Self {
            base_qindex: 0,
            delta_q: Av2DeltaQParams::disabled(),
            using_qmatrix: false,
        }
    }

    fn regular_qp(qp: u8, bit_depth: SampleBitDepth) -> Self {
        Self {
            base_qindex: av2_base_qindex_for_qp(qp, bit_depth),
            delta_q: Av2DeltaQParams::disabled(),
            using_qmatrix: false,
        }
    }

    const fn is_coded_lossless(self) -> bool {
        self.base_qindex == 0 && !self.delta_q.present && !self.using_qmatrix
    }
}

fn av2_base_qindex_for_qp(qp: u8, bit_depth: SampleBitDepth) -> u16 {
    let scaled = (u32::from(qp.max(1)) * 10).div_ceil(3);
    (scaled as u16).min(av2_max_qindex(bit_depth))
}

fn av2_predictive_inter_qp_for_qp(qp: u8, bit_depth: SampleBitDepth) -> u8 {
    let qp = u16::from(qp.max(1));
    // Until delta-q is active, changed predictive tiles share one inter-frame
    // qindex. Keep it below the key-frame QP so zero-MV residuals do not spend
    // the accumulated prediction quality budget too aggressively.
    let scaled = if bit_depth.bits() > 8 {
        qp.div_ceil(6)
    } else {
        (qp * 2).div_ceil(3)
    };
    scaled.clamp(1, u16::from(u8::MAX)) as u8
}

fn av2_qindex_bits(bit_depth: SampleBitDepth) -> u8 {
    if bit_depth.bits() == 8 {
        8
    } else {
        9
    }
}

fn av2_max_qindex(bit_depth: SampleBitDepth) -> u16 {
    match bit_depth.bits() {
        8 => 255,
        10 => 255 + 2 * 24,
        12 => 255 + 4 * 24,
        bits => unreachable!("unsupported AV2 bit depth {bits}"),
    }
}

fn av2_lossless_dc_predictor(bit_depth: SampleBitDepth) -> Av2Sample {
    128u16 << u32::from(bit_depth.bits() - 8)
}

fn av2_lossless_h_pred_left_edge(bit_depth: SampleBitDepth) -> Av2Sample {
    av2_lossless_dc_predictor(bit_depth) + 1
}

fn av2_lossless_v_pred_above_edge(bit_depth: SampleBitDepth) -> Av2Sample {
    av2_lossless_dc_predictor(bit_depth) - 1
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Av2Black444MvpProfile {
    enable_sdp: bool,
    enable_ext_partitions: bool,
    enable_uneven_4way_partitions: bool,
    enable_intra_edge_filter: bool,
    enable_mrls: bool,
    enable_cfl_intra: bool,
    enable_mhccp: bool,
    enable_ibp: bool,
    enable_refmvbank: bool,
    is_drl_reorder_disable: bool,
    def_max_bvp_drl_bits_minus_min: u16,
    allow_frame_max_bvp_drl_bits: bool,
    enable_bawp: bool,
    enable_fsc: bool,
    enable_idtx_intra: bool,
    enable_chroma_dctonly: bool,
    enable_cctx: bool,
    disable_cdf_update: bool,
}

impl Av2Black444MvpProfile {
    fn current() -> Self {
        Self {
            // Keep the first tile payload on the shared luma/chroma tree. AVM
            // decode_partition() enters separate luma/chroma trees at 64x64
            // when SDP is enabled, which is unnecessary for the first black
            // 4:4:4 bring-up stream.
            enable_sdp: false,
            enable_ext_partitions: false,
            enable_uneven_4way_partitions: false,
            enable_intra_edge_filter: false,
            enable_mrls: false,
            enable_cfl_intra: false,
            enable_mhccp: false,
            enable_ibp: false,
            enable_refmvbank: false,
            is_drl_reorder_disable: true,
            def_max_bvp_drl_bits_minus_min: 0,
            allow_frame_max_bvp_drl_bits: false,
            enable_bawp: false,
            enable_fsc: true,
            // AVM read_sequence_transform_quant_entropy_group_tool_flags()
            // derives IDTX intra from FSC when FSC is enabled.
            enable_idtx_intra: true,
            // The regular-q writer reconstructs chroma as DCT_DCT until it
            // grows chroma tx-type selection and signaling.
            enable_chroma_dctonly: true,
            enable_cctx: false,
            // AV2 v1.0.0 tile_group_obu() updates CDFs while decode_tile()
            // parses symbols unless this header flag disables adaptation.
            disable_cdf_update: false,
        }
    }

    fn with_local_ibc_candidates(mut self) -> Self {
        // AVM derives above/left 8x8 block vectors as default IntraBC BV
        // candidates 2 and 3 in mvref_common.c. AV2 sequence syntax stores
        // max_bvp_drl_bits minus MIN_MAX_IBC_DRL_BITS; value 2 therefore
        // permits DRL indices 0..3 without frame-level overrides.
        self.def_max_bvp_drl_bits_minus_min = 2;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Av2ObuType {
    SequenceHeader = 1,
    TemporalDelimiter = 2,
    ClosedLoopKey = 4,
    RegularTileGroup = 7,
    RegularSef = 12,
    ContentInterpretation = 24,
}
