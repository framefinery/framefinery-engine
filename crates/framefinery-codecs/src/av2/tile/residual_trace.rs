#[cfg(feature = "av2-lossy-stats")]
fn inter_residual_trace_enabled() -> bool {
    std::env::var_os("FRAMEFINERY_AV2_INTER_RESIDUAL_TRACE").is_some_and(|value| value != "0")
}

#[cfg(not(feature = "av2-lossy-stats"))]
#[inline(always)]
fn inter_residual_trace_enabled() -> bool {
    false
}

fn trace_inter_txb(
    decision: Av2TileDecision,
    plane: &'static str,
    x0: usize,
    y0: usize,
    row: usize,
    col: usize,
    skip_ctx: u8,
    dc_sign_ctx: Option<u8>,
    source_zero: bool,
    quantized_zero: bool,
    eob: usize,
    coefficients: Option<&[i32; TX4X4_SAMPLES]>,
) {
    if inter_residual_trace_enabled() {
        let mut coeffs = String::new();
        if let Some(coefficients) = coefficients {
            for scan_index in 0..eob {
                let pos = TX4X4_SCAN[scan_index];
                if scan_index != 0 {
                    coeffs.push(',');
                }
                coeffs.push_str(&format!("{pos}:{}", coefficients[pos] / 8));
            }
        }
        eprintln!(
            "FRAMEFINERY_AV2_INTER_RESIDUAL_TRACE mi=({},{}) local_mi=({},{}) plane={} txb=({},{}) skip_ctx={} dc_sign_ctx={} source_zero={} quantized_zero={} eob={} qcoeffs_scan={}",
            y0 / MI_SIZE,
            x0 / MI_SIZE,
            decision.row,
            decision.col,
            plane,
            row,
            col,
            skip_ctx,
            dc_sign_ctx.map_or_else(|| "n/a".to_string(), |ctx| ctx.to_string()),
            source_zero,
            quantized_zero,
            eob,
            coeffs
        );
    }
}

fn trace_inter_candidate_residual(
    plane: &'static str,
    x0: usize,
    y0: usize,
    base_qindex: u16,
    candidate: &Av2LossyQuantizedResidualCandidate,
) {
    if inter_residual_trace_enabled() {
        let residual = candidate
            .residual
            .iter()
            .map(i32::to_string)
            .collect::<Vec<_>>()
            .join(",");
        eprintln!(
            "FRAMEFINERY_AV2_INTER_RESIDUAL_CANDIDATE mi=({},{}) plane={} base_qindex={} kind={:?} residual={}",
            y0 / MI_SIZE,
            x0 / MI_SIZE,
            plane,
            base_qindex,
            candidate.kind,
            residual
        );
    }
}
