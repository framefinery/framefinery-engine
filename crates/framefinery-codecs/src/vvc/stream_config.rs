// Validated, fixed configuration shared by every frame of one VVC stream.
struct VvcStreamConfig {
    geometry: VvcVideoGeometry,
    frame_limit: FrameLimit,
    format: PixelFormat,
    stream_format: VvcPictureFormat,
    stream_frame_layout: PlanarYuvFrameLayout,
    frame_len: usize,
    ctu_cols: usize,
    ctu_count: usize,
    residual_mode: VvcResidualCodingMode,
    residual_policy: VvcResidualCodingPolicy,
    predictive_enabled: bool,
    ctu_sliced_partitioning_supported: bool,
    lossy_ctu_skip_enabled: bool,
    slice_config: VvcSliceSyntaxConfig,
    scc_444_enabled: bool,
    predictive_single_slice_config: Option<VvcSliceSyntaxConfig>,
    luma_qp: i32,
    chroma_qp: i32,
    transform_skip_quant_tables: VvcTransformSkipQuantTables,
    explicit_inter_eligible_luma_leaf_count: usize,
    options: VvcEncodeOptions,
}

impl VvcStreamConfig {
    fn new(
        request: VvcEncodeRequest,
        options: VvcEncodeOptions,
    ) -> Result<(Self, Vec<VvcNalUnit>), String> {
        let format = request.format;
        let request = request.validate()?;
        let geometry = request.geometry;
        let frame_limit = request.frame_limit;
        let stream_format = request.format;
        let stream_layout = PlanarYuvGeometry::new(
            geometry.width,
            geometry.height,
            stream_format.chroma_sampling,
            stream_format.bit_depth,
        )?;
        let stream_frame_layout = PlanarYuvFrameLayout::for_validated_shape(
            geometry.width,
            geometry.height,
            stream_format.chroma_sampling,
            stream_format.bit_depth,
        );
        let frame_len = stream_layout.frame_len();
        let ctu_cols = vvc_picture_ctu_cols(geometry);
        let ctu_count = vvc_picture_ctu_count(geometry);
        let residual_mode = VvcResidualCodingMode::for_encode_options(options);
        let residual_policy = VvcResidualCodingPolicy::new(stream_format, residual_mode)
            .with_fast_search(options.fast_search);
        let predictive_enabled = options.gop.is_predictive();
        let ctu_sliced_partitioning_supported =
            vvc_one_slice_per_ctu_partitioning_supported(geometry);
        let lossy_ctu_skip_enabled = predictive_enabled
            && !residual_mode.is_lossless()
            && vvc_lossy_predictive_ctu_inter_skip_enabled_for_reference_clean_release();
        let mut slice_config = vvc_slice_config_for_input_format(
            residual_mode.slice_config(stream_format, options.qp, options.fast_search),
            format,
        );
        if residual_mode.is_lossless() && options.fast_search == VvcFastSearch::LosslessSpeed {
            slice_config = slice_config.without_lossless_speed_unused_tools();
        }
        slice_config =
            slice_config.with_validated_profile_for_format(options.profile, stream_format)?;
        let scc_444_enabled = VVC_SCC_IBC_PRODUCTION_ENABLED
            && !predictive_enabled
            && residual_mode.is_lossless()
            && options.fast_search == VvcFastSearch::LosslessSpeed
            && stream_format.chroma_sampling == ChromaSampling::Cs444
            && !format.is_rgb()
            && slice_config.profile.allows_ibc();
        if scc_444_enabled {
            slice_config = slice_config.with_scc_444_tools();
        }
        let picture_partitioning = if predictive_enabled && ctu_sliced_partitioning_supported {
            VvcPicturePartitioning::OneSlicePerCtu
        } else {
            residual_mode.picture_partitioning()
        };
        if predictive_enabled {
            slice_config = slice_config.with_inter_enabled();
        }
        let predictive_single_slice_config = predictive_enabled.then(|| {
            slice_config
                .with_picture_parameter_set_id(VVC_PREDICTIVE_SINGLE_SLICE_PPS_ID)
                .without_picture_header_slice_state()
        });
        let luma_qp = slice_config.slice_qp;
        let chroma_qp = if residual_mode.is_lossless() {
            slice_config.slice_qp
        } else {
            vvc_lossy_chroma_qp_for_slice_qp(luma_qp)
        };
        let transform_skip_quant_tables =
            VvcTransformSkipQuantTables::new(format.bit_depth(), luma_qp, chroma_qp);
        let explicit_inter_eligible_luma_leaf_count =
            if stream_format.chroma_sampling == ChromaSampling::Cs444 {
                vvc_explicit_inter_eligible_luma_leaf_count(geometry, stream_format.chroma_sampling)
            } else {
                0
            };
        let emit_base_picture_parameter_set =
            !predictive_enabled || ctu_sliced_partitioning_supported;
        let mut parameter_sets = Vec::with_capacity(
            1 + usize::from(emit_base_picture_parameter_set)
                + usize::from(predictive_single_slice_config.is_some()),
        );
        parameter_sets.push(vvc_sps_unit(
            geometry,
            slice_config,
            stream_format.bit_depth,
        ));
        if emit_base_picture_parameter_set {
            parameter_sets.push(vvc_pps_unit_with_partitioning_and_config(
                geometry,
                picture_partitioning,
                slice_config,
            ));
        }
        if let Some(single_slice_config) = predictive_single_slice_config {
            parameter_sets.push(vvc_pps_unit_with_partitioning_and_config(
                geometry,
                VvcPicturePartitioning::SingleSlice,
                single_slice_config,
            ));
        }
        Ok((
            Self {
                geometry,
                frame_limit,
                format,
                stream_format,
                stream_frame_layout,
                frame_len,
                ctu_cols,
                ctu_count,
                residual_mode,
                residual_policy,
                predictive_enabled,
                ctu_sliced_partitioning_supported,
                lossy_ctu_skip_enabled,
                slice_config,
                scc_444_enabled,
                predictive_single_slice_config,
                luma_qp,
                chroma_qp,
                transform_skip_quant_tables,
                explicit_inter_eligible_luma_leaf_count,
                options,
            },
            parameter_sets,
        ))
    }
}
