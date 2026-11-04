//! One render at one size, as the remote instrument reported it, and how to
//! read it.
//!
//! Three routes are read (makepad `docs/agents/app-remote.md`):
//!
//! - `/snap?all=1` — `{"s":[{"i":id,"ty":type,"r":[x,y,w,h],"t":text,…}]}`, in
//!   tree order, including widgets that were NOT drawn (their rect is zero).
//!   That is what makes "the layout ran out of room" measurable.
//! - `/d` — the drawn tree: a `W<n> <count>` header, then one line per widget,
//!   `index parent id Type x y w h`.
//! - `/log?n=…` — `{"n":seq,"l":[lines]}`; card-host logs its realize report
//!   there as `card-host: realize {json}`.
//!
//! Rects are window-local layout points, y down.
use serde::{Deserialize, Serialize};

/// A rectangle in layout points: x, y, width, height.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub fn new(x: f64, y: f64, w: f64, h: f64) -> Rect {
        Rect { x, y, w, h }
    }
    pub fn right(&self) -> f64 {
        self.x + self.w
    }
    pub fn bottom(&self) -> f64 {
        self.y + self.h
    }
    pub fn is_empty(&self) -> bool {
        self.w <= 0.0 || self.h <= 0.0
    }
    pub fn area(&self) -> f64 {
        if self.is_empty() {
            0.0
        } else {
            self.w * self.h
        }
    }
    pub fn intersection(&self, other: &Rect) -> Rect {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let r = self.right().min(other.right());
        let b = self.bottom().min(other.bottom());
        Rect::new(x, y, (r - x).max(0.0), (b - y).max(0.0))
    }
    /// How far `self` sticks out of `outer` on each side (left, top, right,
    /// bottom), never negative.
    pub fn overhang(&self, outer: &Rect) -> [f64; 4] {
        [
            (outer.x - self.x).max(0.0),
            (outer.y - self.y).max(0.0),
            (self.right() - outer.right()).max(0.0),
            (self.bottom() - outer.bottom()).max(0.0),
        ]
    }
    pub fn as_array(&self) -> [f64; 4] {
        [self.x, self.y, self.w, self.h]
    }
}

/// One widget from `/snap`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Widget {
    pub id: String,
    #[serde(rename = "type")]
    pub ty: String,
    pub rect: Rect,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// The parent's id, from `/d` for drawn widgets; for an undrawn one,
    /// from its inspectable id (`beauty_0_2_5` → `beauty_0_2`) when it has one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    /// Whether `/d` listed it, i.e. it was laid out and drawn.
    pub drawn: bool,
}

/// One line of `/d`.
#[derive(Clone, Debug, PartialEq)]
pub struct TreeNode {
    pub index: i64,
    pub parent: i64,
    pub id: String,
    pub ty: String,
    pub rect: Rect,
}

/// Parse `/d`'s text. Unparseable lines are skipped.
pub fn parse_tree(text: &str) -> Vec<TreeNode> {
    let mut out = Vec::new();
    for line in text.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 8 {
            continue;
        }
        let n = parts.len();
        let nums: Option<Vec<f64>> = parts[n - 4..]
            .iter()
            .map(|p| p.parse::<f64>().ok())
            .collect();
        let (Some(nums), Ok(index), Ok(parent)) =
            (nums, parts[0].parse::<i64>(), parts[1].parse::<i64>())
        else {
            continue;
        };
        out.push(TreeNode {
            index,
            parent,
            id: parts[2..n - 5].join(" "),
            ty: parts[n - 5].to_string(),
            rect: Rect::new(nums[0], nums[1], nums[2], nums[3]),
        });
    }
    out
}

/// Parse `/snap`'s JSON into widgets (parents and `drawn` unset).
pub fn parse_snap(json: &serde_json::Value) -> Vec<Widget> {
    let Some(list) = json.get("s").and_then(|s| s.as_array()) else {
        return Vec::new();
    };
    list.iter()
        .filter_map(|w| {
            let r = w.get("r")?.as_array()?;
            let n = |i: usize| r.get(i).and_then(|v| v.as_f64()).unwrap_or(0.0);
            Some(Widget {
                id: w.get("i")?.as_str()?.to_string(),
                ty: w
                    .get("ty")
                    .and_then(|t| t.as_str())
                    .unwrap_or("")
                    .to_string(),
                rect: Rect::new(n(0), n(1), n(2), n(3)),
                text: w.get("t").and_then(|t| t.as_str()).map(str::to_string),
                parent: None,
                drawn: false,
            })
        })
        .collect()
}

/// Everything one render at one size produced.
#[derive(Clone, Debug, Default)]
pub struct Capture {
    /// `/snap?all=1`.
    pub snap: serde_json::Value,
    /// `/d`.
    pub tree: String,
    /// `/log` lines.
    pub log: Vec<String>,
}

/// The widget id the card is mounted in. card-host's window holds
/// `card := Splash`; everything inside it is the card.
pub const CARD_ROOT: &str = "card";

