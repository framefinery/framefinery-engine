fn av2_dc_predictor_from_edges<EdgeSample>(
    bit_depth: SampleBitDepth,
    tile_origin_x: usize,
    tile_origin_y: usize,
    x0: usize,
    y0: usize,
    mut edge_sample: EdgeSample,
) -> Av2Sample
where
    EdgeSample: FnMut(usize, usize) -> Av2Sample,
{
    let have_left = x0 > tile_origin_x;
    let have_top = y0 > tile_origin_y;
    if !have_left && !have_top {
        return av2_lossless_dc_predictor(bit_depth);
    }

    let mut sum = 0u32;
    let mut count = 0u32;
    if have_top {
        for x in x0..(x0 + TX4X4_SIZE) {
            sum += u32::from(edge_sample(x, y0 - 1));
            count += 1;
        }
    }
    if have_left {
        for y in y0..(y0 + TX4X4_SIZE) {
            sum += u32::from(edge_sample(x0 - 1, y));
            count += 1;
        }
    }
    av2_reference_dc_average(sum, count)
}

fn av2_h_predictor_from_edges<EdgeSample>(
    bit_depth: SampleBitDepth,
    tile_origin_x: usize,
    tile_origin_y: usize,
    x0: usize,
    y0: usize,
    local_y: usize,
    mut edge_sample: EdgeSample,
) -> Av2Sample
where
    EdgeSample: FnMut(usize, usize) -> Av2Sample,
{
    if x0 > tile_origin_x {
        edge_sample(x0 - 1, y0 + local_y)
    } else if y0 > tile_origin_y {
        edge_sample(x0, y0 - 1)
    } else {
        av2_lossless_h_pred_left_edge(bit_depth)
    }
}

fn av2_v_predictor_from_edges<EdgeSample>(
    bit_depth: SampleBitDepth,
    tile_origin_x: usize,
    tile_origin_y: usize,
    x0: usize,
    y0: usize,
    local_x: usize,
    mut edge_sample: EdgeSample,
) -> Av2Sample
where
    EdgeSample: FnMut(usize, usize) -> Av2Sample,
{
    if y0 > tile_origin_y {
        edge_sample(x0 + local_x, y0 - 1)
    } else if x0 > tile_origin_x {
        edge_sample(x0 - 1, y0)
    } else {
        av2_lossless_v_pred_above_edge(bit_depth)
    }
}

fn av2_above_left_predictor_from_edges<EdgeSample>(
    bit_depth: SampleBitDepth,
    tile_origin_x: usize,
    tile_origin_y: usize,
    x0: usize,
    y0: usize,
    mut edge_sample: EdgeSample,
) -> Av2Sample
where
    EdgeSample: FnMut(usize, usize) -> Av2Sample,
{
    let have_left = x0 > tile_origin_x;
    let have_top = y0 > tile_origin_y;
    if have_left && have_top {
        edge_sample(x0 - 1, y0 - 1)
    } else if have_top {
        edge_sample(x0, y0 - 1)
    } else if have_left {
        edge_sample(x0 - 1, y0)
    } else {
        av2_lossless_dc_predictor(bit_depth)
    }
}
