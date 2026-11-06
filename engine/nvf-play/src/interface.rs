//! What the parts of the original's interface are drawn with, shared by the modules that
//! draw one (`message_menu`, `corner_messages`, `hud`, `dialogue_menu`): lengths in the interface's units (960
//! to the height of the view), the interface's colour, the game's fonts and textures
//! read from the owner's archives at run time, a line of text set as glyphs of the
//! font's texture, and the index of the interface's texture atlas.
//!
//! Without the game's files a font is stood in for by one whose characters are all as
//! wide as each other, and its text is drawn with Bevy's font.

use bevy::image::{CompressedImageFormatSupport, CompressedImageFormats, ImageSampler};
use bevy::prelude::*;
use fnv_font::{Font, Glyph, Line};
use nvf_assets::GameData;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};

/// The interface's colour [observed: the brightest text of the owner's screenshot,
/// message-menu.md §3].
pub const AMBER: [f32; 3] = [1.0, 182.0 / 255.0, 66.0 / 255.0];

/// How dark the sheet behind the menus drawn over the world in play makes it (the
/// container and barter menus, the tag-skill and trait lists): 0.75 black [design: the
/// owner's wish after the playtest of 2026-10-11, amber text was hard to read over an
/// orange world; the original blurs the world behind these menus instead (not built),
/// its sheet is lighter: about 0.33 to 0.4 measured on the screenshots].
pub const MENU_BACKING: f32 = 0.75;

/// The stacking level of the character and workbench menus (tag skills, traits, name,
/// level-up, recipes, repair services): over the HUD, the corner and quest messages
/// (50), the dialogue menu and the Pip-Boy (90), the terminal and the casino games (106);
/// under the message boxes (108) these menus may ask, the tutorial (109), the pause menu
/// (110), the console (120) and the loading screen [design: the owner's playtest list 7,
/// item 2, "on top level with nothing in front"].
pub const TOP_MENU_Z: i32 = 107;

/// How dark the sheet behind those menus makes the world: the container and barter
/// menus' 0.93 (`crate::menu::look`) [design: list 7 item 2, "the backgrounds need to be
/// darker", the 0.75 of [`MENU_BACKING`] and the files' 32 of 255 let the world show
/// through the text].
pub const TOP_MENU_SHADE: f32 = 0.93;

/// The sheet those menus spread over the whole screen, on their root node: nothing of the
/// world or of the HUD's messages shows beside them either.
pub fn top_menu_backing() -> BackgroundColor {
    BackgroundColor(Color::BLACK.with_alpha(TOP_MENU_SHADE))
}

/// The menu files' shared fill: `_background_fill_alpha`, 32 of 255 [documented:
/// globals.xml]. The sheets the recipe and repair-services menus spread behind themselves
/// (`RM_Background`, `RSM_Background`) take it; at this strength their edge cannot be seen
/// over the world [observed: the owner's screenshots of both menus, 2026-10-19,
/// docs/spec-notes/repair.md §3].
pub const BACKGROUND_FILL_ALPHA: f32 = 32.0 / 255.0;

/// [`AMBER`] as 0 to 255 per channel: the default of both interface colours.
pub const AMBER_RGB: [u8; 3] = [255, 182, 66];

/// The colour the HUD and the menus are drawn in, and the one of the Pip-Boy's screens
/// (`crate::ui_colour` sets them from the settings page), packed as `0xRRGGBB`. Global,
/// not a resource: [`amber`] is called from drawing code that has no world at hand.
static HUD_RGB: AtomicU32 = AtomicU32::new(pack(AMBER_RGB));
static PIPBOY_RGB: AtomicU32 = AtomicU32::new(pack(AMBER_RGB));

const fn pack([r, g, b]: [u8; 3]) -> u32 {
    (r as u32) << 16 | (g as u32) << 8 | b as u32
}

fn unpack(v: u32) -> [u8; 3] {
    [(v >> 16) as u8, (v >> 8) as u8, v as u8]
}

/// The HUD's and the menus' colour.
pub fn hud_rgb() -> [u8; 3] {
    unpack(HUD_RGB.load(Ordering::Relaxed))
}

pub fn set_hud_rgb(rgb: [u8; 3]) {
    HUD_RGB.store(pack(rgb), Ordering::Relaxed);
}

