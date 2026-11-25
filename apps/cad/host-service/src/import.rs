use std::path::Path;

use cadcraft_dxf::{Tag, Value};
use cadcraft_engine::doc::Drawing;

pub(super) fn read(bytes: Vec<u8>, name: &str, max_fit_points: usize) -> Result<Drawing, String> {
    // CADCraft also reads DWG through DXF. Decode it once and check the exact
    // bytes that the drawing reader will use, regardless of the file's suffix.
    let (dxf, name) = if cadcraft_dwg::is_dwg(&bytes) { (cadcraft_dwg::dwg_to_dxf(&bytes)?, "") } else { (bytes, name) };
    check_fit_points(&dxf, name, max_fit_points)?;
    cadcraft_io::read(&dxf, name).map_err(|e| e.to_string())
}

fn check_fit_points(dxf: &[u8], name: &str, max: usize) -> Result<(), String> {
    let tags = cadcraft_dxf::parse(dxf).map_err(|e| {
        // Preserve the drawing reader's error for a file with another suffix.
        match Path::new(name).extension().and_then(|ext| ext.to_str()) {
            Some(ext) if !ext.is_empty() && !ext.eq_ignore_ascii_case("dxf") => cadcraft_io::IoError::Unsupported(ext.to_ascii_lowercase()).to_string(),
            _ => e.to_string(),
        }
    })?;
    // Borrow each record from the flat tag list. The DXF section/record
    // builders used in the subsequent drawing read would copy every tag.
    let mut section = None;
    let mut in_block = false;
    for record in tags.chunk_by(|_, next| next.code != 0) {
        let Some((head, fields)) = record.split_first() else { continue };
        match text_group(head, 0) {
            Some("SECTION") if section.is_none() => {
                section = Some(fields.first().and_then(|tag| text_group(tag, 2)).unwrap_or(""));
                in_block = false;
            }
            Some("ENDSEC") => section = None,
            Some("BLOCK") if section == Some("BLOCKS") => in_block = true,
            Some("ENDBLK") if section == Some("BLOCKS") => in_block = false,
            Some("SPLINE") if section == Some("ENTITIES") || (section == Some("BLOCKS") && in_block) => {
                // The drawing reader zips groups 11 and 21 and ignores the
                // declared count in group 74.
                let points = point_count(fields, 11);
                if points > max && !valid_control_curve(fields) {
                    return Err(format!("DXF spline needs interpolation through {points} fit points (the service limit is {max})"));
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn text_group(tag: &Tag, code: i32) -> Option<&str> {
    match &tag.value {
        Value::Str(value) if tag.code == code => Some(value),
        _ => None,
    }
}

fn point_count(fields: &[Tag], x: i32) -> usize {
    let count = |code| fields.iter().filter(|tag| tag.code == code).count();
    count(x).min(count(x + 10))
}

// Match the pinned DXF reader's degree and Spline::is_valid. When the
// stored controls and knots already define the curve it does no fit solve.
fn valid_control_curve(fields: &[Tag]) -> bool {
    let degree = fields.iter().find(|tag| tag.code == 71).map_or(3, Tag::i64).clamp(1, 10) as usize;
    let controls = point_count(fields, 10);
    let knots = fields.iter().filter(|tag| tag.code == 40).map(Tag::f64);
    controls > degree && knots.clone().count() == controls + degree + 1 && knots.is_sorted()
}

#[cfg(test)]
mod tests {
    use super::*;
    use cadcraft_engine::doc::EntityKind;

    const THREE_FIT_POINTS: &str = "0\nSPLINE\n74\n3\n11\n0\n21\n0\n11\n1\n21\n2\n11\n3\n21\n1\n";

    fn entities(records: &str) -> Vec<u8> {
        format!("0\nSECTION\n2\nENTITIES\n{records}0\nENDSEC\n0\nEOF\n").into_bytes()
    }

    #[test]
    fn counts_paired_fit_coordinates_per_spline() {
        // Both records have three complete points. One has a trailing x,
        // and the other declares zero fit points despite its coordinates.
        let underdeclared = THREE_FIT_POINTS.replace("74\n3\n", "74\n0\n");
        let dxf = entities(&format!("{THREE_FIT_POINTS}11\n4\n{underdeclared}"));
        let drawing = read(dxf, "drawing.dxf", 3).unwrap();
        assert_eq!(drawing.model.len(), 2);
        for entity in drawing.model.iter() {
            assert!(matches!(&entity.kind, EntityKind::Spline(s) if s.is_valid() && s.fit.len() == 3));
        }
        let err = read(entities(&underdeclared), "drawing.dxf", 2).unwrap_err();
        assert!(err.contains("3 fit points") && err.contains("limit is 2"), "{err}");
    }

    #[test]
    fn checks_splines_in_block_definitions() {
        let dxf = format!("0\nSECTION\n2\nBLOCKS\n0\nBLOCK\n2\nShape\n{THREE_FIT_POINTS}0\nENDBLK\n0\nENDSEC\n0\nEOF\n");
        assert!(read(dxf.clone().into_bytes(), "drawing.dxf", 3).unwrap().blocks.contains_key("Shape"));
        assert!(read(dxf.into_bytes(), "drawing.dxf", 2).unwrap_err().contains("3 fit points"));
    }

    #[test]
    fn allows_control_defined_splines_and_ignores_non_entity_sections() {
        let controls = "71\n1\n10\n0\n20\n0\n10\n3\n20\n1\n";
        let knots = "40\n0\n40\n0\n40\n1\n40\n1\n";
        let dxf = entities(&format!("{THREE_FIT_POINTS}{controls}{knots}"));
        let drawing = read(dxf, "drawing.dxf", 2).unwrap();
        let curve = drawing.model.iter().next().unwrap();
        assert!(matches!(&curve.kind, EntityKind::Spline(s) if s.degree == 1 && s.control.len() == 2 && s.fit.len() == 3));

        // An incomplete or nonmonotone knot vector takes the interpolation branch.
        for invalid in ["40\n0\n", "40\n0\n40\n1\n40\n0\n40\n1\n"] {
            assert!(read(entities(&format!("{THREE_FIT_POINTS}{controls}{invalid}")), "drawing.dxf", 2).unwrap_err().contains("3 fit points"));
        }
        let metadata = format!("0\nSECTION\n2\nTHUMBNAILIMAGE\n{THREE_FIT_POINTS}0\nENDSEC\n0\nEOF\n");
        assert_eq!(read(metadata.into_bytes(), "drawing.dxf", 2).unwrap().model.len(), 0);
    }

    #[test]
    fn checks_binary_dxf_even_under_another_extension() {
        fn text(out: &mut Vec<u8>, code: i16, value: &str) {
            out.extend(code.to_le_bytes());
            out.extend(value.as_bytes());
            out.push(0);
        }
        let mut binary = b"AutoCAD Binary DXF\r\n\x1a\0".to_vec();
        text(&mut binary, 0, "SECTION");
        text(&mut binary, 2, "ENTITIES");
        text(&mut binary, 0, "SPLINE");
        for (x, y) in [(0.0_f64, 0.0_f64), (1.0, 2.0), (3.0, 1.0)] {
            binary.extend(11_i16.to_le_bytes());
            binary.extend(x.to_le_bytes());
            binary.extend(21_i16.to_le_bytes());
            binary.extend(y.to_le_bytes());
        }
        text(&mut binary, 0, "ENDSEC");
        text(&mut binary, 0, "EOF");
        assert_eq!(read(binary.clone(), "drawing.bin", 3).unwrap().model.len(), 1);
        assert!(read(binary, "drawing.bin", 2).unwrap_err().contains("3 fit points"));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn checks_fit_points_after_dwg_decode() {
        let drawing = cadcraft_io::read_dxf(&entities(THREE_FIT_POINTS)).unwrap();
        let dwg = cadcraft_io::write(&drawing, "drawing.dwg").unwrap();
        assert_eq!(read(dwg.clone(), "drawing.dwg", 3).unwrap().model.len(), 1);
        assert!(read(dwg, "drawing.dwg", 2).unwrap_err().contains("3 fit points"));
    }
}
