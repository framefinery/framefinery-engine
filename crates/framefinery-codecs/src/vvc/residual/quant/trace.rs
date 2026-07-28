#[cfg(feature = "vvc-stats")]
fn vvc_tu_trace_sink() -> Option<JsonlInstrumentationSink> {
    match JsonlInstrumentationSink::append_from_env(VVC_TU_TRACE_ENV) {
        Ok(sink) => sink,
        Err(err) => {
            eprintln!("failed to open {VVC_TU_TRACE_ENV}: {err}");
            None
        }
    }
}

#[cfg(feature = "vvc-stats")]
fn write_vvc_luma_tu_trace(
    sink: Option<&mut JsonlInstrumentationSink>,
    region: VvcCtuRegion,
    tu_index: usize,
    node: VvcCodingTreeNode,
    mode: VvcIntraPredictionMode,
    tu: VvcFinalizedLumaTu,
    predicted: &[VvcSample],
    residuals: &[i16],
) {
    let Some(sink) = sink else {
        return;
    };
    let nonzero_ac = tu.ac_levels.iter().filter(|level| **level != 0).count();
    let line = format!(
        "{{\"event\":\"vvc_tu\",\"component\":\"luma\",\"slice\":{},\"tu\":{},\"x\":{},\"y\":{},\"w\":{},\"h\":{},\"mode\":\"{:?}\",\"mode_index\":{},\"transform_skip\":{},\"bdpcm_mode\":\"{:?}\",\"mrl_index\":{},\"mts_index\":{},\"dc\":{},\"has_ac\":{},\"nonzero_ac\":{},\"predicted\":{},\"residuals\":{}}}",
        region.slice_address,
        tu_index,
        node.x,
        node.y,
        node.width,
        node.height,
        mode,
        mode.luma_mode_index(),
        tu.transform_skip,
        tu.bdpcm_mode,
        tu.mrl_index,
        tu.mts_index,
        tu.dc_level,
        tu.has_ac,
        nonzero_ac,
        json_u16_slice(predicted),
        json_i16_slice(residuals),
    );
    if let Err(err) = sink.write_json_line(&line).and_then(|()| sink.flush()) {
        eprintln!("failed to write {VVC_TU_TRACE_ENV}: {err}");
    }
}

#[cfg(feature = "vvc-stats")]
fn write_vvc_chroma_tu_trace(
    sink: Option<&mut JsonlInstrumentationSink>,
    region: VvcCtuRegion,
    tu_index: usize,
    node: VvcCodingTreeNode,
    mode: VvcChromaIntraPredictionMode,
    co_located_luma_mode: VvcIntraPredictionMode,
    tu: VvcFinalizedChromaTu,
    chroma_width: usize,
    chroma_height: usize,
    predicted_cb: &[VvcSample],
    predicted_cr: &[VvcSample],
    cb_residuals: &[i16],
    cr_residuals: &[i16],
) {
    let Some(sink) = sink else {
        return;
    };
    let cb_nonzero_ac = tu.cb_ac_levels.iter().filter(|level| **level != 0).count();
    let cr_nonzero_ac = tu.cr_ac_levels.iter().filter(|level| **level != 0).count();
    let chroma_x = usize::from(node.x);
    let chroma_y = usize::from(node.y);
    let line = format!(
        "{{\"event\":\"vvc_tu\",\"component\":\"chroma\",\"slice\":{},\"tu\":{},\"x\":{},\"y\":{},\"w\":{},\"h\":{},\"chroma_w\":{},\"chroma_h\":{},\"mode\":\"{:?}\",\"co_located_luma_mode\":\"{:?}\",\"co_located_luma_mode_index\":{},\"cb_transform_skip\":{},\"cr_transform_skip\":{},\"bdpcm_mode\":\"{:?}\",\"cb_dc\":{},\"cr_dc\":{},\"cb_has_ac\":{},\"cr_has_ac\":{},\"cb_nonzero_ac\":{},\"cr_nonzero_ac\":{},\"predicted_cb\":{},\"predicted_cr\":{},\"cb_residuals\":{},\"cr_residuals\":{}}}",
        region.slice_address,
        tu_index,
        chroma_x,
        chroma_y,
        node.width,
        node.height,
        chroma_width,
        chroma_height,
        mode,
        co_located_luma_mode,
        co_located_luma_mode.luma_mode_index(),
        tu.cb_transform_skip,
        tu.cr_transform_skip,
        tu.bdpcm_mode,
        tu.cb_dc_level,
        tu.cr_dc_level,
        tu.cb_has_ac,
        tu.cr_has_ac,
        cb_nonzero_ac,
        cr_nonzero_ac,
        json_u16_slice(predicted_cb),
        json_u16_slice(predicted_cr),
        json_i16_slice(cb_residuals),
        json_i16_slice(cr_residuals),
    );
    if let Err(err) = sink.write_json_line(&line).and_then(|()| sink.flush()) {
        eprintln!("failed to write {VVC_TU_TRACE_ENV}: {err}");
    }
}

#[cfg(feature = "vvc-stats")]
fn json_i16_slice(values: &[i16]) -> String {
    let mut out = String::from("[");
    for (idx, value) in values.iter().enumerate() {
        if idx != 0 {
            out.push(',');
        }
        out.push_str(&value.to_string());
    }
    out.push(']');
    out
}

#[cfg(feature = "vvc-stats")]
fn json_u16_slice(values: &[VvcSample]) -> String {
    let mut out = String::from("[");
    for (idx, value) in values.iter().enumerate() {
        if idx != 0 {
            out.push(',');
        }
        out.push_str(&value.to_string());
    }
    out.push(']');
    out
}
