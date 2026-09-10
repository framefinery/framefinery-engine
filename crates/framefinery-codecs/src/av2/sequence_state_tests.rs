use super::*;

fn changing_420_frame(bits: u8, phase: usize) -> Vec<u8> {
    let mut frame = Vec::new();
    for (plane, (width, height)) in [(32, 24), (16, 12), (16, 12)].into_iter().enumerate() {
        for y in 0..height {
            for x in 0..width {
                let block = usize::from((x / 8 + y / 8 + phase) % 3 == 0);
                let sample = ((32 + plane * 48 + block * 48) as u16) << (bits - 8);
                frame.extend_from_slice(&sample.to_le_bytes()[..if bits == 8 { 1 } else { 2 }]);
            }
        }
    }
    frame
}

fn sequence_headers(stream: &[u8]) -> Vec<&[u8]> {
    let mut remaining = stream;
    let mut headers = Vec::new();
    while !remaining.is_empty() {
        let mut length = 0usize;
        let mut shift = 0;
        loop {
            let (&byte, tail) = remaining.split_first().expect("complete OBU length");
            remaining = tail;
            assert!(shift < usize::BITS);
            length |= usize::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                break;
            }
            shift += 7;
        }
        assert!(length > 0 && length <= remaining.len());
        let (obu, tail) = remaining.split_at(length);
        if (obu[0] >> 2) & 0x1f == Av2ObuType::SequenceHeader as u8 {
            headers.push(obu);
        }
        remaining = tail;
    }
    headers
}

#[test]
fn predictive_sequence_capabilities_survive_later_intrabc_matches() {
    let geometry = Av2VideoGeometry {
        width: 32,
        height: 24,
    };
    for bits in [8, 10] {
        let format = PixelFormat::yuv420(bits).unwrap();
        let stream_format = Av2StreamFormat::from_pixel_format(format).unwrap();
        let first = changing_420_frame(bits, 0);
        let later = changing_420_frame(bits, 1);
        let tile_bounds =
            Av2TileLayout::lossless_subsampled_ibc_for_geometry(geometry).local_ibc_tile_bounds();
        let copy_blocks = |frame: &[u8]| {
            ibc::build_local_ibc_subsampled(
                frame,
                geometry,
                stream_format.chroma_format,
                stream_format.bit_depth,
                &tile_bounds,
            )
            .unwrap()
            .stats()
            .selected_copy_blocks()
        };
        assert_eq!(
            copy_blocks(&first),
            0,
            "fixture starts without IntraBC matches"
        );
        assert!(
            copy_blocks(&later) > 0,
            "later fixture must exercise IntraBC"
        );

        // First frame, repeat, changed frame with IntraBC, then a GOP reset,
        // another repeat and changed frame. Keep the original failure sequence.
        let input: Vec<_> = [0, 0, 1, 2, 2, 3]
            .into_iter()
            .flat_map(|phase| changing_420_frame(bits, phase))
            .collect();
        let encode = |input: &[u8], frames| {
            let mut source = input;
            let mut output = Vec::new();
            let mut recon = Vec::new();
            av2_encode_fixed_black_444_with_options_and_frame_metrics(
                &mut source,
                &mut output,
                Some(&mut recon),
                Av2EncodeRequest {
                    params: Av2EncodeParams { frames },
                    geometry,
                    format,
                },
                Av2EncodeOptions {
                    lossless: true,
                    qp: None,
                    gop: crate::settings::GopMode::Fixed(3),
                },
                None,
            )
            .expect("predictive lossless encode");
            assert_eq!(recon, input);
            output
        };
        let stream = encode(&input, 6);
        let starts_with_ibc = encode(&later, 1);
        let headers = sequence_headers(&stream);
        assert_eq!(
            headers.len(),
            1,
            "one sequence configuration persists across frames"
        );
        assert_eq!(
            headers,
            sequence_headers(&starts_with_ibc),
            "{bits}-bit sequence capabilities must not depend on first-frame IntraBC matches"
        );
    }
}
