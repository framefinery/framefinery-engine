#[derive(Debug, Clone, PartialEq, Eq)]
struct Av2TxbEntropyContexts {
    y_above: Vec<u8>,
    y_left: Vec<u8>,
    u_above: Vec<u8>,
    u_left: Vec<u8>,
    v_above: Vec<u8>,
    v_left: Vec<u8>,
}

impl Av2TxbEntropyContexts {
    fn new(visible_rows_mi: usize, visible_cols_mi: usize) -> Self {
        Self {
            y_above: vec![0; visible_cols_mi],
            y_left: vec![0; visible_rows_mi],
            u_above: vec![0; visible_cols_mi],
            u_left: vec![0; visible_rows_mi],
            v_above: vec![0; visible_cols_mi],
            v_left: vec![0; visible_rows_mi],
        }
    }

    fn clear_leaf(
        &mut self,
        row_mi: usize,
        col_mi: usize,
        block_size: Av2MvpBlockSize,
        visible_rows_mi: usize,
        visible_cols_mi: usize,
        chroma_format: Av2ChromaFormat,
    ) {
        let txb_width = block_size
            .tx4x4_width()
            .min(visible_cols_mi.saturating_sub(col_mi));
        let txb_height = block_size
            .tx4x4_height()
            .min(visible_rows_mi.saturating_sub(row_mi));
        for col in col_mi..(col_mi + txb_width).min(self.y_above.len()) {
            self.y_above[col] = 0;
        }
        for row in row_mi..(row_mi + txb_height).min(self.y_left.len()) {
            self.y_left[row] = 0;
        }

        let chroma_span = chroma_tx4x4_span(
            Av2TileDecision {
                kind: Av2TileDecisionKind::Partition(Av2MvpPartition::None),
                row: row_mi,
                col: col_mi,
                block_size,
            },
            visible_rows_mi,
            visible_cols_mi,
            chroma_format,
        );
        for col in
            chroma_span.col..(chroma_span.col + chroma_span.width).min(self.u_above.len())
        {
            self.u_above[col] = 0;
            self.v_above[col] = 0;
        }
        for row in chroma_span.row..(chroma_span.row + chroma_span.height).min(self.u_left.len()) {
            self.u_left[row] = 0;
            self.v_left[row] = 0;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Av2IntrabcContext {
    coded: Vec<bool>,
    ibc: Vec<bool>,
    skip: Vec<bool>,
    rows: usize,
    cols: usize,
}

impl Av2IntrabcContext {
    fn new(visible_rows_mi: usize, visible_cols_mi: usize) -> Self {
        Self {
            coded: vec![false; visible_rows_mi * visible_cols_mi],
            ibc: vec![false; visible_rows_mi * visible_cols_mi],
            skip: vec![false; visible_rows_mi * visible_cols_mi],
            rows: visible_rows_mi,
            cols: visible_cols_mi,
        }
    }

    fn intrabc_ctx(&self, row_mi: usize, col_mi: usize, block_size: Av2MvpBlockSize) -> usize {
        // AV2 v1.0.0 read_intra_frame_mode_info()/get_intrabc_ctx(): the
        // context is derived from the first two available spatial neighbors
        // in AVM's bottom-left, above-right, left, above scan. At a 64x64 SB
        // top boundary AVM suppresses above/above-right for this context.
        self.neighbor_sum(row_mi, col_mi, block_size, true, |state| state.ibc)
    }

    fn skip_txfm_ctx(&self, row_mi: usize, col_mi: usize, block_size: Av2MvpBlockSize) -> usize {
        // AV2 v1.0.0 read_skip_txfm()/get_txb_ctx() uses neighboring
        // skip_txfm state from the same two-neighbor scan, but the line-buffer
        // variant keeps above/above-right available at SB top boundaries.
        self.neighbor_sum(row_mi, col_mi, block_size, false, |state| state.skip)
    }

    fn update_leaf(
        &mut self,
        row_mi: usize,
        col_mi: usize,
        block_size: Av2MvpBlockSize,
        use_intrabc: bool,
        skip_txfm: bool,
    ) {
        for row in row_mi..(row_mi + block_size.mi_height()).min(self.rows) {
            for col in col_mi..(col_mi + block_size.mi_width()).min(self.cols) {
                let index = row * self.cols + col;
                self.coded[index] = true;
                self.ibc[index] = use_intrabc;
                self.skip[index] = skip_txfm;
            }
        }
    }

    fn neighbor_sum(
        &self,
        row_mi: usize,
        col_mi: usize,
        block_size: Av2MvpBlockSize,
        suppress_above_at_sb_top: bool,
        value: impl Fn(Av2IntrabcNeighborState) -> bool,
    ) -> usize {
        let not_at_sb_top_boundary = row_mi % PARTITION_CONTEXT_DIM != 0;
        let include_above = !suppress_above_at_sb_top || not_at_sb_top_boundary;
        let mut count = 0usize;
        let mut sum = 0usize;

        let mut push = |state: Option<Av2IntrabcNeighborState>| {
            if count >= 2 {
                return;
            }
            if let Some(state) = state {
                sum += usize::from(value(state));
                count += 1;
            }
        };

        push(self.bottom_left_state(row_mi, col_mi, block_size));
        if include_above {
            push(self.above_right_state(row_mi, col_mi, block_size));
        }
        push(self.left_state(row_mi, col_mi));
        if include_above {
            push(self.above_state(row_mi, col_mi));
        }
        sum
    }

    fn state_at(&self, row_mi: usize, col_mi: usize) -> Option<Av2IntrabcNeighborState> {
        if row_mi >= self.rows || col_mi >= self.cols {
            return None;
        }
        let index = row_mi * self.cols + col_mi;
        self.coded[index].then_some(Av2IntrabcNeighborState {
            ibc: self.ibc[index],
            skip: self.skip[index],
        })
    }

    fn bottom_left_state(
        &self,
        row_mi: usize,
        col_mi: usize,
        block_size: Av2MvpBlockSize,
    ) -> Option<Av2IntrabcNeighborState> {
        col_mi
            .checked_sub(1)
            .and_then(|col| self.state_at(row_mi + block_size.mi_height().saturating_sub(1), col))
    }

    fn above_right_state(
        &self,
        row_mi: usize,
        col_mi: usize,
        block_size: Av2MvpBlockSize,
    ) -> Option<Av2IntrabcNeighborState> {
        row_mi
            .checked_sub(1)
            .and_then(|row| self.state_at(row, col_mi + block_size.mi_width().saturating_sub(1)))
    }

    fn left_state(&self, row_mi: usize, col_mi: usize) -> Option<Av2IntrabcNeighborState> {
        col_mi
            .checked_sub(1)
            .and_then(|col| self.state_at(row_mi, col))
    }

    fn above_state(&self, row_mi: usize, col_mi: usize) -> Option<Av2IntrabcNeighborState> {
        row_mi
            .checked_sub(1)
            .and_then(|row| self.state_at(row, col_mi))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Av2IntrabcNeighborState {
    ibc: bool,
    skip: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Av2InterModeContext {
    coded: Vec<bool>,
    inter: Vec<bool>,
    ref_frame: Vec<u8>,
    newmv: Vec<bool>,
    mv_row_px: Vec<i16>,
    mv_col_px: Vec<i16>,
    rows: usize,
    cols: usize,
}

impl Av2InterModeContext {
    fn new(visible_rows_mi: usize, visible_cols_mi: usize) -> Self {
        Self {
            coded: vec![false; visible_rows_mi * visible_cols_mi],
            inter: vec![false; visible_rows_mi * visible_cols_mi],
            ref_frame: vec![0; visible_rows_mi * visible_cols_mi],
            newmv: vec![false; visible_rows_mi * visible_cols_mi],
            mv_row_px: vec![0; visible_rows_mi * visible_cols_mi],
            mv_col_px: vec![0; visible_rows_mi * visible_cols_mi],
            rows: visible_rows_mi,
            cols: visible_cols_mi,
        }
    }

    fn intra_inter_ctx(&self, row_mi: usize, col_mi: usize) -> usize {
        let left = self.left_state(row_mi, col_mi);
        let above = self.above_state(row_mi, col_mi);
        match (left, above) {
            (Some(left), Some(above)) => match (!left.inter, !above.inter) {
                (true, true) => 3,
                (true, false) | (false, true) => 1,
                (false, false) => 0,
            },
            (Some(neighbor), None) | (None, Some(neighbor)) => {
                if neighbor.inter {
                    0
                } else {
                    3
                }
            }
            (None, None) => 0,
        }
    }

    fn inter_single_mode_ctx(
        &self,
        row_mi: usize,
        col_mi: usize,
        block_size: Av2MvpBlockSize,
        ref_frame: u8,
    ) -> usize {
        let row_state = self
            .above_right_state(row_mi, col_mi, block_size)
            .or_else(|| self.above_state(row_mi, col_mi));
        let col_state = self
            .bottom_left_state(row_mi, col_mi, block_size)
            .or_else(|| self.left_state(row_mi, col_mi));
        let row_match = row_state.is_some_and(|state| state.inter && state.ref_frame == ref_frame);
        let col_match = col_state.is_some_and(|state| state.inter && state.ref_frame == ref_frame);
        let newmv_count = [row_state, col_state]
            .into_iter()
            .flatten()
            .filter(|state| state.inter && state.ref_frame == ref_frame && state.newmv)
            .count();
        usize::from(row_match) + usize::from(col_match) + usize::from(newmv_count > 0) * 2
    }

    fn single_ref_ctx(
        &self,
        row_mi: usize,
        col_mi: usize,
        ref_frame: u8,
        total_refs: usize,
    ) -> usize {
        let mut this_ref_count = 0usize;
        let mut next_refs_count = 0usize;
        for state in [self.left_state(row_mi, col_mi), self.above_state(row_mi, col_mi)]
            .into_iter()
            .flatten()
            .filter(|state| state.inter)
        {
            if state.ref_frame == ref_frame {
                this_ref_count += 1;
            } else if usize::from(state.ref_frame) > usize::from(ref_frame)
                && usize::from(state.ref_frame) < total_refs
            {
                next_refs_count += 1;
            }
        }
        match this_ref_count.cmp(&next_refs_count) {
            std::cmp::Ordering::Less => 0,
            std::cmp::Ordering::Equal => 1,
            std::cmp::Ordering::Greater => 2,
        }
    }

    fn update_leaf(
        &mut self,
        row_mi: usize,
        col_mi: usize,
        block_size: Av2MvpBlockSize,
        inter: bool,
        ref_frame: u8,
        newmv: bool,
        mv_row_px: i16,
        mv_col_px: i16,
    ) {
        for row in row_mi..(row_mi + block_size.mi_height()).min(self.rows) {
            for col in col_mi..(col_mi + block_size.mi_width()).min(self.cols) {
                let index = row * self.cols + col;
                self.coded[index] = true;
                self.inter[index] = inter;
                self.ref_frame[index] = ref_frame;
                self.newmv[index] = newmv;
                self.mv_row_px[index] = mv_row_px;
                self.mv_col_px[index] = mv_col_px;
            }
        }
    }

    fn nearest_ref_mv(
        &self,
        row_mi: usize,
        col_mi: usize,
        block_size: Av2MvpBlockSize,
        ref_frame: u8,
    ) -> (i16, i16) {
        [
            self.left_state(row_mi, col_mi),
            self.above_state(row_mi, col_mi),
            self.above_right_state(row_mi, col_mi, block_size),
            self.bottom_left_state(row_mi, col_mi, block_size),
        ]
        .into_iter()
        .flatten()
        .find(|state| state.inter && state.ref_frame == ref_frame)
        .map(|state| (state.mv_row_px, state.mv_col_px))
        .unwrap_or((0, 0))
    }

    fn state_at(&self, row_mi: usize, col_mi: usize) -> Option<Av2InterNeighborState> {
        if row_mi >= self.rows || col_mi >= self.cols {
            return None;
        }
        let index = row_mi * self.cols + col_mi;
        self.coded[index].then_some(Av2InterNeighborState {
            inter: self.inter[index],
            ref_frame: self.ref_frame[index],
            newmv: self.newmv[index],
            mv_row_px: self.mv_row_px[index],
            mv_col_px: self.mv_col_px[index],
        })
    }

    fn bottom_left_state(
        &self,
        row_mi: usize,
        col_mi: usize,
        block_size: Av2MvpBlockSize,
    ) -> Option<Av2InterNeighborState> {
        col_mi
            .checked_sub(1)
            .and_then(|col| self.state_at(row_mi + block_size.mi_height().saturating_sub(1), col))
    }

    fn above_right_state(
        &self,
        row_mi: usize,
        col_mi: usize,
        block_size: Av2MvpBlockSize,
    ) -> Option<Av2InterNeighborState> {
        row_mi
            .checked_sub(1)
            .and_then(|row| self.state_at(row, col_mi + block_size.mi_width().saturating_sub(1)))
    }

    fn left_state(&self, row_mi: usize, col_mi: usize) -> Option<Av2InterNeighborState> {
        col_mi
            .checked_sub(1)
            .and_then(|col| self.state_at(row_mi, col))
    }

    fn above_state(&self, row_mi: usize, col_mi: usize) -> Option<Av2InterNeighborState> {
        row_mi
            .checked_sub(1)
            .and_then(|row| self.state_at(row, col_mi))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Av2ActiveInterLeaf {
    Skip { row: usize, col: usize },
    Residual {
        row: usize,
        col: usize,
        mv_row_px: i16,
        mv_col_px: i16,
    },
}

impl Av2ActiveInterLeaf {
    fn origin(self) -> (usize, usize) {
        match self {
            Self::Skip { row, col } | Self::Residual { row, col, .. } => (row, col),
        }
    }
}

fn active_inter_leaf_matches(
    active_inter_leaf: Option<Av2ActiveInterLeaf>,
    decision: Av2TileDecision,
) -> Option<Av2ActiveInterLeaf> {
    let active_inter_leaf = active_inter_leaf?;
    let (row, col) = active_inter_leaf.origin();
    (row == decision.row && col == decision.col).then_some(active_inter_leaf)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Av2InterNeighborState {
    inter: bool,
    ref_frame: u8,
    newmv: bool,
    mv_row_px: i16,
    mv_col_px: i16,
}

fn write_inter_globalmv_skip(
    writer: &mut Av2EntropyWriter,
    decision: Av2TileDecision,
    skip_context: &Av2IntrabcContext,
    inter_context: &Av2InterModeContext,
    total_refs: usize,
) {
    write_inter_intra_flag(writer, decision, inter_context, true);

    let skip_ctx = skip_context.skip_txfm_ctx(decision.row, decision.col, decision.block_size);
    let mut skip_cdf = DEFAULT_SKIP_TXFM_CDFS[skip_ctx];
    writer.write_symbol_with_static_cdf_key(
        "tile.inter.skip_txfm",
        inter_skip_txfm_static_cdf_key(skip_ctx),
        1,
        &mut skip_cdf,
        2,
        false,
    );

    if total_refs > 1 {
        let single_ref_ctx = inter_context.single_ref_ctx(decision.row, decision.col, 0, total_refs);
        let mut single_ref_cdf = DEFAULT_SINGLE_REF_CDFS[single_ref_ctx][0];
        writer.write_symbol_with_static_cdf_key(
            "tile.inter.single_ref",
            inter_single_ref_static_cdf_key(single_ref_ctx),
            1,
            &mut single_ref_cdf,
            2,
            false,
        );
    }

    let mode_ctx =
        inter_context.inter_single_mode_ctx(decision.row, decision.col, decision.block_size, 0);
    let mut mode_cdf = DEFAULT_INTER_SINGLE_MODE_CDFS[mode_ctx];
    writer.write_symbol_with_static_cdf_key(
        "tile.inter.single_mode",
        inter_single_mode_static_cdf_key(mode_ctx),
        1,
        &mut mode_cdf,
        3,
        false,
    );
}

fn write_inter_globalmv_residual(
    writer: &mut Av2EntropyWriter,
    decision: Av2TileDecision,
    skip_context: &Av2IntrabcContext,
    inter_context: &Av2InterModeContext,
    total_refs: usize,
) {
    write_inter_intra_flag(writer, decision, inter_context, true);

    let skip_ctx = skip_context.skip_txfm_ctx(decision.row, decision.col, decision.block_size);
    let mut skip_cdf = DEFAULT_SKIP_TXFM_CDFS[skip_ctx];
    writer.write_symbol_with_static_cdf_key(
        "tile.inter.skip_txfm",
        inter_skip_txfm_static_cdf_key(skip_ctx),
        0,
        &mut skip_cdf,
        2,
        false,
    );

    if total_refs > 1 {
        let single_ref_ctx = inter_context.single_ref_ctx(decision.row, decision.col, 0, total_refs);
        let mut single_ref_cdf = DEFAULT_SINGLE_REF_CDFS[single_ref_ctx][0];
        writer.write_symbol_with_static_cdf_key(
            "tile.inter.single_ref",
            inter_single_ref_static_cdf_key(single_ref_ctx),
            1,
            &mut single_ref_cdf,
            2,
            false,
        );
    }

    let mode_ctx =
        inter_context.inter_single_mode_ctx(decision.row, decision.col, decision.block_size, 0);
    let mut mode_cdf = DEFAULT_INTER_SINGLE_MODE_CDFS[mode_ctx];
    writer.write_symbol_with_static_cdf_key(
        "tile.inter.single_mode",
        inter_single_mode_static_cdf_key(mode_ctx),
        1,
        &mut mode_cdf,
        3,
        false,
    );
}

fn write_inter_intra_flag(
    writer: &mut Av2EntropyWriter,
    decision: Av2TileDecision,
    inter_context: &Av2InterModeContext,
    is_inter: bool,
) {
    let intra_inter_ctx = inter_context.intra_inter_ctx(decision.row, decision.col);
    let mut intra_inter_cdf = DEFAULT_INTRA_INTER_CDFS[intra_inter_ctx];
    writer.write_symbol_with_static_cdf_key(
        "tile.inter.is_inter",
        inter_is_inter_static_cdf_key(intra_inter_ctx),
        usize::from(is_inter),
        &mut intra_inter_cdf,
        2,
        false,
    );
}

const AV2_INTER_MV_FIELD_NAMES: Av2OnePelMvFieldNames = Av2OnePelMvFieldNames {
    shell_set: "tile.inter.mv.shell_set",
    shell_class0: "tile.inter.mv.shell_class0",
    shell_class1: "tile.inter.mv.shell_class1",
    shell_offset_low: "tile.inter.mv.shell_offset_low",
    shell_offset_class2: "tile.inter.mv.shell_offset_class2",
    shell_offset: "tile.inter.mv.shell_offset",
    col_gt: "tile.inter.mv.col_gt",
    col_remainder: "tile.inter.mv.col_remainder",
    col_index: "tile.inter.mv.col_index",
};

fn write_inter_newmv_skip(
    writer: &mut Av2EntropyWriter,
    decision: Av2TileDecision,
    skip_context: &Av2IntrabcContext,
    inter_context: &Av2InterModeContext,
    total_refs: usize,
    mv_row_px: i16,
    mv_col_px: i16,
) {
    write_inter_newmv(
        writer,
        decision,
        skip_context,
        inter_context,
        total_refs,
        mv_row_px,
        mv_col_px,
        true,
    );
}

fn write_inter_newmv_residual(
    writer: &mut Av2EntropyWriter,
    decision: Av2TileDecision,
    skip_context: &Av2IntrabcContext,
    inter_context: &Av2InterModeContext,
    total_refs: usize,
    mv_row_px: i16,
    mv_col_px: i16,
) {
    write_inter_newmv(
        writer,
        decision,
        skip_context,
        inter_context,
        total_refs,
        mv_row_px,
        mv_col_px,
        false,
    );
}

#[allow(clippy::too_many_arguments)]
fn write_inter_newmv(
    writer: &mut Av2EntropyWriter,
    decision: Av2TileDecision,
    skip_context: &Av2IntrabcContext,
    inter_context: &Av2InterModeContext,
    total_refs: usize,
    mv_row_px: i16,
    mv_col_px: i16,
    skip_txfm: bool,
) {
    let intra_inter_ctx = inter_context.intra_inter_ctx(decision.row, decision.col);
    let mut intra_inter_cdf = DEFAULT_INTRA_INTER_CDFS[intra_inter_ctx];
    writer.write_symbol_with_static_cdf_key(
        "tile.inter.is_inter",
        inter_is_inter_static_cdf_key(intra_inter_ctx),
        1,
        &mut intra_inter_cdf,
        2,
        false,
    );

    let skip_ctx = skip_context.skip_txfm_ctx(decision.row, decision.col, decision.block_size);
    let mut skip_cdf = DEFAULT_SKIP_TXFM_CDFS[skip_ctx];
    writer.write_symbol_with_static_cdf_key(
        "tile.inter.skip_txfm",
        inter_skip_txfm_static_cdf_key(skip_ctx),
        usize::from(skip_txfm),
        &mut skip_cdf,
        2,
        false,
    );

    if total_refs > 1 {
        let single_ref_ctx = inter_context.single_ref_ctx(decision.row, decision.col, 0, total_refs);
        let mut single_ref_cdf = DEFAULT_SINGLE_REF_CDFS[single_ref_ctx][0];
        writer.write_symbol_with_static_cdf_key(
            "tile.inter.single_ref",
            inter_single_ref_static_cdf_key(single_ref_ctx),
            1,
            &mut single_ref_cdf,
            2,
            false,
        );
    }

    let mode_ctx =
        inter_context.inter_single_mode_ctx(decision.row, decision.col, decision.block_size, 0);
    let mut mode_cdf = DEFAULT_INTER_SINGLE_MODE_CDFS[mode_ctx];
    writer.write_symbol_with_static_cdf_key(
        "tile.inter.single_mode",
        inter_single_mode_static_cdf_key(mode_ctx),
        2,
        &mut mode_cdf,
        3,
        false,
    );

    let mut drl_cdf = DEFAULT_DRL_CDFS[0][mode_ctx];
    writer.write_symbol_with_static_cdf_key(
        "tile.inter.drl_idx",
        inter_drl_static_cdf_key(mode_ctx),
        0,
        &mut drl_cdf,
        2,
        false,
    );

    let (ref_mv_row_px, ref_mv_col_px) =
        inter_context.nearest_ref_mv(decision.row, decision.col, decision.block_size, 0);
    let delta_row_px = i32::from(mv_row_px) - i32::from(ref_mv_row_px);
    let delta_col_px = i32::from(mv_col_px) - i32::from(ref_mv_col_px);
    write_av2_one_pel_mv_magnitude(
        writer,
        AV2_INTER_MV_FIELD_NAMES,
        delta_row_px.unsigned_abs() as usize,
        delta_col_px.unsigned_abs() as usize,
    );
    if delta_row_px != 0 {
        writer.write_literal_bit("tile.inter.mv.sign", delta_row_px < 0);
    }
    if delta_col_px != 0 {
        writer.write_literal_bit("tile.inter.mv.sign", delta_col_px < 0);
    }
}
