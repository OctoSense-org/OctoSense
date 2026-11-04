//! Target sizes, in layout points.
use serde::{Deserialize, Serialize};

/// What a size stands for. The glance tile is the one a card MUST fit: it
/// cannot scroll, so content past its bottom is lost.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SizeKind {
    Glance,
    Phone,
    Desktop,
    Custom,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TargetSize {
    pub name: String,
    pub kind: SizeKind,
    pub width: f64,
    pub height: f64,
}

/// The glance tile. Provisional: the shell's glance items are 350pt wide
/// (a 390pt phone less 20pt margins) and 78–128pt tall today; M4 fixes the
/// L0 card's tile. Override with `WxH` or `glance=WxH`.
pub const GLANCE: (f64, f64) = (350.0, 160.0);
pub const PHONE: (f64, f64) = (390.0, 844.0);
pub const DESKTOP: (f64, f64) = (1200.0, 800.0);

impl TargetSize {
    /// `glance`, `phone`, `desktop`, `WxH`, or `name=WxH` (a named size
    /// whose kind follows the name when it is one of the three).
    pub fn parse(text: &str) -> Result<TargetSize, String> {
        let (name, dims) = match text.split_once('=') {
            Some((name, dims)) => (name.trim(), Some(dims.trim())),
            None if text.contains(['x', 'X'])
                && text.chars().next().is_some_and(|c| c.is_ascii_digit()) =>
            {
                (text.trim(), Some(text.trim()))
            }
            None => (text.trim(), None),
        };
        let kind = match name {
            "glance" => SizeKind::Glance,
            "phone" => SizeKind::Phone,
            "desktop" => SizeKind::Desktop,
            _ => SizeKind::Custom,
        };
        let (width, height) = match (dims, kind) {
            (Some(dims), _) => {
                parse_dims(dims).ok_or_else(|| format!("size {text:?}: want <w>x<h>"))?
            }
            (None, SizeKind::Glance) => GLANCE,
            (None, SizeKind::Phone) => PHONE,
            (None, SizeKind::Desktop) => DESKTOP,
            (None, SizeKind::Custom) => {
                return Err(format!(
                    "unknown size {text:?}: glance, phone, desktop or <w>x<h>"
                ))
            }
        };
        Ok(TargetSize {
            name: name.to_string(),
            kind,
            width,
            height,
        })
    }

    /// The three sizes ADR 0002 §7 names.
    pub fn defaults() -> Vec<TargetSize> {
        ["glance", "phone", "desktop"]
            .iter()
            .map(|s| TargetSize::parse(s).unwrap())
            .collect()
    }
}

pub fn parse_dims(text: &str) -> Option<(f64, f64)> {
    let (w, h) = text.split_once(['x', 'X'])?;
    let (w, h) = (w.trim().parse::<f64>().ok()?, h.trim().parse::<f64>().ok()?);
    (w.is_finite() && h.is_finite() && w >= 1.0 && h >= 1.0).then_some((w, h))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_dims_and_overrides() {
        assert_eq!(TargetSize::parse("phone").unwrap().width, 390.0);
        let g = TargetSize::parse("glance=300x120").unwrap();
        assert_eq!(
            (g.kind, g.width, g.height),
            (SizeKind::Glance, 300.0, 120.0)
        );
        let c = TargetSize::parse("640x480").unwrap();
        assert_eq!((c.kind, c.name.as_str()), (SizeKind::Custom, "640x480"));
        assert!(TargetSize::parse("huge").is_err());
        assert!(TargetSize::parse("tile=0x5").is_err());
        assert_eq!(TargetSize::defaults().len(), 3);
    }
}
