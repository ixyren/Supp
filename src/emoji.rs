//! Colour emoji in the platform's own style or selectable `iamcal/emoji-data`
//! artwork sets (Apple, Google, Twitter, Facebook), with bundled Noto Color
//! Emoji as fallback.
//!
//! Layout replaces each emoji sequence with a transparent placeholder, and
//! its picture is painted over it. System/bundled glyphs render off the main
//! thread via `fastframe-emoji`; custom `iamcal` artwork sheets download and
//! cache lazily in `<cache>/emoji` and crop 64×64 sprites on demand.

use std::collections::HashMap;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock, RwLock};

use egui::epaint::{ClippedShape, Galley, Shape, TextShape};
use egui::text::LayoutJob;
use egui::{
    Color32, ColorImage, FullOutput, Pos2, Rect, TextFormat, TextureHandle, TextureId,
    TextureOptions, Vec2,
};

use crate::settings::EmojiStyle;

pub use fastframe_emoji::{PLACEHOLDER, only_emoji};

/// Noto Color Emoji, behind the platform's font.
const BUNDLED: &[u8] = include_bytes!("../assets/fonts/NotoColorEmoji.ttf");

/// The newest emoji version the bundled font draws in full (Noto Color
/// Emoji 2.057 has Emoji 18.0). Raise it with the font: a test checks that
/// the font draws every emoji up to it.
const BUNDLED_VERSION: emojis::EmojiVersion = emojis::EmojiVersion::new(18, 0);

/// Pinned `iamcal/emoji-data` dataset version on jsDelivr.
const IAMCAL_VERSION: &str = "16.0.0";
/// Pixel size of each emoji tile in `sheets-256/64.png`.
const TILE_SIZE: u32 = 64;
/// Cell stride in `sheets-256/64.png` (64 px sprite + 1 px border on each side).
const TILE_STRIDE: u32 = 66;
/// Painted height multiplier matching `fastframe_emoji`.
const EMOJI_SIDE: f32 = 1.08;

/// Download/readiness state of the selected [`EmojiStyle`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StyleStatus {
    Ready,
    Downloading,
    Failed(String),
}

#[derive(Clone)]
struct Sheet {
    pixels: Arc<image::RgbaImage>,
    /// Lowercase hyphen-separated Unicode hex key -> `(sheet_x, sheet_y)`.
    coords: Arc<HashMap<String, (u32, u32)>>,
}

impl Sheet {
    fn from_bytes(slug: &str, json_bytes: &[u8], png_bytes: &[u8]) -> Result<Self, String> {
        let img = image::load_from_memory(png_bytes)
            .map_err(|error| format!("Could not decode emoji sheet: {error}"))?
            .to_rgba8();
        let entries: Vec<serde_json::Value> = serde_json::from_slice(json_bytes)
            .map_err(|error| format!("Could not parse emoji metadata: {error}"))?;
        let has_flag = format!("has_img_{slug}");
        let mut coords = HashMap::with_capacity(entries.len() * 2);
        let mut record = |entry: &serde_json::Value| {
            if !entry
                .get(&has_flag)
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false)
            {
                return;
            }
            let (Some(x), Some(y)) = (
                entry.get("sheet_x").and_then(serde_json::Value::as_u64),
                entry.get("sheet_y").and_then(serde_json::Value::as_u64),
            ) else {
                return;
            };
            let pos = (x as u32, y as u32);
            if pos.0 * TILE_STRIDE + 1 + TILE_SIZE > img.width()
                || pos.1 * TILE_STRIDE + 1 + TILE_SIZE > img.height()
            {
                return;
            }
            if let Some(unified) = entry.get("unified").and_then(serde_json::Value::as_str) {
                coords.insert(unified.to_ascii_lowercase(), pos);
            }
            if let Some(non_q) = entry
                .get("non_qualified")
                .and_then(serde_json::Value::as_str)
            {
                coords.insert(non_q.to_ascii_lowercase(), pos);
            }
        };
        for entry in &entries {
            record(entry);
            if let Some(vars) = entry
                .get("skin_variations")
                .and_then(serde_json::Value::as_object)
            {
                for var in vars.values() {
                    record(var);
                }
            }
        }
        Ok(Self {
            pixels: Arc::new(img),
            coords: Arc::new(coords),
        })
    }

    fn coords_for(&self, cluster: &str) -> Option<(u32, u32)> {
        for key in cluster_keys(cluster) {
            if let Some(&pos) = self.coords.get(&key) {
                return Some(pos);
            }
        }
        None
    }

    fn crop_tile(&self, x: u32, y: u32) -> Option<ColorImage> {
        let px = x.checked_mul(TILE_STRIDE)?.checked_add(1)?;
        let py = y.checked_mul(TILE_STRIDE)?.checked_add(1)?;
        if px + TILE_SIZE > self.pixels.width() || py + TILE_SIZE > self.pixels.height() {
            return None;
        }
        let mut rgba = Vec::with_capacity((TILE_SIZE * TILE_SIZE * 4) as usize);
        let raw = self.pixels.as_raw();
        let row_stride = self.pixels.width() as usize * 4;
        let x_off = px as usize * 4;
        let width_bytes = TILE_SIZE as usize * 4;
        for row in py..py + TILE_SIZE {
            let start = row as usize * row_stride + x_off;
            rgba.extend_from_slice(&raw[start..start + width_bytes]);
        }
        Some(ColorImage::from_rgba_unmultiplied(
            [TILE_SIZE as usize, TILE_SIZE as usize],
            &rgba,
        ))
    }
}

