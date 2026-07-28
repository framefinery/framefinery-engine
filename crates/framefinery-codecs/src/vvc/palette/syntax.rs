#[cfg(test)]
pub(super) fn vvc_palette_444_single_entry_syntax(
    geometry: VvcVideoGeometry,
    color: VvcSampledColor,
) -> VvcPalette444Syntax {
    // H.266 7.3.11.6, single-tree 4:4:4 subset:
    // - no predictor reuse because the initial predictor palette is empty,
    // - exactly one explicitly signalled palette entry,
    // - no escape-coded samples,
    // - MaxPaletteIndex == 0, so all sample indices are inferred as 0 and
    //   run/copy/index syntax is not present.
    VvcPalette444Syntax {
        tree_type: VvcPaletteTreeType::SingleTree,
        bit_depth: SampleBitDepth::new(8).expect("valid bit depth"),
        slice_qp: VvcSliceSyntaxConfig::palette_444().slice_qp,
        cb_width: geometry.width,
        cb_height: geometry.height,
        start_comp: 0,
        num_comps: 3,
        max_num_palette_entries: 31,
        num_predicted_palette_entries: 0,
        num_signalled_palette_entries: 1,
        new_palette_entries: vec![color],
        current_palette_size: 1,
        palette_escape_val_present_flag: false,
        max_palette_index: 0,
        palette_indices: Vec::new(),
        palette_escape_values: Vec::new(),
    }
}

#[cfg(test)]
pub(super) fn vvc_palette_444_cu_syntax(
    frame: &VvcSampledFrame,
    origin_x: usize,
    origin_y: usize,
) -> VvcPalette444Syntax {
    vvc_palette_444_cu_syntax_with_config(
        frame,
        origin_x,
        origin_y,
        VvcSliceSyntaxConfig::palette_444(),
    )
}

pub(super) fn vvc_palette_444_cu_syntax_with_config(
    frame: &VvcSampledFrame,
    origin_x: usize,
    origin_y: usize,
    slice_config: VvcSliceSyntaxConfig,
) -> VvcPalette444Syntax {
    let mut entries = Vec::new();
    let mut indices = Vec::new();
    let mut escape_values = Vec::new();
    let width = 8.min(frame.geometry.width.saturating_sub(origin_x));
    let height = 8.min(frame.geometry.height.saturating_sub(origin_y));
    let mut has_escape = false;

    for y_off in 0..height {
        for x_off in 0..width {
            let color = vvc_palette_444_sample_at(frame, origin_x + x_off, origin_y + y_off);
            let (index, escape_value) =
                if let Some(index) = entries.iter().position(|entry| *entry == color) {
                    (index as u8, None)
                } else if entries.len() < 31 {
                    entries.push(color);
                    ((entries.len() - 1) as u8, None)
                } else {
                    // H.266 7.3.11.6 and 7.4.12.6 define
                    // MaxPaletteIndex as CurrentPaletteSize - 1 plus
                    // palette_escape_val_present_flag. PaletteEscapeVal itself
                    // is reconstructed through H.266 8.4.5.3, so code the
                    // inverse level for the active palette slice QP.
                    has_escape = true;
                    (
                        31,
                        Some(vvc_palette_escape_level_color(
                            color,
                            frame.format.bit_depth,
                            slice_config.slice_qp,
                        )),
                    )
                };
            indices.push(index);
            escape_values.push(escape_value);
        }
    }

    if entries.is_empty() {
        entries.push(vvc_palette_444_sample_at(frame, origin_x, origin_y));
        indices.push(0);
        escape_values.push(None);
    }

    let current_palette_size = entries.len() as u8;
    let max_palette_index = current_palette_size.saturating_sub(1) + u8::from(has_escape);
    VvcPalette444Syntax {
        tree_type: VvcPaletteTreeType::SingleTree,
        bit_depth: frame.format.bit_depth,
        slice_qp: slice_config.slice_qp,
        cb_width: width,
        cb_height: height,
        start_comp: 0,
        num_comps: 3,
        max_num_palette_entries: 31,
        num_predicted_palette_entries: 0,
        num_signalled_palette_entries: current_palette_size,
        new_palette_entries: entries,
        current_palette_size,
        palette_escape_val_present_flag: has_escape,
        max_palette_index,
        palette_indices: if max_palette_index == 0 {
            Vec::new()
        } else {
            indices
        },
        palette_escape_values: if has_escape {
            escape_values
        } else {
            Vec::new()
        },
    }
}