/// The Pip-Boy screens' colour.
pub fn pipboy_rgb() -> [u8; 3] {
    unpack(PIPBOY_RGB.load(Ordering::Relaxed))
}

pub fn set_pipboy_rgb(rgb: [u8; 3]) {
    PIPBOY_RGB.store(pack(rgb), Ordering::Relaxed);
}

/// `rgb` at a brightness (0 to 255).
pub fn tint(rgb: [u8; 3], brightness: f32) -> Color {
    let b = brightness / 255.0;
    Color::srgb(f32::from(rgb[0]) / 255.0 * b, f32::from(rgb[1]) / 255.0 * b, f32::from(rgb[2]) / 255.0 * b)
}

/// The interface's colour (the HUD's, [`hud_rgb`]) at a brightness (0 to 255, as the
/// interface files give it).
pub fn amber(brightness: f32) -> Color {
    tint(hud_rgb(), brightness)
}

/// The Pip-Boy screens' colour ([`pipboy_rgb`]) at a brightness, as [`amber`].
pub fn pipboy_amber(brightness: f32) -> Color {
    tint(pipboy_rgb(), brightness)
}

/// A length of the interface (960 units high) as a part of the view's height.
pub fn u(units: f32) -> Val {
    Val::Vh(units / 9.6)
}

pub fn at(left: f32, top: f32, width: f32, height: f32) -> Node {
    Node { position_type: PositionType::Absolute, left: u(left), top: u(top), width: u(width), height: u(height), ..default() }
}

/// [design: fonts.md §3.2, #1221] A zero-size anchor at a text line's baseline. Its
/// layout rounds to a physical pixel, then its 0.25-pixel sampling phase places the
/// fractional glyph children consistently.
pub fn line_anchor(base: f32) -> (Node, LayoutConfig, UiTransform) {
    (
        at(0.0, base, 0.0, 0.0),
        LayoutConfig { use_rounding: true },
        UiTransform::from_translation(Val2::px(0.0, 0.25)),
    )
}

/// Whether the device can sample the block-compressed textures the game ships.
pub fn bc_supported(formats: Option<&CompressedImageFormatSupport>) -> bool {
    formats.is_none_or(|f| f.0.contains(CompressedImageFormats::BC))
}

/// A line of text: where it starts and the line its letters stand on.
#[derive(Debug, Clone, PartialEq)]
pub struct Set {
    pub line: Line,
    pub x: f32,
    pub base: f32,
}

/// A font and its texture (the picture and its size in texels); without the texture the
/// text is drawn with Bevy's font.
pub struct Face {
    pub font: Font,
    pub image: Option<(Handle<Image>, Vec2)>,
}

/// A game texture for the interface: not repeated, so that a stretched picture's two
/// ends do not run into each other.
pub fn texture(path: &str, data: &GameData, images: &mut Assets<Image>, bc: bool) -> Option<(Handle<Image>, Vec2)> {
    texture_read(path, data, images, bc, nvf_assets::texture::dds_image)
}

/// A font's texture: [`texture`] with its largest level only, so a glyph cut out of it
/// never takes in its neighbours from a smaller level ([`nvf_assets::texture::dds_image_top`]).
pub fn font_texture(path: &str, data: &GameData, images: &mut Assets<Image>, bc: bool) -> Option<(Handle<Image>, Vec2)> {
    texture_read(path, data, images, bc, nvf_assets::texture::dds_image_top)
}

fn texture_read(
    path: &str,
    data: &GameData,
    images: &mut Assets<Image>,
    bc: bool,
    decode: fn(&[u8], bool, bool) -> Result<Image, String>,
) -> Option<(Handle<Image>, Vec2)> {
    let bytes = data.read(path)?;
    match decode(&bytes, true, bc) {
        Ok(mut image) => {
            image.sampler = ImageSampler::linear();
            let size = image.size().as_vec2();
            Some((images.add(image), size))
        }
        Err(e) => {
            warn!("interface: {path}: {e}");
            None
        }
    }
}

/// A stand-in font for measuring when the game's cannot be read: every character as wide
/// as the next.
pub fn plain_font(size: f32, advance: f32) -> Font {
    let glyph = Glyph { after: advance, ..default() };
    Font { size, texture: String::new(), glyphs: vec![glyph; 256] }
}