#[derive(Clone)]
enum StyleEntry {
    Idle,
    Downloading,
    Ready(Sheet),
    Failed(String),
}

struct StyleStore {
    active: EmojiStyle,
    generation: u64,
    entries: HashMap<EmojiStyle, StyleEntry>,
}

fn store() -> &'static RwLock<StyleStore> {
    static STORE: OnceLock<RwLock<StyleStore>> = OnceLock::new();
    STORE.get_or_init(|| {
        RwLock::new(StyleStore {
            active: EmojiStyle::System,
            generation: 0,
            entries: HashMap::new(),
        })
    })
}

/// Switches the active emoji artwork style and, when a custom `iamcal` set is
/// chosen, ensures it is cached locally or fetched in the background.
pub fn set_style(ctx: &egui::Context, style: EmojiStyle, cache_dir: PathBuf) {
    let needs_load = {
        let mut guard = store().write().unwrap_or_else(|p| p.into_inner());
        let changed = guard.active != style;
        guard.active = style;
        if changed {
            guard.generation = guard.generation.wrapping_add(1);
        }
        if style == EmojiStyle::System {
            false
        } else {
            let entry = guard.entries.entry(style).or_insert(StyleEntry::Idle);
            match entry {
                StyleEntry::Idle | StyleEntry::Failed(_) => {
                    if cfg!(test) {
                        false
                    } else {
                        *entry = StyleEntry::Downloading;
                        true
                    }
                }
                StyleEntry::Downloading | StyleEntry::Ready(_) => false,
            }
        }
    };
    if !needs_load {
        ctx.request_repaint();
        return;
    }
    let ctx = ctx.clone();
    let _ = std::thread::Builder::new()
        .name(format!("emoji-style-{}", style.slug().unwrap_or("system")))
        .spawn(move || {
            let outcome = load_or_fetch_sheet(style, &cache_dir);
            {
                let mut guard = store().write().unwrap_or_else(|p| p.into_inner());
                match outcome {
                    Ok(sheet) => {
                        guard.entries.insert(style, StyleEntry::Ready(sheet));
                    }
                    Err(error) => {
                        log::warn!("could not load {style:?} emoji style: {error}");
                        guard.entries.insert(style, StyleEntry::Failed(error));
                    }
                }
                guard.generation = guard.generation.wrapping_add(1);
            }
            ctx.request_repaint();
        });
}

/// Returns whether the selected `style` is ready, still downloading, or failed.
pub fn style_status(style: EmojiStyle) -> StyleStatus {
    if style == EmojiStyle::System {
        return StyleStatus::Ready;
    }
    let guard = store().read().unwrap_or_else(|p| p.into_inner());
    match guard.entries.get(&style) {
        Some(StyleEntry::Ready(_)) => StyleStatus::Ready,
        Some(StyleEntry::Downloading) => StyleStatus::Downloading,
        Some(StyleEntry::Failed(error)) => StyleStatus::Failed(error.clone()),
        Some(StyleEntry::Idle) | None => StyleStatus::Ready,
    }
}

