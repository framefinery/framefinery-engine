use super::*;

#[test]
fn unconstrained_video_profiles_signal_level_15_5_with_required_high_tier() {
    // H.266 (09/2023), A.3/A.4. Check actual RBSP field bits, including the
    // tier flag that VTM accepts even when it has the reserved value zero.
    for profile in [
        VvcProfile::Auto,
        VvcProfile::Main10,
        VvcProfile::Main10FourFourFour,
        VvcProfile::Main12,
        VvcProfile::Main12FourFourFour,
    ] {
        for bits in 8..=12 {
            for chroma in [
                ChromaSampling::Cs420,
                ChromaSampling::Cs422,
                ChromaSampling::Cs444,
            ] {
                let bit_depth = SampleBitDepth::new(bits).unwrap();
                let format = VvcPictureFormat {
                    chroma_sampling: chroma,
                    bit_depth,
                };
                if profile.validate_for_format(format).is_err() {
                    continue; // Profile/format pairs rejected by the public configuration contract.
                }
                for lossless in [false, true] {
                    let config = if lossless {
                        VvcSliceSyntaxConfig::residual_lossless(chroma, bit_depth)
                    } else {
                        VvcSliceSyntaxConfig::residual_lossy(chroma)
                    }
                    .with_validated_profile_for_format(profile, format)
                    .unwrap();
                    for config in [config, config.with_inter_enabled()] {
                        for (width, height) in [(16, 16), (1920, 1080), (2560, 1440)] {
                            let sps =
                                vvc_sps_rbsp(VvcVideoGeometry { width, height }, config, bit_depth);
                            assert_eq!(vvc_u_value(&sps, "general_level_idc"), 255);
                            assert_vvc_flag(&sps, "general_tier_flag", true);
                            assert_eq!(
                                vvc_u_value(&sps, "general_profile_idc"),
                                u64::from(
                                    profile
                                        .validate_for_format(format)
                                        .unwrap()
                                        .general_profile_idc()
                                )
                            );
                        }
                    }
                }
            }
        }
    }
}