/// The install's own font files, by number (`None`: the default, [`FONT_FILES`]); set once
/// at start ([`set_font_files`]).
static INSTALL_FONTS: std::sync::OnceLock<[Option<String>; 9]> = std::sync::OnceLock::new();

/// The font files an INI's `[Fonts]` section names: `sFontFile_1` … `_9`, each as a game
/// path in small letters with backslashes; `None` where the INI names none
/// [documented: Fallout_default.ini `[Fonts]`; hypothesis: a localized install points
/// these at its own files, #1439].
pub fn fonts_in_ini(text: &str) -> [Option<String>; 9] {
    std::array::from_fn(|i| {
        let v = fnv_install::ini_get(text, "Fonts", &format!("sFontFile_{}", i + 1))?;
        let v = v.trim().trim_matches('"').replace('/', "\\").to_ascii_lowercase();
        (!v.is_empty()).then_some(v)
    })
}

/// Draw font n with the file `files[n - 1]` names where it names one, from now on (once
/// a run; a second call is ignored).
pub fn set_font_files(files: [Option<String>; 9]) {
    let _ = INSTALL_FONTS.set(files);
}

/// The file a font is read from: a default font file ([`FONT_FILES`]) is the install's
/// own one for that number when its INI names one ([`set_font_files`]); any other path
/// is itself.
pub fn font_file(path: &str) -> String {
    font_file_in(path, INSTALL_FONTS.get())
}

fn font_file_in(path: &str, install: Option<&[Option<String>; 9]>) -> String {
    let own = install.and_then(|fonts| {
        let n = FONT_FILES.iter().position(|f| f.eq_ignore_ascii_case(path))?;
        fonts[n].clone()
    });
    own.unwrap_or_else(|| path.to_string())
}

/// The game's font at `path` (the install's own file for it: [`font_file`]), else `stand_in`.
pub fn face(path: &str, stand_in: Font, data: Option<&GameData>, images: &mut Assets<Image>, bc: bool) -> Face {
    let path = &font_file(path);
    let read = data.and_then(|d| {
        let font = match Font::parse(&d.read(path)?) {
            Ok(f) => f,
            Err(e) => {
                warn!("interface: {path}: {e}");
                return None;
            }
        };
        let image = font_texture(&font.texture_path(), d, images, bc)?;
        Some(Face { font, image: Some(image) })
    });
    read.unwrap_or_else(|| {
        warn!("interface: drawn without the game's font {path}");
        Face { font: stand_in, image: None }
    })
}

/// The INI's fonts by number, font n at `n - 1` [documented: Fallout_default.ini
/// `sFontFile_1` … `_9`].
pub const FONT_FILES: [&str; 9] = [
    "textures\\fonts\\glow_monofonto_large.fnt",
    "textures\\fonts\\monofonto_large.fnt",
    "textures\\fonts\\glow_monofonto_medium.fnt",
    "textures\\fonts\\monofonto_verylarge02_dialogs2.fnt",
    "textures\\fonts\\fixedsys_comp_uniform_width.fnt",
    "textures\\fonts\\glow_monofonto_vl_dialogs.fnt",
    "textures\\fonts\\baked-in_monofonto_large.fnt",
    "textures\\fonts\\glow_futura_caps_large.fnt",
    "textures\\fonts\\nvfont_test.fnt",
];

/// Where a font's letters stand from a line's top: the tallest glyph's rise [design].
pub fn line_base(face: &Face) -> f32 {
    face.font.glyphs.iter().skip(0x20).take(0x5F).filter(|g| g.drawn()).map(|g| g.ascent).fold(0.0, f32::max).max(face.font.size * 0.75)
}

/// Draw a line of text at `origin` (what its `x` and `base` are measured from). Every
/// node that carries the colour also gets `mark`.
pub fn set_text(p: &mut ChildSpawnerCommands, face: &Face, set: &Set, origin: Vec2, color: Color, mark: impl Bundle + Clone) {
    set_marked(p, face, &set.line, set.x - origin.x, set.base - origin.y, color, mark);
}

/// Draw a line of text that starts at `x` and stands on `base`, both from the parent's
/// top left corner.
pub fn set_line(p: &mut ChildSpawnerCommands, face: &Face, line: &Line, x: f32, base: f32, color: Color) {
    set_marked(p, face, line, x, base, color, ());
}

