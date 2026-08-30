#[derive(Clone, Copy)]
struct Av2IntraEdgeContext {
    bit_depth: SampleBitDepth,
    tile_origin: (usize, usize),
    plane_size: (usize, usize),
    edge_limit: (usize, usize),
    subsampling: (usize, usize),
    leaf_origin: (usize, usize),
    leaf_size: (usize, usize),
}

fn av2_directional_above_edge<EdgeSample, IsCoded>(
    context: Av2IntraEdgeContext,
    x0: usize,
    y0: usize,
    mut edge_sample: EdgeSample,
    mut is_coded: IsCoded,
) -> [Av2Sample; 8]
where
    EdgeSample: FnMut(usize, usize) -> Av2Sample,
    IsCoded: FnMut(usize, usize) -> bool,
{
    let (tile_origin_x, tile_origin_y) = context.tile_origin;
    let (plane_width, _) = context.plane_size;
    let (edge_right, _) = context.edge_limit;
    let (sub_x, sub_y) = context.subsampling;
    let (leaf_x0, leaf_y0) = context.leaf_origin;
    let (leaf_width, _) = context.leaf_size;
    let have_top = y0 > tile_origin_y;
    let have_left = x0 > tile_origin_x;
    let mut above = [av2_lossless_v_pred_above_edge(context.bit_depth); 8];
    if have_top {
        let plane_sb_width = MVP_SUPERBLOCK_SIZE / sub_x;
        let plane_sb_height = MVP_SUPERBLOCK_SIZE / sub_y;
        let sb_origin_x = (x0 / plane_sb_width) * plane_sb_width;
        let sb_right = (sb_origin_x + plane_sb_width)
            .min(plane_width)
            .min(edge_right);
        let superblock_top_row = y0 % plane_sb_height == 0;
        for index in 0..above.len() {
            let x = x0 + index;
            let overhang = index >= TX4X4_SIZE;
            let external_top_right_coded = overhang && y0 == leaf_y0 && x < edge_right && {
                superblock_top_row || (x < sb_right && is_coded(x, y0 - 1))
            };
            if x < edge_right
                && (!overhang || x < leaf_x0 + leaf_width || external_top_right_coded)
            {
                above[index] = edge_sample(x, y0 - 1);
            } else if index > 0 {
                above[index] = above[index - 1];
            }
        }
    } else if have_left {
        above.fill(edge_sample(x0 - 1, y0));
    }
    above
}

fn av2_directional_left_edge<EdgeSample, IsCoded>(
    context: Av2IntraEdgeContext,
    x0: usize,
    y0: usize,
    mut edge_sample: EdgeSample,
    mut is_coded: IsCoded,
) -> [Av2Sample; 8]
where
    EdgeSample: FnMut(usize, usize) -> Av2Sample,
    IsCoded: FnMut(usize, usize) -> bool,
{
    let (tile_origin_x, tile_origin_y) = context.tile_origin;
    let (_, plane_height) = context.plane_size;
    let (_, edge_bottom) = context.edge_limit;
    let (sub_x, sub_y) = context.subsampling;
    let (leaf_x0, leaf_y0) = context.leaf_origin;
    let (_, leaf_height) = context.leaf_size;
    let have_top = y0 > tile_origin_y;
    let have_left = x0 > tile_origin_x;
    let mut left = [av2_lossless_h_pred_left_edge(context.bit_depth); 8];
    if have_left {
        let plane_sb_width = MVP_SUPERBLOCK_SIZE / sub_x;
        let plane_sb_height = MVP_SUPERBLOCK_SIZE / sub_y;
        let sb_origin_y = (y0 / plane_sb_height) * plane_sb_height;
        let sb_bottom = (sb_origin_y + plane_sb_height)
            .min(plane_height)
            .min(edge_bottom);
        let superblock_left_col = x0 % plane_sb_width == 0;
        for index in 0..left.len() {
            let y = y0 + index;
            let overhang = index >= TX4X4_SIZE;
            let external_bottom_left_coded =
                overhang && x0 == leaf_x0 && y < sb_bottom && {
                    superblock_left_col || is_coded(x0 - 1, y)
                };
            if y < edge_bottom
                && (!overhang
                    || (x0 == leaf_x0
                        && (y < leaf_y0 + leaf_height || external_bottom_left_coded)))
            {
                left[index] = edge_sample(x0 - 1, y);
            } else if index > 0 {
                left[index] = left[index - 1];
            }
        }
    } else if have_top {
        left.fill(edge_sample(x0, y0 - 1));
    }
    left
}