impl Capture {
    pub fn from_parts(
        snap: serde_json::Value,
        tree: String,
        log_json: &serde_json::Value,
    ) -> Capture {
        let log = log_json
            .get("l")
            .and_then(|l| l.as_array())
            .map(|l| {
                l.iter()
                    .filter_map(|s| s.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        Capture { snap, tree, log }
    }

    /// The card's viewport: the rect of the Splash it is mounted in.
    pub fn viewport(&self) -> Option<Rect> {
        parse_tree(&self.tree)
            .into_iter()
            .find(|n| n.id == CARD_ROOT)
            .map(|n| n.rect)
    }

    /// The card's widgets — the Splash's descendants, drawn or not — in tree
    /// order, with parents and `drawn` filled in. The Splash itself is left
    /// out, and so is the host's chrome.
    pub fn card_widgets(&self) -> Vec<Widget> {
        let tree = parse_tree(&self.tree);
        let Some(root) = tree.iter().find(|n| n.id == CARD_ROOT) else {
            return Vec::new();
        };
        // Drawn descendants of the root, from `/d`'s parent links.
        let mut inside = std::collections::HashSet::new();
        inside.insert(root.index);
        let mut parent_of = std::collections::HashMap::new();
        let by_index: std::collections::HashMap<i64, &TreeNode> =
            tree.iter().map(|n| (n.index, n)).collect();
        for node in &tree {
            // `/d` lists parents before children.
            if inside.contains(&node.parent) {
                inside.insert(node.index);
                if node.parent != root.index {
                    if let Some(p) = by_index.get(&node.parent) {
                        parent_of.insert(node.id.clone(), p.id.clone());
                    }
                }
            }
        }
        let drawn: std::collections::HashSet<&str> = tree
            .iter()
            .filter(|n| inside.contains(&n.index) && n.index != root.index)
            .map(|n| n.id.as_str())
            .collect();
        // Chrome that `/d` places OUTSIDE the root marks the end of the card
        // in `/snap`'s tree order.
        let outside: std::collections::HashSet<&str> = tree
            .iter()
            .filter(|n| !inside.contains(&n.index))
            .map(|n| n.id.as_str())
            .collect();

        let snap = parse_snap(&self.snap);
        let Some(start) = snap.iter().position(|w| w.id == CARD_ROOT) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for mut w in snap.into_iter().skip(start + 1) {
            if outside.contains(w.id.as_str()) || HOST_SIBLINGS.contains(&w.id.as_str()) {
                break;
            }
            w.drawn = drawn.contains(w.id.as_str()) && !w.rect.is_empty();
            w.parent = parent_of
                .get(&w.id)
                .cloned()
                .or_else(|| inspectable_parent(&w.id));
            out.push(w);
        }
        out
    }

    /// card-host's `card-host: realize {json}` line, if it logged one.
    pub fn realize_report(&self) -> Option<serde_json::Value> {
        self.log.iter().rev().find_map(|line| {
            let at = line.find(REALIZE_PREFIX)?;
            serde_json::from_str(&line[at + REALIZE_PREFIX.len()..]).ok()
        })
    }

    /// Error lines (`[E] …`) in the log.
    pub fn log_errors(&self) -> Vec<String> {
        self.log
            .iter()
            .filter(|l| l.starts_with("[E]"))
            .cloned()
            .collect()
    }
}

/// What card-host prefixes its realize report with.
pub const REALIZE_PREFIX: &str = "card-host: realize ";

/// card-host's own widgets after the card in tree order.
const HOST_SIBLINGS: &[&str] = &["sheet", "ai_chat", "tweaker"];

/// card-host's kit lowering names every node `beauty_<path>`; the parent is
/// the path less its last step.
fn inspectable_parent(id: &str) -> Option<String> {
    let rest = id.strip_prefix("beauty_")?;
    let (parent, _) = rest.rsplit_once('_')?;
    Some(format!("beauty_{parent}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tree_lines_parse_from_the_right() {
        let t = parse_tree(
            "W3 2\n0 -1 main_window Window 0 0 390 844\n6 5 card Splash 0 32 412 860\nnoise\n",
        );
        assert_eq!(t.len(), 2);
        assert_eq!(t[1].id, "card");
        assert_eq!(t[1].ty, "Splash");
        assert_eq!(t[1].rect, Rect::new(0.0, 32.0, 412.0, 860.0));
    }

    #[test]
    fn rect_arithmetic() {
        let a = Rect::new(0.0, 0.0, 10.0, 10.0);
        let b = Rect::new(5.0, 5.0, 10.0, 10.0);
        assert_eq!(a.intersection(&b).area(), 25.0);
        assert_eq!(b.overhang(&a), [0.0, 0.0, 5.0, 5.0]);
        assert_eq!(
            inspectable_parent("beauty_0_2_5").as_deref(),
            Some("beauty_0_2")
        );
        assert_eq!(inspectable_parent("beauty_0"), None);
    }
}