/// [`set_line`] with `mark` on every coloured node (each glyph, or the stand-in text).
pub fn set_marked(p: &mut ChildSpawnerCommands, face: &Face, line: &Line, x: f32, base: f32, color: Color, mark: impl Bundle + Clone) {
    let Some((image, size)) = &face.image else {
        // The codes are the run's code page's, not Latin-1 (`Ж` is 0xC6 in Windows-1251).
        let text = line.text();
        let size = face.font.size * 0.75;
        p.spawn(Node { position_type: PositionType::Absolute, left: u(x), top: u(base - size), ..default() }).with_children(|n| {
            n.spawn((Text::new(text), TextFont { font_size: FontSize::Vh(size / 9.6), ..default() }, TextColor(color), mark));
        });
        return;
    };
    // [observed: #1221] Snap the shared baseline once, but keep each texture cut-out's
    // fractional box. Aspect fitting and per-glyph layout rounding made identical glyphs differ.
    p.spawn(line_anchor(base)).with_children(|p| {
        for placed in &line.glyphs {
            let g = face.font.glyph(placed.code);
            if !g.drawn() {
                continue;
            }
            let rect = Rect::new(g.uv[0] * size.x, g.uv[1] * size.y, g.uv[2] * size.x, g.uv[3] * size.y);
            p.spawn((
                at(x + placed.x + g.before, -g.ascent, g.width, g.height),
                ImageNode { rect: Some(rect), color, image_mode: NodeImageMode::Stretch, ..ImageNode::new(image.clone()) },
                LayoutConfig { use_rounding: false },
                mark.clone(),
            ));
        }
    });
}

/// Where a picture lies in an atlas texture: the atlas's file name and the picture's
/// left, top, width and height as parts of the atlas's width and height.
#[derive(Debug, Clone, PartialEq)]
pub struct AtlasPiece {
    pub atlas: String,
    pub rect: [f32; 4],
}

/// Read an atlas index (`textures\interface\interfaceshared.tai`), by picture file name
/// in small letters. A line is the picture's file name, then the atlas's file name, a
/// number, a type, and after them the left and top offsets, a depth offset, and the
/// width and height, all as parts of the atlas; `#` starts a comment [documented: the
/// file's own heading; corner-messages.md §2.2]. Lines that do not read so are left out.
pub fn atlas_index(text: &str) -> HashMap<String, AtlasPiece> {
    let mut out = HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((name, rest)) = line.split_once(char::is_whitespace) else { continue };
        let fields: Vec<&str> = rest.split(',').map(str::trim).collect();
        let num = |i: usize| fields.get(i).and_then(|f| f.parse::<f32>().ok());
        if let (Some(&atlas), Some(x), Some(y), Some(w), Some(h)) = (fields.first(), num(3), num(4), num(6), num(7)) {
            out.insert(name.to_ascii_lowercase(), AtlasPiece { atlas: atlas.to_ascii_lowercase(), rect: [x, y, w, h] });
        }
    }
    out
}

/// The interface's texture atlas index [documented: corner-messages.md §2.2].
pub const ATLAS_INDEX: &str = "textures\\interface\\interfaceshared.tai";

/// The file name an atlas index knows a picture by.
pub fn file_name(path: &str) -> String {
    path.rsplit(['\\', '/']).next().unwrap_or(path).to_ascii_lowercase()
}

/// A picture: its texture and the part of it that is the picture (all of it for `None`).
pub type Picture = (Handle<Image>, Option<Rect>);

/// The atlas index a tile's `texatlas` names, as a game path in small letters: the
/// files name it under the textures folder (`Interface\InterfaceShared.tai` is
/// [`ATLAS_INDEX`]) [documented: GECK wiki Texatlas, its example beside a `filename`
/// under `textures\`; the HUD's `texatlas` traits]. `None` for an empty name.
pub fn atlas_index_path(texatlas: &str) -> Option<String> {
    let t = texatlas.trim().trim_start_matches(['\\', '/']).replace('/', "\\").to_ascii_lowercase();
    if t.is_empty() {
        return None;
    }
    Some(if t.starts_with("textures\\") { t } else { format!("textures\\{t}") })
}