fn active_sheet() -> Option<(u64, Sheet)> {
    let guard = store().read().unwrap_or_else(|p| p.into_inner());
    if guard.active == EmojiStyle::System {
        return None;
    }
    match guard.entries.get(&guard.active) {
        Some(StyleEntry::Ready(sheet)) => Some((guard.generation, sheet.clone())),
        _ => None,
    }
}

fn load_or_fetch_sheet(style: EmojiStyle, cache_dir: &Path) -> Result<Sheet, String> {
    let slug = style
        .slug()
        .ok_or_else(|| "System emoji style uses the OS font".to_owned())?;
    std::fs::create_dir_all(cache_dir)
        .map_err(|error| format!("Could not create emoji cache folder: {error}"))?;
    let json_path = cache_dir.join(format!("emoji-{IAMCAL_VERSION}.json"));
    let png_path = cache_dir.join(format!("sheet-{slug}-64-{IAMCAL_VERSION}.png"));

    let json_url = format!("https://cdn.jsdelivr.net/npm/emoji-datasource@{IAMCAL_VERSION}/emoji.json");
    let png_url = format!(
        "https://cdn.jsdelivr.net/npm/emoji-datasource-{slug}@{IAMCAL_VERSION}/img/{slug}/sheets-256/64.png"
    );

    let json_bytes = read_or_download(&json_path, &json_url)?;
    let png_bytes = read_or_download(&png_path, &png_url)?;
    match Sheet::from_bytes(slug, &json_bytes, &png_bytes) {
        Ok(sheet) => Ok(sheet),
        Err(first_error) => {
            // Re-download corrupted cache once before failing.
            let _ = std::fs::remove_file(&png_path);
            if let Ok(fresh_png) = download_bytes(&png_url)
                && write_atomic(&png_path, &fresh_png).is_ok()
                && let Ok(sheet) = Sheet::from_bytes(slug, &json_bytes, &fresh_png)
            {
                return Ok(sheet);
            }
            Err(first_error)
        }
    }
}

fn read_or_download(path: &Path, url: &str) -> Result<Vec<u8>, String> {
    if let Ok(bytes) = std::fs::read(path)
        && !bytes.is_empty()
    {
        return Ok(bytes);
    }
    let bytes = download_bytes(url)?;
    let _ = write_atomic(path, &bytes);
    Ok(bytes)
}

fn download_bytes(url: &str) -> Result<Vec<u8>, String> {
    let agent = crate::proxy::agent();
    agent
        .get(url)
        .call()
        .map_err(|error| format!("Could not download emoji assets: {error}"))?
        .body_mut()
        .read_to_vec()
        .map_err(|error| format!("Could not read emoji download: {error}"))
}

fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes)?;
    match std::fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(_) => {
            let _ = std::fs::remove_file(path);
            std::fs::rename(&tmp, path)
        }
    }
}

/// Normalizes a Rust emoji grapheme cluster into candidate `iamcal/emoji-data`
/// lowercase unified keys.
fn cluster_keys(cluster: &str) -> Vec<String> {
    let parts: Vec<String> = cluster
        .chars()
        .map(|c| format!("{:04x}", c as u32))
        .collect();
    if parts.is_empty() {
        return Vec::new();
    }
    let exact = parts.join("-");
    let mut keys = Vec::with_capacity(4);
    keys.push(exact.clone());

    let stripped: Vec<&str> = parts
        .iter()
        .map(String::as_str)
        .filter(|part| *part != "fe0f")
        .collect();
    if !stripped.is_empty() {
        let joined = stripped.join("-");
        if !keys.contains(&joined) {
            keys.push(joined);
        }
        // Keycaps like `#️⃣` sometimes omit U+FE0F (`0023-20e3` vs `0023-fe0f-20e3`).
        if stripped.len() == 2 && stripped[1] == "20e3" {
            let keycap = format!("{}-fe0f-20e3", stripped[0]);
            if !keys.contains(&keycap) {
                keys.push(keycap);
            }
        }
        // Single-scalar emoji with default presentation sometimes listed as `<cp>-fe0f`.
        if parts.len() == 1 {
            let with_vs = format!("{}-fe0f", parts[0]);
            if !keys.contains(&with_vs) {
                keys.push(with_vs);
            }
        }
    }
    keys
}