fn av2_smooth_intra_edges<EdgeSample, IsCoded>(
    context: Av2IntraEdgeContext,
    x0: usize,
    y0: usize,
    mut edge_sample: EdgeSample,
    mut is_coded: IsCoded,
) -> ([Av2Sample; TX4X4_SIZE + 1], [Av2Sample; TX4X4_SIZE + 1])
where
    EdgeSample: FnMut(usize, usize) -> Av2Sample,
    IsCoded: FnMut(usize, usize) -> bool,
{
    let (tile_origin_x, tile_origin_y) = context.tile_origin;
    let (plane_width, plane_height) = context.plane_size;
    let (plane_region_right, plane_region_bottom) = context.edge_limit;
    let (sub_x, sub_y) = context.subsampling;
    let (leaf_x0, leaf_y0) = context.leaf_origin;
    let (leaf_width, leaf_height) = context.leaf_size;
    let have_top = y0 > tile_origin_y;
    let have_left = x0 > tile_origin_x;
    let mut above = [av2_lossless_v_pred_above_edge(context.bit_depth); TX4X4_SIZE + 1];
    let mut left = [av2_lossless_h_pred_left_edge(context.bit_depth); TX4X4_SIZE + 1];

    if have_top {
        for local_x in 0..TX4X4_SIZE {
            above[local_x] = edge_sample(x0 + local_x, y0 - 1);
        }
    } else if have_left {
        above[..TX4X4_SIZE].fill(edge_sample(x0 - 1, y0));
    }

    if have_left {
        for local_y in 0..TX4X4_SIZE {
            left[local_y] = edge_sample(x0 - 1, y0 + local_y);
        }
    } else if have_top {
        left[..TX4X4_SIZE].fill(edge_sample(x0, y0 - 1));
    }

    let plane_sb_width = MVP_SUPERBLOCK_SIZE / sub_x;
    let plane_sb_height = MVP_SUPERBLOCK_SIZE / sub_y;
    let sb_origin_x = (x0 / plane_sb_width) * plane_sb_width;
    let sb_right = (sb_origin_x + plane_sb_width)
        .min(plane_width)
        .min(plane_region_right);
    let top_right_x = x0 + TX4X4_SIZE;
    let superblock_top_row = y0 % plane_sb_height == 0;
    let external_top_right_coded =
        have_top && y0 == leaf_y0 && top_right_x < plane_region_right && {
            superblock_top_row || (top_right_x < sb_right && is_coded(top_right_x, y0 - 1))
        };
    if have_top
        && top_right_x < plane_region_right
        && (top_right_x < leaf_x0 + leaf_width || external_top_right_coded)
    {
        above[TX4X4_SIZE] = edge_sample(top_right_x, y0 - 1);
    } else {
        above[TX4X4_SIZE] = above[TX4X4_SIZE - 1];
    }

    let sb_origin_y = (y0 / plane_sb_height) * plane_sb_height;
    let sb_bottom = (sb_origin_y + plane_sb_height)
        .min(plane_height)
        .min(plane_region_bottom);
    let bottom_left_y = y0 + TX4X4_SIZE;
    let superblock_left_col = x0 % plane_sb_width == 0;
    let external_bottom_left_coded =
        have_left && x0 == leaf_x0 && bottom_left_y < sb_bottom && {
            superblock_left_col || is_coded(x0 - 1, bottom_left_y)
        };
    if have_left
        && x0 == leaf_x0
        && bottom_left_y < plane_region_bottom
        && (bottom_left_y < leaf_y0 + leaf_height || external_bottom_left_coded)
    {
        left[TX4X4_SIZE] = edge_sample(x0 - 1, bottom_left_y);
    } else {
        left[TX4X4_SIZE] = left[TX4X4_SIZE - 1];
    }

    (above, left)
}

#[cfg(test)]
mod prediction_edge_tests {
    use super::*;

    fn context(
        edge_limit: (usize, usize),
        leaf_origin: (usize, usize),
        leaf_size: (usize, usize),
    ) -> Av2IntraEdgeContext {
        Av2IntraEdgeContext {
            bit_depth: SampleBitDepth::new(8).expect("8-bit depth is supported"),
            tile_origin: (0, 0),
            plane_size: (16, 16),
            edge_limit,
            subsampling: (1, 1),
            leaf_origin,
            leaf_size,
        }
    }