/// Where a picture is read from: the texture's game path and, for a piece of an atlas,
/// the piece as parts of the texture's width and height (left, top, width, height).
#[derive(Debug, Clone, PartialEq)]
pub struct Located {
    pub texture: String,
    pub part: Option<[f32; 4]>,
}

/// A picture with its size in texels (the atlas piece's, or the whole texture's).
pub type SizedPicture = (Handle<Image>, Option<Rect>, Vec2);

/// The interface's pictures as its files name them, read when first asked for.
#[derive(Default)]
pub struct Pictures {
    atlas: HashMap<String, AtlasPiece>,
    /// Other atlas indexes a tile's `texatlas` named, by game path; empty for one that
    /// could not be read.
    indexes: HashMap<String, HashMap<String, AtlasPiece>>,
    /// Textures read so far, by path; `None` for one that could not be read.
    textures: HashMap<String, Option<(Handle<Image>, Vec2)>>,
}

impl Pictures {
    pub fn load(data: Option<&GameData>) -> Pictures {
        let atlas = data.and_then(|d| d.read(ATLAS_INDEX)).map(|b| atlas_index(&String::from_utf8_lossy(&b))).unwrap_or_default();
        Pictures { atlas, ..default() }
    }

    /// Pictures with this shared atlas index and no files to read (tests).
    pub fn with_atlas(atlas: HashMap<String, AtlasPiece>) -> Pictures {
        Pictures { atlas, ..default() }
    }

    fn texture(&mut self, path: String, data: &GameData, images: &mut Assets<Image>, bc: bool) -> Option<(Handle<Image>, Vec2)> {
        self.textures.entry(path).or_insert_with_key(|p| texture(p, data, images, bc)).clone()
    }

    /// Where a picture named as the interface names it (under the textures folder) is:
    /// its piece of the atlas `texatlas` names when that index has its file name (the
    /// index read once through `read` and kept), else its piece of the shared atlas, else
    /// its own file. The atlas texture is the index's line's file, in the index's folder
    /// [documented: interfaceshared.tai's heading, "InterfaceShared<idx>.dds"; hypothesis:
    /// another index's atlas beside it, as the shared one's]. Falling back to the shared
    /// atlas and the own file when the named index lacks the picture is [design].
    pub fn locate(&mut self, path: &str, texatlas: Option<&str>, read: &dyn Fn(&str) -> Option<Vec<u8>>) -> Located {
        let name = file_name(path);
        if let Some(index) = texatlas.and_then(atlas_index_path).filter(|p| p != ATLAS_INDEX) {
            let pieces = self.indexes.entry(index.clone()).or_insert_with(|| read(&index).map(|b| atlas_index(&String::from_utf8_lossy(&b))).unwrap_or_default());
            if let Some(piece) = pieces.get(&name) {
                let folder = index.rfind('\\').map_or("", |i| &index[..=i]);
                return Located { texture: format!("{folder}{}", piece.atlas), part: Some(piece.rect) };
            }
        }
        if let Some(piece) = self.atlas.get(&name) {
            return Located { texture: format!("textures\\interface\\{}", piece.atlas), part: Some(piece.rect) };
        }
        Located { texture: own_texture(path), part: None }
    }

    /// A picture named as the interface names it (under the textures folder): its
    /// piece of the atlas when the index has its file name, else its own file.
    pub fn picture(&mut self, path: &str, data: Option<&GameData>, images: &mut Assets<Image>, bc: bool) -> Option<Picture> {
        self.picture_sized(path, None, data, images, bc).map(|(image, rect, _)| (image, rect))
    }

    /// [`Pictures::picture`] in the atlas `texatlas` names ([`Pictures::locate`]), with
    /// its size in texels.
    pub fn picture_sized(&mut self, path: &str, texatlas: Option<&str>, data: Option<&GameData>, images: &mut Assets<Image>, bc: bool) -> Option<SizedPicture> {
        let data = data?;
        let at = self.locate(path, texatlas, &|p| data.read(p));
        if let Some([x, y, w, h]) = at.part {
            if let Some((image, size)) = self.texture(at.texture, data, images, bc) {
                let rect = Rect::new(x * size.x, y * size.y, (x + w) * size.x, (y + h) * size.y);
                return Some((image, Some(rect), rect.size()));
            }
        }
        self.texture(own_texture(path), data, images, bc).map(|(image, size)| (image, None, size))
    }