#[derive(Clone, Default)]
struct SheetTextureCache(Arc<std::sync::Mutex<SheetTextures>>);

#[derive(Default)]
struct SheetTextures {
    generation: u64,
    textures: HashMap<String, Option<TextureHandle>>,
}

impl SheetTextureCache {
    fn of(ctx: &egui::Context) -> Self {
        ctx.data_mut(|data| {
            data.get_temp_mut_or_default::<Self>(egui::Id::new("supp-iamcal-emoji-textures"))
                .clone()
        })
    }

    fn texture(
        &self,
        ctx: &egui::Context,
        generation: u64,
        sheet: &Sheet,
        cluster: &str,
    ) -> Option<TextureHandle> {
        let mut guard = self.0.lock().unwrap_or_else(|p| p.into_inner());
        if guard.generation != generation {
            guard.generation = generation;
            guard.textures.clear();
        }
        if let Some(cached) = guard.textures.get(cluster) {
            return cached.clone();
        }
        let handle = sheet.coords_for(cluster).and_then(|(x, y)| {
            let image = sheet.crop_tile(x, y)?;
            Some(ctx.load_texture(
                format!("iamcal-emoji-{generation}-{x}-{y}"),
                image,
                TextureOptions::LINEAR,
            ))
        });
        guard.textures.insert(cluster.to_owned(), handle.clone());
        handle
    }
}

fn fit_square(rect: Rect) -> Rect {
    let side = rect.width().min(rect.height() * EMOJI_SIDE);
    Rect::from_center_size(
        rect.center() + egui::vec2(0.0, rect.height() * 0.02),
        Vec2::splat(side.max(0.0)),
    )
}

/// Whether ZapFast offers this emoji in its pickers and completions. The
/// bundled font is asked last on every platform, so an emoji it draws has a
/// picture everywhere; one the `emojis` list knows before the font does
/// would show as an empty box, so it is left out.
pub fn offered(emoji: &emojis::Emoji) -> bool {
    emoji.emoji_version() <= BUNDLED_VERSION
}

/// Every emoji ZapFast offers, in the `emojis` list's order.
pub fn catalogue() -> impl Iterator<Item = &'static emojis::Emoji> + Clone {
    emojis::iter().filter(|emoji| offered(emoji))
}

/// The emoji `text` names, when ZapFast offers it: what a saved recent or
/// frequent emoji is checked against before a picker shows it.
pub fn lookup(text: &str) -> Option<&'static emojis::Emoji> {
    emojis::get(text).filter(|emoji| offered(emoji))
}

fn setup() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        fastframe_emoji::EmojiSetup::default()
            .bundled(BUNDLED)
            .synchronous(cfg!(any(test, feature = "demo")))
            .install();
    });
}

/// Whether a colour emoji font is available.
pub fn available() -> bool {
    setup();
    fastframe_emoji::available()
}

/// Finds and maps the emoji fonts before the first frame needs them.
pub fn warm_up() {
    setup();
    fastframe_emoji::warm_up();
}

/// Colours emoji inside generic `egui` text using the selected `iamcal`
/// spritesheet when available, falling back seamlessly to `fastframe_emoji`.
#[derive(Default)]
pub struct SuppEmojiPlugin {
    fallback: fastframe_emoji::EmojiPlugin,
    cached_generation: u64,
    seen: HashMap<SeenKey, SeenEntry>,
}

type SeenKey = (usize, Option<Color32>);

struct SeenEntry {
    _galley: Arc<Galley>,
    coloured: Option<ColouredGalley>,
}

#[derive(Clone)]
struct ColouredGalley {
    galley: Arc<Galley>,
    pictures: Vec<(TextureId, Rect)>,
    baked: bool,
}

impl egui::Plugin for SuppEmojiPlugin {
    fn debug_name(&self) -> &'static str {
        "supp-emoji"
    }

    fn output_hook(&mut self, ctx: &egui::Context, output: &mut FullOutput) {
        if let Some((generation, sheet)) = active_sheet() {
            if self.cached_generation != generation {
                self.cached_generation = generation;
                self.seen.clear();
            }
            let cache = SheetTextureCache::of(ctx);
            let mut lookup = |cluster: &str| {
                cache
                    .texture(ctx, generation, &sheet, cluster)
                    .map(|handle| (handle.id(), handle.size_vec2()))
            };
            self.colour_shapes(&mut output.shapes, &mut lookup);
        } else if self.cached_generation != 0 {
            self.cached_generation = 0;
            self.seen.clear();
        }
        // Any remaining unhidden emoji glyphs (System style or emojis newer
        // than iamcal@16.0.0) are coloured by fastframe_emoji, which also
        // flushes texture uploads from `tex_manager` into `textures_delta`.
        self.fallback.output_hook(ctx, output);
    }
}

