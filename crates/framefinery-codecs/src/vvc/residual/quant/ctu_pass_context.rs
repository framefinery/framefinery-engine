#[derive(Debug, Clone, Copy)]
struct VvcCtuSharedPassContext<'a> {
    source_frame: &'a VvcSampledFrame,
    region: VvcCtuRegion,
    policy: VvcResidualCodingPolicy,
    score_metric: VvcResidualScoreMetric,
    inter_reference: Option<&'a VvcReconstructionFrame>,
    temporal_mode_hints: Option<&'a VvcQuantizedColor>,
    ctu_shape: VvcCtuPartitionShape,
}