    /// A picture read from its own file only, with its size in texels: no atlas looked in
    /// (a whole menu's image without `texatlas`, `crate::tiles::draw::Source::Own`).
    pub fn own_sized(&mut self, path: &str, data: Option<&GameData>, images: &mut Assets<Image>, bc: bool) -> Option<SizedPicture> {
        self.texture(own_texture(path), data?, images, bc).map(|(image, size)| (image, None, size))
    }

    /// A texture by the full game path a model names it by (`textures\…`), read once and
    /// kept, with its size in texels: the tile HUD's `nif` tiles' pieces
    /// (`crate::tiles::model`).
    pub fn file(&mut self, path: &str, data: &GameData, images: &mut Assets<Image>, bc: bool) -> Option<(Handle<Image>, Vec2)> {
        self.texture(path.trim().to_ascii_lowercase(), data, images, bc)
    }

    /// The game path of the texture a picture was read from: its atlas's when
    /// [`Pictures::picture_sized`] found it a `piece` (`texatlas` as given there), else
    /// its own file's ([`Pictures::own_sized`], or the atlas's fallback).
    pub fn texture_path(&mut self, path: &str, texatlas: Option<&str>, piece: bool, data: &GameData) -> String {
        if piece {
            self.locate(path, texatlas, &|p| data.read(p)).texture
        } else {
            own_texture(path)
        }
    }
}

/// A picture's own file: its path under the textures folder, in small letters.
fn own_texture(path: &str) -> String {
    format!("textures\\{}", path.trim().trim_start_matches(['\\', '/']).to_ascii_lowercase())
}

#[cfg(test)]
#[path = "interface_layout_tests.rs"]
mod layout_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_atlas_index_is_read() {
        let text = "# Heading\n#\nIcon_A.dds\t\tShared0.dds, 0, 2D, 0.25, 0.5, 0.0, 0.125, 0.0625\n\nnot a line\nb.dds Shared0.dds, 0, 2D, x, 0, 0, 1, 1\n";
        let index = atlas_index(text);
        assert_eq!(index.len(), 1);
        assert_eq!(index["icon_a.dds"], AtlasPiece { atlas: "shared0.dds".into(), rect: [0.25, 0.5, 0.125, 0.0625] });
    }

    #[test]
    fn a_pictures_file_name_is_what_the_atlas_knows_it_by() {
        assert_eq!(file_name("Interface\\Icons\\Message Icons\\Glow_Message_Vaultboy_In_Pain.dds"), "glow_message_vaultboy_in_pain.dds");
        assert_eq!(file_name("a.dds"), "a.dds");
    }

    #[test]
    fn the_installs_ini_names_its_fonts() {
        let ini = "[General]\nsFontFile_1=Nope.fnt\n[Fonts]\nsFontFile_1=Textures\\Fonts\\Glow_Monofonto_Large_RU.fnt\nsFontFile_7 = Textures/Fonts/Baked-in_RU.fnt\nsFontFile_9=\n";
        let fonts = fonts_in_ini(ini);
        assert_eq!(fonts[0].as_deref(), Some("textures\\fonts\\glow_monofonto_large_ru.fnt"));
        assert_eq!(fonts[6].as_deref(), Some("textures\\fonts\\baked-in_ru.fnt"));
        assert!(fonts[1].is_none() && fonts[8].is_none());
        // A default font file is read from the install's own file for its number.
        assert_eq!(font_file_in(FONT_FILES[0], Some(&fonts)), "textures\\fonts\\glow_monofonto_large_ru.fnt");
        assert_eq!(font_file_in("Textures\\Fonts\\Baked-in_Monofonto_Large.fnt", Some(&fonts)), "textures\\fonts\\baked-in_ru.fnt");
        // A font the INI does not name, another path, or no INI: the path as given.
        assert_eq!(font_file_in(FONT_FILES[1], Some(&fonts)), FONT_FILES[1]);
        assert_eq!(font_file_in("textures\\fonts\\other.fnt", Some(&fonts)), "textures\\fonts\\other.fnt");
        assert_eq!(font_file_in(FONT_FILES[0], None), FONT_FILES[0]);
    }
}
