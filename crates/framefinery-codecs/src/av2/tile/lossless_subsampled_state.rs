impl<'a> Av2LosslessSubsampledTileState<'a> {
    fn new(
        geometry: Av2VideoGeometry,
        region: Av2TileRegion,
        chroma_format: Av2ChromaFormat,
        bit_depth: SampleBitDepth,
        mode_search: Av2LosslessSubsampledModeSearch,
        source: &'a [u8],
        recon: &'a mut [u8],
    ) -> Self {
        assert!(
            matches!(
                chroma_format,
                Av2ChromaFormat::Yuv420 | Av2ChromaFormat::Yuv422 | Av2ChromaFormat::Yuv444
            ),
            "AV2 planar lossless state expects 4:2:0, 4:2:2, or 4:4:4 input"
        );
        let layout =
            Av2PlanarTileLayout::for_validated_shape(geometry, region, chroma_format, bit_depth);
        let expected_len = layout.frame_len();
        assert_eq!(
            source.len(),
            expected_len,
            "AV2 planar lossless source length must match geometry"
        );
        let source_backed_recon = mode_search == Av2LosslessSubsampledModeSearch::FastScreenContent;
        if source_backed_recon {
            assert!(
                recon.is_empty() || recon.len() == source.len(),
                "AV2 fast planar lossless reconstruction must be empty or match source"
            );
        } else {
            assert_eq!(
                recon.len(),
                source.len(),
                "AV2 planar lossless reconstruction length must match source"
            );
        }
        Self {
            chroma_format,
            bit_depth,
            mode_search,
            source,
            recon,
            layout,
            source_backed_recon,
        }
    }

    fn plane_geometry(&self, plane: Av2LosslessPlane) -> (usize, usize) {
        self.layout.plane_geometry(plane.planar())
    }

    fn plane_origin(&self, plane: Av2LosslessPlane) -> (usize, usize) {
        self.layout.plane_origin(plane.planar())
    }

    fn plane_region_limit(&self, plane: Av2LosslessPlane) -> (usize, usize) {
        self.layout.plane_region_limit(plane.planar())
    }

    fn plane_subsampling(&self, plane: Av2LosslessPlane) -> (usize, usize) {
        self.layout.plane_subsampling(plane.planar())
    }

    fn coded_mi_for_plane_sample(
        &self,
        plane: Av2LosslessPlane,
        x: usize,
        y: usize,
    ) -> (usize, usize) {
        self.layout
            .coded_mi_for_plane_sample(plane.planar(), x, y)
    }

    fn txb_origin(&self, plane: Av2LosslessPlane, col: usize, row: usize) -> (usize, usize) {
        self.layout.txb_origin(plane.planar(), col, row)
    }
}