impl SuppEmojiPlugin {
    fn colour_shapes(
        &mut self,
        shapes: &mut Vec<ClippedShape>,
        lookup: &mut dyn FnMut(&str) -> Option<(TextureId, Vec2)>,
    ) {
        let mut previous = std::mem::take(&mut self.seen);
        let mut inserts: Vec<(usize, ClippedShape)> = Vec::new();
        for (index, clipped) in shapes.iter_mut().enumerate() {
            for picture in self.visit(&mut clipped.shape, &mut previous, lookup) {
                inserts.push((
                    index,
                    ClippedShape {
                        clip_rect: clipped.clip_rect,
                        shape: picture,
                    },
                ));
            }
        }
        splice_after(shapes, inserts);
    }

    fn visit(
        &mut self,
        shape: &mut Shape,
        previous: &mut HashMap<SeenKey, SeenEntry>,
        lookup: &mut dyn FnMut(&str) -> Option<(TextureId, Vec2)>,
    ) -> Vec<Shape> {
        match shape {
            Shape::Text(text) => self.text(text, previous, lookup),
            Shape::Vec(shapes) => {
                let mut inserts = Vec::new();
                for (index, inner) in shapes.iter_mut().enumerate() {
                    for picture in self.visit(inner, previous, lookup) {
                        inserts.push((index, picture));
                    }
                }
                splice_after(shapes, inserts);
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    fn text(
        &mut self,
        text: &mut TextShape,
        previous: &mut HashMap<SeenKey, SeenEntry>,
        lookup: &mut dyn FnMut(&str) -> Option<(TextureId, Vec2)>,
    ) -> Vec<Shape> {
        if text.angle != 0.0 || !may_hold_emoji(&text.galley.job.text) {
            return Vec::new();
        }
        let key = (Arc::as_ptr(&text.galley) as usize, text.override_text_color);
        let coloured = if let Some(seen) = previous.remove(&key) {
            let coloured = seen.coloured.clone();
            self.seen.insert(key, seen);
            coloured
        } else {
            let coloured = colour_galley(&text.galley, text.override_text_color, lookup);
            self.seen.insert(
                key,
                SeenEntry {
                    _galley: text.galley.clone(),
                    coloured: coloured.clone(),
                },
            );
            coloured
        };
        let Some(coloured) = coloured else {
            return Vec::new();
        };
        text.galley = coloured.galley;
        if coloured.baked {
            text.override_text_color = None;
        }
        let tint = Color32::WHITE.gamma_multiply(text.opacity_factor);
        let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
        coloured
            .pictures
            .iter()
            .map(|(id, rect)| Shape::image(*id, rect.translate(text.pos.to_vec2()), uv, tint))
            .collect()
    }
}

fn splice_after<T>(shapes: &mut Vec<T>, inserts: Vec<(usize, T)>) {
    if inserts.is_empty() {
        return;
    }
    let mut out = Vec::with_capacity(shapes.len() + inserts.len());
    let mut inserts = inserts.into_iter().peekable();
    for (index, shape) in std::mem::take(shapes).into_iter().enumerate() {
        out.push(shape);
        while let Some((_, picture)) = inserts.next_if(|(at, _)| *at == index) {
            out.push(picture);
        }
    }
    *shapes = out;
}

fn may_hold_emoji(text: &str) -> bool {
    !text.is_ascii() && text.chars().any(|c| u32::from(c) >= 0xA9)
}

fn colour_galley(
    galley: &Arc<Galley>,
    override_color: Option<Color32>,
    lookup: &mut dyn FnMut(&str) -> Option<(TextureId, Vec2)>,
) -> Option<ColouredGalley> {
    let text = &galley.job.text;
    let clusters: Vec<(Range<usize>, &str)> = {
        let mut at = 0;
        let mut found = Vec::new();
        for piece in fastframe_emoji::pieces(text) {
            let (fastframe_emoji::Piece::Text(run) | fastframe_emoji::Piece::Emoji(run)) = piece;
            if matches!(piece, fastframe_emoji::Piece::Emoji(_)) {
                found.push((at..at + run.len(), run));
            }
            at += run.len();
        }
        found
    };
    if clusters.is_empty() {
        return None;
    }
    let glyphs = pair_glyphs(galley)?;

    let mut pictures = Vec::new();
    let mut hide: Vec<(usize, usize)> = Vec::new();
    for (range, cluster) in clusters {
        let mine: Vec<&(usize, usize, usize)> = glyphs
            .iter()
            .filter(|(_, _, byte)| range.contains(byte))
            .collect();
        let Some(&&(row, first, _)) = mine.first() else {
            continue;
        };
        let first_glyph = &galley.rows[row].row.glyphs[first];
        if !cluster.starts_with(first_glyph.chr) {
            return None;
        }
        let visible: Vec<(usize, usize)> = mine
            .iter()
            .map(|&&(row, glyph, _)| (row, glyph))
            .filter(|&(row, glyph)| !galley.rows[row].row.glyphs[glyph].uv_rect.is_nothing())
            .collect();
        if visible.is_empty() || visible.iter().all(|&(row, glyph)| glyph_hidden(galley, row, glyph)) {
            continue;
        }
        let Some(rect) = cluster_rect(galley, &mine) else {
            continue;
        };
        if let Some((id, _)) = lookup(cluster) {
            pictures.push((id, fit_square(rect)));
            for &(row, glyph) in &visible {
                let first_vertex = galley.rows[row].row.glyphs[glyph].first_vertex as usize;
                hide.push((row, first_vertex));
            }
        }
    }
    if pictures.is_empty() {
        return None;
    }

    let mut copy = (**galley).clone();
    let baked = override_color.is_some();
    for (index, placed) in copy.rows.iter_mut().enumerate() {
        let hides_here = hide.iter().any(|&(row, _)| row == index);
        if !hides_here && !baked {
            continue;
        }
        let row = Arc::make_mut(&mut placed.row);
        if let Some(color) = override_color {
            let range = row.visuals.glyph_vertex_range.clone();
            for vertex in &mut row.visuals.mesh.vertices[range] {
                vertex.color = color;
            }
        }
        for &(_, first) in hide.iter().filter(|&&(row, _)| row == index) {
            for vertex in row.visuals.mesh.vertices.iter_mut().skip(first).take(4) {
                vertex.color = Color32::TRANSPARENT;
            }
        }
    }
    Some(ColouredGalley {
        galley: Arc::new(copy),
        pictures,
        baked,
    })
}

fn pair_glyphs(galley: &Galley) -> Option<Vec<(usize, usize, usize)>> {
    let mut order: Vec<(usize, usize)> = Vec::new();
    for (row, placed) in galley.rows.iter().enumerate() {
        let mut glyphs: Vec<usize> = (0..placed.row.glyphs.len()).collect();
        glyphs.sort_by_key(|&glyph| placed.row.glyphs[glyph].first_vertex);
        order.extend(glyphs.into_iter().map(|glyph| (row, glyph)));
    }
    if galley.elided {
        order.pop();
    }
    let characters: Vec<usize> = galley
        .job
        .text
        .char_indices()
        .filter(|&(_, c)| c != '\n')
        .map(|(byte, _)| byte)
        .collect();
    if order.len() > characters.len() {
        return None;
    }
    Some(
        order
            .into_iter()
            .zip(characters)
            .map(|((row, glyph), byte)| (row, glyph, byte))
            .collect(),
    )
}

fn glyph_hidden(galley: &Galley, row: usize, glyph: usize) -> bool {
    let row = &galley.rows[row].row;
    let first = row.glyphs[glyph].first_vertex as usize;
    row.visuals
        .mesh
        .vertices
        .iter()
        .skip(first)
        .take(4)
        .all(|vertex| vertex.color.a() == 0)
}

fn cluster_rect(galley: &Galley, glyphs: &[&(usize, usize, usize)]) -> Option<Rect> {
    let row = glyphs.first()?.0;
    let placed = &galley.rows[row];
    glyphs
        .iter()
        .filter(|(at, _, _)| *at == row)
        .map(|&&(_, glyph, _)| {
            let glyph = &placed.row.glyphs[glyph];
            Rect::from_min_max(
                Pos2::new(glyph.pos.x, 0.0),
                Pos2::new(glyph.pos.x + glyph.advance_width, placed.row.size.y),
            )
        })
        .reduce(|a, b| a.union(b))
        .map(|rect| rect.translate(placed.pos.to_vec2()))
}

/// The egui plugin that colours the emoji in every other text: labels,
/// buttons, menus, tooltips and text fields. It leaves the placeholders
/// `append` lays out and the glyphs `editor_job` hides to the paint calls
/// below, so message bodies keep their selectable, copyable placeholders.
pub fn plugin() -> SuppEmojiPlugin {
    setup();
    SuppEmojiPlugin::default()
}

/// Queues pictures to be drawn off the interface thread before a frame shows
/// them: the first page of a picker as it opens.
pub fn prewarm<'a>(ctx: &egui::Context, clusters: impl IntoIterator<Item = &'a str>) {
    setup();
    fastframe_emoji::prewarm(ctx, clusters);
}

/// Appends text with placeholders and records their emoji sequences.
pub fn append(
    ui: &egui::Ui,
    job: &mut LayoutJob,
    placements: &mut Vec<String>,
    text: &str,
    format: &TextFormat,
) -> usize {
    setup();
    fastframe_emoji::append(ui, job, placements, text, format)
}

/// Paints one emoji over `rect`, or its text when no font draws it.
pub fn paint_cluster(ui: &egui::Ui, cluster: &str, rect: Rect) {
    setup();
    if let Some((generation, sheet)) = active_sheet()
        && let Some(texture) = SheetTextureCache::of(ui.ctx()).texture(ui.ctx(), generation, &sheet, cluster)
    {
        ui.painter().image(
            texture.id(),
            fit_square(rect),
            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
            Color32::WHITE,
        );
        return;
    }
    fastframe_emoji::paint_cluster(ui, cluster, rect);
}

/// Builds an editor layout without changing character offsets. Emoji glyphs
/// are transparent and returned for painting over.
pub fn editor_job(
    text: &str,
    format: &egui::TextFormat,
) -> (egui::text::LayoutJob, Vec<(usize, usize, String)>) {
    setup();
    fastframe_emoji::editor_job(text, format)
}

/// Paints the emoji over a galley's placeholders.
pub fn paint(ui: &egui::Ui, galley: &egui::Galley, origin: Pos2, placements: &[String]) {
    setup();
    if placements.is_empty() {
        return;
    }
    if active_sheet().is_some() {
        for (rect, cluster) in fastframe_emoji::placeholder_rects(galley).zip(placements) {
            paint_cluster(ui, cluster, rect.translate(origin.to_vec2()));
        }
        return;
    }
    fastframe_emoji::paint(ui, galley, origin, placements);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The bundled font alone, as a Mac or Windows machine falls back to it.
    fn bundled() -> fastframe_emoji::Emoji {
        fastframe_emoji::EmojiSetup::default()
            .system(false)
            .bundled(BUNDLED)
            .load()
    }

    #[test]
    fn the_bundled_font_renders_colour_emoji() {
        let picture = bundled().render("\u{1F600}", 72).expect("grinning face");
        assert_eq!(picture.size[1], 72);
        assert!(picture.has_colour());
    }

    #[test]
    fn the_bundled_font_joins_sequences() {
        let emoji = bundled();
        for sequence in ["🇩🇪", "👍🏽", "👨‍👩‍👧", "#️⃣", "🏳️‍🌈"] {
            let joined = emoji.render(sequence, 72).expect("a picture");
            let first: String = sequence.chars().take(1).collect();
            assert_ne!(
                Some(joined),
                emoji.render(&first, 72),
                "{sequence} fell back to its first part"
            );
        }
    }

    /// Every emoji a picker offers has the bundled font's own picture: not
    /// nothing, and not the first part of a sequence the font can't join.
    #[test]
    fn the_bundled_font_draws_every_offered_emoji() {
        let emoji = bundled();
        let mut missing = Vec::new();
        for offered in catalogue() {
            let text = offered.as_str();
            let picture = emoji.render(text, 16);
            let parts = text.chars().filter(|c| *c != '\u{FE0F}').count();
            let first: String = text.chars().take(1).collect();
            if picture.is_none() || (parts > 1 && picture == emoji.render(&first, 16)) {
                missing.push(format!("{text} {}", offered.name()));
            }
        }
        assert!(missing.is_empty(), "the bundled font lacks {missing:?}");
    }

    #[test]
    fn the_newest_emoji_are_offered() {
        let newest = lookup("\u{1FAEB}").expect("an Emoji 18.0 face is offered");
        assert_eq!(newest.emoji_version(), BUNDLED_VERSION);
        assert!(lookup("not an emoji").is_none());
    }

    #[test]
    fn placeholders_line_up_with_placements() {
        let mut job = LayoutJob::default();
        let mut placements = Vec::new();
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            append(
                ui,
                &mut job,
                &mut placements,
                "a 😀 b",
                &TextFormat::default(),
            );
        });
        output.textures_delta.clear();
        assert!(available(), "the bundled font is always there");
        assert_eq!(placements, vec!["😀".to_owned()]);
        assert_eq!(job.text.matches(PLACEHOLDER).count(), 1);
    }

