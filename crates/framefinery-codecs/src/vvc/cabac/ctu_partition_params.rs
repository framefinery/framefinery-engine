#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::vvc) struct VvcCtuPartitionParams {
    pub(in crate::vvc) root_width: usize,
    pub(in crate::vvc) root_height: usize,
    pub(in crate::vvc) visible_width: usize,
    pub(in crate::vvc) visible_height: usize,
    pub(in crate::vvc) chroma_sampling: ChromaSampling,
    pub(in crate::vvc) dual_tree_intra: bool,
    pub(in crate::vvc) luma_split_kind: VvcLumaSplitAvailabilityKind,
    pub(in crate::vvc) luma_max_leaf_size: u16,
    pub(in crate::vvc) chroma_tu_count: usize,
    pub(in crate::vvc) luma_tu_count: usize,
    pub(in crate::vvc) luma_tu_intra_modes: [VvcIntraPredictionMode; MAX_VVC_LUMA_TUS],
    pub(in crate::vvc) luma_tu_abs_levels: [u8; MAX_VVC_LUMA_TUS],
    pub(in crate::vvc) luma_tu_negative: [bool; MAX_VVC_LUMA_TUS],
    pub(in crate::vvc) luma_tu_dc_levels: [i16; MAX_VVC_LUMA_TUS],
    pub(in crate::vvc) luma_tu_ac_levels: [[i16; VVC_LUMA_AC_COEFFS_PER_TU]; MAX_VVC_LUMA_TUS],
    pub(in crate::vvc) luma_tu_has_ac: [bool; MAX_VVC_LUMA_TUS],
    pub(in crate::vvc) luma_tu_inter_decisions: [Option<VvcLumaInterDecision>; MAX_VVC_LUMA_TUS],
    pub(in crate::vvc) luma_tu_scc_decisions: [VvcLumaSccDecision; MAX_VVC_LUMA_TUS],
    pub(in crate::vvc) luma_tu_transform_skip: [bool; MAX_VVC_LUMA_TUS],
    pub(in crate::vvc) luma_tu_bdpcm_modes: [VvcBdpcmMode; MAX_VVC_LUMA_TUS],
    pub(in crate::vvc) luma_tu_mrl_index: [u8; MAX_VVC_LUMA_TUS],
    pub(in crate::vvc) luma_tu_mts_index: [u8; MAX_VVC_LUMA_TUS],
    pub(in crate::vvc) cb_dc_abs_level: u8,
    pub(in crate::vvc) cb_dc_negative: bool,
    pub(in crate::vvc) chroma_tu_intra_modes: [VvcChromaIntraPredictionMode; MAX_VVC_CHROMA_TUS],
    pub(in crate::vvc) cb_tu_dc_levels: [i16; MAX_VVC_CHROMA_TUS],
    pub(in crate::vvc) cr_tu_dc_levels: [i16; MAX_VVC_CHROMA_TUS],
    pub(in crate::vvc) cb_tu_ac_levels: [[i16; VVC_CHROMA_AC_COEFFS_PER_TU]; MAX_VVC_CHROMA_TUS],
    pub(in crate::vvc) cr_tu_ac_levels: [[i16; VVC_CHROMA_AC_COEFFS_PER_TU]; MAX_VVC_CHROMA_TUS],
    pub(in crate::vvc) cb_tu_has_ac: [bool; MAX_VVC_CHROMA_TUS],
    pub(in crate::vvc) cr_tu_has_ac: [bool; MAX_VVC_CHROMA_TUS],
    pub(in crate::vvc) cb_tu_transform_skip: [bool; MAX_VVC_CHROMA_TUS],
    pub(in crate::vvc) cr_tu_transform_skip: [bool; MAX_VVC_CHROMA_TUS],
    pub(in crate::vvc) chroma_tu_bdpcm_modes: [VvcBdpcmMode; MAX_VVC_CHROMA_TUS],
}

impl VvcCtuPartitionParams {
    pub(in crate::vvc) fn shape(&self) -> VvcCtuPartitionShape {
        VvcCtuPartitionShape {
            root_width: self.root_width as u16,
            root_height: self.root_height as u16,
            visible_width: self.visible_width as u16,
            visible_height: self.visible_height as u16,
            chroma_sampling: self.chroma_sampling,
            dual_tree_intra: self.dual_tree_intra,
        }
    }

    pub(in crate::vvc) fn single_tree_shape(&self) -> VvcCtuPartitionShape {
        let mut shape = self.shape();
        shape.dual_tree_intra = false;
        shape
    }

    #[cfg(test)]
    pub(in crate::vvc) fn visible_chroma_width(&self) -> u16 {
        // coding_tree() uses luma-coordinate cbWidth/cbHeight even for
        // DUAL_TREE_CHROMA. Chroma subsampling is applied by chroma syntax and
        // transform decisions below the tree, not by shrinking the tree root.
        self.visible_width as u16
    }

    #[cfg(test)]
    pub(in crate::vvc) fn visible_chroma_height(&self) -> u16 {
        self.visible_height as u16
    }

    #[cfg(test)]
    pub(in crate::vvc) fn ctu_chroma_root(&self) -> VvcCodingTreeNode {
        VvcCodingTreeNode::root(
            self.root_width as u16,
            self.root_height as u16,
            VvcTreeType::DualTreeChroma,
        )
    }
}
