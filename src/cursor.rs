use std::{collections::HashMap, io::Read, time::Duration};

use tracing::warn;
use xcursor::{
    parser::{parse_xcursor, Image},
    CursorTheme,
};

use crate::config::Config;

static FALLBACK_CURSOR_DATA: &[u8] = include_bytes!("../resources/cursor.rgba");

pub struct Cursor {
    pub size: u32,
    icons: HashMap<String, Vec<Image>>,
    current: String,
    theme: String,
}

impl Cursor {
    pub fn load() -> Cursor {
        let cursor_theme_name = Config::with(|config| {
            config.cursor_theme.clone()
        });
        let size = Config::with(|config| {
            config.cursor_size
        });

        let theme = CursorTheme::load(&cursor_theme_name);
        let default_cursor = load_icon(&theme, "default")
            .map_err(|err| warn!("Unable to load xcursor: {}, using fallback cursor", err))
            .unwrap_or_else(|_| {
                vec![Image {
                    size: 32,
                    width: 64,
                    height: 64,
                    xhot: 1,
                    yhot: 1,
                    delay: 1,
                    pixels_rgba: Vec::from(FALLBACK_CURSOR_DATA),
                    pixels_argb: vec![], //unused
                }]
            });
        let mut icons = HashMap::new();
        icons.insert("default".to_string(), default_cursor);

        Cursor {
            size,
            icons,
            theme: cursor_theme_name,
            current: "default".to_string(),
        }
    }
    pub fn load_icon(&mut self, name: &str) {
        self.current = name.to_string();
        if self.icons.contains_key(name) {
            return;
        }

        let theme = CursorTheme::load(&self.theme);
        let cursor = load_icon(&theme, name)
            .map_err(|err| warn!("Unable to load xcursor: {}, using fallback cursor", err))
            .unwrap_or_else(|_| {
                vec![Image {
                    size: 32,
                    width: 64,
                    height: 64,
                    xhot: 1,
                    yhot: 1,
                    delay: 1,
                    pixels_rgba: Vec::from(FALLBACK_CURSOR_DATA),
                    pixels_argb: vec![], //unused
                }]
            });

        self.icons.insert(name.to_string(), cursor);
    }
    pub fn get_image(&self, scale: f32, time: Duration) -> Image {
        let size = (self.size as f32 * scale).round() as u32;
        frame(
            time.as_millis() as u32,
            size,
            self.icons.get(&self.current).unwrap(),
        )
    }
}

fn nearest_images(size: u32, images: &[Image]) -> impl Iterator<Item = &Image> {
    // Follow the nominal size of the cursor to choose the nearest
    let nearest_image = images
        .iter()
        .min_by_key(|image| (size as i32 - image.size as i32).abs())
        .unwrap();

    images.iter().filter(move |image| {
        image.width == nearest_image.width && image.height == nearest_image.height
    })
}

fn frame(mut millis: u32, size: u32, images: &[Image]) -> Image {
    let total = nearest_images(size, images).fold(0, |acc, image| acc + image.delay);
    millis %= total;

    for img in nearest_images(size, images) {
        if millis < img.delay {
            return img.clone();
        }
        millis -= img.delay;
    }

    unreachable!()
}

#[derive(thiserror::Error, Debug)]
enum Error {
    #[error("Theme has no default cursor")]
    NoDefaultCursor,
    #[error("Error opening xcursor file: {0}")]
    File(#[from] std::io::Error),
    #[error("Failed to parse XCursor file")]
    Parse,
}

fn load_icon(theme: &CursorTheme, icon_name: &str) -> Result<Vec<Image>, Error> {
    let icon_path = theme.load_icon(icon_name).ok_or(Error::NoDefaultCursor)?;
    let mut cursor_file = std::fs::File::open(icon_path)?;
    let mut cursor_data = Vec::new();
    cursor_file.read_to_end(&mut cursor_data)?;
    parse_xcursor(&cursor_data).ok_or(Error::Parse)
}