fn vvc_palette_444_sample_at(frame: &VvcSampledFrame, x: usize, y: usize) -> VvcSampledColor {
    debug_assert_eq!(frame.format.chroma_sampling, ChromaSampling::Cs444);
    let sample_x = x.min(frame.geometry.width.saturating_sub(1));
    let sample_y = y.min(frame.geometry.height.saturating_sub(1));
    let index = sample_y * frame.geometry.width + sample_x;
    VvcSampledColor {
        y: frame.luma[index],
        u: frame.cb[index],
        v: frame.cr[index],
    }
}

#[cfg(test)]
pub(super) fn vvc_palette_444_binarized_syntax_bits(syntax: VvcPalette444Syntax) -> Vec<bool> {
    let mut bits = Vec::new();
    for token in vvc_palette_444_syntax_tokens(syntax, VvcPalettePredictorMode::SignalNewEntry) {
        append_palette_syntax_token_bits(&mut bits, token);
    }
    bits
}

#[cfg(test)]
pub(super) fn vvc_palette_444_new_entry_token_bit_counts(syntax: VvcPalette444Syntax) -> Vec<u8> {
    vvc_palette_444_syntax_tokens(syntax, VvcPalettePredictorMode::SignalNewEntry)
        .into_iter()
        .filter_map(|token| {
            if !token.name.starts_with("new_palette_entries") {
                return None;
            }
            match token.kind {
                VvcPaletteSyntaxTokenKind::FixedLength { bit_count, .. } => Some(bit_count),
                _ => None,
            }
        })
        .collect()
}

pub(super) fn vvc_palette_444_syntax_tokens(
    syntax: VvcPalette444Syntax,
    predictor_mode: VvcPalettePredictorMode,
) -> Vec<VvcPaletteSyntaxToken> {
    debug_assert_eq!(syntax.tree_type, VvcPaletteTreeType::SingleTree);
    debug_assert_eq!(syntax.start_comp, 0);
    debug_assert_eq!(syntax.num_comps, 3);
    debug_assert_eq!(syntax.max_num_palette_entries, 31);
    debug_assert_eq!(syntax.num_predicted_palette_entries, 0);
    debug_assert_eq!(
        syntax.current_palette_size,
        syntax.num_signalled_palette_entries
    );
    let entry_bit_count = syntax.bit_depth.bits();

    let mut tokens = Vec::new();
    if predictor_mode == VvcPalettePredictorMode::SignalNewEntryAfterPredictor {
        tokens.push(VvcPaletteSyntaxToken {
            name: "palette_predictor_run",
            // H.266 cu_palette_info/xDecodePLTPredIndicator: with a non-empty
            // previous palette, symbol 1 terminates prediction without reusing
            // entries. The following num_signalled_palette_entries then carries
            // this CU's fresh single-entry palette.
            kind: VvcPaletteSyntaxTokenKind::Eg0 { value: 1 },
        });
    }
    tokens.push(VvcPaletteSyntaxToken {
        name: "num_signalled_palette_entries",
        kind: VvcPaletteSyntaxTokenKind::Eg0 {
            value: syntax.num_signalled_palette_entries as u32,
        },
    });
    for entry in &syntax.new_palette_entries {
        tokens.push(VvcPaletteSyntaxToken {
            name: "new_palette_entries[0][i]",
            kind: VvcPaletteSyntaxTokenKind::FixedLength {
                value: entry.y as u32,
                bit_count: entry_bit_count,
            },
        });
    }
    for entry in &syntax.new_palette_entries {
        tokens.push(VvcPaletteSyntaxToken {
            name: "new_palette_entries[1][i]",
            kind: VvcPaletteSyntaxTokenKind::FixedLength {
                value: entry.u as u32,
                bit_count: entry_bit_count,
            },
        });
    }
    for entry in &syntax.new_palette_entries {
        tokens.push(VvcPaletteSyntaxToken {
            name: "new_palette_entries[2][i]",
            kind: VvcPaletteSyntaxTokenKind::FixedLength {
                value: entry.v as u32,
                bit_count: entry_bit_count,
            },
        });
    }
    tokens.push(VvcPaletteSyntaxToken {
        name: "palette_escape_val_present_flag",
        kind: VvcPaletteSyntaxTokenKind::FixedLength {
            value: u32::from(syntax.palette_escape_val_present_flag),
            bit_count: 1,
        },
    });
    if syntax.max_palette_index > 0 {
        // Palette index maps are not a flat list of fixed-width EP bins in
        // VVC. They are written by append_vvc_palette_444_index_map() so the
        // context-coded copy flags and truncated index bins stay synchronized
        // with CABAC state.
    }
    tokens
}
