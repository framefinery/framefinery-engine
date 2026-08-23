use crate::bitstream::insert_emulation_prevention_bytes;
use std::io::Write;

use super::{VvcSyntaxRbsp, VvcSyntaxWriter};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum VvcNalUnitType {
    Trail = 0,
    IdrWRadl = 7,
    IdrNLp = 8,
    Cra = 9,
    Opi = 12,
    Dci = 13,
    Vps = 14,
    Sps = 15,
    Pps = 16,
    PrefixAps = 17,
    SuffixAps = 18,
    PictureHeader = 19,
    AccessUnitDelimiter = 20,
    EndOfSequence = 21,
    EndOfBitstream = 22,
    PrefixSei = 23,
    SuffixSei = 24,
    ReservedNvcl30 = 30,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VvcNalUnit {
    pub nal_unit_type: VvcNalUnitType,
    pub layer_id: u8,
    pub temporal_id: u8,
    pub rbsp_payload: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VvcNalHeader {
    pub forbidden_zero_bit: bool,
    pub nuh_reserved_zero_bit: bool,
    pub layer_id: u8,
    pub nal_unit_type: VvcNalUnitType,
    pub temporal_id: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VvcNalInfo {
    pub nal_unit_type: u8,
    pub layer_id: u8,
    pub temporal_id: u8,
    pub payload_len: usize,
    pub offset: usize,
}

impl VvcNalUnit {
    pub fn eos() -> Self {
        Self {
            nal_unit_type: VvcNalUnitType::EndOfSequence,
            layer_id: 0,
            temporal_id: 0,
            rbsp_payload: Vec::new(),
        }
    }

    pub fn eob() -> Self {
        Self {
            nal_unit_type: VvcNalUnitType::EndOfBitstream,
            layer_id: 0,
            temporal_id: 0,
            rbsp_payload: Vec::new(),
        }
    }
}

pub fn write_annex_b(units: &[VvcNalUnit]) -> Result<Vec<u8>, String> {
    let estimated_size = units
        .iter()
        .map(|unit| 6usize.saturating_add(unit.rbsp_payload.len()))
        .sum();
    let mut out = Vec::with_capacity(estimated_size);
    write_annex_b_to(&mut out, units)?;
    Ok(out)
}

pub fn write_annex_b_to<W: Write + ?Sized>(
    out: &mut W,
    units: &[VvcNalUnit],
) -> Result<usize, String> {
    let headers: Vec<_> = units
        .iter()
        .map(nal_unit_header_bytes)
        .collect::<Result<_, _>>()?;
    let mut bytes_written = 0usize;
    for (unit, header) in units.iter().zip(headers) {
        out.write_all(&[0x00, 0x00, 0x00, 0x01])
            .map_err(|err| format!("failed to write VVC Annex-B start code: {err}"))?;
        out.write_all(&header)
            .map_err(|err| format!("failed to write VVC NAL header: {err}"))?;
        let escaped_payload = insert_emulation_prevention_bytes(&unit.rbsp_payload);
        out.write_all(&escaped_payload)
            .map_err(|err| format!("failed to write VVC NAL payload: {err}"))?;
        bytes_written = bytes_written
            .saturating_add(6)
            .saturating_add(escaped_payload.len());
    }
    Ok(bytes_written)
}

pub fn nal_unit_header_bytes(unit: &VvcNalUnit) -> Result<[u8; 2], String> {
    if unit.layer_id > 55 {
        return Err("VVC nuh_layer_id must be in the range 0..=55".to_string());
    }
    if unit.temporal_id > 6 {
        return Err("VVC temporal_id must be in the range 0..=6".to_string());
    }

    let header = VvcNalHeader {
        forbidden_zero_bit: false,
        nuh_reserved_zero_bit: false,
        layer_id: unit.layer_id,
        nal_unit_type: unit.nal_unit_type,
        temporal_id: unit.temporal_id,
    };
    let bytes = write_nal_unit_header(header).bytes;
    Ok([bytes[0], bytes[1]])
}

pub fn write_nal_unit_header(header: VvcNalHeader) -> VvcSyntaxRbsp {
    let mut writer = VvcSyntaxWriter::new();
    writer.write_flag("forbidden_zero_bit", header.forbidden_zero_bit);
    writer.write_flag("nuh_reserved_zero_bit", header.nuh_reserved_zero_bit);
    writer.write_u("nuh_layer_id", header.layer_id as u64, 6);
    writer.write_u("nal_unit_type", header.nal_unit_type as u64, 5);
    writer.write_u("nuh_temporal_id_plus1", header.temporal_id as u64 + 1, 3);
    writer.finish()
}

pub fn parse_annex_b_nal_units(bytes: &[u8]) -> Result<Vec<VvcNalInfo>, String> {
    let ranges = annex_b_ranges(bytes);
    let mut infos = Vec::with_capacity(ranges.len());

    for (start, end) in ranges {
        if end - start < 2 {
            return Err(format!(
                "NAL unit at offset {start} is too short for a VVC header"
            ));
        }
        let h0 = bytes[start];
        let h1 = bytes[start + 1];
        let forbidden_zero_bit = h0 >> 7;
        let nuh_reserved_zero_bit = (h0 >> 6) & 0x01;
        if forbidden_zero_bit != 0 || nuh_reserved_zero_bit != 0 {
            return Err(format!(
                "invalid VVC NAL header reserved bits at offset {start}"
            ));
        }
        let layer_id = h0 & 0x3f;
        if layer_id > 55 {
            return Err(format!(
                "VVC layer id {layer_id} out of range at offset {start}"
            ));
        }
        let nal_unit_type = h1 >> 3;
        let temporal_id_plus1 = h1 & 0x07;
        if temporal_id_plus1 == 0 {
            return Err(format!("VVC temporal_id_plus1 is zero at offset {start}"));
        }
        infos.push(VvcNalInfo {
            nal_unit_type,
            layer_id,
            temporal_id: temporal_id_plus1 - 1,
            payload_len: end - start - 2,
            offset: start,
        });
    }

    Ok(infos)
}

fn annex_b_ranges(bytes: &[u8]) -> Vec<(usize, usize)> {
    let mut starts = Vec::new();
    let mut i = 0;
    while i + 3 <= bytes.len() {
        if i + 4 <= bytes.len() && bytes[i..i + 4] == [0, 0, 0, 1] {
            starts.push((i, 4));
            i += 4;
        } else if bytes[i..i + 3] == [0, 0, 1] {
            starts.push((i, 3));
            i += 3;
        } else {
            i += 1;
        }
    }

    starts
        .iter()
        .enumerate()
        .map(|(idx, (prefix_pos, prefix_len))| {
            let payload_start = prefix_pos + prefix_len;
            let payload_end = starts
                .get(idx + 1)
                .map(|(next_prefix_pos, _)| *next_prefix_pos)
                .unwrap_or(bytes.len());
            (payload_start, payload_end)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{write_annex_b, write_annex_b_to, VvcNalUnit, VvcNalUnitType};

    #[test]
    fn streaming_annex_b_writer_matches_vector_writer() {
        let units = [
            VvcNalUnit {
                nal_unit_type: VvcNalUnitType::Sps,
                layer_id: 0,
                temporal_id: 0,
                rbsp_payload: vec![0, 0, 0, 1, 2, 3],
            },
            VvcNalUnit {
                nal_unit_type: VvcNalUnitType::Trail,
                layer_id: 0,
                temporal_id: 0,
                rbsp_payload: vec![9, 8, 0, 0, 3],
            },
        ];
        let expected = write_annex_b(&units).expect("valid VVC NAL units");
        let mut streamed = Vec::new();
        let bytes = write_annex_b_to(&mut streamed, &units).expect("valid VVC NAL units");

        assert_eq!(streamed, expected);
        assert_eq!(bytes, expected.len());
    }

    #[test]
    fn streaming_annex_b_writer_validates_headers_before_writing() {
        let units = [
            VvcNalUnit {
                nal_unit_type: VvcNalUnitType::Sps,
                layer_id: 0,
                temporal_id: 0,
                rbsp_payload: vec![1, 2, 3],
            },
            VvcNalUnit {
                nal_unit_type: VvcNalUnitType::Trail,
                layer_id: 56,
                temporal_id: 0,
                rbsp_payload: vec![4, 5, 6],
            },
        ];
        let mut streamed = Vec::new();

        assert!(write_annex_b_to(&mut streamed, &units).is_err());
        assert!(streamed.is_empty());
    }
}
