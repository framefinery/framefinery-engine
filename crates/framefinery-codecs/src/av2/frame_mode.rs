#[derive(Debug, Clone, PartialEq, Eq)]
enum Av2Mvp444FrameMode {
    Black,
    LumaPalette {
        palette: Box<Av2LumaPalette444>,
        ibc: Option<Av2LocalIbc444>,
    },
}

impl Av2Mvp444FrameMode {
    fn from_frame(
        frame: &[u8],
        geometry: Av2VideoGeometry,
        bit_depth: SampleBitDepth,
    ) -> Result<Self, String> {
        let black = av2_black_444_reconstruction_for_geometry_with_depth(geometry, bit_depth);
        if frame == black {
            return Ok(Self::Black);
        }
        let palette = palette::build_luma_palette_444(frame, geometry, bit_depth)?;
        let ibc = if AV2_ENABLE_LUMA_PALETTE_INTRABC_444 {
            Some(ibc::build_local_ibc_444_for_palette(
                frame, geometry, &palette,
            )?)
        } else {
            None
        };
        Ok(Self::LumaPalette {
            palette: Box::new(palette),
            ibc,
        })
    }

    fn allow_screen_content_tools(&self) -> bool {
        true
    }

    fn allow_intrabc(&self) -> bool {
        match self {
            Self::Black => false,
            // Single-tile palette coding reuses prediction and entropy state
            // across 64x64 superblocks. The current local IBC model is still
            // tied to independent 64x64 tiles, so leave it off until the block
            // vector predictor is modeled for multi-superblock tiles.
            Self::LumaPalette { ibc, .. } => AV2_ENABLE_LUMA_PALETTE_INTRABC_444 && ibc.is_some(),
        }
    }

    fn profile(&self) -> Av2Black444MvpProfile {
        let profile = Av2Black444MvpProfile::current();
        if self.allow_intrabc() {
            profile.with_local_ibc_candidates()
        } else {
            profile
        }
    }

    fn reconstruction(&self, geometry: Av2VideoGeometry, bit_depth: SampleBitDepth) -> Vec<u8> {
        match self {
            Self::Black => {
                av2_black_444_reconstruction_for_geometry_with_depth(geometry, bit_depth)
            }
            Self::LumaPalette { palette, .. } => palette.reconstruction().to_vec(),
        }
    }
}