    #[test]
    fn an_editor_job_keeps_the_text_and_places_the_emoji() {
        let format =
            egui::TextFormat::simple(egui::FontId::proportional(14.0), egui::Color32::WHITE);
        let text = "hi 😊 and 👍🏽!";
        let (job, clusters) = editor_job(text, &format);
        assert_eq!(job.text, text, "the galley must mirror the buffer");
        assert_eq!(clusters.len(), 2);
        for (start, length, cluster) in &clusters {
            assert_eq!(
                text.chars().skip(*start).take(*length).collect::<String>(),
                *cluster
            );
        }
    }

    #[test]
    fn emoji_only_messages_are_counted() {
        assert_eq!(only_emoji("😂"), Some(1));
        assert_eq!(only_emoji("😂 🙏"), Some(2));
        assert_eq!(only_emoji("ok 😂"), None);
    }

    #[test]
    fn cluster_keys_normalize_simple_skin_tone_flag_keycap_and_zwj() {
        assert_eq!(cluster_keys("😀"), vec!["1f600", "1f600-fe0f"]);
        assert!(cluster_keys("👍🏽").contains(&"1f44d-1f3fd".to_owned()));
        assert!(cluster_keys("#️⃣").contains(&"0023-fe0f-20e3".to_owned()));
        assert!(cluster_keys("#\u{20E3}").contains(&"0023-fe0f-20e3".to_owned()));
        assert!(cluster_keys("🇩🇪").contains(&"1f1e9-1f1ea".to_owned()));
        assert!(cluster_keys("🏳️‍🌈").contains(&"1f3f3-fe0f-200d-1f308".to_owned()));
    }

