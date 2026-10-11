//! A bounded atlas for the Android drawer's already decoded native icons.
//! Neighboring icons then share a texture and can use one instanced draw.
use makepad_widgets::*;
use std::collections::HashMap;

const SIDE: usize = 2048;
const MAX_ICON: usize = 192;

#[derive(Clone)]
pub(super) struct AtlasIcon {
    pub texture: Texture,
    pub scale: Vec2f,
    pub pan: Vec2f,
    source: TextureId,
}

#[derive(Default)]
pub(super) struct IconAtlas {
    texture: Option<Texture>,
    entries: HashMap<String, AtlasIcon>,
    x: usize,
    y: usize,
    row_height: usize,
    revision: Option<u64>,
}

impl IconAtlas {
    pub(super) fn set_revision(&mut self, revision: u64) {
        if self.revision != Some(revision) {
            *self = Self {
                revision: Some(revision),
                ..Self::default()
            };
        }
    }

    pub(super) fn get(&mut self, cx: &mut Cx, path: &str, source: &Texture) -> Option<AtlasIcon> {
        if let Some(icon) = self.entries.get(path) {
            if icon.source == source.texture_id() {
                return Some(icon.clone());
            }
        }
        if source.animation(cx).is_some() {
            return None;
        }
        let (width, height, data) = match source.get_format(cx) {
            TextureFormat::VecBGRAu8_32 {
                width,
                height,
                data: Some(data),
                ..
            }
            | TextureFormat::VecMipBGRAu8_32 {
                width,
                height,
                data: Some(data),
                ..
            } => (*width, *height, data),
            _ => return None,
        };
        if width == 0
            || height == 0
            || width > MAX_ICON
            || height > MAX_ICON
            || data.len() < width * height
        {
            return None;
        }
        // Preserve every source pixel. Duplicate the outer pixel into a
        // one-pixel gutter so bilinear sampling cannot bleed adjacent icons.
        let (w, h) = (width + 2, height + 2);
        let (x, y, row_height) = if self.x + w > SIDE {
            (0, self.y + self.row_height, 0)
        } else {
            (self.x, self.y, self.row_height)
        };
        // One 16 MiB page at most; unsupported/oversized/full entries keep
        // the ordinary individual texture path rather than growing memory.
        if y + h > SIDE {
            return None;
        }
        let pixels = data[..width * height].to_vec();
        let texture = self
            .texture
            .get_or_insert_with(|| {
                Texture::new_with_format(
                    cx,
                    TextureFormat::VecBGRAu8_32 {
                        width: SIDE,
                        height: SIDE,
                        data: Some(vec![0; SIDE * SIDE]),
                        updated: TextureUpdated::Full,
                    },
                )
            })
            .clone();
        let mut atlas = texture.take_vec_u32(cx);
        for dy in 0..h {
            let sy = dy.saturating_sub(1).min(height - 1);
            let dst = (y + dy) * SIDE + x;
            let src = sy * width;
            atlas[dst] = pixels[src];
            atlas[dst + 1..dst + 1 + width].copy_from_slice(&pixels[src..src + width]);
            atlas[dst + w - 1] = pixels[src + width - 1];
        }
        texture.put_back_vec_u32(
            cx,
            atlas,
            Some(RectUsize::new(PointUsize::new(x, y), SizeUsize::new(w, h))),
        );
        self.x = x + w;
        self.y = y;
        self.row_height = row_height.max(h);
        let icon = AtlasIcon {
            texture,
            source: source.texture_id(),
            scale: vec2(width as f32 / SIDE as f32, height as f32 / SIDE as f32),
            pan: vec2((x + 1) as f32 / SIDE as f32, (y + 1) as f32 / SIDE as f32),
        };
        self.entries.insert(path.to_owned(), icon.clone());
        Some(icon)
    }
}