    fn sample(x: usize, y: usize) -> Av2Sample {
        (y * 16 + x) as Av2Sample
    }

    #[test]
    fn directional_edges_use_spec_fallbacks_at_tile_origin() {
        let edge_context = context((16, 16), (0, 0), (8, 8));
        let above = av2_directional_above_edge(
            edge_context,
            0,
            0,
            |_, _| panic!("tile-origin fallback must not read an edge sample"),
            |_, _| false,
        );
        let left = av2_directional_left_edge(
            edge_context,
            0,
            0,
            |_, _| panic!("tile-origin fallback must not read an edge sample"),
            |_, _| false,
        );

        assert_eq!(
            above,
            [av2_lossless_v_pred_above_edge(edge_context.bit_depth); 8]
        );
        assert_eq!(
            left,
            [av2_lossless_h_pred_left_edge(edge_context.bit_depth); 8]
        );
    }

    #[test]
    fn directional_edges_admit_only_coded_external_overhang_samples() {
        let edge_context = context((16, 16), (4, 4), (4, 4));
        let above = av2_directional_above_edge(edge_context, 4, 4, sample, |x, y| {
            matches!((x, y), (8, 3))
        });
        let left = av2_directional_left_edge(edge_context, 4, 4, sample, |x, y| {
            matches!((x, y), (3, 8))
        });

        assert_eq!(above, [52, 53, 54, 55, 56, 56, 56, 56]);
        assert_eq!(left, [67, 83, 99, 115, 131, 131, 131, 131]);
    }

    #[test]
    fn directional_edges_respect_the_explicit_edge_limit() {
        let edge_context = context((8, 8), (4, 4), (4, 4));
        let above = av2_directional_above_edge(
            edge_context,
            4,
            4,
            sample,
            |_, _| panic!("edge clipping must precede coded-neighbour lookup"),
        );
        let left = av2_directional_left_edge(
            edge_context,
            4,
            4,
            sample,
            |_, _| panic!("edge clipping must precede coded-neighbour lookup"),
        );

        assert_eq!(above, [52, 53, 54, 55, 55, 55, 55, 55]);
        assert_eq!(left, [67, 83, 99, 115, 115, 115, 115, 115]);
    }

    #[test]
    fn smooth_edges_use_spec_fallbacks_at_tile_origin() {
        let edge_context = context((16, 16), (0, 0), (8, 8));
        let (above, left) = av2_smooth_intra_edges(
            edge_context,
            0,
            0,
            |_, _| panic!("tile-origin fallback must not read an edge sample"),
            |_, _| false,
        );

        assert_eq!(
            above,
            [av2_lossless_v_pred_above_edge(edge_context.bit_depth); TX4X4_SIZE + 1]
        );
        assert_eq!(
            left,
            [av2_lossless_h_pred_left_edge(edge_context.bit_depth); TX4X4_SIZE + 1]
        );
    }

    #[test]
    fn smooth_edges_read_interior_top_right_and_bottom_left_samples() {
        let (above, left) = av2_smooth_intra_edges(
            context((16, 16), (4, 4), (8, 8)),
            4,
            4,
            sample,
            |_, _| false,
        );

        assert_eq!(above, [52, 53, 54, 55, 56]);
        assert_eq!(left, [67, 83, 99, 115, 131]);
    }

    #[test]
    fn smooth_edges_admit_coded_external_overhang_samples() {
        let (above, left) = av2_smooth_intra_edges(
            context((16, 16), (4, 4), (4, 4)),
            4,
            4,
            sample,
            |x, y| matches!((x, y), (8, 3) | (3, 8)),
        );

        assert_eq!(above, [52, 53, 54, 55, 56]);
        assert_eq!(left, [67, 83, 99, 115, 131]);
    }

    #[test]
    fn smooth_edges_repeat_the_last_sample_at_region_limits() {
        let (above, left) = av2_smooth_intra_edges(
            context((8, 8), (4, 4), (4, 4)),
            4,
            4,
            sample,
            |_, _| panic!("region clipping must precede coded-neighbour lookup"),
        );

        assert_eq!(above, [52, 53, 54, 55, 55]);
        assert_eq!(left, [67, 83, 99, 115, 115]);
    }
}