    #[test]
    fn sheet_indexes_variations_and_crops_66px_stride_tiles() {
        let mut sheet_img = image::RgbaImage::new(TILE_STRIDE * 2, TILE_STRIDE * 2);
        // Paint cell (1, 1) inside its 1px border with bright red.
        for py in TILE_STRIDE + 1..TILE_STRIDE + 1 + TILE_SIZE {
            for px in TILE_STRIDE + 1..TILE_STRIDE + 1 + TILE_SIZE {
                sheet_img.put_pixel(px, py, image::Rgba([255, 0, 0, 255]));
            }
        }
        let mut png_bytes = std::io::Cursor::new(Vec::new());
        sheet_img
            .write_to(&mut png_bytes, image::ImageFormat::Png)
            .unwrap();
        let json = br#"[
          {
            "unified": "1F44D",
            "sheet_x": 0,
            "sheet_y": 0,
            "has_img_apple": true,
            "skin_variations": {
              "1F3FD": {
                "unified": "1F44D-1F3FD",
                "sheet_x": 1,
                "sheet_y": 1,
                "has_img_apple": true
              }
            }
          }
        ]"#;
        let sheet = Sheet::from_bytes("apple", json, &png_bytes.into_inner()).expect("valid sheet");
        assert_eq!(sheet.coords_for("👍"), Some((0, 0)));
        assert_eq!(sheet.coords_for("👍🏽"), Some((1, 1)));
        let tile = sheet.crop_tile(1, 1).expect("crops red sprite");
        assert_eq!(tile.size, [64, 64]);
        assert_eq!(tile.pixels[0], Color32::from_rgba_unmultiplied(255, 0, 0, 255));
    }
}
